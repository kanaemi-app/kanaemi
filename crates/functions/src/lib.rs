//! The functions a user writes in Luau to fill placeholders: one file of the
//! functions folder each, named by the file. Luau reaches no file but the
//! modules in that folder, and no program.

mod require;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kanaemi_engine::{Call, Functions};
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Value, VmState};

use crate::require::{BUILTIN_CHUNK, FolderRequirer};

/// The extension of a function's file.
pub const EXTENSION: &str = "luau";

/// The functions the IME has, and the modules they require, by their path:
/// written as files of the functions folder would be, so they work copied
/// there too. A file at the top gives a function of its name.
const BUILTINS: [(&str, &str); 7] = [
    (
        "half-num.luau",
        include_str!("../assets/functions/half-num.luau"),
    ),
    (
        "wide-num.luau",
        include_str!("../assets/functions/wide-num.luau"),
    ),
    (
        "kanji-num.luau",
        include_str!("../assets/functions/kanji-num.luau"),
    ),
    ("kanji.luau", include_str!("../assets/functions/kanji.luau")),
    ("daiji.luau", include_str!("../assets/functions/daiji.luau")),
    (
        "grouped-num.luau",
        include_str!("../assets/functions/grouped-num.luau"),
    ),
    (
        "lib/number.luau",
        include_str!("../assets/functions/lib/number.luau"),
    ),
];

/// The `kanaemi` table every function has, by key.
const KANAEMI: [(&str, &str); 1] = [("number", include_str!("../assets/kanaemi/number.luau"))];

/// Long enough for any function that writes a word, short enough that typing
/// does not stall on one that never ends.
const TIME_LIMIT: Duration = Duration::from_millis(50);
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Bytes of printed text kept between two reads of them: plenty for
/// debugging, and no way around the memory limit.
const PRINT_LIMIT: usize = 64 * 1024;
/// Kept in place of what was printed past the limit.
const DROPPED: &str = "(more was printed and dropped)";

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FunctionError {
    #[error("{} could not be read: {message}", path.display())]
    Unreadable { path: PathBuf, message: String },
    #[error("{} is not named as a function may be", path.display())]
    Name { path: PathBuf },
    #[error("{} could not be run: {message}", path.display())]
    Unrunnable { path: PathBuf, message: String },
    #[error("function {name} failed: {message}")]
    Failed { name: String, message: String },
}

/// A line a function printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Printed {
    /// The function running, or the file being read, when it printed.
    pub function: String,
    pub text: String,
}

/// What the state and the callbacks in it share.
#[derive(Default)]
struct Shared {
    deadline: Cell<Option<Instant>>,
    /// Whether the last run was stopped at the time limit.
    timed_out: Cell<bool>,
    running: RefCell<String>,
    printed: RefCell<Vec<Printed>>,
    printed_bytes: Cell<usize>,
}

impl Shared {
    /// Runs `run` as `name`, stopped once it passes the time limit.
    fn run<T>(&self, name: &str, run: impl FnOnce() -> T) -> T {
        name.clone_into(&mut self.running.borrow_mut());
        self.timed_out.set(false);
        self.deadline.set(Some(Instant::now() + TIME_LIMIT));
        let result = run();
        self.deadline.set(None);
        result
    }

    /// Keeps the line `parts` make, joined with tabs, unless it would pass the
    /// limit: then one line says it was dropped, and the parts past the limit
    /// are never copied.
    fn keep_printed(
        &self,
        parts: impl IntoIterator<Item = mlua::Result<mlua::LuaString>>,
    ) -> mlua::Result<()> {
        let room = PRINT_LIMIT.saturating_sub(self.printed_bytes.get());
        if room == 0 {
            return Ok(());
        }
        // A line costs its function's name too, so empty ones add up.
        let cost = self.running.borrow().len() + 1;
        let mut line = Vec::new();
        let mut fits = cost <= room;
        for (i, part) in parts.into_iter().enumerate() {
            if !fits {
                break;
            }
            let part = part?;
            let part = part.as_bytes();
            let tab = usize::from(i > 0);
            fits = line.len() + tab + part.len() + cost <= room;
            if fits {
                line.extend(std::iter::repeat_n(b'\t', tab));
                line.extend_from_slice(&part);
            }
        }
        let (text, bytes) = match fits {
            true => (
                String::from_utf8_lossy(&line).into_owned(),
                self.printed_bytes.get() + line.len() + cost,
            ),
            false => (DROPPED.to_owned(), PRINT_LIMIT),
        };
        self.printed_bytes.set(bytes);
        self.printed.borrow_mut().push(Printed {
            function: self.running.borrow().clone(),
            text,
        });
        Ok(())
    }
}

