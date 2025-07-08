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
use alloc::vec::Vec;
use alloc::{format, vec};
use core::ops::Deref;

use hyperlight_common::flatbuffer_wrappers::function_call::FunctionCall;
use hyperlight_common::flatbuffer_wrappers::function_types::{
    ParameterType, ParameterValue, ReturnType,
};
use hyperlight_common::flatbuffer_wrappers::guest_error::ErrorCode;
use hyperlight_common::flatbuffer_wrappers::util::get_flatbuffer_result;
use hyperlight_guest::error::{HyperlightGuestError, Result};
use hyperlight_guest_bin::guest_function::definition::GuestFunctionDefinition;
use hyperlight_guest_bin::guest_function::register::register_function;
use hyperlight_guest_bin::host_comm::print_output_with_host_print;
use spin::Mutex;
use wasmtime::{Config, Engine, Linker, Module, Store, Val};

use crate::{hostfuncs, marshal, platform, wasip1};

static CUR_ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static CUR_LINKER: Mutex<Option<Linker<()>>> = Mutex::new(None);
static CUR_MODULE: Mutex<Option<Module>> = Mutex::new(None);

#[no_mangle]
pub fn guest_dispatch_function(function_call: &FunctionCall) -> Result<Vec<u8>> {
    let engine = CUR_ENGINE.lock();
    let engine = engine.deref().as_ref().ok_or(HyperlightGuestError::new(
        ErrorCode::GuestError,
        "Wasm runtime is not initialized".to_string(),
    ))?;
    let linker = CUR_LINKER.lock();
    let linker = linker.deref().as_ref().ok_or(HyperlightGuestError::new(
        ErrorCode::GuestError,
        "impossible: wasm runtime has no valid linker".to_string(),
    ))?;
    let module = CUR_MODULE.lock();
    let module = module.deref().as_ref().ok_or(HyperlightGuestError::new(
        ErrorCode::GuestError,
        "No wasm module loaded".to_string(),
    ))?;
    let mut store = Store::new(engine, ());
    let instance = linker.instantiate(&mut store, module)?;
    let func = instance
        .get_func(&mut store, &function_call.function_name)
        .ok_or(HyperlightGuestError::new(
            ErrorCode::GuestError,
            "Function not found".to_string(),
        ))?;
    let mut w_params = vec![];
    for f_param in (function_call.parameters)
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
    {
        w_params.push(marshal::hl_param_to_val(
            &mut store,
            |ctx, name| instance.get_export(ctx, name),
            f_param,
        )?);
    }
    let is_void = ReturnType::Void == function_call.expected_return_type;
    let n_results = if is_void { 0 } else { 1 };
    let mut results = vec![Val::I32(0); n_results];
    func.call(&mut store, &w_params, &mut results)?;
    marshal::val_to_hl_result(
        &mut store,
        |ctx, name| instance.get_export(ctx, name),
        function_call.expected_return_type,
        &results,
    )
}

fn init_wasm_runtime() -> Result<Vec<u8>> {
    let mut config = Config::new();
    config.memory_reservation(0);
    config.memory_guard_size(0);
    config.memory_reservation_for_growth(0);
    config.guard_before_linear_memory(false);
    config.with_custom_code_memory(Some(alloc::sync::Arc::new(platform::WasmtimeCodeMemory {})));
    let engine = Engine::new(&config)?;
    let mut linker = Linker::new(&engine);
    wasip1::register_handlers(&mut linker)?;

    let hostfuncs = hostfuncs::get_host_function_details()
        .host_functions
        .unwrap_or_default();
    for hostfunc in hostfuncs.iter() {
        let captured = hostfunc.clone();
        linker.func_new(
            "env",
            &hostfunc.function_name,
            hostfuncs::hostfunc_type(hostfunc, &engine)?,
            move |c, ps, rs| {
                hostfuncs::call(&captured, c, ps, rs)
                    .map_err(|e| wasmtime::Error::msg(format!("{:?}", e)))
            },
        )?;
    }
    *CUR_ENGINE.lock() = Some(engine);
    *CUR_LINKER.lock() = Some(linker);
    Ok(get_flatbuffer_result::<i32>(0))
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
        let module = unsafe { Module::deserialize(engine, wasm_bytes)? };
        *CUR_MODULE.lock() = Some(module);
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
        let module = unsafe { Module::deserialize_raw(engine, platform::map_buffer(*phys, *len))? };
        *CUR_MODULE.lock() = Some(module);
        Ok(get_flatbuffer_result::<()>(()))
    } else {
        Err(HyperlightGuestError::new(
            ErrorCode::GuestFunctionParameterTypeMismatch,
            "Invalid parameters passed to LoadWasmModulePhys".to_string(),
        ))
    }
}

#[no_mangle]
#[allow(clippy::fn_to_numeric_cast)] // GuestFunctionDefinition expects a function pointer as i64
pub extern "C" fn hyperlight_main() {
    platform::register_page_fault_handler();

    register_function(GuestFunctionDefinition::new(
        "PrintOutput".to_string(),
        vec![ParameterType::String],
        ReturnType::Int,
        print_output_with_host_print as usize,
    ));

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
