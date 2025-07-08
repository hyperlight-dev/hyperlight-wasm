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

use alloc::alloc::{alloc, dealloc, Layout};
use core::ffi::c_void;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use hyperlight_guest_bin::exceptions::handler;

// Wasmtime Embedding Interface

/* We don't have proper support for lazy committing an mmap region, or
 * for setting up guard pages, because the guest doesn't have an *
 * appropriate interrupt handler yet. Consequently, we configure
 * wasmtime not to use any guard region, and precommit memory. */
#[no_mangle]
pub extern "C" fn wasmtime_mmap_new(size: usize, _prot_flags: u32, ret: &mut *mut u8) -> i32 {
    *ret = unsafe { alloc(Layout::from_size_align(size, 0x1000).unwrap()) };
    0
}

/* Because of the precommitted memory strategy, we can't generally
 * support remap */
#[no_mangle]
pub extern "C" fn wasmtime_mmap_remap(addr: *mut u8, size: usize, prot_flags: u32) -> i32 {
    panic!(
        "wasmtime_mmap_remap {:x} {:x} {:x}",
        addr as usize, size, prot_flags
    );
}

#[no_mangle]
pub extern "C" fn wasmtime_munmap(ptr: *mut u8, size: usize) -> i32 {
    unsafe { dealloc(ptr, Layout::from_size_align(size, 0x1000).unwrap()) };
    0
}

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
pub extern "C" fn wasmtime_init_traps(handler: wasmtime_trap_handler_t) -> i32 {
    WASMTIME_REQUESTED_TRAP_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
    // On amd64, vector 6 is #UD
    // See AMD64 Architecture Programmer's Manual, Volume 2
    //    §8.2 Vectors, p. 245
    //      Table 8-1: Interrupt Vector Source and Cause
    handler::handlers[6].store(wasmtime_trap_handler as usize as u64, Ordering::Release);
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
pub extern "C" fn wasmtime_memory_image_new(
    _ptr: *const u8,
    _len: usize,
    ret: &mut *mut c_void,
) -> i32 {
    *ret = core::ptr::null_mut();
    0
}

#[no_mangle]
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
pub extern "C" fn wasmtime_memory_image_free(_image: *mut c_void) {
    /* This should never be called because wasmtime_memory_image_new
     * returns NULL */
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
