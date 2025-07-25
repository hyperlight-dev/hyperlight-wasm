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

//! Parameter and return value marshalling for WASM guest function calls.
//!
//! # Memory Management Contract
//!
//! This module implements a clear memory ownership model for both guest function calls and host function calls:
//!
//! ## Guest Function Parameters (Host → Guest)
//! - When calling guest functions with String or VecBytes parameters, the host allocates memory
//!   in the guest's memory space and passes pointers to the guest.
//! - **The guest owns these allocations and must free them** when no longer needed using the
//!   `free` function exported from the guest module.
//!
//! ## Guest Function Return Values (Guest → Host)  
//! - When guest functions return String or VecBytes values, the guest allocates memory in its
//!   own memory space and returns pointers to the host.
//! - **The host takes ownership of these allocations and will free them** on the next VM entry
//!   to prevent memory leaks.
//!
//! ## Host Function Parameters (Guest → Host)
//! - When guest code calls host functions with String or VecBytes parameters, the guest passes
//!   pointers to data in its own memory space.
//! - **The guest retains ownership** of these allocations and remains responsible for freeing them.
//!
//! ## Host Function Return Values (Host → Guest)
//! - When host functions return String or VecBytes values to the guest, the host allocates memory
//!   in the guest's memory space and returns pointers.
//! - **The guest owns these allocations and must free them** when no longer needed.

extern crate alloc;

use alloc::ffi::CString;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{format, vec};

use hyperlight_common::flatbuffer_wrappers::function_types::{
    ParameterType, ParameterValue, ReturnType, ReturnValue,
};
use hyperlight_common::flatbuffer_wrappers::guest_error::ErrorCode;
use hyperlight_common::flatbuffer_wrappers::util::get_flatbuffer_result;
use hyperlight_guest::error::{HyperlightGuestError, Result};
use wasmtime::{AsContextMut, Extern, Val};

use spin::Mutex;

// Global tracking for return value allocations that need to be freed on next VM entry
static RETURN_VALUE_ALLOCATIONS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

/// Track a return value allocation that should be freed on the next VM entry
fn track_return_value_allocation(addr: i32) {
    RETURN_VALUE_ALLOCATIONS.lock().push(addr);
}

/// Free all tracked return value allocations from previous VM calls
pub fn free_return_value_allocations<C: AsContextMut>(
    ctx: &mut C,
    get_export: &impl Fn(&mut C, &str) -> Option<Extern>,
) -> Result<()> {
    let mut allocations = RETURN_VALUE_ALLOCATIONS.lock();
    for addr in allocations.drain(..) {
        free(ctx, get_export, addr)?;
    }
    Ok(())
}

fn malloc<C: AsContextMut>(
    ctx: &mut C,
    get_export: &impl Fn(&mut C, &str) -> Option<Extern>,
    len: usize,
) -> Result<i32> {
    let malloc = get_export(&mut *ctx, "malloc")
        .and_then(Extern::into_func)
        .ok_or(HyperlightGuestError::new(
            ErrorCode::GuestError,
            "malloc function not exported".to_string(),
        ))?;
    let addr = malloc
        .typed::<i32, i32>(&mut *ctx)?
        .call(&mut *ctx, len as i32)?;
    Ok(addr)
}

fn free<C: AsContextMut>(
    ctx: &mut C,
    get_export: &impl Fn(&mut C, &str) -> Option<Extern>,
    addr: i32,
) -> Result<()> {
    let free = get_export(&mut *ctx, "free")
        .and_then(Extern::into_func)
        .ok_or(HyperlightGuestError::new(
            ErrorCode::GuestError,
            "free function not exported".to_string(),
        ))?;
    free.typed::<i32, ()>(&mut *ctx)?.call(&mut *ctx, addr)?;
    Ok(())
}

fn write<C: AsContextMut>(
    ctx: &mut C,
    get_export: &impl Fn(&mut C, &str) -> Option<Extern>,
    addr: i32,
    bytes: &[u8],
) -> Result<()> {
    let memory = get_export(&mut *ctx, "memory")
        .and_then(Extern::into_memory)
        .ok_or(HyperlightGuestError::new(
            ErrorCode::GuestError,
            "memory not exported".to_string(),
        ))?;
    memory.write(&mut *ctx, addr as usize, bytes).map_err(|e| {
        HyperlightGuestError::new(
            ErrorCode::GuestError,
            format!("error writing to memory: {}", e),
        )
    })?;
    Ok(())
}

fn read<C: AsContextMut>(
    ctx: &mut C,
    get_export: &impl Fn(&mut C, &str) -> Option<Extern>,
    addr: i32,
    bytes: &mut [u8],
) -> Result<()> {
    let memory = get_export(&mut *ctx, "memory")
        .and_then(Extern::into_memory)
        .ok_or(HyperlightGuestError::new(
            ErrorCode::GuestError,
            "memory not exported".to_string(),
        ))?;
    memory.read(&mut *ctx, addr as usize, bytes).map_err(|e| {
        HyperlightGuestError::new(
            ErrorCode::GuestError,
            format!("error reading from memory: {}", e),
        )
    })?;
    Ok(())
}

