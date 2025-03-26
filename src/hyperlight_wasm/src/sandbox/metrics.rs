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

/*!
This module contains the definitions and implementations of the metrics used by the sandbox module
*/
use std::collections::HashMap;
use std::sync::Once;

use hyperlight_host::metrics::{
    HyperlightMetric, HyperlightMetricDefinition, HyperlightMetricEnum, HyperlightMetricType,
};
use once_cell::sync::OnceCell;
use strum::{EnumIter, IntoStaticStr, VariantNames};
use tracing::{instrument, Span};

// This is required to ensure that the metrics are only initialized once
static INIT_METRICS: Once = Once::new();
// This contains a hashmap of all the metrics using metric names defined below as keys
static METRICS: OnceCell<HashMap<&'static str, HyperlightMetric>> = OnceCell::new();

// This is the definition of all the metrics used by the sandbox module
static WASM_SANDBOX_METRIC_DEFINITIONS: &[HyperlightMetricDefinition] = &[
    HyperlightMetricDefinition {
        name: "current_number_of_wasm_sandboxes",
        help: "Current number of wasm sandboxes in this host process",
        metric_type: HyperlightMetricType::IntGauge,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "current_number_of_loaded_wasm_sandboxes",
        help: "Current number of loaded wasm sandboxes in this host process",
        metric_type: HyperlightMetricType::IntGauge,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "number_of_unloads_of_loaded_wasm_sandboxes",
        help: "The number of times that unload_module has been called on a LoadedWasmSandbox",
        metric_type: HyperlightMetricType::IntCounter,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "number_of_loads_of_wasm_sandboxes",
        help: "The number of times that load_module has been called on a WasmSandbox",
        metric_type: HyperlightMetricType::IntCounter,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "current_number_of_proto_wasm_sandboxes",
        help: "Current number of proto wasm sandboxes in this host process",
        metric_type: HyperlightMetricType::IntGauge,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "total_number_of_wasm_sandboxes",
        help: "The total number of wasm sandboxes that have been created by this process",
        metric_type: HyperlightMetricType::IntCounter,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "total_number_of_loaded_wasm_sandboxes",
        help: "The total number of loaded wasm sandboxes that have been created by this process",
        metric_type: HyperlightMetricType::IntCounter,
        labels: &[],
        buckets: &[],
    },
    HyperlightMetricDefinition {
        name: "total_number_of_proto_wasm_sandboxes",
        help: "The total number of proto wasm sandboxes that have been created by this process",
        metric_type: HyperlightMetricType::IntCounter,
        labels: &[],
        buckets: &[],
    },
];

/// There is an enum variant for each error metric in the module
/// the names of the variant take the form of CamelCase, but the metric names are snake_case
/// so for example, the enum variant CurrentNumberOfMultiUseSandboxes corresponds to the
/// metric name current_number_of_multi_use_sandboxes.
/// At runtime we locate the correct metric by looking up the enum variant name in the hashmap and
/// the conversion of the enum variant name to the metric name is done by the IntoStaticStr derive macro
/// along with the strum(serialize_all = "snake_case") attribute.
/// This enum contains all the metrics used by the sandbox module
///
/// The enum is required to derive from EnumIter, IntoStaticStr
/// and strum(serialize_all = "snake_case") performs the name conversion from CamelCase to snake_case
/// when the enum variant is serialized to a string
#[derive(Debug, EnumIter, VariantNames, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum SandboxMetric {
    CurrentNumberOfWasmSandboxes,
    CurrentNumberOfLoadedWasmSandboxes,
    CurrentNumberOfProtoWasmSandboxes,
    TotalNumberOfWasmSandboxes,
    TotalNumberOfLoadedWasmSandboxes,
    TotalNumberOfProtoWasmSandboxes,
    NumberOfUnloadsOfLoadedWasmSandboxes,
    NumberOfLoadsOfWasmSandboxes,
}

// It is required for the enum to implement HyperlightMetricEnum
impl HyperlightMetricEnum<SandboxMetric> for SandboxMetric {
    #[instrument(skip_all, parent = Span::current(), level= "Trace")]
    fn get_init_metrics() -> &'static Once {
        &INIT_METRICS
    }
    #[instrument(skip_all, parent = Span::current(), level= "Trace")]
    fn get_metrics() -> &'static OnceCell<HashMap<&'static str, HyperlightMetric>> {
        &METRICS
    }
    #[instrument(skip_all, parent = Span::current(), level= "Trace")]
    fn get_metric_definitions() -> &'static [HyperlightMetricDefinition] {
        WASM_SANDBOX_METRIC_DEFINITIONS
    }
}

