// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

using Hyperlight.Wasm.Tests.Sample;
using Hyperlight.Wasm.Tests.Sample.ComponentSample.Example;
using Xunit;

namespace Hyperlight.Wasm.Tests;

public sealed class ComponentTests
{
    private static readonly string Assets = Path.Combine(AppContext.BaseDirectory, "assets");
    private static readonly string Component = Path.Combine(Assets, "component_sample.aot");

    private sealed class Host : IHost
    {
        public List<string> Printed { get; } = [];

        public Func<string, string> Reply { get; set; } = input => $"{input} and the host!";

        public void Print(string message) => Printed.Add(message);

        public string HostFunction(string input) => Reply(input);
    }

    private static SandboxOptions Options() => new()
    {
        HeapSize = 200_000_000,
        ScratchSize = 100 << 20,
        // The component is larger than the default input size.
        InputSize = 1 << 20,
        RuntimePath = Path.Combine(Assets, "component-sample-runtime"),
    };

    private static (WasmSandbox Sandbox, ExampleComponent Component) Load(Host host, bool fromMemory = false)
    {
        var sandbox = new WasmSandbox(Options());
        ExampleComponent.RegisterImports(sandbox, host);
        if (fromMemory)
        {
            sandbox.Load(File.ReadAllBytes(Component));
        }
        else
        {
            sandbox.Load(Component);
        }
        return (sandbox, new ExampleComponent(sandbox));
    }

    [Fact]
    public void CallsExportsAndImports()
    {
        var host = new Host();
        var (sandbox, component) = Load(host);
        using (sandbox)
        {
            Assert.Equal(3u, component.Adder.Add(1, 2));
            Assert.Equal("Hello from component and the host!", component.Adder.CallHost("Hello"));
            component.Adder.DoSomething(42);
            Assert.Equal(["42"], host.Printed);
        }
    }

    [Fact]
    public void LoadsFromMemory()
    {
        var (sandbox, component) = Load(new Host(), fromMemory: true);
        using (sandbox)
        {
            Assert.Equal(42u, component.Adder.Add(20, 22));
        }
    }

    [Fact]
    public void HostExceptionsBecomeInnerExceptions()
    {
        var failure = new InvalidDataException("host failure");
        var host = new Host { Reply = _ => throw failure };
        var (sandbox, component) = Load(host);
        using (sandbox)
        {
            var e = Assert.Throws<HyperlightException>(() => component.Adder.CallHost("Hello"));
            Assert.Same(failure, e.InnerException);
            Assert.Contains("host failure", e.Message);
        }
    }

    [Fact]
    public void RestoreRecoversFromTrap()
    {
        var (sandbox, component) = Load(new Host());
        using (sandbox)
        using (var snapshot = sandbox.CreateSnapshot())
        {
            // The debug build of the sample traps on overflow.
            var trap = Assert.Throws<HyperlightException>(() => component.Adder.Add(uint.MaxValue, 1));
            Assert.Contains("add_overflow", trap.Message);
            Assert.Throws<HyperlightException>(() => component.Adder.Add(1, 1));
            sandbox.Restore(snapshot);
            Assert.Equal(7u, component.Adder.Add(3, 4));
        }
    }

    [Fact]
    public void RejectsReentrantCalls()
    {
        var host = new Host();
        var (sandbox, component) = Load(host);
        using (sandbox)
        {
            host.Reply = _ => component.Adder.Add(1, 1).ToString();
            var e = Assert.Throws<HyperlightException>(() => component.Adder.CallHost("Hello"));
            Assert.IsType<InvalidOperationException>(e.InnerException);
        }
    }

    [Fact]
    public void CancellationAndInterrupts()
    {
        var (sandbox, component) = Load(new Host());
        using (sandbox)
        using (var handle = sandbox.CreateInterruptHandle())
        {
            Assert.False(handle.Kill());
            Assert.Equal(2u, component.Adder.Add(1, 1));

            using var cts = new CancellationTokenSource();
            cts.Cancel();
            var args = new[] { new byte[4], new byte[4] };
            Assert.Throws<OperationCanceledException>(
                () => sandbox.Call("component-sample:example/adder#add", args, cts.Token));
            using var live = new CancellationTokenSource();
            var result = sandbox.Call("component-sample:example/adder#add", args, live.Token);
            Assert.Equal(new byte[4], result);
        }
    }

