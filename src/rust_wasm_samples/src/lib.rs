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

mod simd;

use std::ffi::CString;
mod hostfuncs {

    use std::os::raw::c_char;
    extern "C" {
        pub fn HostPrint(s: *const c_char) -> i32;
    }

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

macro_rules! hlprint {
    ($($arg:tt)*) => {{
        let f = format!($($arg)*);
        let s = CString::new(f).unwrap();
        let r;
        unsafe {
            r = hostfuncs::HostPrint(s.as_ptr());
        }
        r
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
