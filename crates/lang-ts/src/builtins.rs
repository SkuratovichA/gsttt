//! The modules Node ships, which resolve to builtins rather than packages, and the globals every runtime has.

/// Names of the builtin modules, sorted so that a lookup is a binary search. `node:test` and the other modules that exist only behind the `node:` prefix are covered by that prefix, not listed here.
const BUILTINS: [&str; 42] = [
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// Whether `spec` names a Node builtin, with or without a subpath (`fs/promises`) or the `node:` prefix.
pub(crate) fn is_builtin(spec: &str) -> bool {
    if spec.starts_with("node:") {
        return true;
    }
    let name = spec.split('/').next().unwrap_or(spec);
    BUILTINS.binary_search(&name).is_ok()
}

/// Names of the globals that a repository does not declare as its own classes, sorted so that a lookup is a binary search. `Request`, `Response`, `Headers`, `FormData`, `Blob`, `File`, `fetch`, `WebSocket`, `EventTarget` and `Event` are left out: a repository may declare and export a class of that name. `Proxy` and `globalThis` are left out too: a proxy has the members of its target, and `globalThis` holds whatever a file hangs on it.
const GLOBALS: [&str; 51] = [
    "AbortController",
    "AbortSignal",
    "AggregateError",
    "Array",
    "ArrayBuffer",
    "Atomics",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "Buffer",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "Float32Array",
    "Float64Array",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Intl",
    "JSON",
    "Map",
    "Math",
    "Number",
    "Object",
    "Promise",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "SharedArrayBuffer",
    "String",
    "Symbol",
    "SyntaxError",
    "TextDecoder",
    "TextEncoder",
    "TypeError",
    "URIError",
    "URL",
    "URLSearchParams",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "console",
    "process",
];

/// Whether `name` is a global of the language or of Node that no repository declares as its own, so that a member read on it is a read on a known value. A name that a file binds is never one, which the caller checks.
pub(crate) fn is_global_builtin(name: &str) -> bool {
    GLOBALS.binary_search(&name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sorted_so_the_search_is_sound() {
        assert!(BUILTINS.is_sorted());
    }

    #[test]
    fn the_globals_are_sorted_so_the_search_is_sound() {
        assert!(GLOBALS.is_sorted());
    }

    #[test]
    fn a_global_is_a_name_no_repository_declares() {
        assert!(is_global_builtin("Object"));
        assert!(is_global_builtin("Buffer"));
        assert!(!is_global_builtin("Headers"));
        assert!(!is_global_builtin("FormData"));
        assert!(!is_global_builtin("fetch"));
        assert!(!is_global_builtin("Proxy"));
        assert!(!is_global_builtin("globalThis"));
    }

    #[test]
    fn subpaths_and_prefixes_count() {
        assert!(is_builtin("fs/promises"));
        assert!(is_builtin("node:test"));
        assert!(is_builtin("path"));
        assert!(!is_builtin("left-pad"));
        assert!(!is_builtin("@scope/fs"));
    }
}