    [Fact]
    public void ValidatesState()
    {
        var (sandbox, _) = Load(new Host());
        var (other, _) = Load(new Host());
        using (other)
        using (var foreign = other.CreateSnapshot())
        {
            Assert.Throws<ArgumentException>(() => sandbox.Restore(foreign));
            var late = Assert.Throws<HyperlightException>(() => sandbox.Register("late", 0, _ => []));
            Assert.Equal(HyperlightErrorKind.InvalidState, late.Kind);
            var missing = Assert.Throws<HyperlightException>(() => sandbox.Call("missing", []));
            Assert.Contains("missing", missing.Message);
            sandbox.Dispose();
            sandbox.Dispose();
            Assert.Throws<ObjectDisposedException>(() => sandbox.Call("missing", []));
        }

        using var empty = new WasmSandbox(Options());
        var unloaded = Assert.Throws<HyperlightException>(() => empty.Call("x", []));
        Assert.Equal(HyperlightErrorKind.InvalidState, unloaded.Kind);
    }

    [Fact]
    public void ReportsMissingRuntime()
    {
        var options = Options();
        options.RuntimePath = Path.Combine(Assets, "no-such-runtime");
        Assert.Throws<HyperlightException>(() => new WasmSandbox(options));
    }

    [Fact]
    public async Task SurvivesGarbageCollection()
    {
        // GC.Collect in a host function runs while the guest call is active.
        var host = new Host();
        host.Reply = input =>
        {
            GC.Collect();
            return input;
        };
        var (sandbox, component) = Load(host);
        using (sandbox)
        {
            using var stop = new CancellationTokenSource();
            var churn = Task.Run(() =>
            {
                while (!stop.IsCancellationRequested)
                {
                    _ = new byte[64 * 1024];
                    GC.Collect(0);
                }
            });
            for (var i = 0; i < 500; i++)
            {
                Assert.Equal((uint)(2 * i), component.Adder.Add((uint)i, (uint)i));
                Assert.Equal($"{i} from component", component.Adder.CallHost(i.ToString()));
            }
            stop.Cancel();
            await churn;
        }
    }
}

public sealed class SignalTests
{
    private static volatile bool _stop;

    /// <summary>
    /// The GC suspends a thread spinning in managed code with SIGRTMIN. With
    /// interrupt signal offset 0 the sandbox replaces that handler and this
    /// test hangs.
    /// </summary>
    [Fact]
    public async Task GcSuspendsSpinningThreadAfterSandboxLoads()
    {
        var assets = Path.Combine(AppContext.BaseDirectory, "assets");
        using var sandbox = new WasmSandbox(new SandboxOptions
        {
            HeapSize = 200_000_000,
            ScratchSize = 100 << 20,
            RuntimePath = Path.Combine(assets, "component-sample-runtime"),
        });
        ExampleComponent.RegisterImports(sandbox, new NoHost());
        sandbox.Load(Path.Combine(assets, "component_sample.aot"));

        _stop = false;
        var spinner = new Thread(Spin) { IsBackground = true };
        spinner.Start();
        try
        {
            await Task.Run(() =>
            {
                for (var i = 0; i < 20; i++)
                {
                    GC.Collect();
                }
            }).WaitAsync(TimeSpan.FromSeconds(30));
        }
        finally
        {
            _stop = true;
        }
        spinner.Join();
    }

    [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
    private static void Spin()
    {
        // No calls or allocations, so the thread has no GC safe point.
        while (!_stop)
        {
        }
    }

    private sealed class NoHost : IHost
    {
        public void Print(string message) { }

        public string HostFunction(string input) => input;
    }
}
