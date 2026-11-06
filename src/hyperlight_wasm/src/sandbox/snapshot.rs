// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Hyperlight Authors.

use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;

use hyperlight_host::sandbox::snapshot::{
    OciDigest, OciReference, OciTag, Snapshot as CoreSnapshot,
};
use hyperlight_host::{Result, Sandbox, new_error};
use serde::{Deserialize, Serialize};

use super::loaded_wasm_sandbox::LoadedWasmSandbox;
use super::wasm_sandbox::WasmSandbox;

// Bump the version suffix when the metadata format changes.
const METADATA_NAMESPACE: &str = "dev.hyperlight.hyperlight-wasm.snapshot.v1";
// Bump when host calls or their argument and result encodings change.
const GUEST_ABI_VERSION: u32 = 1;
const BASE_TAG_SUFFIX: &str = ".hlwasm-base";
const MAX_TAG_LENGTH: usize = 128;

/// An immutable checkpoint of a wasm sandbox.
///
/// `Snapshot<WasmSandbox>` captures the runtime used to load modules.
/// `Snapshot<LoadedWasmSandbox>` captures a loaded module and its state, plus
/// the runtime that [`LoadedWasmSandbox::unload_module`] returns to. Clones
/// share the captured state.
///
/// [`crate::SandboxBuilder::from_snapshot`] builds the captured sandbox type.
/// It takes fresh implementations of the host functions the snapshot calls.
///
/// # Host state
///
/// Host callbacks, files, sockets, and other host resources are outside the
/// snapshot. Restoring guest state leaves changes made by callbacks in place.
///
/// # Portability
///
/// A snapshot loads only on the CPU architecture, CPU vendor, and hypervisor
/// it was saved on. A component snapshot also needs the `WIT_WORLD` file and
/// `WIT_WORLD_NAME` that hyperlight-wasm was built with. Loading rejects a
/// mismatch.
///
/// # Examples
///
/// ```no_run
/// # use hyperlight_wasm::{LoadedWasmSandbox, OciTag, Result, SandboxBuilder, Snapshot};
/// # fn example(mut loaded: LoadedWasmSandbox) -> Result<()> {
/// let digest = loaded.snapshot()?.save("./snapshots", &OciTag::new("app")?)?;
///
/// // In another process:
/// let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load("./snapshots", digest)?;
/// let mut builder = SandboxBuilder::from_snapshot(snapshot);
/// builder.register("Add", |a: i32, b: i32| a + b)?;
/// let mut loaded = builder.build()?;
/// let result: i32 = loaded.call_guest_function("Run", ())?;
/// # Ok(())
/// # }
/// ```
pub struct Snapshot<T> {
    inner: Arc<CoreSnapshot>,
    base: Option<Arc<CoreSnapshot>>,
    metadata: Arc<SnapshotMetadata>,
    target: PhantomData<fn() -> T>,
}

impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            base: self.base.clone(),
            metadata: Arc::clone(&self.metadata),
            target: PhantomData,
        }
    }
}

impl<T> fmt::Debug for Snapshot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SnapshotKind {
    Runtime,
    Loaded,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SnapshotMetadata {
    kind: SnapshotKind,
    guest_abi_version: u32,
    hyperlight_wasm_version: String,
    component_interface: Option<String>,
    base_snapshot: Option<String>,
}

impl SnapshotMetadata {
    fn runtime() -> Self {
        Self {
            kind: SnapshotKind::Runtime,
            guest_abi_version: GUEST_ABI_VERSION,
            hyperlight_wasm_version: env!("CARGO_PKG_VERSION").to_owned(),
            component_interface: super::WASM_RUNTIME_COMPONENT_INTERFACE.map(str::to_owned),
            base_snapshot: None,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.guest_abi_version != GUEST_ABI_VERSION {
            return Err(new_error!(
                "Snapshot guest ABI {} from hyperlight-wasm {} is incompatible with ABI {}",
                self.guest_abi_version,
                self.hyperlight_wasm_version,
                GUEST_ABI_VERSION
            ));
        }
        if self.component_interface.as_deref() != super::WASM_RUNTIME_COMPONENT_INTERFACE {
            return Err(new_error!(
                "Snapshot module/component interface does not match this host"
            ));
        }
        match (self.kind, self.base_snapshot.is_some()) {
            (SnapshotKind::Runtime, true) => Err(new_error!(
                "Runtime snapshot must not reference an unload baseline"
            )),
            (SnapshotKind::Loaded, false) => {
                Err(new_error!("Loaded snapshot is missing its unload baseline"))
            }
            _ => Ok(()),
        }
    }
}

mod private {
    pub trait Sealed: Sized {
        const LOADED: bool;

