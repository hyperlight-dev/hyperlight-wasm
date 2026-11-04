// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

namespace Hyperlight.Wasm;

/// <summary>Settings for a <see cref="WasmSandbox"/>. Unset values use defaults.</summary>
public sealed class SandboxOptions
{
    /// <summary>
    /// The signal offset from <c>SIGRTMIN</c> used unless
    /// <see cref="InterruptSignalOffset"/> is set. The .NET runtime uses
    /// <c>SIGRTMIN</c> itself to suspend threads.
    /// </summary>
    public const byte DefaultInterruptSignalOffset = 1;

    /// <summary>The guest heap size in bytes. Values below the default are ignored.</summary>
    public ulong? HeapSize { get; set; }

    /// <summary>
    /// The guest scratch size in bytes. Scratch holds the guest stack and
    /// the transport buffers. Values below the default are ignored.
    /// </summary>
    public ulong? ScratchSize { get; set; }

    /// <summary>
    /// The largest message in bytes the host sends to the guest, such as a
    /// guest call or a component loaded from memory. Values below the
    /// default are ignored.
    /// </summary>
    public ulong? InputSize { get; set; }

    /// <summary>
    /// The largest message in bytes the guest sends to the host, such as a
    /// host call or a guest result.
    /// </summary>
    public ulong? OutputSize { get; set; }

    /// <summary>
    /// A guest runtime file built for the component's WIT world by the same
    /// hyperlight-wasm version as this package. Unset uses the runtime built
    /// into the native library.
    /// </summary>
    public string? RuntimePath { get; set; }

    /// <summary>
    /// The offset from <c>SIGRTMIN</c> of the signal that interrupts a
    /// running guest on Linux. Ignored on Windows.
    /// </summary>
    public byte InterruptSignalOffset { get; set; } = DefaultInterruptSignalOffset;
}
