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

#[test]
fn a_function_that_ran_too_long_is_not_run_again() {
    let functions = open(&[("loop.luau", "return function() while true do end end")]);
    assert_eq!(call(&functions, "loop", "", None), None);
    let started = Instant::now();
    assert_eq!(call(&functions, "loop", "", None), None);
    assert!(started.elapsed() < TIME_LIMIT / 2);
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
            "half.luau",
            "return function(source, arg)\n  return (source:gsub(utf8.charpattern, function(c)\n    local code = utf8.codepoint(c)\n    if code >= 0xFF10 and code <= 0xFF19 then\n      return string.char(code - 0xFF10 + 0x30)\n    end\n  end))\nend",
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
        call(&functions, "half", "１２こ", None).as_deref(),
        Some("12こ")
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