/// The functions read from one folder.
pub struct LuauFunctions {
    lua: Lua,
    functions: HashMap<String, Function>,
    /// The names of the functions read from the folder, by name.
    read: Vec<String>,
    shared: Rc<Shared>,
    errors: RefCell<Vec<FunctionError>>,
    /// Functions whose failure is already reported, so one that fails at
    /// every conversion is reported once.
    reported: RefCell<HashSet<String>>,
    /// Functions stopped at the time limit, never run again: each run would
    /// stall typing as long.
    stopped: RefCell<HashSet<String>>,
}

impl LuauFunctions {
    /// Reads every function in `dir`. A missing folder has none; a file that
    /// gives something else is a module, and one that cannot be read or run
    /// is left out and reported by [`Self::take_errors`].
    pub fn open(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let shared = Rc::new(Shared::default());
        let mut errors = Vec::new();
        let mut functions = HashMap::new();
        let mut read = Vec::new();
        let lua = match sandbox(dir, &shared) {
            Ok(lua) => {
                for name in builtin_names() {
                    match builtin(&lua, name, &shared) {
                        Ok(function) => {
                            functions.insert(name.to_owned(), function);
                        }
                        Err(error) => errors.push(FunctionError::Unrunnable {
                            path: PathBuf::from(format!("{name}.{EXTENSION}")),
                            message: error.to_string(),
                        }),
                    }
                }
                // A file of a built-in function's name goes in its place.
                for path in files(dir) {
                    match load(&lua, &path, &shared) {
                        Ok(Some((name, function))) => {
                            read.push(name.clone());
                            functions.insert(name, function);
                        }
                        Ok(None) => {}
                        Err(error) => errors.push(error),
                    }
                }
                lua
            }
            // No code ever runs in this one.
            Err(error) => {
                errors.push(FunctionError::Unreadable {
                    path: dir.to_owned(),
                    message: error.to_string(),
                });
                Lua::new()
            }
        };
        Self {
            lua,
            functions,
            read,
            shared,
            errors: RefCell::new(errors),
            reported: RefCell::new(HashSet::new()),
            stopped: RefCell::new(HashSet::new()),
        }
    }

    /// The names of the functions read from the folder, by name; the
    /// built-in ones are not among them.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.read.iter().map(String::as_str)
    }

    /// What went wrong since the last call: files that could not be read or
    /// run, and the first failure of each function.
    pub fn take_errors(&self) -> Vec<FunctionError> {
        self.errors.take()
    }

    /// What was printed since the last call, oldest first.
    pub fn take_printed(&self) -> Vec<Printed> {
        self.shared.printed_bytes.set(0);
        self.shared.printed.take()
    }

    fn fail(&self, name: &str, message: String) {
        if self.reported.borrow_mut().insert(name.to_owned()) {
            self.errors.borrow_mut().push(FunctionError::Failed {
                name: name.to_owned(),
                message,
            });
        }
    }
}

