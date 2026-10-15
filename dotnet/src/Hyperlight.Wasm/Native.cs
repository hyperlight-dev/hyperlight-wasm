// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Hyperlight.Wasm;

/// <summary>Declarations from <c>hyperlight_wasm.h</c>.</summary>
internal static unsafe partial class Native
{
    private const string Library = "hyperlight_wasm_capi";

    [StructLayout(LayoutKind.Sequential)]
    internal struct Bytes
    {
        public byte* Data;
        public nuint Len;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Buf
    {
        public byte* Data;
        public nuint Len;
    }

    [LibraryImport(Library)]
    internal static partial nint hlw_error_new(byte* message, nuint len);

    [LibraryImport(Library)]
    internal static partial uint hlw_error_kind(nint error);

    [LibraryImport(Library)]
    internal static partial void hlw_error_message(nint error, out byte* message, out nuint len);

    [LibraryImport(Library)]
    internal static partial void hlw_error_delete(nint error);

    [LibraryImport(Library)]
    internal static partial void hlw_buf_delete(Buf* buf);

    [LibraryImport(Library)]
    internal static partial nint hlw_config_new();

    [LibraryImport(Library)]
    internal static partial void hlw_config_delete(nint config);

    [LibraryImport(Library)]
    internal static partial void hlw_config_heap_size(nint config, ulong size);

    [LibraryImport(Library)]
    internal static partial void hlw_config_scratch_size(nint config, nuint size);

    [LibraryImport(Library)]
    internal static partial void hlw_config_input_size(nint config, nuint size);

    [LibraryImport(Library)]
    internal static partial void hlw_config_output_size(nint config, nuint size);

    [LibraryImport(Library)]
    internal static partial nint hlw_config_runtime_file(nint config, byte* path, nuint len);

    [LibraryImport(Library)]
    internal static partial nint hlw_config_interrupt_signal_offset(nint config, byte offset);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_new(nint config, out SandboxHandle sandbox);

    [LibraryImport(Library)]
    internal static partial void hlw_sandbox_delete(nint sandbox);

    [LibraryImport(Library)]
    internal static partial void hlw_result_set(nint result, byte* data, nuint len);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_register(
        SandboxHandle sandbox,
        byte* name,
        nuint nameLen,
        nuint nargs,
        delegate* unmanaged<nint, Bytes*, nuint, nint, nint> func,
        nint env,
        delegate* unmanaged<nint, void> finalizer);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_load_file(SandboxHandle sandbox, byte* path, nuint len);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_load(SandboxHandle sandbox, byte* data, nuint len);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_call(
        SandboxHandle sandbox,
        byte* name,
        nuint nameLen,
        Bytes* args,
        nuint nargs,
        Buf* result);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_snapshot(SandboxHandle sandbox, out SnapshotHandle snapshot);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_restore(SandboxHandle sandbox, SnapshotHandle snapshot);

    [LibraryImport(Library)]
    internal static partial void hlw_snapshot_delete(nint snapshot);

    [LibraryImport(Library)]
    internal static partial nint hlw_sandbox_interrupt_handle(SandboxHandle sandbox, out InterruptHandleHandle handle);

    [LibraryImport(Library)]
    [return: MarshalAs(UnmanagedType.U1)]
    internal static partial bool hlw_interrupt_handle_kill(InterruptHandleHandle handle);

    [LibraryImport(Library)]
    internal static partial void hlw_interrupt_handle_delete(nint handle);

    /// <summary>Throws if <paramref name="error"/> is not null, and frees it.</summary>
    internal static void Check(nint error, Exception? inner = null)
    {
        if (error != 0)
        {
            throw TakeError(error, inner);
        }
    }

    internal static HyperlightException TakeError(nint error, Exception? inner = null)
    {
        try
        {
            hlw_error_message(error, out var message, out var len);
            var text = System.Text.Encoding.UTF8.GetString(message, checked((int)len));
            return new HyperlightException((HyperlightErrorKind)hlw_error_kind(error), text, inner);
        }
        finally
        {
            hlw_error_delete(error);
        }
    }

    /// <summary>Creates a native error that the library takes ownership of.</summary>
    internal static nint NewError(string message)
    {
        var bytes = System.Text.Encoding.UTF8.GetBytes(message);
        fixed (byte* p = bytes)
        {
            return hlw_error_new(p, (nuint)bytes.Length);
        }
    }
}

internal sealed class SandboxHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    public SandboxHandle() : base(true) { }

    protected override bool ReleaseHandle()
    {
        Native.hlw_sandbox_delete(handle);
        return true;
    }
}

internal sealed class SnapshotHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    public SnapshotHandle() : base(true) { }

    protected override bool ReleaseHandle()
    {
        Native.hlw_snapshot_delete(handle);
        return true;
    }
}

internal sealed class InterruptHandleHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    public InterruptHandleHandle() : base(true) { }

    protected override bool ReleaseHandle()
    {
        Native.hlw_interrupt_handle_delete(handle);
        return true;
    }
}
