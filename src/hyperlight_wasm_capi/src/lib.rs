/*
Copyright 2026 The Hyperlight Authors.

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

//! C API for hyperlight-wasm, declared in `include/hyperlight_wasm.h`.
//!
//! Guest and host functions take one byte buffer per parameter and return
//! one byte buffer, so the API needs no knowledge of a component's WIT.

#![deny(missing_docs)]
// Type names match the C header.
#![allow(non_camel_case_types)]

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;

use hyperlight_wasm::{
    HyperlightError, InterruptHandle, LoadedWasmSandbox, ProtoWasmSandbox, SandboxBuilder,
    Snapshot, new_error,
};

/// Any error not covered by another kind.
pub const HLW_ERROR_OTHER: u32 = 0;
/// The sandbox is poisoned by an interrupted or failed guest call.
pub const HLW_ERROR_POISONED: u32 = 1;
/// A guest call was interrupted.
pub const HLW_ERROR_CANCELLED: u32 = 2;
/// The function is not valid in the sandbox's current state.
pub const HLW_ERROR_INVALID_STATE: u32 = 3;
/// An argument is invalid.
pub const HLW_ERROR_INVALID_ARGUMENT: u32 = 4;
/// The largest number of parameters a function can have.
pub const HLW_MAX_PARAMS: usize = 32;

/// An error returned to C.
pub struct hlw_error {
    kind: u32,
    message: String,
}

impl hlw_error {
    fn new(kind: u32, message: impl Into<String>) -> *mut hlw_error {
        Box::into_raw(Box::new(hlw_error {
            kind,
            message: message.into(),
        }))
    }
}

fn from_hyperlight(error: HyperlightError) -> *mut hlw_error {
    let kind = match error {
        HyperlightError::PoisonedSandbox => HLW_ERROR_POISONED,
        HyperlightError::ExecutionCanceledByHost() => HLW_ERROR_CANCELLED,
        _ => HLW_ERROR_OTHER,
    };
    hlw_error::new(kind, error.to_string())
}

fn ok() -> *mut hlw_error {
    std::ptr::null_mut()
}

/// Bytes borrowed from C.
#[repr(C)]
pub struct hlw_bytes {
    /// The first byte. May be null when `len` is zero.
    pub data: *const u8,
    /// The number of bytes.
    pub len: usize,
}

/// Bytes owned by this library.
#[repr(C)]
pub struct hlw_buf {
    /// The first byte. Null when `len` is zero.
    pub data: *mut u8,
    /// The number of bytes.
    pub len: usize,
}

/// The result slot of a host function call.
#[derive(Default)]
pub struct hlw_result {
    data: Vec<u8>,
}

/// Sandbox settings.
#[derive(Default)]
pub struct hlw_config {
    builder: SandboxBuilder,
}

impl hlw_config {
    fn update(&mut self, f: impl FnOnce(SandboxBuilder) -> SandboxBuilder) {
        self.builder = f(std::mem::take(&mut self.builder));
    }
}

enum State {
    Proto(ProtoWasmSandbox),
    Loaded(LoadedWasmSandbox),
    /// A load or guest call failed in a way that left no usable sandbox.
    Broken,
}

/// A sandbox that runs one WebAssembly component.
pub struct hlw_sandbox {
    state: State,
}

/// A saved sandbox state.
pub struct hlw_snapshot {
    snapshot: Arc<Snapshot>,
}

/// Interrupts a running guest call.
pub struct hlw_interrupt_handle {
    handle: Arc<dyn InterruptHandle>,
}

/// A host function registered from C.
type HostFunc = unsafe extern "C" fn(
    env: *mut c_void,
    args: *const hlw_bytes,
    nargs: usize,
    result: *mut hlw_result,
) -> *mut hlw_error;

struct HostCallback {
    func: HostFunc,
    env: *mut c_void,
    finalizer: Option<unsafe extern "C" fn(*mut c_void)>,
}

// SAFETY: `hlw_sandbox_register` requires `env` to be usable from any thread.
unsafe impl Send for HostCallback {}
// SAFETY: as for `Send`. Calls are serialized by the sandbox.
unsafe impl Sync for HostCallback {}

impl HostCallback {
    fn invoke(&self, args: Vec<Vec<u8>>) -> Result<Vec<u8>, HyperlightError> {
        let views = args
            .iter()
            .map(|a| hlw_bytes {
                data: a.as_ptr(),
                len: a.len(),
            })
            .collect::<Vec<_>>();
        let mut result = hlw_result::default();
        // SAFETY: `hlw_sandbox_register` requires `func` to be callable with
        // `env`. `views` and `result` outlive the call.
        let error = unsafe { (self.func)(self.env, views.as_ptr(), views.len(), &mut result) };
        if error.is_null() {
            Ok(result.data)
        } else {
            // SAFETY: a non-null error comes from `hlw_error_new` and is
            // owned by this library from here on.
            let error = unsafe { Box::from_raw(error) };
            Err(new_error!("{}", error.message))
        }
    }
}

impl Drop for HostCallback {
    fn drop(&mut self) {
        if let Some(finalizer) = self.finalizer {
            // SAFETY: `hlw_sandbox_register` requires `finalizer` to accept
            // `env`. It runs once, when the last reference is dropped.
            unsafe { finalizer(self.env) }
        }
    }
}

/// A slice from a C pointer and length. The pointer may be null when the
/// length is zero.
///
/// # Safety
/// If `len` is non-zero, `data` must point to `len` readable values that
/// outlive `'a`.
unsafe fn slice<'a, T>(data: *const T, len: usize) -> &'a [T] {
    if len == 0 {
        &[]
    } else {
        // SAFETY: guaranteed by the caller.
        unsafe { std::slice::from_raw_parts(data, len) }
    }
}

/// A UTF-8 string from a C pointer and length.
///
/// # Safety
/// As for [`slice`].
unsafe fn str<'a>(data: *const u8, len: usize) -> Result<&'a str, *mut hlw_error> {
    // SAFETY: guaranteed by the caller.
    let bytes = unsafe { slice(data, len) };
    std::str::from_utf8(bytes)
        .map_err(|e| hlw_error::new(HLW_ERROR_INVALID_ARGUMENT, format!("invalid UTF-8: {e}")))
}

macro_rules! try_c {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return e,
        }
    };
}

/// Expands `$m!(a0 a1 ...)` for every argument count from 0 to
/// [`HLW_MAX_PARAMS`].
macro_rules! for_each_arity {
    ($m:ident) => {
        for_each_arity!(@ $m [] [
            a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 a11 a12 a13 a14 a15
            a16 a17 a18 a19 a20 a21 a22 a23 a24 a25 a26 a27 a28 a29 a30 a31
        ]);
    };
    (@ $m:ident [$($done:ident)*] []) => {
        $m!($($done)*);
    };
    (@ $m:ident [$($done:ident)*] [$next:ident $($rest:ident)*]) => {
        $m!($($done)*);
        for_each_arity!(@ $m [$($done)* $next] [$($rest)*]);
    };
}

macro_rules! count {
    ($($a:ident)*) => { 0usize $(+ { let _ = stringify!($a); 1 })* };
}

/// Creates an error of kind `HLW_ERROR_OTHER` from a UTF-8 message.
///
/// # Safety
/// `message` must point to `len` readable bytes, or `len` must be zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_error_new(message: *const u8, len: usize) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let bytes = unsafe { slice(message, len) };
    hlw_error::new(HLW_ERROR_OTHER, String::from_utf8_lossy(bytes))
}

/// Returns the kind of `error`.
///
/// # Safety
/// `error` must be a live error from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_error_kind(error: *const hlw_error) -> u32 {
    // SAFETY: guaranteed by the caller.
    unsafe { (*error).kind }
}

/// Returns the message of `error`.
///
/// # Safety
/// `error` must be a live error from this library. `message` and `len` must
/// be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_error_message(
    error: *const hlw_error,
    message: *mut *const u8,
    len: *mut usize,
) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let error = &*error;
        *message = error.message.as_ptr();
        *len = error.message.len();
    }
}

/// Frees `error`.
///
/// # Safety
/// `error` must be null or an error from this library that is not used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_error_delete(error: *mut hlw_error) {
    if !error.is_null() {
        // SAFETY: guaranteed by the caller.
        drop(unsafe { Box::from_raw(error) });
    }
}

/// Frees the bytes of `buf` and clears it.
///
/// # Safety
/// `buf` must be null or hold bytes returned by this library, or be empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_buf_delete(buf: *mut hlw_buf) {
    // SAFETY: guaranteed by the caller.
    let Some(buf) = (unsafe { buf.as_mut() }) else {
        return;
    };
    if !buf.data.is_null() {
        // SAFETY: the bytes come from `into_buf`, which leaks a boxed slice
        // of this length.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buf.data, buf.len)) });
    }
    buf.data = std::ptr::null_mut();
    buf.len = 0;
}

fn into_buf(bytes: Vec<u8>) -> hlw_buf {
    let len = bytes.len();
    if len == 0 {
        return hlw_buf {
            data: std::ptr::null_mut(),
            len,
        };
    }
    let data = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>();
    hlw_buf { data, len }
}

/// Creates settings with default values.
#[unsafe(no_mangle)]
pub extern "C" fn hlw_config_new() -> *mut hlw_config {
    Box::into_raw(Box::default())
}

/// Frees `config`.
///
/// # Safety
/// `config` must be null or a config from this library that is not used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_delete(config: *mut hlw_config) {
    if !config.is_null() {
        // SAFETY: guaranteed by the caller.
        drop(unsafe { Box::from_raw(config) });
    }
}

/// Sets the guest heap size in bytes.
///
/// # Safety
/// `config` must be a live config from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_heap_size(config: *mut hlw_config, size: u64) {
    // SAFETY: guaranteed by the caller.
    unsafe { &mut *config }.update(|b| b.with_guest_heap_size(size));
}

/// Sets the guest scratch size in bytes.
///
/// # Safety
/// `config` must be a live config from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_scratch_size(config: *mut hlw_config, size: usize) {
    // SAFETY: guaranteed by the caller.
    unsafe { &mut *config }.update(|b| b.with_guest_scratch_size(size));
}

/// Guest page size in bytes.
const PAGE_SIZE: usize = 4096;

/// Largest message the host can send to the guest by default.
const DEFAULT_INPUT_SIZE: usize = 192 * 1024;

/// Sets the largest message the host can send to the guest. Values below
/// the default are ignored.
///
/// # Safety
/// `config` must be a live config from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_input_size(config: *mut hlw_config, size: usize) {
    if size <= DEFAULT_INPUT_SIZE {
        return;
    }
    // Split a message over a few buffers so the spare buffer hyperlight
    // keeps for a control call stays small. Two more buffers hold that
    // spare and the message header.
    const MESSAGE_BUFFERS: usize = 8;
    let pages = size.div_ceil(MESSAGE_BUFFERS).div_ceil(PAGE_SIZE);
    // SAFETY: guaranteed by the caller.
    unsafe { &mut *config }.update(|b| {
        b.with_h2g_buffer_size(pages * PAGE_SIZE)
            .with_h2g_pool_pages((MESSAGE_BUFFERS + 2) * pages)
    });
}

/// Sets the largest message the guest can send to the host.
///
/// # Safety
/// `config` must be a live config from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_output_size(config: *mut hlw_config, size: usize) {
    // SAFETY: guaranteed by the caller.
    unsafe { &mut *config }.update(|b| {
        // One small-buffer page, plus a request and its reply.
        b.with_g2h_buffer_size(size)
            .with_g2h_pool_pages(1 + 2 * size.div_ceil(PAGE_SIZE))
    });
}

/// Loads the guest runtime from a file.
///
/// # Safety
/// `config` must be a live config from this library. `path` must point to
/// `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_runtime_file(
    config: *mut hlw_config,
    path: *const u8,
    len: usize,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let path = PathBuf::from(try_c!(unsafe { str(path, len) }));
    // SAFETY: guaranteed by the caller.
    unsafe { &mut *config }.update(|b| b.with_runtime_file(path));
    ok()
}

/// Sets the offset from `SIGRTMIN` of the interrupt signal.
///
/// # Safety
/// `config` must be a live config from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_config_interrupt_signal_offset(
    config: *mut hlw_config,
    offset: u8,
) -> *mut hlw_error {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: guaranteed by the caller.
        let config = unsafe { &mut *config };
        match config
            .builder
            .clone()
            .with_interrupt_vcpu_sigrtmin_offset(offset)
        {
            Ok(builder) => config.builder = builder,
            Err(e) => return hlw_error::new(HLW_ERROR_INVALID_ARGUMENT, e.to_string()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (config, offset);
    ok()
}

/// Creates a sandbox.
///
/// # Safety
/// `config` must be null or a live config from this library. `out` must be
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_new(
    config: *const hlw_config,
    out: *mut *mut hlw_sandbox,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let builder = unsafe { config.as_ref() }
        .map(|c| c.builder.clone())
        .unwrap_or_default();
    let proto = try_c!(builder.build().map_err(from_hyperlight));
    // SAFETY: guaranteed by the caller.
    unsafe {
        *out = Box::into_raw(Box::new(hlw_sandbox {
            state: State::Proto(proto),
        }));
    }
    ok()
}

/// Frees `sandbox`.
///
/// # Safety
/// `sandbox` must be null or a sandbox from this library that is not used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_delete(sandbox: *mut hlw_sandbox) {
    if !sandbox.is_null() {
        // SAFETY: guaranteed by the caller.
        drop(unsafe { Box::from_raw(sandbox) });
    }
}

/// Copies `len` bytes from `data` into `result`.
///
/// # Safety
/// `result` must be the result slot passed to the running host function.
/// `data` must point to `len` readable bytes, or `len` must be zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_result_set(result: *mut hlw_result, data: *const u8, len: usize) {
    // SAFETY: guaranteed by the caller.
    unsafe { (*result).data = slice(data, len).to_vec() };
}

fn invalid_state(what: &str) -> *mut hlw_error {
    hlw_error::new(HLW_ERROR_INVALID_STATE, what)
}

/// Registers a host function.
///
/// # Safety
/// `sandbox` must be a live sandbox from this library. `name` must point to
/// `name_len` readable bytes. `func` must be safe to call with `env` from
/// any thread until `finalizer` runs, and `finalizer` must be null or safe
/// to call once with `env`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_register(
    sandbox: *mut hlw_sandbox,
    name: *const u8,
    name_len: usize,
    nargs: usize,
    func: HostFunc,
    env: *mut c_void,
    finalizer: Option<unsafe extern "C" fn(*mut c_void)>,
) -> *mut hlw_error {
    let callback = Arc::new(HostCallback {
        func,
        env,
        finalizer,
    });
    // SAFETY: guaranteed by the caller.
    let name = try_c!(unsafe { str(name, name_len) });
    // SAFETY: guaranteed by the caller.
    let State::Proto(proto) = &mut unsafe { &mut *sandbox }.state else {
        return invalid_state("host functions must be registered before loading");
    };

    macro_rules! register {
        ($($a:ident)*) => {
            if nargs == count!($($a)*) {
                let callback = callback.clone();
                return match proto.register(name, move |$($a: Vec<u8>),*| callback.invoke(vec![$($a),*])) {
                    Ok(()) => ok(),
                    Err(e) => from_hyperlight(e),
                };
            }
        };
    }
    for_each_arity!(register);
    hlw_error::new(
        HLW_ERROR_INVALID_ARGUMENT,
        format!("{nargs} parameters exceed the limit of {HLW_MAX_PARAMS}"),
    )
}

fn load(
    sandbox: &mut hlw_sandbox,
    load: impl FnOnce(hyperlight_wasm::WasmSandbox) -> hyperlight_wasm::Result<LoadedWasmSandbox>,
) -> *mut hlw_error {
    let State::Proto(proto) = std::mem::replace(&mut sandbox.state, State::Broken) else {
        return invalid_state("a component is already loaded");
    };
    match proto.load_runtime().and_then(load) {
        Ok(loaded) => {
            sandbox.state = State::Loaded(loaded);
            ok()
        }
        Err(e) => from_hyperlight(e),
    }
}

/// Loads a precompiled component from a file.
///
/// # Safety
/// `sandbox` must be a live sandbox from this library. `path` must point to
/// `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_load_file(
    sandbox: *mut hlw_sandbox,
    path: *const u8,
    len: usize,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let path = try_c!(unsafe { str(path, len) });
    // SAFETY: guaranteed by the caller.
    load(unsafe { &mut *sandbox }, |s| s.load_module(path))
}

/// Loads a precompiled component from memory.
///
/// # Safety
/// `sandbox` must be a live sandbox from this library. `data` must point to
/// `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_load(
    sandbox: *mut hlw_sandbox,
    data: *const u8,
    len: usize,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let data = unsafe { slice(data, len) };
    // SAFETY: guaranteed by the caller.
    load(unsafe { &mut *sandbox }, |s| {
        s.load_module_from_buffer(data)
    })
}

fn loaded(sandbox: &mut hlw_sandbox) -> Result<&mut LoadedWasmSandbox, *mut hlw_error> {
    match &mut sandbox.state {
        State::Loaded(loaded) => Ok(loaded),
        State::Proto(_) => Err(invalid_state("no component is loaded")),
        State::Broken => Err(invalid_state("the sandbox failed and can only be deleted")),
    }
}

fn call(
    loaded: &mut LoadedWasmSandbox,
    name: &str,
    mut args: Vec<Vec<u8>>,
) -> Result<Vec<u8>, *mut hlw_error> {
    macro_rules! call {
        ($($a:ident)*) => {{
            const N: usize = count!($($a)*);
            if args.len() == N {
                return match <[Vec<u8>; N]>::try_from(std::mem::take(&mut args)) {
                    Ok([$($a),*]) => loaded
                        .call_guest_function::<Vec<u8>>(name, ($($a,)*))
                        .map_err(from_hyperlight),
                    Err(_) => unreachable!("length checked"),
                };
            }
        }};
    }
    for_each_arity!(call);
    Err(hlw_error::new(
        HLW_ERROR_INVALID_ARGUMENT,
        format!(
            "{} arguments exceed the limit of {HLW_MAX_PARAMS}",
            args.len()
        ),
    ))
}

/// Calls a guest function.
///
/// # Safety
/// `sandbox` must be a live sandbox from this library. `name` must point to
/// `name_len` readable bytes. `args` must point to `nargs` values whose
/// bytes are readable. `result` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_call(
    sandbox: *mut hlw_sandbox,
    name: *const u8,
    name_len: usize,
    args: *const hlw_bytes,
    nargs: usize,
    result: *mut hlw_buf,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let name = try_c!(unsafe { str(name, name_len) });
    // SAFETY: guaranteed by the caller.
    let args = unsafe { slice(args, nargs) }
        .iter()
        // SAFETY: guaranteed by the caller.
        .map(|a| unsafe { slice(a.data, a.len) }.to_vec())
        .collect();
    // SAFETY: guaranteed by the caller.
    let sandbox = unsafe { &mut *sandbox };
    let loaded = try_c!(loaded(sandbox));
    // A panic in the guest call leaves the sandbox in an unknown state.
    match catch_unwind(AssertUnwindSafe(|| call(loaded, name, args))) {
        Ok(Ok(bytes)) => {
            // SAFETY: guaranteed by the caller.
            unsafe { *result = into_buf(bytes) };
            ok()
        }
        Ok(Err(e)) => e,
        Err(panic) => {
            sandbox.state = State::Broken;
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            hlw_error::new(HLW_ERROR_OTHER, format!("guest call panicked: {message}"))
        }
    }
}

/// Saves the state of a loaded sandbox.
///
/// # Safety
/// `sandbox` must be a live sandbox from this library. `out` must be
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_snapshot(
    sandbox: *mut hlw_sandbox,
    out: *mut *mut hlw_snapshot,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let loaded = try_c!(loaded(unsafe { &mut *sandbox }));
    let snapshot = try_c!(loaded.snapshot().map_err(from_hyperlight));
    // SAFETY: guaranteed by the caller.
    unsafe { *out = Box::into_raw(Box::new(hlw_snapshot { snapshot })) };
    ok()
}

/// Restores a snapshot.
///
/// # Safety
/// `sandbox` and `snapshot` must be live objects from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_restore(
    sandbox: *mut hlw_sandbox,
    snapshot: *const hlw_snapshot,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let loaded = try_c!(loaded(unsafe { &mut *sandbox }));
    // SAFETY: guaranteed by the caller.
    let snapshot = unsafe { &*snapshot }.snapshot.clone();
    match loaded.restore(snapshot) {
        Ok(()) => ok(),
        Err(e) => from_hyperlight(e),
    }
}

/// Frees `snapshot`.
///
/// # Safety
/// `snapshot` must be null or a snapshot from this library that is not used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_snapshot_delete(snapshot: *mut hlw_snapshot) {
    if !snapshot.is_null() {
        // SAFETY: guaranteed by the caller.
        drop(unsafe { Box::from_raw(snapshot) });
    }
}

/// Returns a handle that interrupts guest calls.
///
/// # Safety
/// `sandbox` must be a live sandbox from this library. `out` must be
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_sandbox_interrupt_handle(
    sandbox: *mut hlw_sandbox,
    out: *mut *mut hlw_interrupt_handle,
) -> *mut hlw_error {
    // SAFETY: guaranteed by the caller.
    let loaded = try_c!(loaded(unsafe { &mut *sandbox }));
    let handle = try_c!(loaded.interrupt_handle().map_err(from_hyperlight));
    // SAFETY: guaranteed by the caller.
    unsafe { *out = Box::into_raw(Box::new(hlw_interrupt_handle { handle })) };
    ok()
}

/// Interrupts the running guest call.
///
/// # Safety
/// `handle` must be a live interrupt handle from this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_interrupt_handle_kill(handle: *const hlw_interrupt_handle) -> bool {
    // SAFETY: guaranteed by the caller.
    unsafe { &*handle }.handle.kill()
}

/// Frees `handle`.
///
/// # Safety
/// `handle` must be null or a handle from this library that is not used
/// afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_interrupt_handle_delete(handle: *mut hlw_interrupt_handle) {
    if !handle.is_null() {
        // SAFETY: guaranteed by the caller.
        drop(unsafe { Box::from_raw(handle) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn unused(
        _env: *mut c_void,
        _args: *const hlw_bytes,
        _nargs: usize,
        _result: *mut hlw_result,
    ) -> *mut hlw_error {
        unreachable!()
    }

    fn take(error: *mut hlw_error) -> Option<(u32, String)> {
        if error.is_null() {
            return None;
        }
        // SAFETY: errors returned by this library are owned by the caller.
        let error = unsafe { Box::from_raw(error) };
        Some((error.kind, error.message))
    }

    fn sandbox() -> *mut hlw_sandbox {
        let mut sandbox = std::ptr::null_mut();
        // SAFETY: a null config selects defaults and `sandbox` is writable.
        assert_eq!(
            take(unsafe { hlw_sandbox_new(std::ptr::null(), &mut sandbox) }),
            None
        );
        sandbox
    }

    #[test]
    fn errors() {
        let message = "bad \u{1F388}";
        // SAFETY: `message` is readable for its length.
        let error = unsafe { hlw_error_new(message.as_ptr(), message.len()) };
        let (mut ptr, mut len) = (std::ptr::null(), 0);
        // SAFETY: `error` is live and the outputs are writable.
        unsafe {
            assert_eq!(hlw_error_kind(error), HLW_ERROR_OTHER);
            hlw_error_message(error, &mut ptr, &mut len);
            assert_eq!(slice(ptr, len), message.as_bytes());
            hlw_error_delete(error);
            hlw_error_delete(std::ptr::null_mut());
        }

        let kind = |e| take(from_hyperlight(e)).unwrap().0;
        assert_eq!(kind(HyperlightError::PoisonedSandbox), HLW_ERROR_POISONED);
        assert_eq!(
            kind(HyperlightError::ExecutionCanceledByHost()),
            HLW_ERROR_CANCELLED
        );
        assert_eq!(kind(new_error!("x")), HLW_ERROR_OTHER);
    }

    #[test]
    fn buffers() {
        let mut result = hlw_result::default();
        // SAFETY: the data is readable and `result` is a live result slot.
        unsafe {
            hlw_result_set(&mut result, [1u8, 2, 3].as_ptr(), 3);
            assert_eq!(result.data, [1, 2, 3]);
            hlw_result_set(&mut result, std::ptr::null(), 0);
            assert!(result.data.is_empty());
        }

        let mut buf = into_buf(vec![4, 5]);
        // SAFETY: `buf` holds bytes from `into_buf`.
        unsafe {
            assert_eq!(slice(buf.data, buf.len), [4, 5]);
            hlw_buf_delete(&mut buf);
        }
        assert!(buf.data.is_null() && buf.len == 0);
        let mut empty = into_buf(Vec::new());
        assert!(empty.data.is_null());
        // SAFETY: an empty buffer and null are accepted.
        unsafe {
            hlw_buf_delete(&mut empty);
            hlw_buf_delete(std::ptr::null_mut());
        }
    }

    #[test]
    fn config() {
        let config = hlw_config_new();
        let path = "/no/such/runtime";
        let invalid = [0xffu8];
        // SAFETY: `config` is live and the strings are readable.
        unsafe {
            hlw_config_heap_size(config, 8 << 20);
            hlw_config_scratch_size(config, 8 << 20);
            hlw_config_input_size(config, 1 << 20);
            hlw_config_output_size(config, 1 << 20);
            assert_eq!(take(hlw_config_interrupt_signal_offset(config, 1)), None);
            #[cfg(target_os = "linux")]
            assert_eq!(
                take(hlw_config_interrupt_signal_offset(config, u8::MAX)).map(|e| e.0),
                Some(HLW_ERROR_INVALID_ARGUMENT)
            );
            assert_eq!(
                take(hlw_config_runtime_file(config, invalid.as_ptr(), 1)).map(|e| e.0),
                Some(HLW_ERROR_INVALID_ARGUMENT)
            );
            assert_eq!(
                take(hlw_config_runtime_file(config, path.as_ptr(), path.len())),
                None
            );
            let mut sandbox = std::ptr::null_mut();
            assert!(take(hlw_sandbox_new(config, &mut sandbox)).is_some());
            assert!(sandbox.is_null());
            hlw_config_delete(config);
            hlw_config_delete(std::ptr::null_mut());
        }
    }

    static FINALIZED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn finalize(_env: *mut c_void) {
        FINALIZED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn invalid_calls() {
        let sandbox = sandbox();
        let name = "f";
        let mut snapshot = std::ptr::null_mut();
        let mut handle = std::ptr::null_mut();
        let mut result = hlw_buf {
            data: std::ptr::null_mut(),
            len: 0,
        };
        // SAFETY: `sandbox` is live, the strings are readable and the outputs
        // are writable.
        unsafe {
            let state = |e| take(e).map(|e: (u32, String)| e.0);
            let call =
                hlw_sandbox_call(sandbox, name.as_ptr(), 1, std::ptr::null(), 0, &mut result);
            assert_eq!(state(call), Some(HLW_ERROR_INVALID_STATE));
            assert_eq!(
                state(hlw_sandbox_snapshot(sandbox, &mut snapshot)),
                Some(HLW_ERROR_INVALID_STATE)
            );
            assert_eq!(
                state(hlw_sandbox_interrupt_handle(sandbox, &mut handle)),
                Some(HLW_ERROR_INVALID_STATE)
            );

            let before = FINALIZED.load(std::sync::atomic::Ordering::SeqCst);
            let too_many = hlw_sandbox_register(
                sandbox,
                name.as_ptr(),
                1,
                HLW_MAX_PARAMS + 1,
                unused,
                std::ptr::null_mut(),
                Some(finalize),
            );
            assert_eq!(state(too_many), Some(HLW_ERROR_INVALID_ARGUMENT));
            let bad_name = hlw_sandbox_register(
                sandbox,
                [0xffu8].as_ptr(),
                1,
                0,
                unused,
                std::ptr::null_mut(),
                Some(finalize),
            );
            assert_eq!(state(bad_name), Some(HLW_ERROR_INVALID_ARGUMENT));
            // The finalizer runs when registration fails.
            assert_eq!(
                FINALIZED.load(std::sync::atomic::Ordering::SeqCst),
                before + 2
            );

            let garbage = [0u8; 16];
            let load = hlw_sandbox_load(sandbox, garbage.as_ptr(), garbage.len());
            assert!(take(load).is_some());
            // A failed load leaves only deletion.
            let reload = hlw_sandbox_load(sandbox, garbage.as_ptr(), garbage.len());
            assert_eq!(state(reload), Some(HLW_ERROR_INVALID_STATE));
            let call =
                hlw_sandbox_call(sandbox, name.as_ptr(), 1, std::ptr::null(), 0, &mut result);
            assert_eq!(state(call), Some(HLW_ERROR_INVALID_STATE));
            hlw_sandbox_delete(sandbox);
            hlw_sandbox_delete(std::ptr::null_mut());
            hlw_snapshot_delete(std::ptr::null_mut());
            hlw_interrupt_handle_delete(std::ptr::null_mut());
        }
    }
}
