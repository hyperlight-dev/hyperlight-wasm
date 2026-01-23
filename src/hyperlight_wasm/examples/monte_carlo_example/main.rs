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

//! Monte Carlo Pi Estimator - Host Example
//!
//! Demonstrates running a WASI P2 component built with native `wasm32-wasip2`
//! target and `wit-bindgen`. The host provides a random number generator
//! that the guest component imports to estimate Pi.
//!
//! Build the guest first: `just build-monte-carlo-example`
//! Run: `just examples-components`

extern crate alloc;

use std::env;
use std::path::Path;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

mod bindings {
    hyperlight_component_macro::host_bindgen!("../wasip2_guest/monte-carlo-world.wasm");
}

/// Host-provided RNG state for the guest component.
///
/// The guest cannot generate randomness in `no_std` mode, so the host
/// provides random numbers via the imported `random` interface.
struct State {
    rng: StdRng,
}

impl State {
    fn new() -> Self {
        State {
            rng: StdRng::from_entropy(),
        }
    }
}

impl bindings::my::monte_carlo::Random for State {
    fn random(&mut self) -> f32 {
        self.rng.r#gen::<f32>()
    }
}

#[allow(refining_impl_trait)]
impl bindings::my::monte_carlo::EstimatorImports for State {
    type Random = State;

    fn random(&mut self) -> &mut Self::Random {
        self
    }
}

fn main() {
    let state = State::new();

    let mut sb: hyperlight_wasm::ProtoWasmSandbox = hyperlight_wasm::SandboxBuilder::new()
        .with_guest_input_buffer_size(1000000)
        .with_guest_heap_size(10000000)
        .with_guest_stack_size(1000000)
        .build()
        .unwrap();

    let rt = bindings::register_host_functions(&mut sb, state);

    let sb = sb.load_runtime().unwrap();

    // Always look in x64/release since the guest is always built in release mode
    // (debug mode pulls in WASI dependencies that we don't support)
    let proj_dir = env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let mod_path = Path::new(&proj_dir)
        .join("../../x64/release/monte_carlo.aot")
        .canonicalize()
        .expect("Failed to find monte_carlo.aot - run 'just build-monte-carlo-example' first");
    let sb = sb.load_module(mod_path).unwrap();

    let mut wrapped = bindings::EstimatorSandbox { sb, rt };

    // Estimate pi with different sample sizes
    for samples in [100, 1000, 10000] {
        let pi_estimate =
            bindings::my::monte_carlo::EstimatorExports::estimate_pi(&mut wrapped, samples);
        println!(
            "Pi estimate with {} samples: {:.6} (error: {:.6})",
            samples,
            pi_estimate,
            (pi_estimate - std::f32::consts::PI).abs()
        );
    }
}
