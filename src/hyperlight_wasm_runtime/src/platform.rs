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
use hyperlight_guest_bin::exception::arch;
use hyperlight_guest_bin::paging;

// Extremely stupid virtual address allocator
// 0x1_0000_0000 is where the module is
// we start at
// 0x100_0000_0000 and go up from there
static FIRST_VADDR: AtomicU64 = AtomicU64::new(0x100_0000_0000u64);
fn page_fault_handler(
    _exception_number: u64,
    info: *mut arch::ExceptionInfo,
    _ctx: *mut arch::Context,
    page_fault_address: u64,
) -> bool {
    let error_code = unsafe { (&raw const (*info).error_code).read_volatile() };
    // TODO: check if this is a guard-region trap (which can't happen
    // right now since we don't actually set the permissions properly
    // in mprotect)

    // TODO: replace this with some generic virtual memory area data
    // structure in hyperlight core
    if (error_code & 0x1) == 0x0 && page_fault_address >= 0x100_0000_0000u64 {
        unsafe {
            let phys_page = hyperlight_guest::prim_alloc::alloc_phys_pages(1);
            let virt_base = (page_fault_address & !0xFFF) as *mut u8;
            paging::map_region(
                phys_page,
                virt_base,
                hyperlight_guest_bin::OS_PAGE_SIZE as u64,
                vmem::MappingKind::Basic(vmem::BasicMapping {
                    readable: true,
                    writable: true,
                    executable: true,
                }),
            );
            virt_base.write_bytes(0u8, hyperlight_guest_bin::OS_PAGE_SIZE as usize);
        }
        return true; // Try again!
    }
    false
}
pub(crate) fn register_page_fault_handler() {
    // On amd64, vector 14 is #PF
    // See AMD64 Architecture Programmer's Manual, Volume 2
    //    §8.2 Vectors, p. 245
    //      Table 8-1: Interrupt Vector Source and Cause
    arch::HANDLERS[14].store(page_fault_handler as usize as u64, Ordering::Release);
}

// Wasmtime Embedding Interface

/* We don't actually have any sensible virtual memory areas, so
 * we just give out virtual addresses very coarsely with
 * probably-more-than-enough space between them, and take over
 * page-fault handling to hardcoded check if memory is in this region
 * (see above) */
#[no_mangle]
pub extern "C" fn wasmtime_mmap_new(_size: usize, _prot_flags: u32, ret: &mut *mut u8) -> i32 {
    if _size > 0x100_0000_0000 {
        panic!("wasmtime_mmap_{:x} {:x}", _size, _prot_flags);
    }
    *ret = FIRST_VADDR.fetch_add(0x100_0000_0000, Ordering::Relaxed) as *mut u8;
    0
}

/* Remap is only used for changing the region size (which is presently
 * a no-op, since we just hand out very large regions and treat them all
 * the same), or possibly for changing permissions, which will be a no-op
 * as we don't properly implement permissions at the moment. */