        fn restore(
            inner: hyperlight_host::Sandbox,
            snapshot: super::Snapshot<Self>,
        ) -> hyperlight_host::Result<Self>;
    }
}

/// A sandbox lifecycle supported by [`Snapshot`].
#[allow(private_bounds)]
pub trait SnapshotTarget: private::Sealed {}

impl private::Sealed for WasmSandbox {
    const LOADED: bool = false;

    fn restore(inner: Sandbox, snapshot: Snapshot<Self>) -> Result<Self> {
        Self::new_from_snapshot(inner, snapshot)
    }
}

impl SnapshotTarget for WasmSandbox {}

impl private::Sealed for LoadedWasmSandbox {
    const LOADED: bool = true;

    fn restore(inner: Sandbox, snapshot: Snapshot<Self>) -> Result<Self> {
        Self::new(inner, snapshot.runtime_snapshot()?)
    }
}

impl SnapshotTarget for LoadedWasmSandbox {}

impl<T> Snapshot<T> {
    fn new(
        inner: Arc<CoreSnapshot>,
        base: Option<Arc<CoreSnapshot>>,
        metadata: SnapshotMetadata,
    ) -> Self {
        Self {
            inner,
            base,
            metadata: Arc::new(metadata),
            target: PhantomData,
        }
    }

    /// Save to an OCI image layout and return the manifest digest.
    ///
    /// Loaded checkpoints also save `<tag>.hlwasm-base`. Copy both tags when
    /// distributing the layout. Tags ending in `.hlwasm-base` are reserved.
    /// A failed save can leave a loaded checkpoint previously saved under `tag`
    /// unloadable.
    pub fn save(&self, path: impl AsRef<Path>, tag: &OciTag) -> Result<OciDigest> {
        if tag.as_str().ends_with(BASE_TAG_SUFFIX) {
            return Err(new_error!(
                "Snapshot tags ending in '{}' are reserved for unload baselines",
                BASE_TAG_SUFFIX
            ));
        }
        let base_tag = if self.base.is_some() {
            Some(baseline_tag(tag)?)
        } else {
            None
        };
        let mut metadata = self.metadata.as_ref().clone();
        if let (Some(base), Some(base_tag)) = (&self.base, base_tag) {
            metadata.base_snapshot = Some(base.save(path.as_ref(), &base_tag)?.to_string());
        }
        self.inner
            .with_metadata(METADATA_NAMESPACE, &metadata)?
            .save(path, tag)
    }

    pub(super) fn inner(&self) -> Arc<CoreSnapshot> {
        Arc::clone(&self.inner)
    }
}

impl<T: SnapshotTarget> Snapshot<T> {
    /// Load from a trusted OCI layout, skipping blob digest checks.
    ///
    /// Fails if the snapshot captures the other sandbox type or is incompatible
    /// with this host.
    ///
    /// Blob files stay mapped while this checkpoint or a sandbox using it is
    /// alive. Overwriting, truncating, or deleting those files can corrupt
    /// guest memory and cause undefined behavior.
    pub fn load(path: impl AsRef<Path>, reference: impl Into<OciReference>) -> Result<Self> {
        Self::load_inner(path.as_ref(), reference.into(), false)
    }

    /// Load and verify the digest of every blob, including the unload baseline.
    ///
    /// Verification checks integrity, not authenticity. Use a trusted source.
    /// The file lifetime requirements of [`Self::load`] also apply.
    pub fn checked_load(
        path: impl AsRef<Path>,
        reference: impl Into<OciReference>,
    ) -> Result<Self> {
        Self::load_inner(path.as_ref(), reference.into(), true)
    }