fn read_cstr<C: AsContextMut>(
    ctx: &mut C,
    get_export: &impl Fn(&mut C, &str) -> Option<Extern>,
    addr: i32,
) -> Result<CString> {
    let mut addr = addr;
    let memory = get_export(&mut *ctx, "memory")
        .and_then(Extern::into_memory)
        .ok_or(HyperlightGuestError::new(
            ErrorCode::GuestError,
            "memory not exported".to_string(),
        ))?;
    let mut byte = [0];
    let mut string = Vec::new();
    loop {
        memory
            .read(&mut *ctx, addr as usize, &mut byte)
            .map_err(|e| {
                HyperlightGuestError::new(
                    ErrorCode::GuestError,
                    format!("error reading from memory: {}", e),
                )
            })?;
        if byte[0] == 0 {
            break;
        }
        string.push(byte[0]);
        addr += 1;
    }

    CString::new(string).map_err(|e| {
        HyperlightGuestError::new(
            ErrorCode::GuestError,
            format!("invalid c string in memory: {}", e),
        )
    })
}

/// Convert a hyperlight parameter value to a wasmtime value.
///
/// For String and VecBytes parameter types, this allocates memory in the guest's memory space
/// and returns a pointer. The guest function is responsible for freeing this memory when it is no
/// longer needed using the `free` function exported from the guest module.
pub fn hl_param_to_val<C: AsContextMut>(
    mut ctx: C,
    get_export: impl Fn(&mut C, &str) -> Option<Extern>,
    param: &ParameterValue,
) -> Result<Val> {
    match param {
        ParameterValue::Int(i) => Ok(Val::I32(*i)),
        ParameterValue::UInt(u) => Ok(Val::I32(*u as i32)),
        ParameterValue::Long(l) => Ok(Val::I64(*l)),
        ParameterValue::ULong(u) => Ok(Val::I64(*u as i64)),
        ParameterValue::Bool(b) => Ok(Val::I32(if *b { 1 } else { 0 })),
        ParameterValue::Float(f) => Ok(Val::F32(f.to_bits())),
        ParameterValue::Double(f) => Ok(Val::F64(f.to_bits())),
        ParameterValue::String(s) => {
            let s = CString::new(s.as_str()).unwrap();
            let nbytes = s.count_bytes() + 1; // include the NUL terminator
            let addr = malloc(&mut ctx, &get_export, nbytes)?;
            write(&mut ctx, &get_export, addr, s.as_bytes_with_nul())?;
            Ok(Val::I32(addr))
        }
        ParameterValue::VecBytes(b) => {
            let addr = malloc(&mut ctx, &get_export, b.len())?;
            write(&mut ctx, &get_export, addr, b)?;
            Ok(Val::I32(addr))
            // TODO: check that the next parameter is the correct length
        }
    }
}

/// Convert guest function return values to hyperlight return value.
///
/// For String and VecBytes return types, the guest has allocated memory in its own memory space
/// and returned pointers. The host takes ownership of these allocations and tracks them for
/// automatic cleanup on the next VM entry to prevent memory leaks.
pub fn val_to_hl_result<C: AsContextMut>(
    mut ctx: C,
    get_export: impl Fn(&mut C, &str) -> Option<Extern>,
    rt: ReturnType,
    rvs: &[Val],
) -> Result<Vec<u8>> {
    if let ReturnType::Void = rt {
        return Ok(get_flatbuffer_result::<()>(()));
    }
    match (rt, rvs[0]) {
        (ReturnType::Int, Val::I32(i)) => Ok(get_flatbuffer_result::<i32>(i)),
        (ReturnType::UInt, Val::I32(u)) => Ok(get_flatbuffer_result::<u32>(u as u32)),
        (ReturnType::Long, Val::I64(l)) => Ok(get_flatbuffer_result::<i64>(l)),
        (ReturnType::ULong, Val::I64(u)) => Ok(get_flatbuffer_result::<u64>(u as u64)),
        /* todo: get_flatbuffer_result_from_bool is missing */
        (ReturnType::Float, Val::F32(f)) => Ok(get_flatbuffer_result::<f32>(f32::from_bits(f))),
        (ReturnType::Double, Val::F64(f)) => Ok(get_flatbuffer_result::<f64>(f64::from_bits(f))),
        (ReturnType::String, Val::I32(p)) => {
            // Track this allocation so it can be freed on next VM entry
            track_return_value_allocation(p);
            Ok(get_flatbuffer_result::<&str>(
                read_cstr(&mut ctx, &get_export, p)?.to_str().map_err(|e| {
                    HyperlightGuestError::new(
                        ErrorCode::GuestError,
                        format!("non-UTF-8 c string in guest function return: {}", e),
                    )
                })?,
            ))
        }
        (ReturnType::VecBytes, Val::I32(ret)) => {
            // Track this allocation so it can be freed on next VM entry
            track_return_value_allocation(ret);
            let mut size_bytes = [0; 4];
            read(&mut ctx, &get_export, ret, &mut size_bytes)?;
            let size = i32::from_le_bytes(size_bytes);
            let mut bytes = vec![0; size as usize];
            read(&mut ctx, &get_export, ret + 4, &mut bytes)?;
            Ok(get_flatbuffer_result::<&[u8]>(&bytes))
        }
        (_, _) => Err(HyperlightGuestError::new(
            ErrorCode::GuestError,
            format!(
                "Hyperlight/wasm function return type combination unsupported: {:?} / {:?}",
                rt, rvs[0]
            ),
        )),
    }
}

