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

/**
 * \file hyperlight_wasm.h
 *
 * C API for running WebAssembly components in Hyperlight sandboxes.
 *
 * Guest and host functions are identified by their wire names and take one
 * encoded byte buffer per parameter. Their results are one encoded byte
 * buffer. The encoding and naming are described in hyperlight's
 * docs/component-wire-format.md.
 *
 * Functions that can fail return an `hlw_error *`, which is NULL on success.
 * The caller owns a returned error and frees it with `hlw_error_delete`.
 *
 * A sandbox must not be used from two threads at once. Interrupt handles may
 * be used from any thread.
 */

#ifndef HYPERLIGHT_WASM_H
#define HYPERLIGHT_WASM_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** An error. */
typedef struct hlw_error hlw_error;

/** Sandbox settings. */
typedef struct hlw_config hlw_config;

/** A sandbox that runs one WebAssembly component. */
typedef struct hlw_sandbox hlw_sandbox;

/** A saved sandbox state. */
typedef struct hlw_snapshot hlw_snapshot;

/** Interrupts a running guest call. */
typedef struct hlw_interrupt_handle hlw_interrupt_handle;

/** The result slot of a host function call. */
typedef struct hlw_result hlw_result;

/** Bytes borrowed from the caller. */
typedef struct hlw_bytes {
  const uint8_t *data;
  size_t len;
} hlw_bytes;

/** Bytes owned by the library. Free with `hlw_buf_delete`. */
typedef struct hlw_buf {
  uint8_t *data;
  size_t len;
} hlw_buf;

/** Any error not covered by another kind. */
#define HLW_ERROR_OTHER 0
/**
 * The sandbox is poisoned by an interrupted or failed guest call. Restore a
 * snapshot to use it again.
 */
#define HLW_ERROR_POISONED 1
/** A guest call was interrupted with `hlw_interrupt_handle_kill`. */
#define HLW_ERROR_CANCELLED 2
/** The function is not valid in the sandbox's current state. */
#define HLW_ERROR_INVALID_STATE 3
/** An argument is invalid. */
#define HLW_ERROR_INVALID_ARGUMENT 4

/** The largest number of parameters a function can have. */
#define HLW_MAX_PARAMS 32

/** Creates an error of kind `HLW_ERROR_OTHER` from a UTF-8 message. */
hlw_error *hlw_error_new(const char *message, size_t len);

/** Returns the kind of `error`, one of the `HLW_ERROR_*` values. */
uint32_t hlw_error_kind(const hlw_error *error);

/**
 * Returns the UTF-8 message of `error`, which is not NUL-terminated. It is
 * valid until `error` is deleted.
 */
void hlw_error_message(const hlw_error *error, const char **message,
                       size_t *len);

/** Frees `error`. Accepts NULL. */
void hlw_error_delete(hlw_error *error);

/** Frees the bytes of `buf` and clears it. Accepts NULL. */
void hlw_buf_delete(hlw_buf *buf);

/** Creates settings with default values. */
hlw_config *hlw_config_new(void);

/** Frees `config`. Accepts NULL. */
void hlw_config_delete(hlw_config *config);

/** Sets the guest heap size in bytes. Smaller values than the default are ignored. */
void hlw_config_heap_size(hlw_config *config, uint64_t size);

/**
 * Sets the guest scratch size in bytes. Scratch holds the guest stack and the
 * transport buffers. Smaller values than the default are ignored.
 */
void hlw_config_scratch_size(hlw_config *config, size_t size);

/**
 * Sets the largest message in bytes the host can send to the guest, such as
 * a guest call or a component loaded with `hlw_sandbox_load`. Smaller values
 * than the default are ignored.
 */
void hlw_config_input_size(hlw_config *config, size_t size);

/**
 * Sets the largest message in bytes the guest can send to the host, such as
 * a host call or a guest result.
 */
void hlw_config_output_size(hlw_config *config, size_t size);