    fn load_inner(path: &Path, reference: OciReference, checked: bool) -> Result<Self> {
        let load = |reference| {
            if checked {
                CoreSnapshot::checked_load(path, reference)
            } else {
                CoreSnapshot::load(path, reference)
            }
        };
        let inner = Arc::new(load(reference)?);
        let metadata = inner
            .metadata::<SnapshotMetadata>(METADATA_NAMESPACE)?
            .ok_or_else(|| new_error!("Snapshot is missing hyperlight-wasm metadata"))?;
        metadata.validate()?;
        let expected = if T::LOADED {
            SnapshotKind::Loaded
        } else {
            SnapshotKind::Runtime
        };
        if metadata.kind != expected {
            return Err(new_error!(
                "Snapshot captures {:?} state, expected {:?}",
                metadata.kind,
                expected
            ));
        }
        let base = metadata
            .base_snapshot
            .as_ref()
            .map(|digest| {
                let digest = digest.parse::<OciDigest>()?;
                load(OciReference::from(digest)).map(Arc::new)
            })
            .transpose()?;
        Ok(Self::new(inner, base, metadata))
    }

    pub(super) fn build(self, inner: Sandbox) -> Result<T> {
        T::restore(inner, self)
    }
}

impl Snapshot<WasmSandbox> {
    pub(super) fn runtime(inner: Arc<CoreSnapshot>) -> Self {
        Self::new(inner, None, SnapshotMetadata::runtime())
    }
}

impl Snapshot<LoadedWasmSandbox> {
    pub(super) fn loaded(inner: Arc<CoreSnapshot>, runtime: &Snapshot<WasmSandbox>) -> Self {
        let mut metadata = runtime.metadata.as_ref().clone();
        metadata.kind = SnapshotKind::Loaded;
        Self::new(inner, Some(runtime.inner()), metadata)
    }

    pub(super) fn runtime_snapshot(&self) -> Result<Snapshot<WasmSandbox>> {
        let base = self
            .base
            .as_ref()
            .ok_or_else(|| new_error!("Loaded snapshot is missing its unload baseline"))?;
        let mut metadata = self.metadata.as_ref().clone();
        metadata.kind = SnapshotKind::Runtime;
        metadata.base_snapshot = None;
        Ok(Snapshot::new(Arc::clone(base), None, metadata))
    }
}

fn baseline_tag(tag: &OciTag) -> Result<OciTag> {
    let max = MAX_TAG_LENGTH - BASE_TAG_SUFFIX.len();
    if tag.as_str().len() > max {
        return Err(new_error!(
            "Loaded snapshot tags must be at most {} bytes",
            max
        ));
    }
    OciTag::new(format!("{}{BASE_TAG_SUFFIX}", tag.as_str()))
}

#[cfg(test)]
mod tests {
    use examples_common::get_wasm_module_path;
    use hyperlight_host::HyperlightError;

    use super::*;
    use crate::SandboxBuilder;

    fn runtime_sandbox() -> Result<WasmSandbox> {
        let mut proto = SandboxBuilder::new()
            .with_guest_heap_size(1280 * 1024)
            .build()?;
        proto.register("TestHostFunc", |value: i32| value + 1)?;
        proto.load_runtime()
    }

    fn restore<T: SnapshotTarget>(snapshot: Snapshot<T>) -> Result<T> {
        let mut builder = SandboxBuilder::from_snapshot(snapshot);
        builder.register("TestHostFunc", |value: i32| value + 1)?;
        builder.build()
    }

    fn loaded_sandbox() -> Result<LoadedWasmSandbox> {
        runtime_sandbox()?.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)
    }

    #[test]
    fn metadata_checks_abi_and_component_contract() {
        let metadata = SnapshotMetadata::runtime();
        metadata.validate().unwrap();

        let mut invalid = metadata.clone();
        invalid.guest_abi_version += 1;
        assert!(
            invalid
                .validate()
                .unwrap_err()
                .to_string()
                .contains("guest ABI")
        );

        let mut invalid = metadata;
        invalid.component_interface = Some("different-interface".to_owned());
        assert!(
            invalid
                .validate()
                .unwrap_err()
                .to_string()
                .contains("interface")
        );
    }

