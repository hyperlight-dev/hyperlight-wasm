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

    /// <summary>
    /// The guest scratch size in bytes. Scratch holds the guest heap, stack
    /// and transport buffers. Values below the default are ignored.
    /// </summary>
    public ulong? ScratchSize { get; set; }

    /// <summary>
    /// The size in bytes of each host-to-guest buffer. These buffers carry
    /// guest calls and components loaded from memory. A message spans
    /// several buffers, so the pool and queue must hold the largest message.
    /// </summary>
    public ulong? H2GBufferSize { get; set; }

    /// <summary>The guest pages for host-to-guest buffers. Values of 48 or less are ignored.</summary>
    public ulong? H2GPoolPages { get; set; }

    /// <summary>The number of in-flight host-to-guest buffers.</summary>
    public ulong? H2GQueueSize { get; set; }

    /// <summary>
    /// The size in bytes of each guest-to-host buffer. These buffers carry
    /// host calls and guest results.
    /// </summary>
    public ulong? G2HBufferSize { get; set; }

    /// <summary>The guest pages for guest-to-host buffers.</summary>
    public ulong? G2HPoolPages { get; set; }

    /// <summary>The number of in-flight guest-to-host buffers.</summary>
    public ulong? G2HQueueSize { get; set; }

    /// <summary>
    /// A guest runtime file built for the component's WIT world by the same
    /// hyperlight-wasm version as this package. Components need it unless
    /// the native library was built for their world with <c>WIT_WORLD</c>.
    /// The packaged library has no world.
    /// </summary>
    public string? RuntimePath { get; set; }

    /// <summary>
    /// The offset from <c>SIGRTMIN</c> of the signal that interrupts a
    /// running guest on Linux. Change it only if the process uses that
    /// signal itself. Ignored on Windows and macOS.
    /// </summary>
    public byte InterruptSignalOffset { get; set; } = DefaultInterruptSignalOffset;
}
