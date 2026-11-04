// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using Hyperlight.Component;

namespace Hyperlight.Wasm;

/// <summary>
/// A Hyperlight micro VM that runs one WebAssembly component.
/// </summary>
/// <remarks>
/// Register host functions, then load a precompiled component, then call
/// its exports. Generated bindings do this through
/// <see cref="IHostFunctionRegistry"/> and <see cref="IGuestCaller"/>.
/// Calls are serialized. Host functions run on the calling thread and must
/// not call back into the same sandbox.
/// </remarks>
public sealed unsafe class WasmSandbox : IGuestCaller, IHostFunctionRegistry, IDisposable
{
    private readonly SandboxHandle _handle;
    private readonly CallState _state = new();
    private readonly object _lock = new();
    private bool _loaded;
    private bool _inCall;
    private InterruptHandle? _cancelHandle;

    /// <summary>Creates a sandbox with <paramref name="options"/>, or defaults if null.</summary>
    /// <exception cref="HyperlightException">No hypervisor is available, or the settings are invalid.</exception>
    public WasmSandbox(SandboxOptions? options = null)
    {
        options ??= new SandboxOptions();
        var config = Native.hlw_config_new();
        try
        {
            if (options.HeapSize is { } heap)
            {
                Native.hlw_config_heap_size(config, heap);
            }
            if (options.ScratchSize is { } scratch)
            {
                Native.hlw_config_scratch_size(config, checked((nuint)scratch));
            }
            if (options.InputSize is { } input)
            {
                Native.hlw_config_input_size(config, checked((nuint)input));
            }
            if (options.OutputSize is { } output)
            {
                Native.hlw_config_output_size(config, checked((nuint)output));
            }
            if (options.RuntimePath is { } runtime)
            {
                var path = Encoding.UTF8.GetBytes(Path.GetFullPath(runtime));
                fixed (byte* p = path)
                {
                    Native.Check(Native.hlw_config_runtime_file(config, p, (nuint)path.Length));
                }
            }
            Native.Check(Native.hlw_config_interrupt_signal_offset(config, options.InterruptSignalOffset));
            Native.Check(Native.hlw_sandbox_new(config, out _handle));
        }
        finally
        {
            Native.hlw_config_delete(config);
        }
    }

    /// <summary>
    /// Registers a host function. It receives one encoded buffer per
    /// parameter and returns the encoded result. Exceptions it throws fail
    /// the guest call, which then throws a <see cref="HyperlightException"/>
    /// with the exception as <see cref="Exception.InnerException"/>.
    /// </summary>
    /// <exception cref="HyperlightException">A component is already loaded, or the parameter count exceeds 32.</exception>
    public void Register(string function, int parameterCount, Func<byte[][], byte[]> implementation)
    {
        ArgumentNullException.ThrowIfNull(function);
        ArgumentNullException.ThrowIfNull(implementation);
        ArgumentOutOfRangeException.ThrowIfNegative(parameterCount);
        var name = Encoding.UTF8.GetBytes(function);
        lock (_lock)
        {
            ThrowIfDisposed();
            // The native library frees this handle through Release, also when
            // registration fails.
            var env = GCHandle.ToIntPtr(GCHandle.Alloc(new HostFunction(implementation, _state)));
            fixed (byte* p = name)
            {
                Native.Check(Native.hlw_sandbox_register(
                    _handle,
                    p,
                    (nuint)name.Length,
                    (nuint)parameterCount,
                    &Invoke,
                    env,
                    &Release));
            }
        }
    }

    /// <summary>
    /// Loads a component precompiled with hyperlight-wasm-aot. After a
    /// failed load the sandbox can only be disposed.
    /// </summary>
    public void Load(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        var bytes = Encoding.UTF8.GetBytes(Path.GetFullPath(path));
        lock (_lock)
        {
            ThrowIfDisposed();
            fixed (byte* p = bytes)
            {
                Native.Check(Native.hlw_sandbox_load_file(_handle, p, (nuint)bytes.Length));
            }
            _loaded = true;
        }
    }