/// Convert guest-provided WASM values to hyperlight parameters for host function calls.
///
/// For String and VecBytes parameter types, the guest passes pointers to data in its own
/// memory space. The guest retains ownership of these allocations and remains responsible
/// for freeing them. This function only reads the data without taking ownership.
pub fn val_to_hl_param<'a, C: AsContextMut>(
    ctx: &mut C,
    get_export: impl Fn(&mut C, &str) -> Option<Extern>,
    state: &mut (impl Iterator<Item = &'a Val>, Option<u32>),
    pt: &ParameterType,
) -> Option<ParameterValue> {
    let ps = &mut state.0;
    let last_vec_len = &mut state.1;
    if let Some(l) = *last_vec_len {
        if *pt == ParameterType::Int {
            *last_vec_len = None;
            return Some(ParameterValue::Int(l as i32));
        } else {
            panic!("Host function details missing expected vector buffer length");
        }
    }
    let Some(v) = ps.next() else {
        panic!("Host function call missing parameter of type {:?}", pt);
    };
    match (pt, v) {
        (ParameterType::Int, Val::I32(i)) => Some(ParameterValue::Int(*i)),
        (ParameterType::UInt, Val::I32(u)) => Some(ParameterValue::UInt(*u as u32)),
        (ParameterType::Long, Val::I64(l)) => Some(ParameterValue::Long(*l)),
        (ParameterType::ULong, Val::I64(u)) => Some(ParameterValue::ULong(*u as u64)),
        (ParameterType::Bool, Val::I32(b)) => Some(ParameterValue::Bool(*b == 0)),
        (ParameterType::Float, Val::F32(f)) => Some(ParameterValue::Float(f32::from_bits(*f))),
        (ParameterType::Double, Val::F64(f)) => Some(ParameterValue::Double(f64::from_bits(*f))),
        (ParameterType::String, Val::I32(p)) => Some(ParameterValue::String(
            read_cstr(ctx, &get_export, *p)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        )),
        (ParameterType::VecBytes, Val::I32(p)) => {
            let Some(Val::I32(l)) = ps.next() else {
                panic!("Host function call missing vecbytes length parameter");
            };
            *last_vec_len = Some(*l as u32);
            let mut bytes = vec![0; *l as usize];
            read(ctx, &get_export, *p, &mut bytes).unwrap();
            Some(ParameterValue::VecBytes(bytes.clone()))
        }
        (_, _) => panic!(
            "Host function return type combination unsupported: {:?} / {:?}",
            pt, v
        ),
    }
}

/// Convert a hyperlight return value to a wasmtime value for host function returns.
///
/// For String and VecBytes return types, this allocates memory in the guest's memory space
/// and returns a pointer. The guest owns these allocations and must free them when no longer needed
/// using the `free` function exported from the guest module.
pub fn hl_return_to_val<C: AsContextMut>(
    ctx: &mut C,
    get_export: impl Fn(&mut C, &str) -> Option<Extern>,
    rv: ReturnValue,
) -> Result<Val> {
    match rv {
        ReturnValue::Int(i) => Ok(Val::I32(i)),
        ReturnValue::UInt(u) => Ok(Val::I32(u as i32)),
        ReturnValue::Long(l) => Ok(Val::I64(l)),
        ReturnValue::ULong(u) => Ok(Val::I64(u as i64)),
        ReturnValue::Bool(b) => Ok(Val::I32(if b { 1 } else { 0 })),
        ReturnValue::Float(f) => Ok(Val::F32(f.to_bits())),
        ReturnValue::Double(f) => Ok(Val::F64(f.to_bits())),
        ReturnValue::String(s) => {
            let s = CString::new(s.as_str()).unwrap();
            let nbytes = s.count_bytes() + 1; // include the NUL terminator
            let addr = malloc(ctx, &get_export, nbytes)?;
            write(ctx, &get_export, addr, s.as_bytes_with_nul())?;
            Ok(Val::I32(addr))
        }
        ReturnValue::VecBytes(b) => {
            let addr = malloc(ctx, &get_export, b.len())?;
            write(ctx, &get_export, addr, b.as_ref())?;
            Ok(Val::I32(addr))
            // TODO: check that the next parameter is the correct length
        }
        ReturnValue::Void(()) => Ok(Val::I32(0)),
    }
}
