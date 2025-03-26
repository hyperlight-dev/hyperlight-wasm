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

/// A very minimal implementation of just enough wasip1 functions for the
/// things that were working in the old host to continue working
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use hyperlight_common::flatbuffer_wrappers::function_types::{ParameterValue, ReturnType};
use hyperlight_guest::error::Result;
use hyperlight_guest::host_function_call::{call_host_function, get_host_return_value};
use wasmtime::{Caller, Extern, Linker};

pub(crate) fn register_handlers<T>(linker: &mut Linker<T>) -> Result<()> {
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_seek",
        |fd: i32, filedelta: i64, whence: i32, _retptr: i32| -> i32 {
            panic!("fd_seek called {} {} {}", fd, filedelta, whence);
        },
    )?;
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_write",
        |mut ctx: Caller<'_, T>, fd: i32, iovs: i32, iovs_len: i32, retptr: i32| {
            if fd != 1 {
                return -1;
            }
            let iovs = iovs as usize;
            let retptr = retptr as usize;
            let Some(memory) = ctx.get_export("memory").and_then(Extern::into_memory) else {
                return -1;
            };
            let mut total_written: i32 = 0;
            for i in 0..iovs_len as usize {
                // iovec is size 8
                let iov = iovs + 8 * i;
                let mut bytes = [0u8; 4];
                // offset 0 iovec.buf
                memory.read(&mut ctx, iov, &mut bytes).unwrap();
                let buf = i32::from_le_bytes(bytes);
                // offset 4 is iovec.buf_len
                memory.read(&mut ctx, iov + 4, &mut bytes).unwrap();
                let buf_len = i32::from_le_bytes(bytes);
                let mut string_bytes = vec![0u8; buf_len as usize];
                memory
                    .read(&mut ctx, buf as usize, &mut string_bytes)
                    .unwrap();
                let Ok(str) = core::str::from_utf8(&string_bytes) else {
                    return -2;
                };
                let Ok(()) = call_host_function(
                    "HostPrint",
                    Some(Vec::from(&[ParameterValue::String(str.to_string())])),
                    ReturnType::Int,
                ) else {
                    return -3;
                };
                let Ok(written) = get_host_return_value::<i32>() else {
                    return -4;
                };
                total_written += written;
            }
            memory
                .write(&mut ctx, retptr, &total_written.to_le_bytes())
                .unwrap();
            0
        },
    )?;
    linker.func_wrap("wasi_snapshot_preview1", "fd_close", |fd: i32| -> i32 {
        panic!("fd_close called {}", fd);
    })?;
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "fd_fdstat_get",
        |mut ctx: Caller<'_, T>, fd: i32, retptr: i32| {
            if fd != 1 {
                return -1;
            }
            if let Some(()) = (|| {
                let retptr = retptr as usize;
                let memory = ctx.get_export("memory")?.into_memory()?;
                // offset 0 is fdstat.fs_filetype; 4 is regular_file
                memory
                    .write(&mut ctx, retptr, &(4_u16).to_le_bytes())
                    .unwrap();
                // offset 2 is fdstat.fs_flags; 0 is no particular flags
                memory
                    .write(&mut ctx, retptr + 2, &(0_u16).to_le_bytes())
                    .unwrap();
                // offset 8 fdstat.fs_rights_base; 0b100011 is read/write/seek
                memory
                    .write(&mut ctx, retptr + 8, &(0b100011_u64).to_le_bytes())
                    .unwrap();
                // offset 8 fdstat.fs_rights_inheriting; 0b100011 is read/write/seek
                memory
                    .write(&mut ctx, retptr + 16, &(0b100011_u64).to_le_bytes())
                    .unwrap();
                Some(())
            })() {
                0
            } else {
                -1
            }
        },
    )?;
    Ok(())
}
