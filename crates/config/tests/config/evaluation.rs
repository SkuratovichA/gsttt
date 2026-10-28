use std::time::{Duration, Instant};

use gsttt_config::fingerprint;
use gsttt_core::Setup;
use serde_json::json;

use crate::tree::{Tree, evaluate_at, failure, fixture};

#[gsttt_scenario::scenario("config", "No config means defaults")]
#[test]
fn a_repository_without_a_config_gets_the_defaults() {
    let setup = Tree::empty().evaluate().unwrap();
    assert_eq!(setup, Setup::default());
    assert!(setup.detect);
    assert!(setup.modules.is_empty() && setup.packs.is_empty() && setup.externals.is_empty());
    assert!(setup.disable.is_empty() && setup.disable_packs.is_empty());
    assert_eq!(setup.max_file_bytes, 2 * 1024 * 1024);
}

#[gsttt_scenario::scenario("config", "An error names its line")]
#[test]
fn an_error_starts_with_the_file_and_the_line() {
    let message = failure("local g = require(\"gsttt\")\nlocal missing = nil\nmissing()\n");
    assert!(message.starts_with(".gsttt/init.lua:3:"), "{message}");
    let syntax = failure("local g = require(\"gsttt\")\n\nlocal = 1\n");
    assert!(syntax.starts_with(".gsttt/init.lua:3:"), "{syntax}");
}

#[gsttt_scenario::scenario("config", "A config fills the setup")]
#[test]
fn a_config_fills_the_setup() {
    let setup = evaluate_at(&fixture("full")).unwrap();
    insta::assert_json_snapshot!(setup);
}

#[gsttt_scenario::scenario("config", "Options must be named")]
#[test]
fn the_options_of_an_external_are_a_table_of_named_values() {
    let declare = |options: &str| {
        format!(
            "local g = require(\"gsttt\")\ng.external {{ name = \"k\", cmd = {{ \"x\" }}, files = {{ \"*.k\" }}, {options} }}\n"
        )
    };
    // Left out, empty and nil are one setup: an empty Lua table is an empty list until the setup says what it means.
    let none = Tree::with_init(&declare("")).evaluate().unwrap();
    assert_eq!(none.externals.first().unwrap().options, json!({}));
    for same in ["options = {}", "options = nil"] {
        assert_eq!(Tree::with_init(&declare(same)).evaluate().unwrap(), none);
    }
    let named = Tree::with_init(&declare("options = { b = 1, a = { 2 } }"))
        .evaluate()
        .unwrap();
    assert_eq!(
        named.externals.first().unwrap().options,
        json!({ "a": [2], "b": 1 })
    );

    for (options, found) in [
        ("options = { 1, 2 }", "a list"),
        ("options = 3", "a number"),
        ("options = \"fast\"", "a string"),
        ("options = true", "a boolean"),
    ] {
        let message = failure(&declare(options));
        assert!(message.starts_with(".gsttt/init.lua:2:"), "{message}");
        assert!(
            message.contains(&format!(
                "`options` of `k` must be a table of named options, not {found}"
            )),
            "{message}"
        );
    }
}

#[gsttt_scenario::scenario("config", "A misspelt module is named with the known ones")]
#[test]
fn a_misspelt_module_is_named_with_the_known_ones() {
    let message = failure("local g = require(\"gsttt\")\ng.tss {}\n");
    assert!(message.starts_with(".gsttt/init.lua:2:"), "{message}");
    assert!(message.contains("unknown module `tss`"), "{message}");
    assert!(message.contains("ts, python, rust, openapi"), "{message}");
    let disabled = failure("local g = require(\"gsttt\")\ng.disable \"opnapi\"\n");
    assert!(disabled.contains("unknown module `opnapi`"), "{disabled}");
}

#[gsttt_scenario::scenario("config", "A misspelt key is named")]
#[test]
fn a_misspelt_key_is_named() {
    let message = failure("local g = require(\"gsttt\")\ng.setup { exlude = { \"a\" } }\n");
    assert!(message.starts_with(".gsttt/init.lua:2:"), "{message}");
    assert!(message.contains("`exlude`"), "{message}");
    let external =
        failure("local g = require(\"gsttt\")\ng.external { name = \"k\", cmnd = {} }\n");
    assert!(external.contains("`cmnd`"), "{external}");
    let pack = failure(
        "local g = require(\"gsttt\")\ng.pack { name = \"x\", language = \"ts\", call = {} }\n",
    );
    assert!(
        pack.contains("pack `x`") && pack.contains("`call`"),
        "{pack}"
    );
}

