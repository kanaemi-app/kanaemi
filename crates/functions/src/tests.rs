use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A folder of its own for each test, emptied first.
fn folder(files: &[(&str, &str)]) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kanaemi-functions-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (name, source) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    dir
}

fn open(files: &[(&str, &str)]) -> LuauFunctions {
    LuauFunctions::open(folder(files))
}

fn call(
    functions: &LuauFunctions,
    name: &str,
    source: &str,
    argument: Option<&str>,
) -> Option<String> {
    functions.call(&Call {
        name,
        source,
        argument,
    })
}

#[test]
fn a_file_gives_the_function_its_name_names() {
    let functions = open(&[(
        "wrap.luau",
        "return function(source, arg) return (arg or '[') .. source end",
    )]);
    assert!(functions.has("wrap"));
    assert_eq!(
        call(&functions, "wrap", "よみ", None).as_deref(),
        Some("[よみ")
    );
    assert_eq!(
        call(&functions, "wrap", "よみ", Some("<")).as_deref(),
        Some("<よみ")
    );
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn only_luau_files_are_read() {
    let functions = open(&[("a.lua", "return function() return 'a' end"), ("b.txt", "")]);
    assert_eq!(functions.names().count(), 0);
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn a_missing_folder_has_no_functions() {
    let functions = LuauFunctions::open(std::env::temp_dir().join("kanaemi-functions-missing"));
    assert_eq!(functions.names().count(), 0);
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn files_that_cannot_be_run_or_named_are_reported_and_left_out() {
    let functions = open(&[
        ("broken.luau", "return function("),
        ("a b.luau", "return function() return '' end"),
        ("ok.luau", "return function() return 'ok' end"),
    ]);
    assert_eq!(functions.names().collect::<Vec<_>>(), ["ok"]);
    let errors = functions.take_errors();
    assert!(
        matches!(errors[0], FunctionError::Name { .. }),
        "{errors:?}"
    );
    assert!(
        matches!(errors[1], FunctionError::Unrunnable { .. }),
        "{errors:?}"
    );
}

#[test]
fn a_module_in_a_folder_inside_is_required_but_is_no_function() {
    let functions = open(&[
        (
            "lib/util.luau",
            "return function(s) return '<' .. s .. '>' end",
        ),
        (
            "wrap.luau",
            "local util = require('./lib/util')\nreturn function(s) return util(s) end",
        ),
    ]);
    assert_eq!(functions.names().collect::<Vec<_>>(), ["wrap"]);
    assert_eq!(
        call(&functions, "wrap", "よみ", None).as_deref(),
        Some("<よみ>")
    );
}

#[test]
fn a_module_beside_the_functions_is_run_once_however_often_required() {
    let functions = open(&[
        ("lib/log.luau", "return { loads = 0 }"),
        (
            "state.luau",
            "local log = require('./lib/log')\nlog.loads += 1\nreturn {}",
        ),
        (
            "seen.luau",
            "local state = require('./state')\nlocal log = require('./lib/log')\nreturn function() return tostring(log.loads) end",
        ),
    ]);
    assert_eq!(call(&functions, "seen", "", None).as_deref(), Some("1"));
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn nothing_outside_the_functions_folder_is_required() {
    let dir = folder(&[(
        "out.luau",
        "return function() return require('../outside') end",
    )]);
    let outside = dir.with_file_name(format!(
        "{}-outside",
        dir.file_name().unwrap().to_str().unwrap()
    ));
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("outside.luau"), "return 'secret'").unwrap();
    fs::write(
        dir.join("far.luau"),
        format!(
            "return function() return require('../{}/outside') end",
            outside.file_name().unwrap().to_str().unwrap()
        ),
    )
    .unwrap();
    fs::write(dir.with_file_name("outside.luau"), "return 'secret'").unwrap();
    let functions = LuauFunctions::open(&dir);
    assert_eq!(call(&functions, "out", "", None), None);
    assert_eq!(call(&functions, "far", "", None), None);
    assert_eq!(functions.take_errors().len(), 2);
}

#[test]
fn the_functions_folder_itself_is_no_module_beside_it() {
    let dir = folder(&[(
        "here.luau",
        "return function() return tostring(require('./')) end",
    )]);
    let beside = dir.with_extension(EXTENSION);
    fs::write(&beside, "return 'outside'").unwrap();
    let functions = LuauFunctions::open(&dir);
    let required = call(&functions, "here", "", None);
    fs::remove_file(&beside).unwrap();
    assert_eq!(required, None);
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_functions_folder_is_followed() {
    let dir = folder(&[(
        "linked.luau",
        "return function() return require('./shared/module') end",
    )]);
    let shared = folder(&[("module.luau", "return 'shared'")]);
    std::os::unix::fs::symlink(&shared, dir.join("shared")).unwrap();
    let functions = LuauFunctions::open(&dir);
    assert_eq!(
        call(&functions, "linked", "", None).as_deref(),
        Some("shared")
    );
}

#[test]
fn what_a_function_prints_is_kept_with_its_name() {
    let functions = open(&[(
        "loud.luau",
        "print('loading')\nreturn function(s) print('got', s, 1, nil) return s end",
    )]);
    assert_eq!(
        call(&functions, "loud", "よみ", None).as_deref(),
        Some("よみ")
    );
    let printed = functions.take_printed();
    assert_eq!(
        printed,
        [
            Printed {
                function: "loud".to_owned(),
                text: "loading".to_owned()
            },
            Printed {
                function: "loud".to_owned(),
                text: "got\tよみ\t1\tnil".to_owned()
            },
        ]
    );
    assert_eq!(functions.take_printed(), []);
}

#[test]
fn catching_errors_does_not_get_past_the_time_limit() {
    let functions = open(&[(
        "stubborn.luau",
        "return function() while true do pcall(function() while true do end end) end end",
    )]);
    let started = Instant::now();
    assert_eq!(call(&functions, "stubborn", "", None), None);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn a_file_that_gives_no_function_is_a_module_the_functions_require() {
    let functions = open(&[
        (
            "util.luau",
            "return { wrap = function(s) return '<' .. s .. '>' end }",
        ),
        (
            "wrap.luau",
            "local util = require('./util')\nreturn function(s) return util.wrap(s) end",
        ),
    ]);
    assert_eq!(functions.names().collect::<Vec<_>>(), ["wrap"]);
    assert_eq!(
        call(&functions, "wrap", "よみ", None).as_deref(),
        Some("<よみ>")
    );
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn nil_gives_no_text_and_is_no_failure() {
    let functions = open(&[("none.luau", "return function() return nil end")]);
    assert_eq!(call(&functions, "none", "", None), None);
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn an_error_or_a_value_other_than_a_string_fails_and_is_reported_once() {
    let functions = open(&[
        ("boom.luau", "return function() error('boom') end"),
        ("number.luau", "return function() return 1 end"),
    ]);
    for _ in 0..2 {
        assert_eq!(call(&functions, "boom", "", None), None);
        assert_eq!(call(&functions, "number", "", None), None);
    }
    let errors = functions.take_errors();
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn a_function_that_never_ends_is_stopped() {
    let functions = open(&[("loop.luau", "return function() while true do end end")]);
    let started = Instant::now();
    assert_eq!(call(&functions, "loop", "", None), None);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(functions.take_errors().len(), 1);
}

#[test]
fn a_file_that_never_ends_is_stopped() {
    let functions = open(&[("loop.luau", "while true do end")]);
    assert_eq!(functions.names().count(), 0);
    assert_eq!(functions.take_errors().len(), 1);
}

/// A function that counts its calls in `lib/calls`, and runs forever on
/// those `stalls` says, as a moment the machine stops it would look.
fn stalling(stalls: &str) -> LuauFunctions {
    open(&[
        ("lib/calls.luau", "return { count = 0 }"),
        (
            "stall.luau",
            &format!(
                "local calls = require('./lib/calls')\nreturn function()\n  calls.count += 1\n  if ({stalls})(calls.count) then while true do end end\n  return 'done'\nend"
            ),
        ),
        (
            "calls.luau",
            "local calls = require('./lib/calls')\nreturn function() return tostring(calls.count) end",
        ),
    ])
}

#[test]
fn a_function_that_ran_too_long_once_is_run_again() {
    let functions = stalling("function(n) return n == 1 end");
    assert_eq!(call(&functions, "stall", "", None), None);
    assert_eq!(call(&functions, "stall", "", None).as_deref(), Some("done"));
}

#[test]
fn a_function_that_ran_too_long_now_and_then_is_never_stopped() {
    let functions = stalling(&format!("function(n) return n % {TIMES_TOO_LONG} ~= 0 end"));
    for _ in 0..3 {
        for _ in 1..TIMES_TOO_LONG {
            assert_eq!(call(&functions, "stall", "", None), None);
        }
        assert_eq!(call(&functions, "stall", "", None).as_deref(), Some("done"));
    }
}

#[test]
fn a_function_that_ran_too_long_time_after_time_is_not_run_again() {
    let functions = stalling("function() return true end");
    for _ in 0..TIMES_TOO_LONG + 2 {
        assert_eq!(call(&functions, "stall", "", None), None);
    }
    assert_eq!(
        call(&functions, "calls", "", None),
        Some(TIMES_TOO_LONG.to_string())
    );
    let errors = functions.take_errors();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        errors[1].to_string().contains("not run again"),
        "{errors:?}"
    );
}

#[test]
fn a_file_that_ran_too_long_once_is_read_again() {
    let functions = open(&[
        ("lib/once.luau", "return { stalled = false }"),
        (
            "late.luau",
            "local once = require('./lib/once')\nif not once.stalled then once.stalled = true while true do end end\nreturn function() return 'late' end",
        ),
    ]);
    assert_eq!(call(&functions, "late", "", None).as_deref(), Some("late"));
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn what_is_printed_is_kept_only_up_to_a_limit() {
    let functions = open(&[(
        "flood.luau",
        "return function() local s = string.rep('x', 1024 * 1024) for i = 1, 100 do print(s) end return '' end",
    )]);
    call(&functions, "flood", "", None);
    let kept: usize = functions.take_printed().iter().map(|p| p.text.len()).sum();
    assert!(kept <= PRINT_LIMIT + 1024, "{kept}");
}

#[test]
fn a_line_printed_past_the_limit_is_dropped_without_being_copied_whole() {
    let functions = open(&[(
        "wide.luau",
        "return function() local s = string.rep('x', 1024 * 1024) print(table.unpack(table.create(1000, s))) return '' end",
    )]);
    assert_eq!(call(&functions, "wide", "", None).as_deref(), Some(""));
    let printed = functions.take_printed();
    assert_eq!(printed.len(), 1);
    assert_eq!(printed[0].text, DROPPED);
}

#[test]
fn empty_lines_printed_count_toward_the_limit_too() {
    let functions = open(&[(
        "blank.luau",
        "return function() for i = 1, 100000 do print() end return '' end",
    )]);
    call(&functions, "blank", "", None);
    assert!(functions.take_printed().len() < 100000 / 4);
}

#[test]
fn a_function_that_takes_too_much_memory_is_stopped() {
    let functions = open(&[(
        "grow.luau",
        "return function() local t = {} for i = 1, 1e9 do t[i] = string.rep('x', 1024) end end",
    )]);
    assert_eq!(call(&functions, "grow", "", None), None);
    assert_eq!(functions.take_errors().len(), 1);
}

#[test]
fn a_function_still_works_after_another_failed() {
    let functions = open(&[
        ("boom.luau", "return function() error('boom') end"),
        ("ok.luau", "return function(s) return s end"),
    ]);
    assert_eq!(call(&functions, "boom", "", None), None);
    assert_eq!(
        call(&functions, "ok", "よみ", None).as_deref(),
        Some("よみ")
    );
}

#[test]
fn the_libraries_that_write_text_and_tell_the_time_are_there() {
    let functions = open(&[(
        "all.luau",
        "return function(s) \
         return string.upper('a') .. table.concat({'b'}) .. math.floor(1.5) \
         .. utf8.char(12354) .. bit32.band(3, 1) .. type(os.date('%Y')) .. type(os.time()) \
         .. type(os.clock()) .. tostring(#s) end",
    )]);
    assert_eq!(
        call(&functions, "all", "xy", None).as_deref(),
        Some("Ab1あ1stringnumbernumber2")
    );
}

#[test]
fn nothing_that_reaches_files_or_programs_is_there() {
    let functions = open(&[(
        "reach.luau",
        "return function() \
         return tostring(io) .. tostring(os.execute) .. tostring(os.getenv) \
         .. tostring(os.remove) .. tostring(package) end",
    )]);
    assert_eq!(
        call(&functions, "reach", "", None).as_deref(),
        Some("nilnilnilnilnil")
    );
}

#[test]
fn functions_share_a_module_they_require() {
    let functions = open(&[
        ("lib/store.luau", "return { value = 'first' }"),
        (
            "set.luau",
            "local store = require('./lib/store')\nreturn function(s) store.value = s return s end",
        ),
        (
            "get.luau",
            "local store = require('./lib/store')\nreturn function() return store.value end",
        ),
    ]);
    assert_eq!(call(&functions, "get", "", None).as_deref(), Some("first"));
    call(&functions, "set", "second", None);
    assert_eq!(call(&functions, "get", "", None).as_deref(), Some("second"));
}

#[test]
fn the_built_in_libraries_cannot_be_changed() {
    let functions = open(&[
        (
            "break.luau",
            "return function() string.upper = nil return '' end",
        ),
        (
            "upper.luau",
            "return function(s) return string.upper(s) end",
        ),
    ]);
    assert_eq!(call(&functions, "break", "", None), None);
    assert_eq!(call(&functions, "upper", "a", None).as_deref(), Some("A"));
}

#[test]
fn random_numbers_differ_between_states() {
    let source = "return function() return tostring(math.random(1, 1e9)) end";
    let first = call(&open(&[("r.luau", source)]), "r", "", None);
    std::thread::sleep(Duration::from_millis(2));
    let second = call(&open(&[("r.luau", source)]), "r", "", None);
    assert_ne!(first, second);
}

#[test]
fn the_examples_of_the_specification_work() {
    let functions = open(&[
        (
            "dai.luau",
            "return function(source, arg)\n  local digits = kanaemi.number.digits(source)\n  local kanji = digits and kanaemi.number.counted(digits, false)\n  return kanji and (\"第\" .. kanji)\nend",
        ),
        (
            "date.luau",
            "return function(source, arg)\n  return os.date(arg or \"%Y-%m-%d\")\nend",
        ),
        ("lib/counter.luau", "return { count = 0 }"),
        (
            "count.luau",
            "local counter = require(\"./lib/counter\")\nreturn function(source, arg)\n  counter.count += 1\n  return tostring(counter.count)\nend",
        ),
    ]);
    assert_eq!(
        call(&functions, "dai", "１２", None).as_deref(),
        Some("第十二")
    );
    assert_eq!(
        call(&functions, "date", "", Some("%%")).as_deref(),
        Some("%")
    );
    assert_eq!(
        call(&functions, "date", "", None).map(|d| d.len()),
        Some(10)
    );
    assert_eq!(call(&functions, "count", "", None).as_deref(), Some("1"));
    assert_eq!(call(&functions, "count", "", None).as_deref(), Some("2"));
    assert_eq!(functions.take_errors(), []);
}

/// What a Luau test file has besides the functions: `test(name, body)` runs
/// `body` and keeps how it failed, and `eq(actual, expected)` fails unless
/// the two are equal.
const PRELUDE: &str = r#"
local failures = {}
local count = 0
local function test(name, body)
	count += 1
	local ok, message = pcall(body)
	if not ok then
		table.insert(failures, `{name}: {message}`)
	end
end
local function eq(actual, expected)
	if actual ~= expected then
		error(`expected {expected}, got {actual}`, 2)
	end
end
return test, eq, failures, function()
	return count
end
"#;

/// Runs each Luau test file in `tests/luau`, in the sandbox the functions run
/// in, with the built-in functions by name in `functions`.
#[test]
fn the_luau_tests_pass() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("luau");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == EXTENSION))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    let mut failures = Vec::new();
    for path in paths {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let shared = Rc::new(Shared::default());
        let lua = sandbox(&dir, &shared).unwrap();
        let functions = lua.create_table().unwrap();
        for name in builtin_names() {
            let function = builtin(&lua, name, &shared).unwrap();
            functions.raw_set(name, function).unwrap();
        }
        let (test, eq, found, count): (Function, Function, mlua::Table, Function) =
            lua.load(PRELUDE).eval().unwrap();
        let environment = lua.create_table().unwrap();
        environment.raw_set("functions", functions).unwrap();
        environment.raw_set("test", test).unwrap();
        environment.raw_set("eq", eq).unwrap();
        let globals = lua.create_table().unwrap();
        globals.raw_set("__index", lua.globals()).unwrap();
        environment.set_metatable(Some(globals)).unwrap();
        let source = fs::read_to_string(&path).unwrap();
        if let Err(error) = lua
            .load(source)
            // As one of the built-in files, to require their modules.
            .set_name(format!("{BUILTIN_CHUNK}{file}"))
            .set_environment(environment)
            .exec()
        {
            failures.push(format!("{file}: {error}"));
        }
        for message in found.sequence_values::<String>() {
            failures.push(format!("{file}: {}", message.unwrap()));
        }
        assert!(count.call::<usize>(()).unwrap() > 0, "{file} has no test");
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn a_file_of_a_builtin_s_name_goes_in_its_place() {
    let functions = open(&[("kanji.luau", "return function() return 'mine' end")]);
    assert_eq!(
        call(&functions, "kanji", "1", None).as_deref(),
        Some("mine")
    );
    assert_eq!(call(&functions, "daiji", "1", None).as_deref(), Some("壱"));
}

#[test]
fn every_function_has_the_kanaemi_helpers_and_cannot_change_them() {
    let functions = open(&[
        (
            "counted.luau",
            "return function(s) return kanaemi.number.counted(kanaemi.number.digits(s), false) end",
        ),
        (
            "break.luau",
            "return function() kanaemi.number.counted = nil return '' end",
        ),
    ]);
    assert_eq!(call(&functions, "break", "", None), None);
    assert_eq!(
        call(&functions, "counted", "２０２６", None).as_deref(),
        Some("二千二十六")
    );
}

#[test]
fn the_builtin_files_work_copied_into_the_functions_folder() {
    let functions = open(&BUILTINS);
    assert_eq!(
        functions.names().collect::<HashSet<_>>(),
        builtin_names().collect::<HashSet<_>>()
    );
    let builtins = open(&[]);
    for name in builtin_names() {
        for source in ["0", "007", "12", "２０２６", "100010", "1111"] {
            let copied = call(&functions, name, source, None);
            assert!(copied.is_some() || call(&builtins, name, source, None).is_none());
            // A function of another value each time is only to give one.
            if call(&builtins, name, source, None) == call(&builtins, name, source, None) {
                assert_eq!(
                    copied,
                    call(&builtins, name, source, None),
                    "{name} {source}"
                );
            }
        }
    }
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn a_function_left_out_of_the_folder_gives_way_to_the_builtin_of_its_name() {
    let dir = folder(&[
        ("kanji.luau", "return function() return 'mine' end"),
        ("dai.luau", "return function() return 'dai' end"),
        ("broken.luau", "return function("),
    ]);
    let without = Without {
        builtins: Vec::new(),
        files: vec!["kanji".to_owned(), "dai".to_owned(), "broken".to_owned()],
    };
    let functions = LuauFunctions::open_without(&dir, &without);
    assert_eq!(
        call(&functions, "kanji", "12", None).as_deref(),
        Some("十二")
    );
    assert!(!functions.has("dai"));
    assert_eq!(functions.names().count(), 0);
    assert_eq!(functions.take_errors(), []);
}

#[test]
fn a_builtin_left_out_is_no_function() {
    let without = Without {
        builtins: vec!["kanji".to_owned()],
        files: Vec::new(),
    };
    let functions = LuauFunctions::open_without(folder(&[]), &without);
    assert!(!functions.has("kanji"));
    assert!(functions.has("daiji"));
    let mine = LuauFunctions::open_without(
        folder(&[("kanji.luau", "return function() return 'mine' end")]),
        &without,
    );
    assert_eq!(call(&mine, "kanji", "12", None).as_deref(), Some("mine"));
}

#[test]
fn the_builtins_are_shown_by_their_sources() {
    let shown: Vec<&str> = builtin_sources().map(|(name, _)| name).collect();
    assert_eq!(
        shown.iter().copied().collect::<HashSet<_>>(),
        builtin_names().collect::<HashSet<_>>()
    );
    assert!(builtin_sources().all(|(_, source)| source.contains("return function")));
}