    /// <summary>
    /// Loads a component precompiled with hyperlight-wasm-aot from memory.
    /// After a failed load the sandbox can only be disposed.
    /// </summary>
    public void Load(ReadOnlySpan<byte> component)
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            fixed (byte* p = component)
            {
                Native.Check(Native.hlw_sandbox_load(_handle, p, (nuint)component.Length));
            }
            _loaded = true;
        }
    }

    /// <inheritdoc />
    public byte[] Call(string function, byte[][] args) => Call(function, args, CancellationToken.None);

    /// <summary>
    /// Calls the guest function <paramref name="function"/> with one encoded
    /// buffer per parameter and returns the encoded result.
    /// </summary>
    /// <remarks>
    /// Cancelling <paramref name="cancellationToken"/> interrupts the guest
    /// and poisons the sandbox. A cancellation just before the guest starts
    /// running can be missed.
    /// </remarks>
    /// <exception cref="OperationCanceledException">The call was cancelled.</exception>
    /// <exception cref="HyperlightException">The call failed.</exception>
    public byte[] Call(string function, byte[][] args, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(function);
        ArgumentNullException.ThrowIfNull(args);
        var name = Encoding.UTF8.GetBytes(function);
        var pins = new GCHandle[args.Length];
        var views = new Native.Bytes[args.Length];
        lock (_lock)
        {
            ThrowIfDisposed();
            if (_inCall)
            {
                throw new InvalidOperationException("A host function cannot call into its own sandbox.");
            }
            cancellationToken.ThrowIfCancellationRequested();
            var registration = default(CancellationTokenRegistration);
            if (cancellationToken.CanBeCanceled && _loaded)
            {
                _cancelHandle ??= CreateInterruptHandle();
                registration = cancellationToken.Register(static h => ((InterruptHandle)h!).Kill(), _cancelHandle);
            }
            _inCall = true;
            _state.HostException = null;
            try
            {
                for (var i = 0; i < args.Length; i++)
                {
                    var arg = args[i] ?? throw new ArgumentNullException(nameof(args), $"Argument {i} is null.");
                    pins[i] = GCHandle.Alloc(arg, GCHandleType.Pinned);
                    views[i] = new Native.Bytes
                    {
                        Data = (byte*)pins[i].AddrOfPinnedObject(),
                        Len = (nuint)arg.Length,
                    };
                }
                Native.Buf result = default;
                nint error;
                fixed (byte* p = name)
                fixed (Native.Bytes* v = views)
                {
                    error = Native.hlw_sandbox_call(_handle, p, (nuint)name.Length, v, (nuint)views.Length, &result);
                }
                registration.Dispose();
                if (error != 0)
                {
                    var exception = Native.TakeError(error, _state.HostException);
                    if (exception.Kind == HyperlightErrorKind.Cancelled && cancellationToken.IsCancellationRequested)
                    {
                        throw new OperationCanceledException(exception.Message, exception, cancellationToken);
                    }
                    throw exception;
                }
                try
                {
                    return new ReadOnlySpan<byte>(result.Data, checked((int)result.Len)).ToArray();
                }
                finally
                {
                    Native.hlw_buf_delete(&result);
                }
            }
            finally
            {
                registration.Dispose();
                _inCall = false;
                foreach (var pin in pins)
                {
                    if (pin.IsAllocated)
                    {
                        pin.Free();
                    }
                }
            }
        }
    }

    /// <summary>Saves the state of the loaded sandbox.</summary>
    public Snapshot CreateSnapshot()
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            Native.Check(Native.hlw_sandbox_snapshot(_handle, out var snapshot));
            return new Snapshot(this, snapshot);
        }
    }

    /// <summary>
    /// Restores <paramref name="snapshot"/>, which must come from this
    /// sandbox. Restoring also recovers a poisoned sandbox.
    /// </summary>
    public void Restore(Snapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);
        if (snapshot.Owner != this)
        {
            throw new ArgumentException("The snapshot comes from another sandbox.", nameof(snapshot));
        }
        lock (_lock)
        {
            ThrowIfDisposed();
            Native.Check(Native.hlw_sandbox_restore(_handle, snapshot.Handle));
        }
    }

    /// <summary>
    /// Returns a handle that interrupts guest calls from any thread. The
    /// sandbox must be loaded.
    /// </summary>
    public InterruptHandle CreateInterruptHandle()
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            Native.Check(Native.hlw_sandbox_interrupt_handle(_handle, out var handle));
            return new InterruptHandle(handle);
        }
    }

    /// <summary>Frees the micro VM and the registered host functions.</summary>
    public void Dispose()
    {
        lock (_lock)
        {
            _cancelHandle?.Dispose();
            _handle.Dispose();
        }
    }

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_handle.IsClosed, this);

    [UnmanagedCallersOnly]
    private static nint Invoke(nint env, Native.Bytes* args, nuint nargs, nint result)
    {
        var function = (HostFunction)GCHandle.FromIntPtr(env).Target!;
        try
        {
            var managed = new byte[checked((int)nargs)][];
            for (var i = 0; i < managed.Length; i++)
            {
                managed[i] = new ReadOnlySpan<byte>(args[i].Data, checked((int)args[i].Len)).ToArray();
            }
            var bytes = function.Implementation(managed) ?? [];
            fixed (byte* p = bytes)
            {
                Native.hlw_result_set(result, p, (nuint)bytes.Length);
            }
            return 0;
        }
        catch (Exception e)
        {
            function.State.HostException ??= e;
            return Native.NewError($"{e.GetType().FullName}: {e.Message}");
        }
    }

    [UnmanagedCallersOnly]
    private static void Release(nint env) => GCHandle.FromIntPtr(env).Free();

    /// <summary>State shared with host functions, which must not keep the sandbox alive.</summary>
    private sealed class CallState
    {
        public Exception? HostException;
    }

    private sealed record HostFunction(Func<byte[][], byte[]> Implementation, CallState State);
}

/// <summary>A saved sandbox state.</summary>
public sealed class Snapshot : IDisposable
{
    internal Snapshot(WasmSandbox owner, SnapshotHandle handle)
    {
        Owner = owner;
        Handle = handle;
    }

    internal WasmSandbox Owner { get; }

    internal SnapshotHandle Handle { get; }

    /// <inheritdoc />
    public void Dispose() => Handle.Dispose();
}

/// <summary>Interrupts guest calls of a sandbox from any thread.</summary>
public sealed class InterruptHandle : IDisposable
{
    private readonly InterruptHandleHandle _handle;

    internal InterruptHandle(InterruptHandleHandle handle) => _handle = handle;

    /// <summary>
    /// Interrupts the running guest call, which then fails with
    /// <see cref="HyperlightErrorKind.Cancelled"/> and poisons the sandbox.
    /// Blocks until the guest stops.
    /// </summary>
    /// <returns>Whether a call was running.</returns>
    public bool Kill() => Native.hlw_interrupt_handle_kill(_handle);

    /// <inheritdoc />
    public void Dispose() => _handle.Dispose();
}
