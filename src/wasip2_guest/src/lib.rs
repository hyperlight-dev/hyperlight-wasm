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

//! Monte Carlo Pi Estimator - WASI P2 Component Example
//!
//! This guest component demonstrates host-guest interaction where the guest
//! depends on the host to provide random numbers (since `no_std` components
//! cannot generate randomness).
//!
//! Build with: `cargo build --lib --target wasm32-wasip2 --release`
//!
//! Note: Must use release mode - debug mode pulls in WASI dependencies.

#![no_std]
#![no_main]

wit_bindgen::generate!({
    world: "estimator",
    path: "wit",
});

struct Component;

impl Guest for Component {
    fn estimate_pi(samples: u32) -> f32 {
        let mut inside_circle = 0u32;

        for _ in 0..samples {
            // Get random x and y from host (we can't generate randomness!)
            let x = my::monte_carlo::random::random();
            let y = my::monte_carlo::random::random();

            // Check if point is inside unit circle
            if x * x + y * y <= 1.0 {
                inside_circle += 1;
            }
        }

        // Pi ≈ 4 * (points inside circle / total points)
        4.0 * (inside_circle as f32) / (samples as f32)
    }
}

export!(Component);
