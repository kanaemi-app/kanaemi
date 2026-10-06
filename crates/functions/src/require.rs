//! `require` that finds modules in the functions folder only, by their path
//! from the file that requires them. A symlink in the folder is followed, so a
//! user may bring a module in from elsewhere on purpose.

use std::io;
use std::path::{Component, Path, PathBuf};

use mlua::luau::{NavigateError, Require};
use mlua::{Function, Lua};

use crate::EXTENSION;

/// The file of a folder a module may be.
const INIT: &str = "init";

pub(crate) struct FolderRequirer {
    root: PathBuf,
    /// Where navigation is: a module's path without its extension, or a
    /// folder.
    at: PathBuf,
    /// The file of the module `at` names, if any.
    module: Option<PathBuf>,
}

impl FolderRequirer {
    pub(crate) fn new(root: &Path) -> Self {
        let root = normalized(root);
        Self {
            at: root.clone(),
            root,
            module: None,
        }
    }

    fn go(&mut self, at: PathBuf) -> Result<(), NavigateError> {
        let at = normalized(&at);
        if !at.starts_with(&self.root) {
            return Err(NavigateError::NotFound);
        }
        // `at` may be the folder itself, whose `.luau` is beside it.
        self.module = module_of(&at)?.filter(|module| module.starts_with(&self.root));
        self.at = at;
        Ok(())
    }
}

impl Require for FolderRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with('@')
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        let path = chunk_name
            .strip_prefix('@')
            .ok_or(NavigateError::NotFound)?;
        // A chunk name may end with the line it is at.
        let path = match path.rsplit_once(':') {
            Some((path, line)) if line.parse::<u32>().is_ok() => path,
            _ => path,
        };
        let at = normalized(Path::new(path));
        if !at.starts_with(&self.root) {
            return Err(NavigateError::NotFound);
        }
        self.module = at.is_file().then(|| at.clone());
        self.at = at;
        Ok(())
    }

    fn jump_to_alias(&mut self, _path: &str) -> Result<(), NavigateError> {
        Err(NavigateError::NotFound)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        if self.at == self.root {
            return Err(NavigateError::NotFound);
        }
        let mut parent = self.at.clone();
        parent.pop();
        self.go(parent)
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        self.go(self.at.join(name))
    }

    fn has_module(&self) -> bool {
        self.module.as_deref().is_some_and(Path::is_file)
    }

    fn cache_key(&self) -> String {
        self.module
            .as_deref()
            .unwrap_or(&self.at)
            .display()
            .to_string()
    }

    fn has_config(&self) -> bool {
        false
    }

    fn config(&self) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::NotFound.into())
    }

    fn loader(&self, lua: &Lua) -> mlua::Result<Function> {
        let module = self
            .module
            .as_deref()
            .ok_or_else(|| mlua::Error::runtime("no module here"))?;
        lua.load(module)
            .set_name(format!("@{}", module.display()))
            .into_function()
    }
}

/// The file of the module at `path`: `path.luau`, or `init.luau` in the folder
/// `path`. A folder without one is a step on the way to a module.
fn module_of(path: &Path) -> Result<Option<PathBuf>, NavigateError> {
    let mut file = path.as_os_str().to_owned();
    file.push(format!(".{EXTENSION}"));
    let file = PathBuf::from(file);
    let init = path.join(format!("{INIT}.{EXTENSION}"));
    match (file.is_file(), path.is_dir()) {
        (true, true) if init.is_file() => Err(NavigateError::Ambiguous),
        (true, _) => Ok(Some(file)),
        (false, true) => Ok(init.is_file().then_some(init)),
        (false, false) => Err(NavigateError::NotFound),
    }
}

/// `path` made absolute, with `.` and `..` resolved by name rather than on
/// disk, so a symlink inside stays inside.
fn normalized(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            component => out.push(component),
        }
    }
    out
}
