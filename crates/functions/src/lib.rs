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
const BUILTINS: [(&str, &str); 16] = [
    (
        "wareki.luau",
        include_str!("../assets/functions/wareki.luau"),
    ),
    (
        "seireki.luau",
        include_str!("../assets/functions/seireki.luau"),
    ),
    ("eto.luau", include_str!("../assets/functions/eto.luau")),
    (
        "choice.luau",
        include_str!("../assets/functions/choice.luau"),
    ),
    ("date.luau", include_str!("../assets/functions/date.luau")),
    ("uuid.luau", include_str!("../assets/functions/uuid.luau")),
    ("ulid.luau", include_str!("../assets/functions/ulid.luau")),
    (
        "random.luau",
        include_str!("../assets/functions/random.luau"),
    ),
    (
        "lib/date.luau",
        include_str!("../assets/functions/lib/date.luau"),
    ),
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
/// Times in a row a function or a file may run past the time limit before it
/// is given up: a moment the machine stalls passes, a loop does not.
const TIMES_TOO_LONG: u32 = 3;
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

    /// Runs `read` as `name` like [`Self::run`], again while it is stopped at
    /// the time limit, up to [`TIMES_TOO_LONG`] times: a file is read only
    /// once, so a stall then would leave it out until it is read again.
    fn read<T>(&self, name: &str, mut read: impl FnMut() -> mlua::Result<T>) -> mlua::Result<T> {
        let mut times = 1;
        loop {
            let result = self.run(name, &mut read);
            if result.is_ok() || !self.timed_out.get() || times == TIMES_TOO_LONG {
                return result;
            }
            times += 1;
        }
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
    /// How many times in a row each function was stopped at the time limit.
    /// One stopped [`TIMES_TOO_LONG`] times is never run again: each run
    /// would stall typing as long.
    too_long: RefCell<HashMap<String, u32>>,
}

/// The functions not to use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Without {
    /// Built-in functions, by name.
    pub builtins: Vec<String>,
    /// Functions of the folder, by the name of their file without its
    /// extension. A built-in function of the name is used in its place.
    pub files: Vec<String>,
}

impl LuauFunctions {
    /// Reads every function in `dir`. A missing folder has none; a file that
    /// gives something else is a module, and one that cannot be read or run
    /// is left out and reported by [`Self::take_errors`].
    pub fn open(dir: impl AsRef<Path>) -> Self {
        Self::open_without(dir, &Without::default())
    }

    /// Like [`Self::open`], leaving out the functions of `without`. A file
    /// left out is not run for its function, and none of its errors are
    /// reported; another file may still require it as a module.
    pub fn open_without(dir: impl AsRef<Path>, without: &Without) -> Self {
        let dir = dir.as_ref();
        let left_out = |names: &[String], name: &str| names.iter().any(|n| n == name);
        let shared = Rc::new(Shared::default());
        let mut errors = Vec::new();
        let mut functions = HashMap::new();
        let mut read = Vec::new();
        let lua = match sandbox(dir, &shared) {
            Ok(lua) => {
                for name in builtin_names().filter(|n| !left_out(&without.builtins, n)) {
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
                let named =
                    |path: &PathBuf| path.file_stem().and_then(|s| s.to_str()).map(str::to_owned);
                for path in files(dir) {
                    if named(&path).is_some_and(|name| left_out(&without.files, &name)) {
                        continue;
                    }
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
            too_long: RefCell::new(HashMap::new()),
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

    /// How many times in a row `name` has run too long, counting the run
    /// just ended.
    fn ran_too_long(&self, name: &str) -> u32 {
        let mut too_long = self.too_long.borrow_mut();
        if !self.shared.timed_out.get() {
            too_long.remove(name);
            return 0;
        }
        let times = too_long.entry(name.to_owned()).or_default();
        *times += 1;
        *times
    }
}

impl Functions for LuauFunctions {
    fn has(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    fn call(&self, call: &Call) -> Option<String> {
        let function = self.functions.get(call.name)?;
        if self.too_long.borrow().get(call.name) == Some(&TIMES_TOO_LONG) {
            return None;
        }
        let result = self.shared.run(call.name, || {
            function.call::<Value>((call.source, call.argument))
        });
        if self.ran_too_long(call.name) == TIMES_TOO_LONG {
            // Reported even after its first failure was: it is never run again.
            self.errors.borrow_mut().push(FunctionError::Failed {
                name: call.name.to_owned(),
                message: format!(
                    "ran too long {TIMES_TOO_LONG} times in a row; not run again until read again"
                ),
            });
            let _ = self.lua.gc_collect();
            return None;
        }
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
                Err(mlua::Error::runtime("ran too long"))
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
    kanaemi.raw_set("random", random(lua)?)?;
    kanaemi.raw_set("time", time(lua)?)?;
    kanaemi.set_readonly(true);
    Ok(kanaemi)
}

/// The most bytes `kanaemi.random.bytes` gives at once: more than any ID
/// needs, and no way around the memory limit.
const RANDOM_BYTES_LIMIT: usize = 1024;

/// `kanaemi.random`: random bytes from the operating system, fit for what must
/// not repeat or be guessed, as `math.random` is not.
fn random(lua: &Lua) -> mlua::Result<mlua::Table> {
    let random = lua.create_table()?;
    let bytes = lua.create_function(|lua, count: i64| {
        let count = usize::try_from(count)
            .ok()
            .filter(|&count| count <= RANDOM_BYTES_LIMIT)
            .ok_or_else(|| {
                mlua::Error::runtime(format!("asks {count} bytes, not 0 to {RANDOM_BYTES_LIMIT}"))
            })?;
        let mut bytes = vec![0; count];
        getrandom::fill(&mut bytes).map_err(mlua::Error::runtime)?;
        lua.create_string(bytes)
    })?;
    random.raw_set("bytes", bytes)?;
    random.set_readonly(true);
    Ok(random)
}

/// `kanaemi.time`: the clock to the millisecond, as Luau's `os.time` tells
/// only whole seconds.
fn time(lua: &Lua) -> mlua::Result<mlua::Table> {
    let time = lua.create_table()?;
    let milliseconds = lua.create_function(|_, ()| {
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(mlua::Error::runtime)?;
        // Exact in a Luau number for some 285,000 years.
        Ok(since.as_millis() as f64)
    })?;
    time.raw_set("milliseconds", milliseconds)?;
    time.set_readonly(true);
    Ok(time)
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

/// The built-in functions by name, with the source of each, to show what they
/// do.
pub fn builtin_sources() -> impl Iterator<Item = (&'static str, &'static str)> {
    builtin_names().filter_map(|name| {
        let file = format!("{name}.{EXTENSION}");
        BUILTINS
            .iter()
            .find(|(path, _)| *path == file)
            .map(|&(_, source)| (name, source))
    })
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
    shared.read(name, || {
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
        .read(name, || {
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
