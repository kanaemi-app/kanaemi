//! Which file COM is told to load the text service from.

use std::path::{Path, PathBuf};

/// The DLL COM loads, given the module that registers the text service.
///
/// On ARM64 the `kanaemi.dll` beside the module is an ARM64X forwarder to
/// one DLL for ARM64 processes and one for x64 processes. Either of those
/// registers the forwarder, not itself: the other kind of process could not
/// load it.
pub fn registered_dll(module: &Path) -> PathBuf {
    module.with_file_name("kanaemi.dll")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dll_loaded_directly_registers_itself() {
        let module = Path::new("Kanaemi").join("x86").join("kanaemi.dll");
        assert_eq!(registered_dll(&module), module);
    }

    #[test]
    fn a_dll_behind_the_forwarder_registers_the_forwarder() {
        let folder = Path::new("Kanaemi");
        for name in ["kanaemi_arm64.dll", "kanaemi_x64.dll"] {
            assert_eq!(
                registered_dll(&folder.join(name)),
                folder.join("kanaemi.dll")
            );
        }
    }
}