/**
 * Loads the guest runtime from the UTF-8 path `path` instead of using the
 * built-in one. The runtime must be built by the same hyperlight-wasm
 * version as this library.
 */
hlw_error *hlw_config_runtime_file(hlw_config *config, const char *path,
                                   size_t len);

/**
 * Sets the offset from SIGRTMIN of the signal that interrupts a running
 * guest. Change it when the process already uses SIGRTMIN, as .NET does.
 * Has no effect on Windows.
 */
hlw_error *hlw_config_interrupt_signal_offset(hlw_config *config,
                                              uint8_t offset);

/**
 * Creates a sandbox. `config` may be NULL for default settings and is not
 * consumed.
 */
hlw_error *hlw_sandbox_new(const hlw_config *config, hlw_sandbox **out);

/** Frees `sandbox`. Accepts NULL. */
void hlw_sandbox_delete(hlw_sandbox *sandbox);

/**
 * A host function. It receives `nargs` encoded arguments, which are valid
 * until it returns. It stores its encoded result with `hlw_result_set`.
 * Leaving the result unset returns an empty buffer. It returns NULL on
 * success or an error, which the library frees.
 *
 * It runs on the thread that called `hlw_sandbox_call`.
 */
typedef hlw_error *(*hlw_host_func)(void *env, const hlw_bytes *args,
                                    size_t nargs, hlw_result *result);

/** Copies `len` bytes from `data` into `result`. */
void hlw_result_set(hlw_result *result, const uint8_t *data, size_t len);

/**
 * Registers `func` as the host function `name` with `nargs` parameters.
 * Must be called before the component is loaded. `finalizer`, which may be
 * NULL, is called with `env` when the sandbox no longer needs it. It is
 * also called if registration fails. `env` must be usable from any thread.
 */
hlw_error *hlw_sandbox_register(hlw_sandbox *sandbox, const char *name,
                                size_t name_len, size_t nargs,
                                hlw_host_func func, void *env,
                                void (*finalizer)(void *));

/**
 * Loads the precompiled component at the UTF-8 path `path`. If loading
 * fails, the sandbox can only be deleted.
 */
hlw_error *hlw_sandbox_load_file(hlw_sandbox *sandbox, const char *path,
                                 size_t len);

/**
 * Loads a precompiled component from memory. If loading fails, the sandbox
 * can only be deleted.
 */
hlw_error *hlw_sandbox_load(hlw_sandbox *sandbox, const uint8_t *data,
                            size_t len);

/**
 * Calls the guest function `name` with `nargs` encoded arguments and stores
 * its encoded result in `result`.
 */
hlw_error *hlw_sandbox_call(hlw_sandbox *sandbox, const char *name,
                            size_t name_len, const hlw_bytes *args,
                            size_t nargs, hlw_buf *result);

/** Saves the state of a loaded sandbox. */
hlw_error *hlw_sandbox_snapshot(hlw_sandbox *sandbox, hlw_snapshot **out);

/**
 * Restores `snapshot`, which must come from `sandbox`. Restoring also
 * recovers a poisoned sandbox.
 */
hlw_error *hlw_sandbox_restore(hlw_sandbox *sandbox,
                               const hlw_snapshot *snapshot);

/** Frees `snapshot`. Accepts NULL. */
void hlw_snapshot_delete(hlw_snapshot *snapshot);

/** Returns a handle that interrupts guest calls of a loaded sandbox. */
hlw_error *hlw_sandbox_interrupt_handle(hlw_sandbox *sandbox,
                                        hlw_interrupt_handle **out);

/**
 * Interrupts the running guest call, which then fails with
 * `HLW_ERROR_CANCELLED`. Blocks until the guest stops. Returns whether a
 * call was running.
 */
bool hlw_interrupt_handle_kill(const hlw_interrupt_handle *handle);

/** Frees `handle`. Accepts NULL. */
void hlw_interrupt_handle_delete(hlw_interrupt_handle *handle);

#ifdef __cplusplus
}
#endif

#endif
