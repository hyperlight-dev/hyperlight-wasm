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

//! Runs the `example` world of `component_sample` through the C API.
//!
//! The guest runtime must be built for that world, so run with `WIT_WORLD`
//! set to `src/tests/rust_guests/component_sample/wit/component-world.wasm`
//! and `--include-ignored`, as `just examples-components` does.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use hyperlight_wasm_capi::*;

type HostFunc =
    unsafe extern "C" fn(*mut c_void, *const hlw_bytes, usize, *mut hlw_result) -> *mut hlw_error;

const ADD: &str = "component-sample:example/adder#add";
const CALL_HOST: &str = "component-sample:example/adder#call-host";
const DO_SOMETHING: &str = "component-sample:example/adder#do-something";
const PRINT: &str = "component-sample:example/host#print";
const HOST_FUNCTION: &str = "component-sample:example/host#host-function";

fn encode_string(s: &str) -> Vec<u8> {
    [&(s.len() as u32).to_le_bytes()[..], s.as_bytes()].concat()
}

fn decode_string(bytes: &[u8]) -> String {
    let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 4 + len);
    String::from_utf8(bytes[4..].to_vec()).unwrap()
}

#[derive(Debug)]
struct Error {
    kind: u32,
    message: String,
}

fn check(error: *mut hlw_error) -> Result<(), Error> {
    if error.is_null() {
        return Ok(());
    }
    let (mut ptr, mut len) = (std::ptr::null(), 0);
    // SAFETY: `error` is a live error owned by the caller.
    unsafe {
        hlw_error_message(error, &mut ptr, &mut len);
        let message = String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned();
        let kind = hlw_error_kind(error);
        hlw_error_delete(error);
        Err(Error { kind, message })
    }
}

#[track_caller]
fn ok(error: *mut hlw_error) {
    if let Err(e) = check(error) {
        panic!("error {}: {}", e.kind, e.message);
    }
}

/// Host state shared with the callbacks through `env`.
#[derive(Default)]
struct Host {
    printed: Mutex<Vec<String>>,
    fail: bool,
}

unsafe extern "C" fn print(
    env: *mut c_void,
    args: *const hlw_bytes,
    nargs: usize,
    _result: *mut hlw_result,
) -> *mut hlw_error {
    // SAFETY: `env` is the `Host` passed at registration and the arguments
    // are valid for the call.
    unsafe {
        let host = &*(env as *const Host);
        assert_eq!(nargs, 1);
        let arg = std::slice::from_raw_parts((*args).data, (*args).len);
        host.printed.lock().unwrap().push(decode_string(arg));
    }
    std::ptr::null_mut()
}

unsafe extern "C" fn host_function(
    env: *mut c_void,
    args: *const hlw_bytes,
    nargs: usize,
    result: *mut hlw_result,
) -> *mut hlw_error {
    // SAFETY: as for `print`.
    unsafe {
        let host = &*(env as *const Host);
        if host.fail {
            let message = "host failure";
            return hlw_error_new(message.as_ptr(), message.len());
        }
        assert_eq!(nargs, 1);
        let input = decode_string(std::slice::from_raw_parts((*args).data, (*args).len));
        let output = encode_string(&format!("{input} and the host!"));
        hlw_result_set(result, output.as_ptr(), output.len());
    }
    std::ptr::null_mut()
}

unsafe extern "C" fn drop_host(env: *mut c_void) {
    // SAFETY: `env` is a reference from `Arc::into_raw` given to one
    // registration, and the sandbox calls this once after its last use.
    drop(unsafe { Arc::from_raw(env as *const Host) });
}

struct Sandbox(*mut hlw_sandbox);

impl Sandbox {
    fn new(host: &Arc<Host>, from_memory: bool) -> Sandbox {
        let path = examples_common::get_wasm_module_path("component_sample.aot").unwrap();
        // SAFETY: every pointer passed is live and every string readable.
        unsafe {
            let config = hlw_config_new();
            hlw_config_heap_size(config, 200_000_000);
            hlw_config_scratch_size(config, 100 << 20);
            hlw_config_input_size(config, 1 << 20);
            let mut sandbox = std::ptr::null_mut();
            ok(hlw_sandbox_new(config, &mut sandbox));
            hlw_config_delete(config);

            for (name, func) in [(PRINT, print as HostFunc), (HOST_FUNCTION, host_function)] {
                let env = Arc::into_raw(host.clone()) as *mut c_void;
                ok(hlw_sandbox_register(
                    sandbox,
                    name.as_ptr(),
                    name.len(),
                    1,
                    func,
                    env,
                    Some(drop_host),
                ));
            }
            if from_memory {
                let bytes = std::fs::read(&path).unwrap();
                ok(hlw_sandbox_load(sandbox, bytes.as_ptr(), bytes.len()));
            } else {
                ok(hlw_sandbox_load_file(sandbox, path.as_ptr(), path.len()));
            }
            Sandbox(sandbox)
        }
    }

