//! The link step over groups written out by hand, without the files they would be read from.

use gsttt_core::connector::{LinkFile, LinkInput};
use gsttt_core::contract::{ContractKey, Role};
use gsttt_core::fact::{ContractFact, FileFacts, Import, ImportKind, Resolution, ResolvedImport};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;

use super::{Extra, Function, Group, MAX_DEPTH, MAX_VISITS, Mount, Prefix, Target, link};

fn serves(line: u32, path: &str) -> ContractFact {
    ContractFact {
        key: ContractKey::http("GET", path),
        role: Role::Serves,
        line,
        via: "axum".to_owned(),
    }
}

fn literal(text: &str) -> Prefix {
    Prefix::Literal(text.to_owned())
}

fn mount(prefix: Prefix, anchor: u32) -> Mount {
    Mount {
        prefix,
        target: Target::Local(anchor),
    }
}

fn path(names: &[&str]) -> Target {
    Target::Path(names.iter().map(|name| (*name).to_owned()).collect())
}

/// A group with no base that serves `routes` and mounts `mounts`.
fn group(anchor: u32, routes: Vec<ContractFact>, mounts: Vec<Mount>) -> Group {
    Group {
        anchor,
        base: Prefix::none(),
        routes,
        mounts,
    }
}

fn function(name: &str, returns: Vec<Target>) -> Function {
    Function {
        name: name.to_owned(),
        returns,
    }
}

/// A file as linking sees it: what was read from it and the imports that were resolved.
struct Source {
    extra: Extra,
    imports: Vec<ResolvedImport>,
}

fn source(groups: Vec<Group>, functions: Vec<Function>) -> Source {
    Source {
        extra: Extra { groups, functions },
        imports: Vec::new(),
    }
}

/// An import of `spec` that brings `name` in and reaches `file`.
fn imported(name: &str, spec: &str, file: u32) -> ResolvedImport {
    let mut import = Import::new(spec, ImportKind::Static, 1);
    import.names = vec![name.to_owned()];
    ResolvedImport {
        import,
        to: Resolution::File {
            file: FileId::from_raw(file),
        },
    }
}