#[gsttt_scenario::scenario("config", "Tables become lists and maps")]
#[test]
fn tables_become_lists_and_maps() {
    let setup = Tree::with_init(
        r#"
        local g = require("gsttt")
        g.ts { conditions = { "import", "types" }, paths = { ["@app/*"] = { "src/*" } }, extra = {} }
        "#,
    )
    .evaluate()
    .unwrap();
    assert_eq!(
        setup.options("ts"),
        &json!({
            "conditions": ["import", "types"],
            "paths": { "@app/*": ["src/*"] },
            "extra": [],
        })
    );
}

#[gsttt_scenario::scenario("config", "A pack in a local module")]
#[test]
fn a_pack_in_a_local_module_is_added() {
    let setup = evaluate_at(&fixture("billing")).unwrap();
    let names: Vec<&str> = setup.packs.iter().map(|pack| pack.name.as_str()).collect();
    assert_eq!(names, ["billing"]);
    assert_eq!(
        setup
            .packs
            .iter()
            .map(|pack| pack.calls.len())
            .sum::<usize>(),
        1
    );
}

#[gsttt_scenario::scenario("config", "Requires cannot leave the config directory")]
#[test]
fn a_require_cannot_leave_the_config_directory() {
    let tree = Tree::with_init("require(\"../secrets\")\n");
    tree.write(".gsttt/secrets.lua", "error('the secrets were read')");
    let message = tree.evaluate().unwrap_err().to_string();
    assert!(message.starts_with(".gsttt/init.lua:1:"), "{message}");
    assert!(message.contains("`../secrets`"), "{message}");
    assert!(!message.contains("were read"), "{message}");

    let absolute = failure("require(\"/etc/passwd\")\n");
    assert!(absolute.contains("`/etc/passwd`"), "{absolute}");
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_config_directory_is_refused() {
    let tree = Tree::with_init("require(\"escape\")\n");
    tree.write(".gsttt/lua/other.lua", "return {}");
    tree.write("outside.lua", "error('the file outside was read')");
    std::os::unix::fs::symlink(
        tree.root().join("outside.lua"),
        tree.root().join(".gsttt/lua/escape.lua"),
    )
    .unwrap();
    let message = tree.evaluate().unwrap_err().to_string();
    assert!(message.contains("resolves outside .gsttt/lua"), "{message}");
    assert!(!message.contains("was read"), "{message}");
}

#[gsttt_scenario::scenario("config", "The machine is out of reach")]
#[test]
fn the_machine_is_out_of_reach() {
    let open = failure("io.open(\"/etc/passwd\")\n");
    assert!(open.contains("global 'io'"), "{open}");
    let getenv = failure("os.getenv(\"HOME\")\n");
    assert!(getenv.contains("global 'os'"), "{getenv}");

    let tree = Tree::empty();
    tree.write("canary.lua", "error('the canary was read')");
    let canary = tree.root().join("canary.lua");
    tree.write(".gsttt/init.lua", &format!("dofile({canary:?})\n"));
    let dofile = tree.evaluate().unwrap_err().to_string();
    assert!(dofile.contains("global 'dofile'"), "{dofile}");
    assert!(!dofile.contains("canary"), "{dofile}");
}

#[gsttt_scenario::scenario("config", "Runaway configs stop")]
#[test]
fn runaway_configs_stop_at_their_limits() {
    let forever = failure("while true do end\n");
    assert!(forever.contains("instruction limit"), "{forever}");
    let huge = failure("local text = string.rep(\"x\", 1000000000)\n");
    assert!(huge.contains("memory limit"), "{huge}");
}

#[gsttt_scenario::scenario("config", "A table that expands is refused")]
#[test]
fn a_table_that_expands_is_refused_before_it_is_built() {
    // Each level holds the level below twice: forty levels are 2^40 values in JSON, from a table of a few dozen.
    let message = failure(
        "local t = { \"x\" }\nfor _ = 1, 40 do t = { t, t } end\nrequire(\"gsttt\").setup { exclude = t }\n",
    );
    assert!(message.starts_with(".gsttt/init.lua:3:"), "{message}");
    assert!(
        message.contains("g.setup.exclude[1]") && message.contains("more than 1000000 values"),
        "{message}"
    );
}

/// The message of an `init.lua` that must fail at once. A native call that is not bounded runs for minutes, or until the memory limit, so the time is part of what the scenario proves.
fn failure_at_once(init: &str) -> String {
    let started = Instant::now();
    let message = failure(init);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "took {:?}: {message:.100}",
        started.elapsed()
    );
    message
}