    fn call(&self, name: &str, args: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
        let views = args
            .iter()
            .map(|a| hlw_bytes {
                data: a.as_ptr(),
                len: a.len(),
            })
            .collect::<Vec<_>>();
        let mut buf = hlw_buf {
            data: std::ptr::null_mut(),
            len: 0,
        };
        // SAFETY: the sandbox is live, `views` outlives the call and `buf`
        // is writable.
        unsafe {
            check(hlw_sandbox_call(
                self.0,
                name.as_ptr(),
                name.len(),
                views.as_ptr(),
                views.len(),
                &mut buf,
            ))?;
            let bytes = if buf.len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(buf.data, buf.len).to_vec()
            };
            hlw_buf_delete(&mut buf);
            Ok(bytes)
        }
    }

    fn add(&self, left: u32, right: u32) -> u32 {
        let bytes = self
            .call(
                ADD,
                &[left.to_le_bytes().to_vec(), right.to_le_bytes().to_vec()],
            )
            .unwrap();
        u32::from_le_bytes(bytes.try_into().unwrap())
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // SAFETY: the sandbox is live and not used afterwards.
        unsafe { hlw_sandbox_delete(self.0) };
    }
}

#[test]
#[ignore = "needs a guest runtime built for the component_sample example world"]
fn calls_exports_and_imports() {
    let host = Arc::new(Host::default());
    let sandbox = Sandbox::new(&host, false);

    assert_eq!(sandbox.add(1, 2), 3);
    let mut snapshot = std::ptr::null_mut();
    // SAFETY: the sandbox is live and `snapshot` is writable.
    ok(unsafe { hlw_sandbox_snapshot(sandbox.0, &mut snapshot) });

    // The debug build of the sample traps on overflow. A trapped instance
    // can only be used again after a restore.
    let trap = sandbox
        .call(
            ADD,
            &[u32::MAX.to_le_bytes().to_vec(), 1u32.to_le_bytes().to_vec()],
        )
        .unwrap_err();
    assert!(trap.message.contains("add_overflow"), "{}", trap.message);
    assert!(sandbox.call(ADD, &[vec![0; 4], vec![0; 4]]).is_err());
    // SAFETY: the sandbox and snapshot are live.
    unsafe {
        ok(hlw_sandbox_restore(sandbox.0, snapshot));
        hlw_snapshot_delete(snapshot);
    }
    assert_eq!(sandbox.add(3, 4), 7);

    let reply = sandbox.call(CALL_HOST, &[encode_string("Hello")]).unwrap();
    assert_eq!(decode_string(&reply), "Hello from component and the host!");

    let reply = sandbox
        .call(DO_SOMETHING, &[42u32.to_le_bytes().to_vec()])
        .unwrap();
    assert!(reply.is_empty());
    assert_eq!(*host.printed.lock().unwrap(), ["42"]);

    let missing = sandbox
        .call("component-sample:example/adder#missing", &[])
        .unwrap_err();
    assert!(missing.message.contains("missing"), "{}", missing.message);

    drop(sandbox);
    // Both registrations released their reference.
    assert_eq!(Arc::strong_count(&host), 1);
}

#[test]
#[ignore = "needs a guest runtime built for the component_sample example world"]
fn loads_from_memory() {
    let sandbox = Sandbox::new(&Arc::new(Host::default()), true);
    assert_eq!(sandbox.add(20, 22), 42);
}

#[test]
#[ignore = "needs a guest runtime built for the component_sample example world"]
fn host_errors_fail_the_call() {
    let host = Arc::new(Host {
        fail: true,
        ..Host::default()
    });
    let sandbox = Sandbox::new(&host, false);
    let mut snapshot = std::ptr::null_mut();
    // SAFETY: the sandbox is live and `snapshot` is writable.
    ok(unsafe { hlw_sandbox_snapshot(sandbox.0, &mut snapshot) });

    let error = sandbox
        .call(CALL_HOST, &[encode_string("Hello")])
        .unwrap_err();
    assert!(error.message.contains("host failure"), "{}", error.message);

    // SAFETY: the sandbox and snapshot are live.
    unsafe {
        ok(hlw_sandbox_restore(sandbox.0, snapshot));
        hlw_snapshot_delete(snapshot);
    }
    assert_eq!(sandbox.add(2, 3), 5);
}

#[test]
#[ignore = "needs a guest runtime built for the component_sample example world"]
fn interrupt_handle_is_idle_between_calls() {
    let sandbox = Sandbox::new(&Arc::new(Host::default()), false);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the sandbox is live and `handle` is writable.
    unsafe {
        ok(hlw_sandbox_interrupt_handle(sandbox.0, &mut handle));
        assert!(!hlw_interrupt_handle_kill(handle));
    }
    assert_eq!(sandbox.add(1, 1), 2);
    // SAFETY: the handle is live and not used afterwards.
    unsafe { hlw_interrupt_handle_delete(handle) };
}
