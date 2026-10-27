/*
Copyright 2024 The Hyperlight Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

use core::ffi::c_void;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use hyperlight_common::vmem;
use hyperlight_guest_bin::{exception, paging};

// Wasmtime Embedding Interface

const WASMTIME_PROT_READ: u32 = 1 << 0;
const WASMTIME_PROT_WRITE: u32 = 1 << 1;
const WASMTIME_PROT_EXEC: u32 = 1 << 2;

#[allow(non_camel_case_types)] // we didn't choose the name!
type wasmtime_trap_handler_t =
    extern "C" fn(ip: usize, fp: usize, has_faulting_addr: bool, faulting_addr: usize);
static WASMTIME_REQUESTED_TRAP_HANDLER: spin::Mutex<Option<wasmtime_trap_handler_t>> =
    spin::Mutex::new(None);

extern "C" fn page_fault_handler(pc: u64, fp: u64, va: u64) -> ! {
    let guard = WASMTIME_REQUESTED_TRAP_HANDLER.lock();
    let handler = *guard;
    drop(guard);
    if let Some(handler) = handler {
        handler(pc as usize, fp as usize, true, va as usize);
    }
    panic!("wasmtime did not handle page fault");
}
extern "C" fn undef_handler(pc: u64, fp: u64) -> ! {
    let guard = WASMTIME_REQUESTED_TRAP_HANDLER.lock();
    let handler = *guard;
    drop(guard);
    if let Some(handler) = handler {
        handler(pc as usize, fp as usize, false, 0);
    }
    panic!("wasmtime did not handle undefined instruction");
}

// Extremely stupid virtual address allocator
// 0x1_0000_0000 is where the module is
// we start at
// 0x100_0000_0000 and go up from there
static FIRST_VADDR: AtomicU64 = AtomicU64::new(0x100_0000_0000u64);

/* We don't actually have any sensible virtual memory areas, so
 * we just give out virtual addresses very coarsely with
 * probably-more-than-enough space between them */
