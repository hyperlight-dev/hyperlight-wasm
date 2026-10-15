// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

namespace Hyperlight.Wasm;

/// <summary>The kind of a <see cref="HyperlightException"/>.</summary>
public enum HyperlightErrorKind
{
    /// <summary>Any error not covered by another kind.</summary>
    Other = 0,

    /// <summary>
    /// The sandbox is poisoned by an interrupted or failed guest call.
    /// Restore a snapshot to use it again.
    /// </summary>
    Poisoned = 1,

    /// <summary>A guest call was interrupted.</summary>
    Cancelled = 2,

    /// <summary>The operation is not valid in the sandbox's current state.</summary>
    InvalidState = 3,

    /// <summary>An argument is invalid.</summary>
    InvalidArgument = 4,
}

/// <summary>An error reported by Hyperlight.</summary>
public sealed class HyperlightException : Exception
{
    internal HyperlightException(HyperlightErrorKind kind, string message, Exception? inner)
        : base(message, inner)
    {
        Kind = kind;
    }

    /// <summary>The kind of error.</summary>
    public HyperlightErrorKind Kind { get; }
}
