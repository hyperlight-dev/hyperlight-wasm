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

mod hostfuncs {

    mod host {
        extern "C" {
            #[link_name = "TestHostFunc"]
            pub(super) fn test_host_func(a: i32) -> i32;
        }
    }
    pub(super) fn test_host_func(a: i32) -> i32 {
        unsafe { host::test_host_func(a) }
    }
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
extern "C" {
    fn fd_write(fd: i32, iovs: i32, iovs_len: i32, retptr: i32) -> i32;
}

fn wasi_print(s: &str) -> i32 {
    let buf = s.as_ptr();
    let len = s.len();
    let iov: [u32; 2] = [buf as u32, len as u32];
    let mut written: u32 = 0;
    unsafe {
        fd_write(
            1, // stdout
            iov.as_ptr() as i32,
            1, // one iovec
            &mut written as *mut u32 as i32,
        );
    }
    written as i32
}

macro_rules! hlprint {
    ($($arg:tt)*) => {{
        let f = format!($($arg)*);
        wasi_print(&f)
    }}
}

#[no_mangle]
pub extern "C" fn hello_world() -> i32 {
    hlprint!("Hello from Wasm in Hyperlight!\n");
    0
}

#[no_mangle]
pub extern "C" fn add(left: usize, right: usize) -> usize {
    left + right
}

#[no_mangle]
pub extern "C" fn call_host_function(a: i32) -> i32 {
    hostfuncs::test_host_func(a)
}
