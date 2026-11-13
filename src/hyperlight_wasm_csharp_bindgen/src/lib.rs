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

//! Generates C# host bindings from WIT for the Hyperlight.Wasm.Build
//! MSBuild task, which loads this library at build time.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// Generates C# bindings for a WIT world.
///
/// `wit` is the UTF-8 path of a `.wit` file or directory. An empty `world`
/// selects the package's last world. All generated types go under the
/// namespace `ns`.
///
/// Returns 0 and the C# source in `out`, or 1 and a UTF-8 error message.
/// Free `out` with [`hlw_bindgen_free`].
///
/// # Safety
/// Each string pointer must point to its length in readable bytes, or the
/// length must be zero. `out` and `out_len` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_bindgen_csharp(
    wit: *const u8,
    wit_len: usize,
    world: *const u8,
    world_len: usize,
    ns: *const u8,
    ns_len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: guaranteed by the caller.
    let args = unsafe { (str(wit, wit_len), str(world, world_len), str(ns, ns_len)) };
    let result = match args {
        // The generator panics on WIT it cannot read, as the bindgen
        // macros do. The message goes back to the build as the error.
        (Ok(wit), Ok(world), Ok(ns)) => catch_unwind(AssertUnwindSafe(|| {
            let world = (!world.is_empty()).then_some(world);
            hyperlight_component_csharp::generate_from_path(Path::new(wit), world, ns)
                .map_err(|e| e.to_string())
        }))
        .unwrap_or_else(|panic| Err(panic_message(panic.as_ref()))),
        _ => Err("argument is not valid UTF-8".to_string()),
    };
    let (status, text) = match result {
        Ok(source) => (0, source),
        Err(message) => (1, message),
    };
    let bytes = Box::into_raw(text.into_bytes().into_boxed_slice());
    // SAFETY: guaranteed by the caller.
    unsafe {
        *out_len = bytes.len();
        *out = bytes.cast();
    }
    status
}

fn panic_message(panic: &(dyn Any + Send)) -> String {
    match panic.downcast_ref::<String>() {
        Some(message) => message.clone(),
        None => panic.downcast_ref::<&str>().map_or_else(
            || "WIT binding generation failed".to_string(),
            |m| m.to_string(),
        ),
    }
}

/// Frees text returned by [`hlw_bindgen_csharp`].
///
/// # Safety
/// `data` and `len` must come from one call to [`hlw_bindgen_csharp`], and
/// `data` must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hlw_bindgen_free(data: *mut u8, len: usize) {
    // SAFETY: guaranteed by the caller.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) });
}

/// # Safety
/// `data` must point to `len` readable bytes, or `len` must be zero.
unsafe fn str<'a>(data: *const u8, len: usize) -> Result<&'a str, std::str::Utf8Error> {
    if len == 0 {
        return Ok("");
    }
    // SAFETY: guaranteed by the caller.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, len) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate(wit: &str, world: &str, ns: &str) -> (i32, String) {
        let (mut out, mut len) = (std::ptr::null_mut(), 0);
        // SAFETY: the strings are readable and the outputs writable.
        unsafe {
            let status = hlw_bindgen_csharp(
                wit.as_ptr(),
                wit.len(),
                world.as_ptr(),
                world.len(),
                ns.as_ptr(),
                ns.len(),
                &mut out,
                &mut len,
            );
            let text = String::from_utf8(std::slice::from_raw_parts(out, len).to_vec()).unwrap();
            hlw_bindgen_free(out, len);
            (status, text)
        }
    }

    const WIT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/rust_guests/component_sample/wit/example.wit"
    );

    #[test]
    fn generates_selected_world() {
        let (status, source) = generate(WIT, "example", "Sample");
        assert_eq!(status, 0, "{source}");
        assert!(source.contains("public sealed class ExampleComponent"));
        assert!(source.contains("\"component-sample:example/adder#add\""));
    }

    #[test]
    fn reports_errors() {
        let (status, message) = generate(WIT, "no-such-world", "Sample");
        assert_eq!(status, 1);
        assert!(message.contains("no-such-world"), "{message}");
        let (status, message) = generate("/no/such/file.wit", "", "Sample");
        assert_eq!(status, 1);
        assert!(message.contains("/no/such/file.wit"), "{message}");
        let (status, _) = generate(WIT, "example", "Not A Namespace");
        assert_eq!(status, 1);
    }

    #[test]
    fn dotnet_runtime_source_is_up_to_date() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../dotnet/src/Hyperlight.Wasm/Component/Runtime.cs"
        );
        let committed = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
        assert!(
            committed == hyperlight_component_csharp::RUNTIME_SOURCE,
            "copy hyperlight_component_csharp::RUNTIME_SOURCE to {path}"
        );
    }
}