    #[test]
    fn metadata_checks_baseline_relationship() {
        let mut metadata = SnapshotMetadata::runtime();
        metadata.base_snapshot = Some("sha256:unused".to_owned());
        assert!(metadata.validate().is_err());
        metadata.kind = SnapshotKind::Loaded;
        metadata.validate().unwrap();
        metadata.base_snapshot = None;
        assert!(metadata.validate().is_err());
    }

    #[test]
    fn baseline_tag_obeys_oci_limit() {
        let max = MAX_TAG_LENGTH - BASE_TAG_SUFFIX.len();
        assert!(baseline_tag(&OciTag::new("a".repeat(max)).unwrap()).is_ok());
        assert!(baseline_tag(&OciTag::new("a".repeat(max + 1)).unwrap()).is_err());
    }

    #[test]
    fn rejects_missing_or_incompatible_persisted_metadata() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let snapshot = SandboxBuilder::new().build()?.load_runtime()?.snapshot()?;
        let tag = OciTag::new("runtime")?;

        snapshot.inner.save(directory.path(), &tag)?;
        assert!(
            Snapshot::<WasmSandbox>::checked_load(directory.path(), tag.clone())
                .unwrap_err()
                .to_string()
                .contains("missing hyperlight-wasm metadata")
        );

        let mut metadata = snapshot.metadata.as_ref().clone();
        metadata.guest_abi_version += 1;
        snapshot
            .inner
            .with_metadata(METADATA_NAMESPACE, &metadata)?
            .save(directory.path(), &tag)?;
        assert!(
            Snapshot::<WasmSandbox>::checked_load(directory.path(), tag)
                .unwrap_err()
                .to_string()
                .contains("guest ABI")
        );
        Ok(())
    }

    #[test]
    fn tag_validation_precedes_writes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let snapshot = SandboxBuilder::new().build()?.load_runtime()?.snapshot()?;
        let loaded = Snapshot::<LoadedWasmSandbox>::loaded(snapshot.inner(), &snapshot);