impl Functions for LuauFunctions {
    fn has(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    fn call(&self, call: &Call) -> Option<String> {
        let function = self.functions.get(call.name)?;
        if self.stopped.borrow().contains(call.name) {
            return None;
        }
        let result = self.shared.run(call.name, || {
            function.call::<Value>((call.source, call.argument))
        });
        let text = match result {
            Ok(Value::Nil) => return None,
            Ok(Value::String(text)) => text.to_str().map(|text| text.to_owned()),
            Ok(other) => Err(mlua::Error::runtime(format!(
                "gave a {} instead of a string",
                other.type_name()
            ))),
            Err(error) => Err(error),
        };
        // Leftovers of a failed call go now, not at some later conversion.
        if text.is_err() {
            let _ = self.lua.gc_collect();
        }
        if self.shared.timed_out.get() {
            self.stopped.borrow_mut().insert(call.name.to_owned());
        }
        text.map_err(|error| self.fail(call.name, error.to_string()))
            .ok()
    }
}

/// A Luau state whose built-in libraries cannot be changed, whose `require`
/// finds modules in `dir` only, and whose functions stop at the limits.
fn sandbox(dir: &Path, shared: &Rc<Shared>) -> mlua::Result<Lua> {
    let lua = Lua::new_with(StdLib::ALL_SAFE, LuaOptions::new())?;
    let globals = lua.globals();
    globals.raw_set(
        "require",
        lua.create_require_function(FolderRequirer::new(dir, &BUILTINS))?,
    )?;
    globals.raw_set("print", print(&lua, shared)?)?;
    globals.raw_set("kanaemi", kanaemi(&lua, shared)?)?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos() ^ since.as_secs() as u32);
    lua.load(format!("math.randomseed({seed})")).exec()?;
    lua.sandbox(true)?;
    lua.set_memory_limit(MEMORY_LIMIT)?;
    lua.set_interrupt({
        let shared = shared.clone();
        move |_| match shared.deadline.get() {
            Some(deadline) if Instant::now() > deadline => {
                shared.timed_out.set(true);
                Err(mlua::Error::runtime(
                    "ran too long; not run again until read again",
                ))
            }
            _ => Ok(VmState::Continue),
        }
    });
    Ok(lua)
}

/// The `kanaemi` table, read-only like the standard libraries.
fn kanaemi(lua: &Lua, shared: &Shared) -> mlua::Result<mlua::Table> {
    let kanaemi = lua.create_table()?;
    for (key, source) in KANAEMI {
        let table = shared.run(key, || {
            lua.load(source)
                .set_name(format!("=kanaemi.{key}"))
                .eval::<mlua::Table>()
        })?;
        table.set_readonly(true);
        kanaemi.raw_set(key, table)?;
    }
    kanaemi.set_readonly(true);
    Ok(kanaemi)
}

/// `print` that keeps each line, with the function that printed it, as Luau's
/// own would write it.
fn print(lua: &Lua, shared: &Rc<Shared>) -> mlua::Result<Function> {
    let tostring: Function = lua.globals().raw_get("tostring")?;
    let shared = shared.clone();
    lua.create_function(move |_, values: MultiValue| {
        shared.keep_printed(
            values
                .into_iter()
                .map(|value| tostring.call::<mlua::LuaString>(value)),
        )
    })
}

/// The function files in `dir`, by name.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == EXTENSION))
        .collect();
    files.sort();
    files
}

/// The names of the built-in functions: their files at the top.
fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTINS
        .iter()
        .filter(|(file, _)| !file.contains('/'))
        .filter_map(|(file, _)| file.strip_suffix(&format!(".{EXTENSION}")))
}

/// Through `require`, as a file of the folder is, so the modules it shares
/// run once.
fn builtin(lua: &Lua, name: &str, shared: &Shared) -> mlua::Result<Function> {
    shared.run(name, || {
        lua.load("return require(...)")
            .set_name(format!("{BUILTIN_CHUNK}{name}.{EXTENSION}"))
            .call::<Function>(format!("./{name}"))
    })
}

fn load(
    lua: &Lua,
    path: &Path,
    shared: &Shared,
) -> Result<Option<(String, Function)>, FunctionError> {
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|name| is_name(name))
        .ok_or_else(|| FunctionError::Name {
            path: path.to_owned(),
        })?;
    // Through `require`, so a file other functions require too is run once.
    let value = shared
        .run(name, || {
            lua.load("return require(...)")
                .set_name(format!("@{}", path.display()))
                .call::<Value>(format!("./{name}"))
        })
        .map_err(|error| FunctionError::Unrunnable {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
    match value {
        Value::Function(function) => Ok(Some((name.to_owned(), function))),
        // A module the functions require.
        _ => Ok(None),
    }
}

/// Whether a placeholder can name the function.
fn is_name(name: &str) -> bool {
    !name.is_empty() && !name.contains([':', ' ', '{', '}', '\\'])
}

#[cfg(test)]
mod tests;
