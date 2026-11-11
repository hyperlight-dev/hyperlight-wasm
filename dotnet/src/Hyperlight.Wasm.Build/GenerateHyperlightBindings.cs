// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Build.Framework;
using Microsoft.Build.Utilities;

namespace Hyperlight.Wasm.Build;

/// <summary>Generates C# bindings for each WIT item.</summary>
public sealed unsafe class GenerateHyperlightBindings : Microsoft.Build.Utilities.Task
{
    /// <summary>
    /// WIT files or directories. Optional metadata: <c>World</c> selects a
    /// world and <c>Namespace</c> overrides <see cref="DefaultNamespace"/>.
    /// </summary>
    [Required]
    public ITaskItem[] Wit { get; set; } = [];

    /// <summary>The native hyperlight_wasm_csharp_bindgen library.</summary>
    [Required]
    public string BindgenLibrary { get; set; } = "";

    /// <summary>The directory for generated files.</summary>
    [Required]
    public string OutputDirectory { get; set; } = "";

    /// <summary>The namespace of items without <c>Namespace</c> metadata.</summary>
    public string DefaultNamespace { get; set; } = "";

    /// <summary>The generated C# files.</summary>
    [Output]
    public ITaskItem[] Generated { get; set; } = [];

    /// <summary><c>hlw_bindgen_csharp</c>. Returns 0 and the C# source, or 1 and an error message.</summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int GenerateBindings(
        byte* wit, nuint witLength,
        byte* world, nuint worldLength,
        byte* ns, nuint nsLength,
        byte** output, nuint* outputLength);

    /// <summary><c>hlw_bindgen_free</c>. Frees the output of <see cref="GenerateBindings"/>.</summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate void FreeBindings(byte* output, nuint outputLength);

    /// <inheritdoc />
    public override bool Execute()
    {
        nint library;
        try
        {
            library = NativeLibrary.Load(BindgenLibrary);
        }
        catch (Exception e)
        {
            Log.LogError("Cannot load the Hyperlight bindings generator '{0}': {1}", BindgenLibrary, e.Message);
            return false;
        }
        try
        {
            if (!NativeLibrary.TryGetExport(library, "hlw_bindgen_csharp", out var generateExport)
                || !NativeLibrary.TryGetExport(library, "hlw_bindgen_free", out var freeExport))
            {
                Log.LogError("The Hyperlight bindings generator '{0}' does not match this package version.", BindgenLibrary);
                return false;
            }
            var generate = Marshal.GetDelegateForFunctionPointer<GenerateBindings>(generateExport);
            var free = Marshal.GetDelegateForFunctionPointer<FreeBindings>(freeExport);
            Directory.CreateDirectory(OutputDirectory);
            var generated = new List<ITaskItem>();
            var outputs = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var item in Wit)
            {
                var wit = item.GetMetadata("FullPath");
                var world = item.GetMetadata("World");
                var ns = item.GetMetadata("Namespace");
                if (ns.Length == 0)
                {
                    ns = DefaultNamespace;
                }
                if (!TryGenerate(generate, free, wit, world, ns, out var text))
                {
                    Log.LogError(null, "HLWIT001", null, item.ItemSpec, 0, 0, 0, 0, "Cannot generate Hyperlight bindings: {0}", text);
                    continue;
                }
                var name = Path.GetFileNameWithoutExtension(wit.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
                if (world.Length > 0)
                {
                    name += "." + world;
                }
                var output = Path.Combine(OutputDirectory, name + ".g.cs");
                if (!outputs.Add(output))
                {
                    Log.LogError(null, "HLWIT002", null, item.ItemSpec, 0, 0, 0, 0, "Two HyperlightWit items generate '{0}'. Give them different World metadata or file names.", output);
                    continue;
                }
                // Rewriting unchanged output would force a recompile.
                if (!File.Exists(output) || File.ReadAllText(output) != text)
                {
                    File.WriteAllText(output, text);
                }
                generated.Add(new TaskItem(output));
            }
            Generated = [.. generated];
            return !Log.HasLoggedErrors;
        }
        finally
        {
            NativeLibrary.Free(library);
        }
    }

    private static bool TryGenerate(
        GenerateBindings generate,
        FreeBindings free,
        string wit,
        string world,
        string ns,
        out string text)
    {
        var witBytes = Encoding.UTF8.GetBytes(wit);
        var worldBytes = Encoding.UTF8.GetBytes(world);
        var nsBytes = Encoding.UTF8.GetBytes(ns);
        byte* output;
        nuint length;
        int status;
        fixed (byte* w = witBytes)
        fixed (byte* wo = worldBytes)
        fixed (byte* n = nsBytes)
        {
            status = generate(w, (nuint)witBytes.Length, wo, (nuint)worldBytes.Length, n, (nuint)nsBytes.Length, &output, &length);
        }
        try
        {
            text = Encoding.UTF8.GetString(output, checked((int)length));
        }
        finally
        {
            free(output, length);
        }
        return status == 0;
    }
}