        assert!(
            loaded
                .save(directory.path(), &OciTag::new("a".repeat(117))?)
                .is_err()
        );
        assert!(
            snapshot
                .save(directory.path(), &OciTag::new("app.hlwasm-base")?)
                .is_err()
        );
        assert!(!directory.path().join("index.json").exists());
        Ok(())
    }

    #[test]
    fn checked_load_verifies_the_baseline() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let runtime = SandboxBuilder::new().build()?.load_runtime()?.snapshot()?;
        let loaded = Snapshot::<LoadedWasmSandbox>::loaded(runtime.inner(), &runtime);
        let tag = OciTag::new("application")?;
        loaded.save(directory.path(), &tag)?;
        let restored = Snapshot::<LoadedWasmSandbox>::load(directory.path(), tag.clone())?;
        let base_digest = restored.metadata.base_snapshot.as_ref().unwrap();
        let blob = |digest: &str| {
            directory
                .path()
                .join("blobs")
                .join(digest.replace(':', "/"))
        };
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(blob(base_digest))?)?;
        let config_path = blob(manifest["config"]["digest"].as_str().unwrap());
        let mut config = std::fs::read(&config_path)?;
        *config.iter_mut().find(|byte| **byte == b'\n').unwrap() = b' ';
        std::fs::write(config_path, config)?;

        Snapshot::<LoadedWasmSandbox>::load(directory.path(), tag.clone())?;
        assert!(
            Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), tag.clone()).is_err()
        );
        std::fs::remove_file(blob(base_digest))?;
        assert!(Snapshot::<LoadedWasmSandbox>::load(directory.path(), tag).is_err());
        Ok(())
    }

    #[test]
    fn persists_runtime_and_checks_lifecycle() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let snapshot = runtime_sandbox()?.snapshot()?;
        let tag = OciTag::new("runtime")?;
        let digest = snapshot.save(directory.path(), &tag)?;

        assert!(Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), tag).is_err());
        let snapshot = Snapshot::<WasmSandbox>::checked_load(directory.path(), digest)?;
        let mut loaded =
            restore(snapshot)?.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        Ok(())
    }

    #[test]
    fn persists_mutable_state_and_preserves_unload() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut loaded = loaded_sandbox()?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        let snapshot = loaded.snapshot()?;
        let digest = snapshot.save(directory.path(), &OciTag::new("application")?)?;
        drop(loaded);
        drop(snapshot);

        assert!(Snapshot::<WasmSandbox>::checked_load(directory.path(), digest.clone()).is_err());
        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
        let mut loaded = restore(snapshot)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 2);
        assert_eq!(
            loaded.call_guest_function::<i32>("call_host_function", 4i32)?,
            5
        );

        let runtime = loaded.unload_module()?;
        let mut loaded = runtime.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        Ok(())
    }

    #[test]
    fn unloaded_snapshot_captures_clean_runtime() -> Result<()> {
        let runtime = runtime_sandbox()?;
        let baseline = runtime.snapshot()?;
        let mut loaded = runtime.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        loaded.call_guest_function::<i32>("counter", ())?;
        let mut runtime = loaded.unload_module()?;
        let snapshot = runtime.snapshot()?;
        // Module memory after unload must not reach a runtime checkpoint.
        assert!(Arc::ptr_eq(&snapshot.inner, &baseline.inner));
        runtime.restore(snapshot.clone())?;
        let mut loaded =
            restore(snapshot)?.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        Ok(())
    }

    #[test]
    fn failed_runtime_restore_keeps_the_sandbox() -> Result<()> {
        let mut proto = SandboxBuilder::new()
            .with_guest_heap_size(1280 * 1024)
            .build()?;
        proto.register("TestHostFunc", |value: i32| value + 1)?;
        proto.register("Extra", || 0i32)?;
        let foreign = proto.load_runtime()?.snapshot()?;

        let mut runtime = loaded_sandbox()?.unload_module()?;
        assert!(matches!(
            runtime.restore(foreign),
            Err(HyperlightError::SnapshotHostFunctionMismatch { .. })
        ));
        let mut loaded = runtime.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        Ok(())
    }

    #[test]
    fn restoring_loaded_state_replaces_the_unload_baseline() -> Result<()> {
        let snapshot = loaded_sandbox()?.snapshot()?;
        for load_from_runtime in [false, true] {
            let mut proto = SandboxBuilder::new()
                .with_guest_heap_size(1280 * 1024)
                .build()?;
            proto.register("TestHostFunc", |value: i32| value + 1)?;
            proto.register("DestinationOnly", || 0i32)?;
            let runtime = proto.load_runtime()?;
            let loaded = if load_from_runtime {
                runtime.load_from_snapshot(snapshot.clone())?
            } else {
                let mut loaded =
                    runtime.load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
                loaded.restore(snapshot.clone())?;
                loaded
            };
            // A destination-only baseline would require DestinationOnly here.
            restore(loaded.unload_module()?.snapshot()?)?;
        }
        Ok(())
    }

    #[test]
    fn validates_callback_replacements() -> Result<()> {
        let snapshot = loaded_sandbox()?.snapshot()?;
        let error = SandboxBuilder::from_snapshot(snapshot.clone())
            .build()
            .unwrap_err();
        assert!(matches!(
            error,
            HyperlightError::SnapshotHostFunctionMismatch { missing, signature_mismatches }
                if missing == ["TestHostFunc"] && signature_mismatches.is_empty()
        ));
        let mut builder = SandboxBuilder::from_snapshot(snapshot);
        builder.register("TestHostFunc", |_: String| 0i32)?;
        assert!(matches!(
            builder.build().unwrap_err(),
            HyperlightError::SnapshotHostFunctionMismatch { missing, signature_mismatches }
                if missing.is_empty() && signature_mismatches.len() == 1
                    && signature_mismatches[0].contains("TestHostFunc")
        ));
        Ok(())
    }

    #[test]
    fn restores_custom_print_routing() -> Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let directory = tempfile::tempdir()?;
        let mut original = loaded_sandbox()?;
        let digest = original
            .snapshot()?
            .save(directory.path(), &OciTag::new("application")?)?;
        drop(original);
        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
        let calls = Arc::new(AtomicUsize::new(0));
        let print_calls = Arc::clone(&calls);
        let mut builder =
            SandboxBuilder::from_snapshot(snapshot).with_host_print_fn(move |message: String| {
                print_calls.fetch_add(1, Ordering::Relaxed);
                i32::try_from(message.len()).unwrap()
            });
        builder.register("TestHostFunc", |value: i32| value + 1)?;
        let mut loaded = builder.build()?;
        loaded.call_guest_function::<i32>("hello_world", ())?;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        Ok(())
    }

    #[test]
    fn restores_after_original_aot_file_is_removed() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let module = directory.path().join("original.aot");
        std::fs::copy(get_wasm_module_path("rust_wasm_samples.aot")?, &module)?;
        let mut loaded = runtime_sandbox()?.load_module(&module)?;
        loaded.call_guest_function::<i32>("counter", ())?;
        let snapshot = loaded.snapshot()?;
        let digest = snapshot.save(directory.path(), &OciTag::new("application")?)?;
        drop(loaded);
        drop(snapshot);
        std::fs::remove_file(module)?;

        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
        assert_eq!(
            restore(snapshot)?.call_guest_function::<i32>("counter", ())?,
            2
        );
        Ok(())
    }

    #[test]
    fn buffer_loading_and_resaving_preserve_state() -> Result<()> {
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        let bytes = std::fs::read(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        // The module is sent through the transport, so the pool must hold it.
        let mut proto = SandboxBuilder::new()
            .with_guest_heap_size(1280 * 1024)
            .with_h2g_pool_pages(128)
            .with_h2g_queue_size(128)
            .build()?;
        proto.register("TestHostFunc", |value: i32| value + 1)?;
        let mut loaded = proto.load_runtime()?.load_module_from_buffer(&bytes)?;
        loaded.call_guest_function::<i32>("counter", ())?;
        let digest = loaded
            .snapshot()?
            .save(first.path(), &OciTag::new("application")?)?;
        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(first.path(), digest.clone())?;
        assert_eq!(snapshot.save(second.path(), &OciTag::new("copy")?)?, digest);
        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(second.path(), digest)?;
        assert_eq!(
            restore(snapshot)?.call_guest_function::<i32>("counter", ())?,
            2
        );
        Ok(())
    }

    #[test]
    fn sandboxes_from_one_checkpoint_are_isolated() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut loaded = loaded_sandbox()?;
        loaded.call_guest_function::<i32>("counter", ())?;
        let digest = loaded
            .snapshot()?
            .save(directory.path(), &OciTag::new("application")?)?;
        drop(loaded);

        let snapshot = Snapshot::<LoadedWasmSandbox>::load(directory.path(), digest.clone())?;
        let mut first = restore(snapshot.clone())?;
        assert_eq!(first.call_guest_function::<i32>("counter", ())?, 2);
        assert_eq!(first.call_guest_function::<i32>("counter", ())?, 3);
        let mut second = restore(snapshot)?;
        assert_eq!(second.call_guest_function::<i32>("counter", ())?, 2);

        // Guest writes are copy-on-write, so the mapped blob still holds the saved state.
        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
        assert_eq!(
            restore(snapshot)?.call_guest_function::<i32>("counter", ())?,
            2
        );
        Ok(())
    }

    #[test]
    fn recaptures_restored_state_with_its_baseline() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut loaded = loaded_sandbox()?;
        loaded.call_guest_function::<i32>("counter", ())?;
        let digest = loaded
            .snapshot()?
            .save(directory.path(), &OciTag::new("first")?)?;
        drop(loaded);

        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
        let mut loaded = restore(snapshot)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 2);
        let digest = loaded
            .snapshot()?
            .save(directory.path(), &OciTag::new("second")?)?;
        drop(loaded);

        let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
        let mut loaded = restore(snapshot)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 3);
        let mut loaded = loaded
            .unload_module()?
            .load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        Ok(())
    }

    #[test]
    fn restores_in_another_process() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut loaded = loaded_sandbox()?;
        loaded.call_guest_function::<i32>("counter", ())?;
        loaded
            .snapshot()?
            .save(directory.path(), &OciTag::new("application")?)?;
        drop(loaded);

        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "sandbox::snapshot::tests::snapshot_reader",
                "--ignored",
                "--nocapture",
            ])
            .env("HYPERLIGHT_WASM_SNAPSHOT_PATH", directory.path())
            .status()?;
        assert!(status.success(), "snapshot reader exited with {status}");
        // libtest succeeds without running anything when the filter matches no test.
        assert!(directory.path().join("reader-ran").exists());
        Ok(())
    }

    #[test]
    #[ignore = "Invoked by restores_in_another_process"]
    fn snapshot_reader() -> Result<()> {
        let Some(path) = std::env::var_os("HYPERLIGHT_WASM_SNAPSHOT_PATH") else {
            return Ok(());
        };
        let snapshot =
            Snapshot::<LoadedWasmSandbox>::checked_load(&path, OciTag::new("application")?)?;
        let mut loaded = restore(snapshot)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 2);
        let mut loaded = loaded
            .unload_module()?
            .load_module(get_wasm_module_path("rust_wasm_samples.aot")?)?;
        assert_eq!(loaded.call_guest_function::<i32>("counter", ())?, 1);
        std::fs::write(Path::new(&path).join("reader-ran"), [])?;
        Ok(())
    }

    // Runs only in builds with `WIT_WORLD` set, through `just examples-components`.
    #[cfg(component)]
    mod component {
        use super::*;

        mod bindings {
            hyperlight_component_macro::host_bindgen!({
                path: "../tests/rust_guests/component_sample/wit/component-world.wasm",
            });
        }

        use bindings::component_sample::example::{Adder, ExampleExports};

        struct State;

        impl bindings::component_sample::example::Host<hyperlight_common::component::Negative> for State {
            fn r#print(&mut self, _message: String) {}

            fn r#host_function(&mut self, input: String) -> String {
                format!("{input} and the host!")
            }
        }

        #[allow(refining_impl_trait)]
        impl
            bindings::component_sample::example::ExampleImports<
                hyperlight_common::component::Negative,
            > for State
        {
            type Host = State;

            fn r#host(&mut self) -> &mut Self {
                self
            }
        }

        #[test]
        fn persists_component_and_rebinds_callbacks() -> Result<()> {
            let directory = tempfile::tempdir()?;
            let module = get_wasm_module_path("component_sample.aot")?;
            let mut proto = SandboxBuilder::new()
                .with_guest_heap_size(200_000_000)
                .with_guest_scratch_size(100 * 1024 * 1024)
                .build()?;
            let rt = bindings::register_host_functions(&mut proto, State)?;
            let sb = proto.load_runtime()?.load_module(&module)?;
            let mut wrapped = bindings::ExampleSandbox { sb, rt };
            assert_eq!(ExampleExports::adder(&mut wrapped).add(1, 2)?, 3);
            let digest = wrapped
                .sb
                .snapshot()?
                .save(directory.path(), &OciTag::new("component")?)?;
            drop(wrapped);

            let snapshot = Snapshot::<LoadedWasmSandbox>::checked_load(directory.path(), digest)?;
            let mut builder = SandboxBuilder::from_snapshot(snapshot);
            let rt = bindings::register_host_functions(&mut builder, State)?;
            let mut wrapped = bindings::ExampleSandbox {
                sb: builder.build()?,
                rt,
            };
            assert_eq!(
                ExampleExports::adder(&mut wrapped).call_host("Restored".to_owned())?,
                "Restored from component and the host!"
            );

            let sb = wrapped.sb.unload_module()?.load_module(&module)?;
            let mut wrapped = bindings::ExampleSandbox { sb, rt: wrapped.rt };
            assert_eq!(ExampleExports::adder(&mut wrapped).add(4, 3)?, 7);
            Ok(())
        }
    }
}
