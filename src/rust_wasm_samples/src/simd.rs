#![cfg(target_arch = "wasm32")]

use std::arch::wasm32::*;

#[no_mangle]
pub extern "C" fn simd_test() -> i32 {
    // Simple SIMD test function

    let a = i32x4_splat(100);
    let b = i32x4_splat(50);
    let result = i32x4_add(a, b);
    i32x4_extract_lane::<0>(result)
}