#[cfg(test)]
mod tests {
    use hyperlight_host::metrics::get_metrics_registry;
    use hyperlight_host::metrics::tests::HyperlightMetricEnumTest;
    use hyperlight_host::{
        int_counter_get, int_counter_inc, int_counter_inc_by, int_counter_reset, int_gauge_add,
        int_gauge_dec, int_gauge_get, int_gauge_inc, int_gauge_set, int_gauge_sub,
    };
    use lazy_static::lazy_static;
    use prometheus::Registry;
    use strum::{IntoEnumIterator, VariantNames};

    use super::*;

    impl HyperlightMetricEnumTest<SandboxMetric> for SandboxMetric {
        fn get_enum_variant_names() -> &'static [&'static str] {
            SandboxMetric::VARIANTS
        }
    }

    #[test]
    fn test_enum_has_variant_for_all_metrics() {
        <super::SandboxMetric as HyperlightMetricEnumTest<SandboxMetric>>::enum_has_variant_for_all_metrics();
    }
    #[test]
    fn test_metric_definitions() {
        <super::SandboxMetric as HyperlightMetricEnumTest<SandboxMetric>>::check_metric_definitions(
        );
    }
    #[test]
    #[ignore]
    /// This test is ignored because as it uses real counters if it runs at the same time as other tests it may fail.
    ///
    /// Marking this test as ignored means that running `cargo test` will not
    /// run it. This feature will allow a developer who runs that command
    /// from their workstation to be successful without needing to know about
    /// test interdependencies. This test will, however, be run explicitly as a
    /// part of the CI pipeline.
    fn test_metrics() {
        let iter: SandboxMetricIter = SandboxMetric::iter();
        for sandbox_metric in iter {
            match sandbox_metric.get_hyperlight_metric() {
                Ok(hyperlight_metric) => match hyperlight_metric {
                    HyperlightMetric::IntGauge(int_gauge) => {
                        let gauge = <super::SandboxMetric as HyperlightMetricEnumTest<
                            SandboxMetric,
                        >>::get_intguage_metric(int_gauge.name);
                        assert!(gauge.is_ok());
                        let gauge = gauge.unwrap();
                        int_gauge_set!(&sandbox_metric, 0);
                        assert_eq!(gauge.get(), 0);
                        int_gauge_inc!(&sandbox_metric);
                        assert_eq!(gauge.get(), 1);
                        int_gauge_dec!(&sandbox_metric);
                        assert_eq!(gauge.get(), 0);
                        int_gauge_add!(&sandbox_metric, 5);
                        assert_eq!(gauge.get(), 5);
                        int_gauge_sub!(&sandbox_metric, 2);
                        assert_eq!(gauge.get(), 3);
                        int_gauge_set!(&sandbox_metric, 10);
                        assert_eq!(gauge.get(), 10);
                        let val = int_gauge_get!(&sandbox_metric);
                        assert_eq!(val, 10);
                    }
                    HyperlightMetric::IntCounter(int_counter) => {
                        let counter = <super::SandboxMetric as HyperlightMetricEnumTest<
                            SandboxMetric,
                        >>::get_intcounter_metric(
                            int_counter.name
                        );
                        assert!(counter.is_ok());
                        let counter = counter.unwrap();
                        int_counter_reset!(&sandbox_metric);
                        assert_eq!(counter.get(), 0);
                        int_counter_inc!(&sandbox_metric);
                        assert_eq!(counter.get(), 1);
                        int_counter_inc_by!(&sandbox_metric, 5);
                        assert_eq!(counter.get(), 6);
                        int_counter_reset!(&sandbox_metric);
                        assert_eq!(counter.get(), 0);
                        let result = int_counter_get!(&sandbox_metric);
                        assert_eq!(result, 0);
                    }
                    _ => {
                        panic!("metric is not an IntGauge or IntCounter");
                    }
                },
                Err(e) => {
                    panic!("error getting metric: {}", e);
                }
            }
        }
    }
    #[test]
    #[ignore]
    /// This test is ignored because it is requires that metrics and registry have not been set or initialised yet.
    ///
    /// Marking this test as ignored means that running `cargo test` will not
    /// run it. This feature will allow a developer who runs that command
    /// from their workstation to be successful without needing to know about
    /// test interdependencies. This test will, however, be run explicitly as a
    /// part of the CI pipeline.
    fn test_gather_metrics() {
        lazy_static! {
            static ref REGISTRY: Registry = Registry::default();
        }
        test_metrics();
        let registry = get_metrics_registry();
        let result = registry.gather();
        assert_eq!(result.len(), 8);
    }
}