/// A table whose length is two to the fortieth by its border alone: forty-one keys, each twice the one before, all in the hash part of the table.
fn sparse_table() -> String {
    let keys: Vec<String> = (0..=40)
        .map(|power| format!("[{}] = true", 1u64 << power))
        .collect();
    format!("local sparse = {{ {} }}\n", keys.join(", "))
}

#[gsttt_scenario::scenario("config", "Repeating nothing returns at once")]
#[test]
fn repeating_nothing_returns_at_once() {
    let started = Instant::now();
    Tree::with_init(
        "assert(string.rep(\"\", 2 ^ 62) == \"\")\nassert(string.rep(\"\", 2 ^ 62, \"\") == \"\")\nassert((\"\"):rep(math.maxinteger) == \"\")\n",
    )
    .evaluate()
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

#[gsttt_scenario::scenario("config", "A range beyond the budget is refused")]
#[test]
fn a_range_beyond_the_budget_is_refused_before_it_is_walked() {
    for (call, name) in [
        ("table.move({}, 1, 2 ^ 40, 2)", "'move'"),
        ("table.move({}, \"1\", \"1e12\", 2)", "'move'"),
        (
            "table.concat(setmetatable({}, { __index = rawlen }), \"\", 1, 2 ^ 40)",
            "'concat'",
        ),
        (
            "table.concat(setmetatable({}, { __index = rawlen }), \"\", -2 ^ 40)",
            "'concat'",
        ),
    ] {
        let message = failure_at_once(&format!("{call}\n"));
        assert!(
            message.starts_with(".gsttt/init.lua:1: bad argument")
                && message.contains(name)
                && message.contains("more than 1000000 elements"),
            "{call}: {message}"
        );
    }
}

#[gsttt_scenario::scenario("config", "A table longer than the budget is refused")]
#[test]
fn a_table_longer_than_the_budget_is_refused_before_it_is_walked() {
    for call in [
        "table.insert(sparse, 1, \"x\")",
        "table.remove(sparse, 1)",
        "table.sort(sparse)",
        "table.concat(sparse)",
    ] {
        let message = failure_at_once(&format!("{}{call}\n", sparse_table()));
        assert!(
            message.starts_with(".gsttt/init.lua:2: bad argument #1 to")
                && message.contains("longer than 1000000 elements"),
            "{call}: {message}"
        );
    }
}

#[gsttt_scenario::scenario("config", "A table with a length metamethod is refused")]
#[test]
fn a_table_with_a_length_metamethod_is_refused() {
    // The length is asked again by the function after any check of it, and this table answers differently the second time.
    let stateful = "local asked = 0\nlocal list = setmetatable({}, { __len = function() asked = asked + 1 return asked == 1 and 3 or 2 ^ 40 end })\n";
    for call in [
        "table.sort(list)",
        "table.insert(list, 1, \"x\")",
        "table.remove(list, 1)",
        "table.concat(list)",
    ] {
        let message = failure_at_once(&format!("{stateful}{call}\n"));
        assert!(
            message.starts_with(".gsttt/init.lua:3: bad argument #1 to")
                && message.contains("`__len`"),
            "{call}: {message}"
        );
    }
}

#[gsttt_scenario::scenario("config", "Library calls within the budget are unchanged")]
#[test]
fn library_calls_within_the_budget_are_unchanged() {
    Tree::with_init(
        r#"
local list = { 5, 3, 9 }
table.insert(list, 1, 7)
table.insert(list, 2, 8)
assert(table.concat(list, ",") == "7,8,5,3,9")
assert(table.remove(list, 1) == 7 and table.remove(list) == 9)
table.sort(list)
assert(table.concat(list, ",") == "3,5,8")
table.sort(list, function(a, b) return a > b end)
assert(table.concat(list, ",") == "8,5,3")
assert(table.move(list, 1, 3, 2) == list and table.concat(list, ",") == "8,8,5,3")
assert(table.concat({ 1, 2, 3 }, "-", 2, 3) == "2-3")
assert(table.concat({ 1, 2, 3 }, "-", 2) == "2-3")
assert(string.rep("ab", 3, ",") == "ab,ab,ab" and string.rep("", 3, ",") == ",,")
local ok, message = pcall(table.insert, list, 9, "x")
assert(not ok and tostring(message):find("bad argument #2 to 'insert' (position out of bounds)", 1, true), tostring(message))
"#,
    )
    .evaluate()
    .unwrap();
    let message = failure("table.insert({}, 9, \"x\")\n");
    assert_eq!(
        message,
        ".gsttt/init.lua:1: bad argument #2 to 'insert' (position out of bounds)"
    );
}

#[gsttt_scenario::scenario("config", "A finalizer or a weak mode is refused")]
#[test]
fn a_finalizer_or_a_weak_mode_is_refused() {
    for (field, name) in [
        ("__gc = function() end", "`__gc`"),
        ("__mode = \"k\"", "`__mode`"),
        ("__mode = \"v\"", "`__mode`"),
    ] {
        let message = failure_at_once(&format!("setmetatable({{}}, {{ {field} }})\n"));
        assert!(
            message.starts_with(".gsttt/init.lua:1: ") && message.contains(name),
            "{field}: {message}"
        );
    }
    Tree::with_init("assert(type(setmetatable({}, { __index = {} })) == \"table\")\n")
        .evaluate()
        .unwrap();
}

/// Lets the collector run many cycles. The config has no `collectgarbage` to ask for one, so it allocates until one runs.
const CHURN: &str =
    "local function churn() for _ = 1, 40 do local _ = string.rep(\"x\", 2 ^ 20) end end\n";

/// Twenty thousand tables chained through the keys of one table, in an order that makes the collector walk the chain again for every link it finds out of turn: minutes of work if the table is weak, none if it is not. The weak mode is written after `setmetatable`.
const KEY_CHAIN: &str = r#"
local count = 20000
local nodes = {}
for i = 1, count do nodes[i] = {} end
local seed = 12345
for i = count, 2, -1 do
  seed = (seed * 1103515245 + 12345) % 2147483648
  local j = seed % i + 1
  nodes[i], nodes[j] = nodes[j], nodes[i]
end
local meta = {}
local chain = setmetatable({}, meta)
for i = 1, count - 1 do chain[nodes[i]] = nodes[i + 1] end
local head = nodes[1]
nodes = nil
meta.__mode = "k"
churn()
"#;

#[gsttt_scenario::scenario("config", "A weak mode written after setmetatable is not seen")]
#[test]
fn a_weak_mode_written_after_setmetatable_is_not_seen() {
    // The table stays strong: the value that nothing else holds is still in it after the collector has run many times.
    let strong = [
        CHURN,
        r#"
local meta = {}
local held = setmetatable({}, meta)
meta.__mode = "v"
held.entry = {}
churn()
assert(held.entry ~= nil, "the entry was collected")
"#,
    ]
    .concat();
    Tree::with_init(&strong).evaluate().unwrap();
    // A chain of such keys, which the collector needs minutes to clear if it believes the mode, is left alone.
    let started = Instant::now();
    Tree::with_init(&[CHURN, KEY_CHAIN].concat())
        .evaluate()
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "took {:?}",
        started.elapsed()
    );
}