#[no_mangle]
pub extern "C" fn wasmtime_mmap_remap(addr: *mut u8, size: usize, prot_flags: u32) -> i32 {
    if size > 0x100_0000_0000 {
        panic!(
            "wasmtime_mmap_remap {:x} {:x} {:x}",
            addr as usize, size, prot_flags
        );
    }
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_munmap(_ptr: *mut u8, _size: usize) -> i32 {
    0
}

/* TODO: implement permissions properly */
#[no_mangle]
pub extern "C" fn wasmtime_mprotect(_ptr: *mut u8, _size: usize, prot_flags: u32) -> i32 {
    /* currently all memory is allocated RWX; we assume that
     * restricting to R or RX can be ignored */
    if prot_flags == 1 || prot_flags == 3 || prot_flags == 5 {
        return 0;
    }
    -1
}

#[no_mangle]
pub extern "C" fn wasmtime_page_size() -> usize {
    unsafe { hyperlight_guest_bin::OS_PAGE_SIZE as usize }
}

#[allow(non_camel_case_types)] // we didn't choose the name!
type wasmtime_trap_handler_t =
    extern "C" fn(ip: usize, fp: usize, has_faulting_addr: bool, faulting_addr: usize);
static WASMTIME_REQUESTED_TRAP_HANDLER: AtomicU64 = AtomicU64::new(0);
fn wasmtime_trap_handler(
    exception_number: u64,
    info: *mut arch::ExceptionInfo,
    ctx: *mut arch::Context,
    _page_fault_address: u64,
) -> bool {
    let requested_handler = WASMTIME_REQUESTED_TRAP_HANDLER.load(Ordering::Relaxed);
    if requested_handler != 0 {
        #[allow(clippy::collapsible_if)] // We will add more cases
        if exception_number == 6 {
            // #UD
            // we assume that handle_trap always longjmp's away, so don't bother
            // setting up a terribly proper stack frame
            unsafe {
                let orig_rip = (&raw mut (*info).rip).read_volatile();
                (&raw mut (*info).rip).write_volatile(requested_handler);
                // TODO: This only works on amd64 sysv
                (&raw mut (*ctx).gprs[9]).write_volatile(orig_rip);
                let orig_rbp = (&raw mut (*ctx).gprs[8]).read_volatile();
                (&raw mut (*ctx).gprs[10]).write_volatile(orig_rbp);
                (&raw mut (*ctx).gprs[11]).write_volatile(0);
                (&raw mut (*ctx).gprs[12]).write_volatile(0);
            }
            return true;
        }
        // TODO: Add handlers for any other traps that wasmtime needs
    }
    false
}

#[no_mangle]
pub extern "C" fn wasmtime_init_traps(handler: wasmtime_trap_handler_t) -> i32 {
    WASMTIME_REQUESTED_TRAP_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
    // On amd64, vector 6 is #UD
    // See AMD64 Architecture Programmer's Manual, Volume 2
    //    §8.2 Vectors, p. 245
    //      Table 8-1: Interrupt Vector Source and Cause
    arch::HANDLERS[6].store(wasmtime_trap_handler as usize as u64, Ordering::Release);
    // TODO: Add handlers for any other traps that wasmtime needs,
    //       probably including at least some floating-point
    //       exceptions
    // TODO: Ensure that invalid accesses to mprotect()'d regions also
    //       need to trap, although those will need to go through the
    //       page fault handler instead of using this handler that
    //       takes over the exception.
    0
}

// Copy a VA range to a new VA. Old and new VA, and len, must be
// page-aligned.
fn copy_va_mapping(base: *const u8, len: usize, to_va: *mut u8, remap_original: bool) {
    debug_assert!((base as usize).is_multiple_of(vmem::PAGE_SIZE));
    debug_assert!(len.is_multiple_of(vmem::PAGE_SIZE));
    // TODO: all this barrier code is amd64 specific. It should be
    // refactored to use some better architecture-independent APIs.
    //
    // On amd64, "upgrades" including the first time that a a valid
    // translation exists for a VA, only need a light (serialising
    // instruction) barrier.  Since invlpg is also a barrier, we don't
    // even need that, if we did do a downgrade remap just before.
    let mut needs_first_valid_exposure_barrier = false;

    // TODO: make this more efficient by directly exposing the ability
    // to traverse an entire VA range in
    // hyperlight_guest_bin::paging::virt_to_phys, and coalescing
    // continuous ranges there.
    let base_u = base as u64;
    let va_page_bases = (base_u..(base_u + len as u64)).step_by(vmem::PAGE_SIZE);
    let mappings = va_page_bases.flat_map(paging::virt_to_phys);
    for mapping in mappings {
        // TODO: Deduplicate with identical logic in hyperlight_host snapshot.
        let (new_kind, was_writable) = match mapping.kind {
            // Skip unmapped pages, since they will be unmapped in
            // both the original and the new copy
            vmem::MappingKind::Unmapped => continue,
            vmem::MappingKind::Basic(bm) if bm.writable => (
                vmem::MappingKind::Cow(vmem::CowMapping {
                    readable: bm.readable,
                    executable: bm.executable,
                }),
                true,
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
        let do_downgrade = remap_original && was_writable;
        if do_downgrade {
            // If necessary, remap the original page as Cow, instead
            // of whatever it is now, to ensure that any more writes to
            // that region do not change the image base.
            //
            // TODO: could the table traversal needed for this be fused
            // with the table traversal that got the original mapping,
            // above?
            unsafe {
                paging::map_region(
                    mapping.phys_base,
                    mapping.virt_base as *mut u8,
                    vmem::PAGE_SIZE as u64,
                    new_kind,
                );
            }
        }
        // map the same pages to the new VA
        unsafe {
            paging::map_region(
                mapping.phys_base,
                to_va.wrapping_add((mapping.virt_base - base_u) as usize),
                vmem::PAGE_SIZE as u64,
                new_kind,
            );
        }
        if do_downgrade {
            // Since we have downgraded a page from writable to CoW we
            // need to do an invlpg on it. Because invlpg is a
            // serialising instruction, we don't need the other
            // barrier for the new mapping.
            unsafe {
                core::arch::asm!("invlpg [{}]", in(reg) mapping.virt_base, options(readonly, nostack, preserves_flags));
            }
            needs_first_valid_exposure_barrier = false;
        } else {
            needs_first_valid_exposure_barrier = true;
        }
    }
    if needs_first_valid_exposure_barrier {
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
    copy_va_mapping(ptr, len, new_virt, true);
    *ret = new_virt as *mut c_void;
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_memory_image_map_at(
    image: *mut c_void,
    addr: *mut u8,
    len: usize,
) -> i32 {
    copy_va_mapping(image as *mut u8, len, addr, false);
    0
}

#[no_mangle]
pub extern "C" fn wasmtime_memory_image_free(_image: *mut c_void) {
    /* This should never be called in practice, because we simply
     * restore the snapshot rather than actually unload/destroy instances */
    panic!("wasmtime_memory_image_free");
}

/* Because we only have a single thread in the guest at the moment, we
 * don't need real thread-local storage. */
static FAKE_TLS: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
#[no_mangle]
pub extern "C" fn wasmtime_tls_get() -> *mut u8 {
    FAKE_TLS.load(Ordering::Acquire)
}
#[no_mangle]
pub extern "C" fn wasmtime_tls_set(ptr: *mut u8) {
    FAKE_TLS.store(ptr, Ordering::Release)
}

pub struct WasmtimeCodeMemory {}
// TODO: Actually change the page tables for W^X
impl wasmtime::CustomCodeMemory for WasmtimeCodeMemory {
    fn required_alignment(&self) -> usize {
        unsafe { hyperlight_guest_bin::OS_PAGE_SIZE as usize }
    }
    fn publish_executable(
        &self,
        _ptr: *const u8,
        _len: usize,
    ) -> core::result::Result<(), wasmtime::Error> {
        Ok(())
    }
    fn unpublish_executable(
        &self,
        _ptr: *const u8,
        _len: usize,
    ) -> core::result::Result<(), wasmtime::Error> {
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
