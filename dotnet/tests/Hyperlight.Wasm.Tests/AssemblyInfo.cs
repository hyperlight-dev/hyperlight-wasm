// Copyright 2026 The Hyperlight Authors.
// SPDX-License-Identifier: Apache-2.0

// Parallel tests fail intermittently on arm64 KVM with an unknown VM exit.
[assembly: Xunit.v3.Parallelization(Mode = Xunit.Sdk.ParallelMode.None)]