#[gsttt_scenario::scenario("config", "A method added to a class after its objects exist is found")]
#[test]
fn a_method_added_to_a_class_after_its_objects_exist_is_found() {
    Tree::with_init(
        r#"
local Class = {}
Class.__index = Class
local object = setmetatable({}, Class)
function Class.late() return "found" end
assert(object.late() == "found")
local Derived = setmetatable({}, Class)
Derived.__index = Derived
local child = setmetatable({}, Derived)
function Class.later() return "inherited" end
assert(child.later() == "inherited")
"#,
    )
    .evaluate()
    .unwrap();
}

#[gsttt_scenario::scenario("config", "getmetatable gives back the table that was set")]
#[test]
fn getmetatable_gives_back_the_table_that_was_set() {
    Tree::with_init(
        r#"
local Class = {}
Class.__index = Class
local object = setmetatable({}, Class)
assert(getmetatable(object) == Class)
assert(getmetatable(setmetatable({}, Class)) == getmetatable(object))
assert(getmetatable(setmetatable({}, { __metatable = "sealed" })) == "sealed")
assert(getmetatable(setmetatable({}, { __metatable = false })) == false)
assert(getmetatable({}) == nil)
setmetatable(object, nil)
assert(getmetatable(object) == nil)
"#,
    )
    .evaluate()
    .unwrap();
}

