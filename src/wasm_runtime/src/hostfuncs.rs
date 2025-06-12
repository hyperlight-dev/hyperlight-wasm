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

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use hyperlight_common::flatbuffer_wrappers::function_types::{
    ParameterType, ReturnType, ReturnValue,
};
use hyperlight_common::flatbuffer_wrappers::guest_error::ErrorCode;
use hyperlight_guest::error::{HyperlightGuestError, Result};
use hyperlight_guest_bin::host_comm::call_host_function;
use wasmtime::{Caller, Engine, FuncType, Val, ValType};

use crate::marshal;

pub(crate) type HostFunctionDefinition =
    hyperlight_common::flatbuffer_wrappers::host_function_definition::HostFunctionDefinition;
pub(crate) type HostFunctionDetails =
    hyperlight_common::flatbuffer_wrappers::host_function_details::HostFunctionDetails;

pub(crate) fn get_host_function_details() -> HostFunctionDetails {
    hyperlight_guest_bin::host_comm::get_host_function_details()
}

pub(crate) fn hostfunc_type(d: &HostFunctionDefinition, e: &Engine) -> Result<FuncType> {
    let mut params = Vec::new();
    let mut last_was_vec = false;
    for p in (d.parameter_types).iter().flatten() {
        if last_was_vec && *p != ParameterType::Int {
            return Err(HyperlightGuestError::new(
                ErrorCode::GuestError,
                "Host function vector parameter missing length".to_string(),
            ));
        }

        params.push(match p {
            ParameterType::Int | ParameterType::UInt => ValType::I32,
            ParameterType::Long | ParameterType::ULong => ValType::I64,
            ParameterType::Bool => ValType::I32,
            ParameterType::Float => ValType::F32,
            ParameterType::Double => ValType::F64,
            ParameterType::String => ValType::I32,
            ParameterType::VecBytes => {
                last_was_vec = true;
                ValType::I32
            }
        });
    }
    let mut results = Vec::new();
    match d.return_type {
        ReturnType::Void => {}
        ReturnType::Int | ReturnType::UInt => results.push(ValType::I32),
        ReturnType::Long | ReturnType::ULong => results.push(ValType::I64),
        ReturnType::Bool => results.push(ValType::I32),
        ReturnType::Float => results.push(ValType::F32),
        ReturnType::Double => results.push(ValType::F64),
        ReturnType::String => results.push(ValType::I32),
        // TODO: this comment about using i64 for VecBytes doesn't seem to match with what
        //       hl_return_to_val was doing, check if this is still correct.
        /* For compatibility with old host, we return
         * a packed i64 with a (wasm32) pointer in the lower half and
         * a length in the upper half. */
        ReturnType::VecBytes => results.push(ValType::I64),
    }
    Ok(FuncType::new(e, params, results))
}

pub(crate) fn call(
    d: &HostFunctionDefinition,
    mut c: Caller<'_, ()>,
    ps: &[Val],
    rs: &mut [Val],
) -> Result<()> {
    let params = d
        .parameter_types
        .iter()
        .flatten()
        .scan((ps.iter(), None), |s, t| {
            marshal::val_to_hl_param(&mut c, |c, n| c.get_export(n), s, t)
        })
        .collect();

    let rv = call_host_function::<ReturnValue>(&d.function_name, Some(params), d.return_type)
        .expect("Host function call failed");

    assert!(
        return_type_from_val(&rv) == d.return_type,
        "Host function return type mismatch"
    );

    if rs.is_empty() {
        assert!(
            d.return_type == ReturnType::Void,
            "Host function return type mismatch"
        );
        return Ok(());
    }

    rs[0] = marshal::hl_return_to_val(&mut c, |c, n| c.get_export(n), rv)?;

    Ok(())
}

fn return_type_from_val(val: &ReturnValue) -> ReturnType {
    match val {
        ReturnValue::Int(_) => ReturnType::Int,
        ReturnValue::UInt(_) => ReturnType::UInt,
        ReturnValue::Long(_) => ReturnType::Long,
        ReturnValue::ULong(_) => ReturnType::ULong,
        ReturnValue::Float(_) => ReturnType::Float,
        ReturnValue::Double(_) => ReturnType::Double,
        ReturnValue::String(_) => ReturnType::String,
        ReturnValue::VecBytes(_) => ReturnType::VecBytes,
        ReturnValue::Bool(_) => ReturnType::Bool,
        ReturnValue::Void(_) => ReturnType::Void,
    }
}
