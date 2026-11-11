// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Hyperlight Authors.

//! Reads metadata that the guest runtime's build script stores in ELF
//! sections. The build script of this crate includes this file too.

/// The section holding the runtime's hyperlight-wasm version and the hash
/// of its WIT world, separated by a newline. The hash is empty for runtimes
/// built for core modules.
const IDENTITY_SECTION: &str = ".note_hyperlight_wasm_runtime";

/// The hyperlight-wasm version and WIT world of a guest runtime.
pub(crate) struct RuntimeIdentity {
    pub(crate) version: String,
    pub(crate) component_interface: Option<String>,
}

/// Returns the contents of the section `name` of the ELF image `bytes`.
pub(crate) fn section<'a>(bytes: &'a [u8], name: &str) -> Result<Option<&'a [u8]>, String> {
    let elf = goblin::elf::Elf::parse(bytes).map_err(|e| format!("not an ELF file: {e}"))?;
    Ok(elf
        .section_headers
        .iter()
        .find(|h| elf.shdr_strtab.get_at(h.sh_name) == Some(name))
        .and_then(|h| bytes.get(h.sh_offset as usize..(h.sh_offset + h.sh_size) as usize)))
}

/// Reads the identity stored in the runtime ELF image `bytes`.
pub(crate) fn identity(bytes: &[u8]) -> Result<RuntimeIdentity, String> {
    let section = section(bytes, IDENTITY_SECTION)?
        .ok_or("no hyperlight-wasm runtime metadata, so not a runtime from this version")?;
    let text =
        std::str::from_utf8(section).map_err(|e| format!("invalid runtime metadata: {e}"))?;
    let (version, interface) = text.split_once('\n').ok_or("invalid runtime metadata")?;
    Ok(RuntimeIdentity {
        version: version.to_owned(),
        component_interface: Some(interface).filter(|i| !i.is_empty()).map(str::to_owned),
    })
}