#[no_mangle]
pub extern "C" fn wasmtime_mmap_new(size: usize, prot_flags: u32, ret: &mut *mut u8) -> i32 {
    if size > 0x100_0000_0000 {
        panic!("wasmtime_mmap_{:x} {:x}", size, prot_flags);
    }
    let va = FIRST_VADDR.fetch_add(0x100_0000_0000, Ordering::Relaxed) as *mut u8;
    wasmtime_mmap_remap(va, size, prot_flags);
    *ret = va;
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_mmap_remap(addr: *mut u8, size: usize, prot_flags: u32) -> i32 {
    // Since wasmtime needs a huge buffer region (4GB) and we don't
    // have hugepages yet, don't actually record anything unless the
    // protection flags tell us we need to.  One of
    // wasmtime_mmap_remap, wasmtime_mprotect, or
    // WasmtimeCodeMemory::publish will have a chance to change this
    // later.
    if prot_flags != 0 {
        unsafe {
            paging::map_region(
                0,
                addr,
                size as u64,
                vmem::MappingKind::ZeroInit(vmem::BasicMapping {
                    readable: prot_flags & WASMTIME_PROT_READ != 0,
                    writable: prot_flags & WASMTIME_PROT_WRITE != 0,
                    executable: prot_flags & WASMTIME_PROT_EXEC != 0,
                }),
            );
        }
    }
    paging::barrier::first_valid_same_ctx();
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_munmap(_ptr: *mut u8, _size: usize) -> i32 {
    0
}

// On most architectures, "upgrades" including the first time that
// a a valid translation exists for a VA, need a lighter barrier
// than full downgrades, and don't even need that if another
// barrier has been used.
struct BarrierTracker {
    downgrade_start: Option<u64>,
    has_ops_since_last_downgrade: bool,
}
impl BarrierTracker {
    fn new() -> Self {
        Self {
            downgrade_start: None,
            has_ops_since_last_downgrade: false,
        }
    }
    fn track(&mut self, here: u64, is_downgrade: bool) {
        if is_downgrade && self.downgrade_start.is_none() {
            self.downgrade_start = Some(here);
        } else if !is_downgrade {
            if let Some(start) = self.downgrade_start {
                paging::barrier::downgrade_in_place(start..here);
                self.downgrade_start = None;
                self.has_ops_since_last_downgrade = false;
            } else {
                self.has_ops_since_last_downgrade = true;
            }
        }
    }
}

/* TODO: implement permissions properly */
#[no_mangle]
pub extern "C" fn wasmtime_mprotect(ptr: *mut u8, size: usize, prot_flags: u32) -> i32 {
    let base_u = ptr as u64;
    let mut bt = BarrierTracker::new();
    let readable = prot_flags & WASMTIME_PROT_READ != 0;
    let writable = prot_flags & WASMTIME_PROT_WRITE != 0;
    let executable = prot_flags & WASMTIME_PROT_EXEC != 0;
    unsafe {
        paging::modify_mapping(ptr, size as u64, |va, mapping| {
            bt.track(va, true);
            match mapping.kind {
                // The main reason an unmapped region would show up in
                // an mprotect'd region is if it was left unmapped by
                // wasmtime_mmap_new above, so just treat it
                // more-or-less the same as ZeroInit.
                vmem::MappingKind::Unmapped => vmem::MappingKind::ZeroInit(vmem::BasicMapping {
                    readable,
                    writable,
                    executable,
                }),
                vmem::MappingKind::ZeroInit(_) => vmem::MappingKind::ZeroInit(vmem::BasicMapping {
                    readable,
                    writable,
                    executable,
                }),
                vmem::MappingKind::Basic(_) => vmem::MappingKind::Basic(vmem::BasicMapping {
                    readable,
                    writable,
                    executable,
                }),
                vmem::MappingKind::Cow(_) if !writable => {
                    vmem::MappingKind::Basic(vmem::BasicMapping {
                        readable,
                        writable,
                        executable,
                    })
                }
                vmem::MappingKind::Cow(_) => vmem::MappingKind::Cow(vmem::CowMapping {
                    readable,
                    executable,
                }),
            }
        });
    }
    bt.track(base_u + size as u64, false);

    0
}

#[no_mangle]
pub extern "C" fn wasmtime_page_size() -> usize {
    unsafe { hyperlight_guest_bin::OS_PAGE_SIZE as usize }
}

#[no_mangle]
pub extern "C" fn wasmtime_init_traps(handler: wasmtime_trap_handler_t) -> i32 {
    *WASMTIME_REQUESTED_TRAP_HANDLER.lock() = Some(handler);
    exception::register_page_fault_handler(page_fault_handler);
    exception::register_undefined_instruction_handler(undef_handler);
    // TODO: Add handlers for any other traps that wasmtime needs,
    //       probably including at least some floating-point
    //       exceptions
    0
}

#[derive(PartialEq)]
enum RemapOriginal {
    MakeCow,
    DontChange,
}

#[derive(PartialEq)]
enum ModifyCopy {
    MakeWritable,
    DontChange,
}

// Copy a VA range to a new VA. Old and new VA, and len, must be
// page-aligned.
fn copy_va_mapping(
    base: *const u8,
    len: usize,
    to_va: *mut u8,
    remap_original: RemapOriginal,
    modify_copy: ModifyCopy,
) {
    debug_assert!((base as usize).is_multiple_of(vmem::PAGE_SIZE));
    debug_assert!(len.is_multiple_of(vmem::PAGE_SIZE));
    let base_u = base as u64;

    let mut bt = BarrierTracker::new();

    unsafe {
        paging::modify_mapping(base as *mut u8, len as u64, |va, mapping| {
            // TODO: Deduplicate with identical logic in hyperlight_host snapshot.
            let (new_kind, was_writable) = match mapping.kind {
                // Skip unmapped pages, since they will be unmapped in
                // both the original and the new copy
                vmem::MappingKind::Unmapped => return vmem::MappingKind::Unmapped,
                vmem::MappingKind::ZeroInit(bm) => (vmem::MappingKind::ZeroInit(bm), false),
                vmem::MappingKind::Basic(bm) if bm.writable => (
                    vmem::MappingKind::Cow(vmem::CowMapping {
                        readable: bm.readable,
                        executable: bm.executable,
                    }),
                    true,
                ),
                vmem::MappingKind::Basic(bm) if modify_copy == ModifyCopy::MakeWritable => (
                    vmem::MappingKind::Cow(vmem::CowMapping {
                        readable: bm.readable,
                        executable: bm.executable,
                    }),
                    false,
                ),
                vmem::MappingKind::Basic(bm) => (
                    vmem::MappingKind::Basic(vmem::BasicMapping {
                        readable: bm.readable,
                        writable: false,
                        executable: bm.executable,
                    }),
                    false,
                ),
                vmem::MappingKind::Cow(cm) => (vmem::MappingKind::Cow(cm), false),
            };
            // map the same pages to the new VA
            paging::map_region(
                mapping.phys_base + (va - mapping.virt_base),
                to_va.wrapping_add((va - base_u) as usize),
                mapping.len,
                new_kind,
            );
            let do_downgrade = (remap_original == RemapOriginal::MakeCow) && was_writable;
            bt.track(va, do_downgrade);
            if do_downgrade {
                new_kind
            } else {
                mapping.kind
            }
        });
    }

    bt.track(base_u + len as u64, false);
    if bt.has_ops_since_last_downgrade {
        paging::barrier::first_valid_same_ctx();
    }
}

// Create a copy-on-write memory image from some existing VA range.
// `ptr` and `len` must be page-aligned (which is guaranteed by the
// wasmtime-platform.h interface).
#[no_mangle]
pub extern "C" fn wasmtime_memory_image_new(
    ptr: *const u8,
    len: usize,
    ret: &mut *mut c_void,
) -> i32 {
    // Choose an arbitrary VA, which we will use as the memory image
    // identifier. We will construct the image by mapping a copy of
    // the original VA range here, making the original copy CoW as we
    // go.
    let new_virt = FIRST_VADDR.fetch_add(0x100_0000_0000, Ordering::Relaxed) as *mut u8;
    copy_va_mapping(
        ptr,
        len,
        new_virt,
        RemapOriginal::MakeCow,
        ModifyCopy::DontChange,
    );
    *ret = new_virt as *mut c_void;
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_memory_image_map_at(
    image: *mut c_void,
    addr: *mut u8,
    len: usize,
) -> i32 {
    copy_va_mapping(
        image as *mut u8,
        len,
        addr,
        RemapOriginal::DontChange,
        ModifyCopy::MakeWritable,
    );
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_memory_image_free(_image: *mut c_void) {
    /* This should never be called in practice, because we simply
     * restore the snapshot rather than actually unload/destroy instances */
    panic!("wasmtime_memory_image_free");
}

/* Because we only have a single thread in the guest at the moment, we
 * don't need real thread-local storage. Wasmtime 47 introduced indexed
 * TLS slots, while the LTS release still uses a single implicit slot. */
static FAKE_TLS: [AtomicPtr<u8>; 2] = [const { AtomicPtr::new(core::ptr::null_mut()) }; 2];

#[cfg(feature = "wasmtime_lts")]
#[no_mangle]
pub extern "C" fn wasmtime_tls_get() -> *mut u8 {
    FAKE_TLS[0].load(Ordering::Acquire)
}
#[cfg(feature = "wasmtime_lts")]
#[no_mangle]
pub extern "C" fn wasmtime_tls_set(ptr: *mut u8) {
    FAKE_TLS[0].store(ptr, Ordering::Release)
}

#[cfg(not(feature = "wasmtime_lts"))]
#[no_mangle]
pub extern "C" fn wasmtime_tls_get(slot: usize) -> *mut u8 {
    FAKE_TLS[slot].load(Ordering::Acquire)
}
#[cfg(not(feature = "wasmtime_lts"))]
#[no_mangle]
pub extern "C" fn wasmtime_tls_set(slot: usize, ptr: *mut u8) {
    FAKE_TLS[slot].store(ptr, Ordering::Release)
}

pub struct WasmtimeCodeMemory {}
// TODO: Actually change the page tables for W^X
impl wasmtime::CustomCodeMemory for WasmtimeCodeMemory {
    fn required_alignment(&self) -> usize {
        unsafe { hyperlight_guest_bin::OS_PAGE_SIZE as usize }
    }
    fn publish_executable(
        &self,
        ptr: *const u8,
        len: usize,
    ) -> core::result::Result<(), wasmtime::Error> {
        wasmtime_mprotect(ptr as *mut u8, len, WASMTIME_PROT_READ | WASMTIME_PROT_EXEC);
        Ok(())
    }
    fn unpublish_executable(
        &self,
        ptr: *const u8,
        len: usize,
    ) -> core::result::Result<(), wasmtime::Error> {
        wasmtime_mprotect(
            ptr as *mut u8,
            len,
            WASMTIME_PROT_READ | WASMTIME_PROT_WRITE,
        );
        Ok(())
    }
}

pub(crate) unsafe fn map_buffer(phys: u64, len: u64) -> NonNull<[u8]> {
    // TODO: Use a VA allocator
    let virt = phys as *mut u8;
    unsafe {
        paging::map_region(
            phys,
            virt,
            len,
            vmem::MappingKind::Basic(vmem::BasicMapping {
                readable: true,
                writable: true,
                executable: true,
            }),
        );
        paging::barrier::first_valid_same_ctx();
        NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(virt, len as usize))
    }
}
