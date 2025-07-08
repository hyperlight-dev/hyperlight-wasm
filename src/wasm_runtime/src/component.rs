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

use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::result::Result::*;

use hyperlight_common::flatbuffer_wrappers::function_call::FunctionCall;
use hyperlight_common::flatbuffer_wrappers::function_types::{
    ParameterType, ParameterValue, ReturnType,
};
use hyperlight_common::flatbuffer_wrappers::guest_error::ErrorCode;
use hyperlight_common::flatbuffer_wrappers::util::get_flatbuffer_result;
use hyperlight_guest::error::{HyperlightGuestError, Result};
use hyperlight_guest_bin::guest_function::definition::GuestFunctionDefinition;
use hyperlight_guest_bin::guest_function::register::register_function;
use hyperlight_guest_bin::host_comm::call_host_function;
use spin::Mutex;
use wasmtime::component::{Component, Instance, Linker};
use wasmtime::{Config, Engine, Store};

use crate::platform;

static CUR_ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static CUR_LINKER: Mutex<Option<Linker<()>>> = Mutex::new(None);
static CUR_STORE: Mutex<Option<Store<()>>> = Mutex::new(None);
static CUR_INSTANCE: Mutex<Option<Instance>> = Mutex::new(None);

hyperlight_wasm_macro::wasm_guest_bindgen!();

// dummy for compatibility with the module loading approach
fn init_wasm_runtime(_function_call: &FunctionCall) -> Result<Vec<u8>> {
    Ok(get_flatbuffer_result::<i32>(0))
}

fn load_component_common(engine: &Engine, component: Component) -> Result<()> {
    let mut store = Store::new(engine, ());
    let instance = (*CUR_LINKER.lock())
        .as_ref()
        .unwrap()
        .instantiate(&mut store, &component)?;
    *CUR_STORE.lock() = Some(store);
    *CUR_INSTANCE.lock() = Some(instance);
    Ok(())
}

fn load_wasm_module(function_call: &FunctionCall) -> Result<Vec<u8>> {
    if let (
        ParameterValue::VecBytes(ref wasm_bytes),
        ParameterValue::Int(ref _len),
        Some(ref engine),
    ) = (
        &function_call.parameters.as_ref().unwrap()[0],
        &function_call.parameters.as_ref().unwrap()[1],
        &*CUR_ENGINE.lock(),
    ) {
        let component = unsafe { Component::deserialize(engine, wasm_bytes)? };
        load_component_common(engine, component)?;
        Ok(get_flatbuffer_result::<i32>(0))
    } else {
        Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Invalid parameters passed to LoadWasmModule".to_string(),
        ))
    }
}

fn load_wasm_module_phys(function_call: &FunctionCall) -> Result<Vec<u8>> {
    if let (ParameterValue::ULong(ref phys), ParameterValue::ULong(ref len), Some(ref engine)) = (
        &function_call.parameters.as_ref().unwrap()[0],
        &function_call.parameters.as_ref().unwrap()[1],
        &*CUR_ENGINE.lock(),
    ) {
        let component =
            unsafe { Component::deserialize_raw(engine, platform::map_buffer(*phys, *len))? };
        load_component_common(engine, component)?;
        Ok(get_flatbuffer_result::<()>(()))
    } else {
        Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Invalid parameters passed to LoadWasmModulePhys".to_string(),
        ))
    }
}

#[no_mangle]
pub extern "C" fn hyperlight_main() {
    platform::register_page_fault_handler();

    let mut config = Config::new();
    config.memory_reservation(0);
    config.memory_guard_size(0);
    config.memory_reservation_for_growth(0);
    config.guard_before_linear_memory(false);
    config.with_custom_code_memory(Some(alloc::sync::Arc::new(platform::WasmtimeCodeMemory {})));
    let engine = Engine::new(&config).unwrap();
    let linker = Linker::new(&engine);
    *CUR_ENGINE.lock() = Some(engine);
    *CUR_LINKER.lock() = Some(linker);

    hyperlight_guest_wasm_init();

    register_function(GuestFunctionDefinition::new(
        "InitWasmRuntime".to_string(),
        vec![],
        ReturnType::Int,
        init_wasm_runtime as usize,
    ));
    register_function(GuestFunctionDefinition::new(
        "LoadWasmModule".to_string(),
        vec![ParameterType::VecBytes, ParameterType::Int],
        ReturnType::Int,
        load_wasm_module as usize,
    ));
    register_function(GuestFunctionDefinition::new(
        "LoadWasmModulePhys".to_string(),
        vec![ParameterType::ULong, ParameterType::ULong],
        ReturnType::Void,
        load_wasm_module_phys as usize,
    ));
}

#[no_mangle]
pub fn guest_dispatch_function(function_call: FunctionCall) -> Result<Vec<u8>> {
    Err(HyperlightGuestError::new(
        ErrorCode::GuestFunctionNotFound,
        function_call.function_name.clone(),
    ))
}
