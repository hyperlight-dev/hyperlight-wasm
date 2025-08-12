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

use hyperlight_guest_bin::exceptions::handler;
use hyperlight_guest_bin::paging;

// Extremely stupid virtual address allocator
// 0x1_0000_0000 is where the module is
// we start at
// 0x100_0000_0000 and go up from there
static FIRST_VADDR: AtomicU64 = AtomicU64::new(0x100_0000_0000u64);

#[hyperlight_guest_tracing::trace_function]
fn page_fault_handler(
    _exception_number: u64,
    info: *mut handler::ExceptionInfo,
    _ctx: *mut handler::Context,
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
            let phys_page = paging::alloc_phys_pages(1);
            let virt_base = (page_fault_address & !0xFFF) as *mut u8;
            paging::map_region(
                phys_page,
                virt_base,
                hyperlight_guest_bin::OS_PAGE_SIZE as u64,
            );
            virt_base.write_bytes(0u8, hyperlight_guest_bin::OS_PAGE_SIZE as usize);
        }
        return true; // Try again!
    }
    false
}

#[hyperlight_guest_tracing::trace_function]
pub(crate) fn register_page_fault_handler() {
    // On amd64, vector 14 is #PF
    // See AMD64 Architecture Programmer's Manual, Volume 2
    //    §8.2 Vectors, p. 245
    //      Table 8-1: Interrupt Vector Source and Cause
    handler::HANDLERS[14].store(page_fault_handler as usize as u64, Ordering::Release);
}

// Wasmtime Embedding Interface

/* We don't actually have any sensible virtual memory areas, so
 * we just give out virtual addresses very coarsely with
 * probably-more-than-enough space between them, and take over
 * page-fault handling to hardcoded check if memory is in this region
 * (see above) */
#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
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
#[hyperlight_guest_tracing::trace_function]
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
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_munmap(_ptr: *mut u8, _size: usize) -> i32 {
    0
}

/* TODO: implement permissions properly */
#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_mprotect(_ptr: *mut u8, _size: usize, prot_flags: u32) -> i32 {
    /* currently all memory is allocated RWX; we assume that
     * restricting to R or RX can be ignored */
    if prot_flags == 1 || prot_flags == 3 || prot_flags == 5 {
        return 0;
    }
    -1
}

#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_page_size() -> usize {
    unsafe { hyperlight_guest_bin::OS_PAGE_SIZE as usize }
}

#[allow(non_camel_case_types)] // we didn't choose the name!
type wasmtime_trap_handler_t =
    extern "C" fn(ip: usize, fp: usize, has_faulting_addr: bool, faulting_addr: usize);
static WASMTIME_REQUESTED_TRAP_HANDLER: AtomicU64 = AtomicU64::new(0);

#[hyperlight_guest_tracing::trace_function]
fn wasmtime_trap_handler(
    exception_number: u64,
    info: *mut handler::ExceptionInfo,
    ctx: *mut handler::Context,
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
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_init_traps(handler: wasmtime_trap_handler_t) -> i32 {
    WASMTIME_REQUESTED_TRAP_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
    // On amd64, vector 6 is #UD
    // See AMD64 Architecture Programmer's Manual, Volume 2
    //    §8.2 Vectors, p. 245
    //      Table 8-1: Interrupt Vector Source and Cause
    handler::HANDLERS[6].store(wasmtime_trap_handler as usize as u64, Ordering::Release);
    // TODO: Add handlers for any other traps that wasmtime needs,
    //       probably including at least some floating-point
    //       exceptions
    // TODO: Ensure that invalid accesses to mprotect()'d regions also
    //       need to trap, although those will need to go through the
    //       page fault handler instead of using this handler that
    //       takes over the exception.
    0
}

// The wasmtime_memory_image APIs are not yet supported.
#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_memory_image_new(
    _ptr: *const u8,
    _len: usize,
    ret: &mut *mut c_void,
) -> i32 {
    *ret = core::ptr::null_mut();
    0
}

#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_memory_image_map_at(
    _image: *mut c_void,
    _addr: *mut u8,
    _len: usize,
) -> i32 {
    /* This should never be called because wasmtime_memory_image_new
     * returns NULL */
    panic!("wasmtime_memory_image_map_at");
}

#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_memory_image_free(_image: *mut c_void) {
    /* This should never be called because wasmtime_memory_image_new
     * returns NULL */
    panic!("wasmtime_memory_image_free");
}

/* Because we only have a single thread in the guest at the moment, we
 * don't need real thread-local storage. */
static FAKE_TLS: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());

#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
pub extern "C" fn wasmtime_tls_get() -> *mut u8 {
    FAKE_TLS.load(Ordering::Acquire)
}

#[no_mangle]
#[hyperlight_guest_tracing::trace_function]
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

#[hyperlight_guest_tracing::trace_function]
pub(crate) unsafe fn map_buffer(phys: u64, len: u64) -> NonNull<[u8]> {
    // TODO: Use a VA allocator
    let virt = phys as *mut u8;
    unsafe {
        paging::map_region(phys, virt, len);
        NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(virt, len as usize))
    }
}