#[gsttt_scenario::scenario("config", "A protected metatable cannot be replaced or removed")]
#[test]
fn a_protected_metatable_cannot_be_replaced_or_removed() {
    for (sealed, call) in [
        ("\"sealed\"", "setmetatable(object, {})"),
        ("\"sealed\"", "setmetatable(object, nil)"),
        (
            "\"sealed\"",
            "setmetatable(object, { __metatable = \"other\" })",
        ),
        ("false", "setmetatable(object, {})"),
    ] {
        let message = failure(&format!(
            "local object = setmetatable({{}}, {{ __metatable = {sealed} }})\n{call}\n"
        ));
        assert_eq!(
            message, ".gsttt/init.lua:2: cannot change a protected metatable",
            "{sealed}: {call}"
        );
    }
    // A metatable without the field is replaced and removed as in Lua.
    Tree::with_init(
        r#"
local object = setmetatable({}, { __index = { kind = "first" } })
setmetatable(object, { __index = { kind = "second" } })
assert(object.kind == "second")
setmetatable(object, nil)
assert(object.kind == nil)
"#,
    )
    .evaluate()
    .unwrap();
}

#[gsttt_scenario::scenario("config", "A metamethod added after setmetatable has no effect")]
#[test]
fn a_metamethod_added_after_setmetatable_has_no_effect() {
    Tree::with_init(
        r#"
local meta = {}
local object = setmetatable({}, meta)
meta.__index = function() return "late" end
meta.__len = function() return 7 end
meta.__call = function() return "called" end
assert(object.missing == nil)
assert(#object == 0)
assert(not pcall(object))
-- A table that gets the metatable after the field was added has the field.
local later = setmetatable({}, meta)
assert(later.missing == "late" and #later == 7 and later() == "called")
"#,
    )
    .evaluate()
    .unwrap();
}

#[gsttt_scenario::scenario("config", "The metatable of the gsttt module is out of reach")]
#[test]
fn the_metatable_of_the_gsttt_module_is_out_of_reach() {
    Tree::with_init(
        r#"
local g = require("gsttt")
assert(getmetatable(g) == false)
assert(not pcall(setmetatable, g, {}))
assert(not pcall(setmetatable, g, nil))
-- The module is still read-only.
assert(not pcall(function() g.setup_again = true end))
"#,
    )
    .evaluate()
    .unwrap();
    let message = failure("local g = require(\"gsttt\")\nsetmetatable(g, nil)\n");
    assert_eq!(
        message,
        ".gsttt/init.lua:2: cannot change a protected metatable"
    );
}

#[gsttt_scenario::scenario("config", "A long error message is cut")]
#[test]
fn a_long_error_message_is_cut() {
    let message = failure("error(string.rep(\"x\", 100000))\n");
    assert_eq!(message.len(), 1024, "{message:.100}");
    assert!(
        message.starts_with(".gsttt/init.lua:1: xxx") && message.ends_with("x…"),
        "{message:.100}"
    );
}

#[gsttt_scenario::scenario("config", "Patterns are not available")]
#[test]
fn the_pattern_functions_are_not_there() {
    for (call, missing) in [
        (r#"("abc"):match("a.c")"#, "method 'match'"),
        (r#"string.gsub("abc", "b", "x")"#, "field 'gsub'"),
        (r#"string.gmatch("abc", ".")"#, "field 'gmatch'"),
    ] {
        let message = failure(&format!("{call}\n"));
        assert!(
            message.contains("attempt to call a nil value") && message.contains(missing),
            "{call}: {message}"
        );
    }
}

#[gsttt_scenario::scenario("config", "Find reads its text as given")]
#[test]
fn find_looks_for_the_text_and_never_for_a_pattern() {
    let setup = Tree::with_init(
        r#"
        local first, last = ("a.b"):find(".")
        require("gsttt").setup {
            exclude = { first .. "," .. last, tostring(("a.b"):find("%.")), tostring(("axb"):find("a.b", 1, false)) },
        }
        "#,
    )
    .evaluate()
    .unwrap();
    assert_eq!(setup.exclude, ["2,2", "nil", "nil"]);
}

#[gsttt_scenario::scenario("config", "Addresses stay out of a config")]
#[test]
fn a_config_cannot_read_an_address() {
    let setup = Tree::with_init(
        r#"require("gsttt").setup { exclude = { tostring({}), tostring(print), ("%s"):format({}) } }"#,
    )
    .evaluate()
    .unwrap();
    assert_eq!(setup.exclude, ["table", "function", "table"]);
    let message = failure("local _ = string.format(\"%p\", {})\n");
    assert!(message.contains("`%p`"), "{message}");
}

#[gsttt_scenario::scenario("config", "Iteration order is fixed")]
#[test]
fn pairs_visits_keys_in_a_fixed_order() {
    let init = r#"
        local g = require("gsttt")
        local letters = { b = 1, a = 2, c = 3 }
        local keys = {}
        for key in pairs(letters) do keys[#keys + 1] = key end
        g.setup { exclude = keys }
    "#;
    // Each evaluation is a new Lua state with its own hash seed.
    for _ in 0..25 {
        let setup = Tree::with_init(init).evaluate().unwrap();
        assert_eq!(setup.exclude, ["a", "b", "c"]);
    }
}

#[gsttt_scenario::scenario("config", "Comments do not change the fingerprint")]
#[test]
fn comments_do_not_change_the_fingerprint() {
    let plain = "local g = require(\"gsttt\")\ng.setup { http_prefixes = { \"/api\" } }\n";
    let commented = format!("-- the gateway prefix\n{plain}-- the end\n");
    let print = |init: &str| fingerprint(&Tree::with_init(init).evaluate().unwrap()).unwrap();
    assert_eq!(print(plain), print(&commented));
    assert_ne!(print(plain), print(&plain.replace("/api", "/v2")));

    // A value in the options of a module is part of the setup, and so is one in the options of an external.
    let module = |tsconfig: &str| {
        format!("local g = require(\"gsttt\")\ng.ts {{ tsconfig = \"{tsconfig}\" }}\n")
    };
    assert_ne!(print(&module("a.json")), print(&module("b.json")));
    let external = |strict: bool| {
        format!(
            "local g = require(\"gsttt\")\ng.external {{ name = \"k\", cmd = {{ \"x\" }}, files = {{ \"*.x\" }}, options = {{ strict = {strict} }} }}\n"
        )
    };
    assert_ne!(print(&external(true)), print(&external(false)));
}

#[test]
fn setup_merges_and_the_other_functions_accumulate() {
    let setup = Tree::with_init(
        r#"
        local g = require("gsttt")
        g.setup { exclude = { "a" }, max_file_bytes = 1e6 }
        g.setup { detect = false }
        g.python()
        g.rust { }
        g.disable "ts"
        g.disable "ts"
        g.disable_pack "axios"
        print("prints go to the log", 1, nil)
        "#,
    )
    .evaluate()
    .unwrap();
    assert_eq!(setup.exclude, ["a"]);
    assert!(!setup.detect);
    assert_eq!(setup.max_file_bytes, 1_000_000);
    let modules: Vec<(&str, &serde_json::Value)> = setup
        .modules
        .iter()
        .map(|module| (module.module.as_str(), &module.options))
        .collect();
    assert_eq!(modules, [("python", &json!(null)), ("rust", &json!(null))]);
    assert_eq!(setup.disable, ["ts"]);
    assert_eq!(setup.disable_packs, ["axios"]);
}

#[test]
fn the_gsttt_module_refuses_what_would_be_silently_ignored() {
    let init = |body: &str| format!("local g = require(\"gsttt\")\n{body}\n");
    let twice = failure(&init("g.ts {}\ng.ts {}"));
    assert!(
        twice.contains("module `ts` is already turned on"),
        "{twice}"
    );
    let list = failure(&init("g.ts { \"a\" }"));
    assert!(list.contains("g.ts takes named options"), "{list}");
    let text = failure(&init("g.ts \"a\""));
    assert!(
        text.contains("g.ts takes a table of options, got string"),
        "{text}"
    );
    let assign = failure(&init("g.extra = 1"));
    assert!(
        assign.contains("gsttt is read-only") && assign.contains("`extra`"),
        "{assign}"
    );
    let function = failure(&init("g.ts { tsconfig = print }"));
    assert!(
        function.contains("g.ts.tsconfig: a function cannot be part of the setup"),
        "{function}"
    );
    let setup = failure(&init("g.setup()"));
    assert!(setup.contains("g.setup takes a table, got nil"), "{setup}");
    let size = failure(&init("g.setup { max_file_bytes = \"big\" }"));
    assert!(
        size.starts_with(".gsttt/init.lua:2:") && size.contains("g.setup:"),
        "{size}"
    );
}

#[test]
fn an_external_is_checked_when_it_is_declared() {
    let declare = |fields: &str| {
        failure(&format!(
            "local g = require(\"gsttt\")\ng.external {{ {fields} }}\n"
        ))
    };
    let named = declare("name = \"bad name\", cmd = { \"x\" }, files = { \"*.x\" }");
    assert!(
        named.contains("the name `bad name` must be 1 to 64 letters, digits"),
        "{named}"
    );
    let long = declare(&format!(
        "name = \"{}\", cmd = {{ \"x\" }}, files = {{ \"*.x\" }}",
        "x".repeat(65)
    ));
    assert!(long.contains("must be 1 to 64 letters, digits"), "{long}");
    let module = declare("name = \"ts\", cmd = { \"x\" }, files = { \"*.x\" }");
    assert!(module.contains("already a module of gsttt"), "{module}");
    let command = declare("name = \"k\", cmd = {}, files = { \"*.x\" }");
    assert!(command.contains("needs a `cmd`"), "{command}");
    let files = declare("name = \"k\", cmd = { \"x\" }, files = {}");
    assert!(files.contains("needs `files`"), "{files}");
    let glob = declare("name = \"k\", cmd = { \"x\" }, files = { \"[\" }");
    assert!(glob.contains("holds `[`"), "{glob}");
    let env = declare("name = \"k\", cmd = { \"x\" }, files = { \"*.x\" }, env = { \"A=B\" }");
    assert!(env.contains("`env` of `k`") && env.contains("A=B"), "{env}");
    let missing = declare("name = \"k\"");
    assert!(missing.contains("missing field"), "{missing}");

    let twice = failure(
        "local g = require(\"gsttt\")\nlocal k = { name = \"k\", cmd = { \"x\" }, files = { \"*.x\" } }\ng.external(k)\ng.external(k)\n",
    );
    assert!(twice.contains("`k` is declared twice"), "{twice}");
}

#[test]
fn a_pack_is_added_once() {
    let twice = failure(
        "local g = require(\"gsttt\")\nlocal p = { name = \"p\", language = \"ts\" }\ng.pack(p)\ng.pack(p)\n",
    );
    assert!(twice.contains("pack `p` is added twice"), "{twice}");
}

#[test]
fn modules_load_once_from_files_or_folders_and_errors_name_their_own_line() {
    let tree = Tree::with_init(
        r#"
        local g = require("gsttt")
        local first = require("shared")
        local second = require("shared")
        assert(first == second, "a module is evaluated once")
        assert(require("gsttt") == g)
        assert(require("deep.folder").answer == 42)
        assert(require("quiet") == true)
        g.setup { exclude = { first.name } }
        "#,
    );
    tree.write(".gsttt/lua/shared.lua", "return { name = 'once' }");
    tree.write(".gsttt/lua/deep/folder/init.lua", "return { answer = 42 }");
    tree.write(".gsttt/lua/quiet.lua", "local nothing = 1");
    assert_eq!(tree.evaluate().unwrap().exclude, ["once"]);

    let broken = Tree::with_init("require(\"broken\")\n");
    broken.write(".gsttt/lua/broken.lua", "local fine = 1\nfine()\n");
    let message = broken.evaluate().unwrap_err().to_string();
    assert!(message.starts_with(".gsttt/lua/broken.lua:2:"), "{message}");

    let missing = failure("require(\"nope.deeper\")\n");
    assert!(
        missing.contains(
            "module `nope.deeper` not found; tried .gsttt/lua/nope/deeper.lua and .gsttt/lua/nope/deeper/init.lua"
        ),
        "{missing}"
    );

    let cycle = Tree::with_init("require(\"a\")\n");
    cycle.write(".gsttt/lua/a.lua", "require('b')");
    cycle.write(".gsttt/lua/b.lua", "require('a')");
    let message = cycle.evaluate().unwrap_err().to_string();
    assert!(
        message.contains("module `a` requires itself (loading a, b)"),
        "{message}"
    );
}
