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
/// Calls are serialized. Host functions run on the calling thread. Calls
/// from a host function back into its own sandbox throw.
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
            if (options.ScratchSize is { } scratch)
            {
                Native.hlw_config_scratch_size(config, checked((nuint)scratch));
            }
            Set(config, options.H2GBufferSize, &Native.hlw_config_h2g_buffer_size);
            Set(config, options.H2GPoolPages, &Native.hlw_config_h2g_pool_pages);
            Set(config, options.H2GQueueSize, &Native.hlw_config_h2g_queue_size);
            Set(config, options.G2HBufferSize, &Native.hlw_config_g2h_buffer_size);
            Set(config, options.G2HPoolPages, &Native.hlw_config_g2h_pool_pages);
            Set(config, options.G2HQueueSize, &Native.hlw_config_g2h_queue_size);
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
    /// Loads a component precompiled with hyperlight-wasm-aot. Loading runs
    /// the component's start functions, which cannot be interrupted. After a
    /// failed load the sandbox can only be disposed.
    /// </summary>
    public void Load(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        var bytes = Encoding.UTF8.GetBytes(Path.GetFullPath(path));
        lock (_lock)
        {
            ThrowIfDisposed();
            _state.HostException = null;
            fixed (byte* p = bytes)
            {
                Native.Check(Native.hlw_sandbox_load_file(_handle, p, (nuint)bytes.Length), _state.HostException);
            }
            _loaded = true;
        }
    }

    /// <summary>
    /// Loads a component precompiled with hyperlight-wasm-aot from memory,
    /// as <see cref="Load(string)"/> does.
    /// </summary>
    public void Load(ReadOnlySpan<byte> component)
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            _state.HostException = null;
            fixed (byte* p = component)
            {
                Native.Check(Native.hlw_sandbox_load(_handle, p, (nuint)component.Length), _state.HostException);
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
    /// and poisons the sandbox. A guest in a host function stops when that
    /// function returns.
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
            Cancellation? cancellation = null;
            if (cancellationToken.CanBeCanceled && _loaded)
            {
                _cancelHandle ??= CreateInterruptHandle();
                cancellation = new Cancellation(_cancelHandle);
                registration = cancellationToken.Register(static c => ((Cancellation)c!).Cancel(), cancellation);
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
                cancellation?.Stop();
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
                cancellation?.Stop();
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
    /// sandbox must be loaded. Creating a handle waits for a running call,
    /// so create it before the call it interrupts.
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

    private static void Set(nint config, ulong? value, delegate*<nint, nuint, void> setter)
    {
        if (value is { } v)
        {
            setter(config, checked((nuint)v));
        }
    }

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
            // The message reaches the guest. The caller gets the exception
            // as the InnerException.
            return Native.NewError("The host function threw an exception.");
        }
    }

    [UnmanagedCallersOnly]
    private static void Release(nint env) => GCHandle.FromIntPtr(env).Free();

    /// <summary>State shared with host functions, which must not keep the sandbox alive.</summary>
    private sealed class CallState
    {
        public Exception? HostException;
    }

    /// <summary>
    /// Interrupts one guest call when its token is cancelled. A kill before
    /// the guest starts is lost, so a failed kill is retried until the call
    /// ends. The retry runs on the thread pool because the token can be
    /// cancelled from a host function on the calling thread.
    /// </summary>
    private sealed class Cancellation(InterruptHandle handle)
    {
        private volatile bool _done;
        private Task? _retry;

        public void Cancel()
        {
            if (!TryKill())
            {
                _retry = Task.Run(Retry);
            }
        }

        /// <summary>Stops retrying. Call after disposing the token registration.</summary>
        public void Stop()
        {
            _done = true;
            _retry?.Wait();
        }

        private void Retry()
        {
            var spin = new SpinWait();
            while (!_done && !TryKill())
            {
                spin.SpinOnce();
            }
        }

        // The sandbox can be disposed from a host function during the call.
        private bool TryKill()
        {
            try
            {
                return handle.Kill();
            }
            catch (ObjectDisposedException)
            {
                return true;
            }
        }
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
    /// A guest in a host function stops when that function returns.
    /// </summary>
    /// <returns>Whether the guest was running.</returns>
    public bool Kill() => Native.hlw_interrupt_handle_kill(_handle);

    /// <inheritdoc />
    public void Dispose() => _handle.Dispose();
}
