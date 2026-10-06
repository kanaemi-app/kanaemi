//! `require` that finds a module by its path from the file that requires it,
//! among the files that file is one of: the functions folder, or the built-in
//! files. A symlink in the folder is followed, so a user may bring a module in
//! from elsewhere on purpose.

use std::io;
use std::path::{Component, Path, PathBuf};

use mlua::luau::{NavigateError, Require};
use mlua::{Function, Lua};

use crate::EXTENSION;

/// The file of a folder a module may be.
const INIT: &str = "init";

/// How a built-in file's chunk is named, before its path among them.
pub(crate) const BUILTIN_CHUNK: &str = "@builtin:/";

/// Where navigation is: a module's path without its extension, or a folder.
enum At {
    Folder(PathBuf),
    /// Components of a path among the built-in files.
    Builtin(Vec<String>),
}

enum Module {
    File(PathBuf),
    /// Its path among the built-in files, and its source.
    Builtin(&'static str, &'static str),
}

pub(crate) struct FolderRequirer {
    root: PathBuf,
    /// The built-in files by their path, `/` between folders.
    builtins: &'static [(&'static str, &'static str)],
    at: At,
    module: Option<Module>,
}

impl FolderRequirer {
    pub(crate) fn new(root: &Path, builtins: &'static [(&'static str, &'static str)]) -> Self {
        let root = normalized(root);
        Self {
            at: At::Folder(root.clone()),
            root,
            builtins,
            module: None,
        }
    }

    fn go(&mut self, at: At) -> Result<(), NavigateError> {
        self.module = match &at {
            At::Folder(path) => {
                if !path.starts_with(&self.root) {
                    return Err(NavigateError::NotFound);
                }
                // `path` may be the folder itself, whose `.luau` is beside it.
                module_of(path)?
                    .filter(|module| module.starts_with(&self.root))
                    .map(Module::File)
            }
            At::Builtin(components) => self.builtin_of(components)?,
        };
        self.at = at;
        Ok(())
    }

    /// The built-in file of the module at `components`, as [`module_of`]
    /// finds one on disk.
    fn builtin_of(&self, components: &[String]) -> Result<Option<Module>, NavigateError> {
        let path = components.join("/");
        let find = |key: String| {
            self.builtins
                .iter()
                .find(|(file, _)| *file == key)
                .map(|&(file, source)| Module::Builtin(file, source))
        };
        let file = find(format!("{path}.{EXTENSION}"));
        let folder = format!("{path}/");
        let is_folder = path.is_empty()
            || self
                .builtins
                .iter()
                .any(|(file, _)| file.starts_with(&folder));
        let init = find(format!("{folder}{INIT}.{EXTENSION}"));
        match (file, is_folder) {
            (Some(_), true) if init.is_some() => Err(NavigateError::Ambiguous),
            (Some(file), _) => Ok(Some(file)),
            (None, true) => Ok(init),
            (None, false) => Err(NavigateError::NotFound),
        }
    }
}

impl Require for FolderRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with('@')
    }

    fn reset(&mut self, chunk_name: &str) -> Result<(), NavigateError> {
        // A chunk name may end with the line it is at.
        let chunk_name = match chunk_name.rsplit_once(':') {
            Some((name, line)) if line.parse::<u32>().is_ok() => name,
            _ => chunk_name,
        };
        if let Some(path) = chunk_name.strip_prefix(BUILTIN_CHUNK) {
            self.module = self
                .builtins
                .iter()
                .find(|(file, _)| *file == path)
                .map(|&(file, source)| Module::Builtin(file, source));
            self.at = At::Builtin(path.split('/').map(str::to_owned).collect());
            return Ok(());
        }
        let path = chunk_name
            .strip_prefix('@')
            .ok_or(NavigateError::NotFound)?;
        let at = normalized(Path::new(path));
        if !at.starts_with(&self.root) {
            return Err(NavigateError::NotFound);
        }
        self.module = at.is_file().then(|| Module::File(at.clone()));
        self.at = At::Folder(at);
        Ok(())
    }

    fn jump_to_alias(&mut self, _path: &str) -> Result<(), NavigateError> {
        Err(NavigateError::NotFound)
    }

    fn to_parent(&mut self) -> Result<(), NavigateError> {
        let parent = match &self.at {
            At::Folder(path) if *path == self.root => return Err(NavigateError::NotFound),
            At::Folder(path) => At::Folder(path.parent().unwrap_or(path).to_owned()),
            At::Builtin(components) if components.is_empty() => {
                return Err(NavigateError::NotFound);
            }
            At::Builtin(components) => At::Builtin(components[..components.len() - 1].to_vec()),
        };
        self.go(parent)
    }

    fn to_child(&mut self, name: &str) -> Result<(), NavigateError> {
        let child = match &self.at {
            At::Folder(path) => At::Folder(path.join(name)),
            At::Builtin(components) => At::Builtin(
                components
                    .iter()
                    .cloned()
                    .chain([name.to_owned()])
                    .collect(),
            ),
        };
        self.go(child)
    }

    fn has_module(&self) -> bool {
        match &self.module {
            Some(Module::File(path)) => path.is_file(),
            Some(Module::Builtin(..)) => true,
            None => false,
        }
    }

    fn cache_key(&self) -> String {
        match &self.module {
            Some(Module::File(path)) => path.display().to_string(),
            Some(Module::Builtin(file, _)) => format!("{BUILTIN_CHUNK}{file}"),
            None => String::new(),
        }
    }

    fn has_config(&self) -> bool {
        false
    }

    fn config(&self) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::NotFound.into())
    }

    fn loader(&self, lua: &Lua) -> mlua::Result<Function> {
        match &self.module {
            Some(Module::File(path)) => lua
                .load(path.as_path())
                .set_name(format!("@{}", path.display()))
                .into_function(),
            Some(Module::Builtin(file, source)) => lua
                .load(*source)
                .set_name(format!("{BUILTIN_CHUNK}{file}"))
                .into_function(),
            None => Err(mlua::Error::runtime("no module here")),
        }
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