/// What linking finds in `files`, as `<file> <line> <key>`. A module path `fileN` reaches the file numbered `N`, and
/// any other leads out of the project.
/// Runs `run` over `files` the way the engine hands them to `link`: the `n`th of them is file `n`, and the module path
/// `file<n>` resolves to it.
fn over<T>(
    files: &[Source],
    run: impl FnOnce(&LinkInput<'_, Extra>, &dyn Fn(&RepoPath, &str) -> Resolution) -> T,
) -> T {
    let location = RepoPath::normalize("src/main.rs").unwrap();
    let facts = FileFacts::default();
    let input = LinkInput::new(
        files
            .iter()
            .enumerate()
            .map(|(index, file)| LinkFile {
                id: FileId::from_raw(u32::try_from(index).unwrap()),
                path: &location,
                facts: &facts,
                imports: &file.imports,
                extra: &file.extra,
            })
            .collect(),
    );
    let resolve = |_: &RepoPath, module: &str| {
        module
            .strip_prefix("file")
            .and_then(|number| number.parse().ok())
            .map_or(Resolution::Builtin, |file| Resolution::File {
                file: FileId::from_raw(file),
            })
    };
    run(&input, &resolve)
}

fn linked(files: &[Source]) -> Vec<String> {
    over(files, link)
        .into_iter()
        .map(|found| {
            format!(
                "{} {} {}",
                found.file.raw(),
                found.fact.line,
                found.fact.key
            )
        })
        .collect()
}

/// The walks that a cap cut short.
fn cuts(files: &[Source]) -> Vec<super::Cut> {
    over(files, |input, resolve| super::follow(input, resolve).1)
}

#[test]
fn a_router_mounted_twice_is_served_behind_each_prefix() {
    let files = [source(
        vec![
            group(
                0,
                Vec::new(),
                vec![mount(literal("/a"), 10), mount(literal("/b"), 10)],
            ),
            group(10, vec![serves(5, "/x")], Vec::new()),
        ],
        Vec::new(),
    )];
    assert_eq!(linked(&files), ["0 5 http GET /a/x", "0 5 http GET /b/x"]);
}

#[test]
fn a_router_mounted_twice_behind_the_same_prefix_is_reported_once() {
    let files = [source(
        vec![
            group(
                0,
                Vec::new(),
                vec![mount(literal("/a"), 10), mount(literal("/a/"), 10)],
            ),
            group(10, vec![serves(5, "/x")], Vec::new()),
        ],
        Vec::new(),
    )];
    assert_eq!(linked(&files), ["0 5 http GET /a/x"]);
}

#[test]
fn prefixes_add_up_and_a_bare_slash_adds_nothing() {
    let files = [source(
        vec![
            group(
                0,
                Vec::new(),
                vec![mount(literal("/a/"), 10), mount(literal("/"), 30)],
            ),
            group(10, Vec::new(), vec![mount(literal("b"), 20)]),
            group(20, vec![serves(7, "/x")], Vec::new()),
            group(30, vec![serves(9, "/y")], Vec::new()),
        ],
        Vec::new(),
    )];
    // `/y` is served where it was declared and nowhere else, so linking has nothing to add for it.
    assert_eq!(linked(&files), ["0 7 http GET /a/b/x"]);
}

#[test]
fn a_prefix_that_is_computed_places_nothing_below_it() {
    let scoped = Group {
        base: Prefix::Computed,
        ..group(20, vec![serves(8, "/z")], Vec::new())
    };
    let files = [source(
        vec![
            group(
                0,
                Vec::new(),
                vec![
                    mount(Prefix::Computed, 10),
                    mount(literal("/a"), 20),
                    mount(literal("/b"), 30),
                ],
            ),
            group(10, vec![serves(5, "/x")], Vec::new()),
            scoped,
            group(30, vec![serves(9, "/y")], Vec::new()),
        ],
        Vec::new(),
    )];
    assert_eq!(linked(&files), ["0 9 http GET /b/y"]);
}

#[test]
fn a_router_is_found_through_the_functions_that_hand_it_on() {
    let files = [
        source(
            vec![group(
                0,
                Vec::new(),
                vec![Mount {
                    prefix: literal("/api"),
                    target: path(&["file1", "router"]),
                }],
            )],
            Vec::new(),
        ),
        source(
            vec![group(10, vec![serves(4, "/x")], Vec::new())],
            vec![
                function("router", vec![path(&["Self", "inner"])]),
                function("inner", vec![Target::Local(10)]),
            ],
        ),
    ];
    assert_eq!(linked(&files), ["1 4 http GET /api/x"]);
}

#[test]
fn an_imported_function_keeps_the_name_it_was_defined_with() {
    let mut main = source(
        vec![group(
            0,
            Vec::new(),
            vec![Mount {
                prefix: literal("/api"),
                target: path(&["renamed"]),
            }],
        )],
        Vec::new(),
    );
    main.imports = vec![imported("renamed", "crate::bookings::router", 1)];
    let bookings = source(
        vec![group(10, vec![serves(4, "/x")], Vec::new())],
        vec![function("router", vec![Target::Local(10)])],
    );
    assert_eq!(linked(&[main, bookings]), ["1 4 http GET /api/x"]);
}

#[test]
fn a_function_that_cannot_be_placed_leads_nowhere() {
    let files = [source(
        vec![
            group(
                0,
                Vec::new(),
                vec![
                    Mount {
                        prefix: literal("/a"),
                        target: path(&["missing"]),
                    },
                    Mount {
                        prefix: literal("/b"),
                        target: path(&["elsewhere", "router"]),
                    },
                ],
            ),
            group(10, vec![serves(5, "/x")], Vec::new()),
        ],
        vec![function("router", vec![Target::Local(10)])],
    )];
    assert_eq!(linked(&files), Vec::<String>::new());
}

#[test]
fn functions_that_return_each_other_end_the_chase() {
    let files = [source(
        vec![group(
            0,
            Vec::new(),
            vec![Mount {
                prefix: literal("/a"),
                target: path(&["first"]),
            }],
        )],
        vec![
            function("first", vec![path(&["second"])]),
            function("second", vec![path(&["first"])]),
        ],
    )];
    assert_eq!(linked(&files), Vec::<String>::new());
}

#[test]
fn routers_that_mount_each_other_are_walked_once_around() {
    let files = [source(
        vec![
            group(0, Vec::new(), vec![mount(literal("/in"), 10)]),
            group(10, vec![serves(2, "/ping")], vec![mount(literal("/p"), 20)]),
            group(20, vec![serves(3, "/pong")], vec![mount(literal("/q"), 10)]),
        ],
        Vec::new(),
    )];
    assert_eq!(
        linked(&files),
        ["0 2 http GET /in/ping", "0 3 http GET /in/p/pong"]
    );
}

#[test]
fn a_chain_of_mounts_that_no_program_has_stops_at_the_depth_limit() {
    let depth = u32::try_from(MAX_DEPTH * 4).unwrap();
    let groups = (0..depth)
        .map(|level| {
            group(
                level,
                vec![serves(level + 1, "/r")],
                vec![mount(literal("/p"), level + 1)],
            )
        })
        .collect();
    // The first router sits at the root with no prefix, so it adds nothing, and the walk goes no further than the limit.
    assert_eq!(linked(&[source(groups, Vec::new())]).len(), MAX_DEPTH - 1);
}

#[test]
fn routers_mounted_twice_at_every_level_do_not_run_away() {
    let levels = 40_u32;
    let groups = (0..levels)
        .map(|level| {
            group(
                level,
                vec![serves(level + 1, "/r")],
                vec![
                    mount(literal("/a"), level + 1),
                    mount(literal("/b"), level + 1),
                ],
            )
        })
        .collect();
    // A path for each way down would be 2^40 of them; the walk stops long before.
    assert!(linked(&[source(groups, Vec::new())]).len() <= MAX_VISITS * 2);
}

/// Routers `0..levels`, each mounting the next one twice behind different prefixes.
fn fanning(levels: u32) -> Vec<Group> {
    (0..levels)
        .map(|level| {
            group(
                level,
                vec![serves(level + 1, "/r")],
                vec![
                    mount(literal("/a"), level + 1),
                    mount(literal("/b"), level + 1),
                ],
            )
        })
        .collect()
}

#[gsttt_scenario::scenario("rust", "Each router that starts a walk has a budget of its own")]
#[test]
fn each_router_that_starts_a_walk_has_a_budget_of_its_own() {
    // The first router fans out until the walk from it has no visits left.
    let mut groups = fanning(40);
    // The second is a router of its own that nothing mounts.
    let mut second = group(100, vec![serves(500, "/ok")], Vec::new());
    second.base = literal("/v2");
    groups.push(second);
    let found = linked(&[source(groups, Vec::new())]);
    assert!(
        found.contains(&"0 500 http GET /v2/ok".to_owned()),
        "{} facts, none of them /v2/ok",
        found.len()
    );
}

#[gsttt_scenario::scenario("rust", "A walk that a cap cuts is reported")]
#[test]
fn a_walk_that_a_cap_cuts_is_reported() {
    let root = FileId::from_raw(0);
    let deep = (0..u32::try_from(MAX_DEPTH * 2).unwrap())
        .map(|level| {
            group(
                level,
                vec![serves(level + 1, "/r")],
                vec![mount(literal("/p"), level + 1)],
            )
        })
        .collect();
    assert_eq!(
        cuts(&[source(deep, Vec::new())]),
        [super::Cut {
            file: root,
            anchor: 0,
            cap: super::Cap::Depth
        }]
    );
    assert_eq!(
        cuts(&[source(fanning(40), Vec::new())]),
        [super::Cut {
            file: root,
            anchor: 0,
            cap: super::Cap::Visits
        }]
    );
    let small = vec![
        group(0, vec![serves(1, "/a")], vec![mount(literal("/p"), 10)]),
        group(10, vec![serves(2, "/b")], Vec::new()),
    ];
    assert_eq!(cuts(&[source(small, Vec::new())]), Vec::<super::Cut>::new());
}
