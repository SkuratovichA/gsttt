//! The modules Node ships, which resolve to builtins rather than packages.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sorted_so_the_search_is_sound() {
        assert!(BUILTINS.is_sorted());
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
