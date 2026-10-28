//! Every rule kind over single files, without the engine around them.

use gsttt_config::{builtin_packs, eval_pack};
use gsttt_core::rules::Pack;
use gsttt_core::text::LineIndex;
use proptest::prelude::*;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast;

use super::{Sites, read};

fn sites(source: &str, pack: &str) -> Sites {
    let pack = eval_pack("test.lua", pack).unwrap();
    let parse = ast::SourceFile::parse(source, Edition::CURRENT);
    read(&parse.syntax_node(), &LineIndex::new(source), &[pack])
}

/// The line of `needle`, counting from 1.
fn line_of(source: &str, needle: &str) -> u32 {
    let offset = source.find(needle).unwrap();
    let before = source.get(..offset).unwrap();
    u32::try_from(before.matches('\n').count()).unwrap() + 1
}

/// Each contract site as `<line> <key>`.
fn found(sites: &Sites) -> Vec<String> {
    sites
        .found
        .iter()
        .map(|found| format!("{} {}", found.fact.line, found.fact.key))
        .collect()
}

fn unresolved(sites: &Sites) -> Vec<(u32, &str)> {
    sites
        .unresolved
        .iter()
        .map(|site| (site.line, site.detail.as_str()))
        .collect()
}

#[test]
fn calls_method_calls_and_macros_are_sites() {
    let pack = r#"return { name = "t", language = "rust", calls = {
        { callee = { "env.var", "env!" }, kind = "env", role = "calls", key = "$arg1" },
        { callee = "*.route", kind = "http", role = "serves", key = "$arg1", method = "$arg2.method" },
    } }"#;
    let source = r#"fn main() {
    let a = env::var("a");
    let b = env!("b");
    Router::new()
        .route("/r", get(show).post(create))
        .route("/t", trace(probe).any(fallback))
        .route("/u", method_router);
}
"#;
    assert_eq!(
        found(&sites(source, pack)),
        [
            "2 env A",
            "3 env B",
            "5 http GET /r",
            "5 http POST /r",
            "6 http * /t",
            "6 http TRACE /t",
            "7 http * /u",
        ]
    );
}

#[test]
fn a_method_call_is_where_its_method_is_written() {
    let pack = r#"return { name = "t", language = "rust", calls = {
        { callee = "*.route", kind = "http", role = "serves", key = "$arg1" },
    } }"#;
    let source =
        "fn main() {\n    Router::new()\n        .layer(l)\n        .route(\"/r\", h);\n}\n";
    assert_eq!(found(&sites(source, pack)), ["4 http * /r"]);
}

#[test]
fn attributes_are_sites_with_keywords() {
    let pack = r#"return { name = "t", language = "rust", decorators = {
        { decorator = { "get", "post" }, kind = "http", role = "serves", key = "$arg1", method = "$callee" },
        { decorator = "route", kind = "http", role = "serves", key = "$arg1", method = "$method" },
        { decorator = "orm", kind = "table", role = "declares", key = "$table_name" },
        { decorator = "var", kind = "env", role = "calls", key = "$member" },
    } }"#;
    let source = r#"#[get("/a")]
async fn a() {}

#[route("/b", method = "GET", method = "post")]
async fn b() {}

#[orm(table_name = "bookings")]
struct Booking;

#[orm(primary_key)]
struct Other;

#[var]
fn watched() {}
"#;
    let out = sites(source, pack);
    assert_eq!(
        found(&out),
        [
            "1 http GET /a",
            "4 http GET /b",
            "4 http POST /b",
            "7 table bookings",
            "13 env WATCHED",
        ]
    );
    assert_eq!(unresolved(&out), []);
}

#[test]
fn an_attribute_with_a_computed_value_is_a_blind_spot() {
    let pack = r#"return { name = "t", language = "rust", decorators = {
        { decorator = "orm", kind = "table", role = "declares", key = "$table_name" },
    } }"#;
    let source = "#[orm(table_name = TABLE)]\nstruct Booking;\n";
    let out = sites(source, pack);
    assert_eq!(found(&out), Vec::<String>::new());
    assert_eq!(unresolved(&out), [(1, "orm")]);
}

#[test]
fn embed_rules_take_the_literal_sql() {
    let pack = r#"return { name = "t", language = "rust", embeds = {
        { callee = { "query_as!", "sqlx.query_as!" }, lang = "sql", text = "$arg2" },
        { callee = "sqlx.query", lang = "sql" },
    } }"#;
    let source = r#"fn f() {
    sqlx::query_as!(Booking, "SELECT id FROM bookings", id);
    sqlx::query("SELECT 1");
    sqlx::query(sql);
    sqlx::query(&format!("SELECT {}", columns));
}
"#;
    let out = sites(source, pack);
    assert_eq!(
        out.embedded
            .iter()
            .map(|document| (
                document.line,
                document.lang.as_str(),
                document.text.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (2, "sql", "SELECT id FROM bookings"),
            (3, "sql", "SELECT 1")
        ]
    );
    assert_eq!(
        unresolved(&out),
        [(4, "sqlx.query"), (5, "sqlx.query")],
        "a document built at run time is a blind spot"
    );
}

#[test]
fn chain_rules_read_paths_fields_and_indexes() {
    let pack = r#"return { name = "t", language = "rust", chains = {
        { root = "settings.env", kind = "env", role = "calls", segments = 1 },
        { root = "Config", kind = "env", role = "calls", segments = 1 },
    } }"#;
    let source = r#"fn f(name: &str) {
    let a = settings.env.database_url;
    let b = settings.env.port.to_string();
    let c = settings.env[name];
    let d = Config::TIMEOUT;
    let e = other.env.port;
    let g = settings.env["literal"];
}
"#;
    let out = sites(source, pack);
    assert_eq!(
        found(&out),
        [
            "2 env DATABASE_URL",
            "3 env PORT",
            "5 env TIMEOUT",
            "7 env LITERAL"
        ]
    );
    assert_eq!(unresolved(&out), [(4, "settings.env")]);
}

#[test]
fn assign_rules_read_constants_lets_fields_and_assignments() {
    let pack = r#"return { name = "t", language = "rust", assigns = {
        { target = "TABLE", kind = "table", role = "declares", key = "$arg1" },
        { target = "table_name", class = "Entity", kind = "table", role = "declares", key = "$arg1" },
    } }"#;
    let source = r#"const TABLE: &str = "bookings";
static OTHER: &str = "nope";
impl Entity for Booking {
    const TABLE: &'static str = "rooms";
    fn f() { let table_name = "guests"; }
}
impl Other for Booking {
    fn f() { let table_name = "nope"; }
}
fn g() {
    let cfg = Config { TABLE: "tables" };
    cfg.TABLE = "assigned";
}
"#;
    assert_eq!(
        found(&sites(source, pack)),
        [
            "1 table bookings",
            "4 table rooms",
            "5 table guests",
            "11 table tables",
            "12 table assigned",
        ]
    );
}

#[test]
fn a_site_is_one_blind_spot_however_many_rules_read_it() {
    let pack = r#"return { name = "t", language = "rust", calls = {
        { callee = "*.run", kind = "queue", role = "calls", key = "$arg1" },
        { callee = "*.run", kind = "table", role = "calls", key = "$arg1" },
    } }"#;
    let out = sites("fn f() { worker.run(name); }\n", pack);
    assert_eq!(unresolved(&out), [(1, "worker.run")]);
}

#[test]
fn a_fact_beats_a_blind_spot_at_the_same_site() {
    let pack = r#"return { name = "t", language = "rust", calls = {
        { callee = "*.route", kind = "http", role = "serves", key = "$arg1", method = "$arg2.method" },
        { callee = "*.route", kind = "http", role = "serves", key = "", method = "$arg1.method",
          receiver = { from = "web.resource", key = "$arg1" } },
    } }"#;
    let source = r#"fn f(path: &str) {
    web::resource("/x").route(web::get().to(show));
    app.route(path, web::get().to(show));
}
"#;
    let out = sites(source, pack);
    assert_eq!(found(&out), ["2 http GET /x"]);
    assert_eq!(unresolved(&out), [(3, "app.route")]);
}

#[test]
fn a_receiver_is_what_the_file_says_it_is() {
    let pack = r#"return { name = "t", language = "rust", calls = {
        { callee = { "*.get", "*.delete" }, kind = "http", role = "calls", key = "$arg1", method = "$callee",
          receiver = { from = { "reqwest.Client.new", "Client.new", "Client.builder" } } },
        { callee = "*.request", kind = "http", role = "calls", key = "$arg2", method = "$arg1.last",
          receiver = { from = "Client.new" } },
    } }"#;
    let source = r#"struct Api { http: reqwest::Client, cache: HashMap<String, String> }
static SHARED: LazyLock<Client> = LazyLock::new(|| Client::new());
impl Api {
    fn a(&self) {
        self.http.get("/field");
        self.cache.get("/cache");
    }
}
fn b(client: &Client, map: &HashMap<String, String>) {
    client.get("/param");
    map.get("/map");
    SHARED.get("/static");
    let built = Client::builder().timeout(t).build().unwrap();
    built.get("/built");
    let typed: Client = make();
    typed.get("/typed");
    let unknown = make();
    unknown.get("/unknown");
    Client::new().get("/chain");
    other.get("/missing");
    let c = Client::new();
    c.request(Method::PUT, "/request");
    c.get(url);
}
"#;
    let out = sites(source, pack);
    let expected: Vec<String> = [
        ("/field", "GET /field"),
        ("/param", "GET /param"),
        ("/static", "GET /static"),
        ("/built", "GET /built"),
        ("/typed", "GET /typed"),
        ("/chain", "GET /chain"),
        ("/request", "PUT /request"),
    ]
    .iter()
    .map(|(needle, key)| format!("{} http {key}", line_of(source, needle)))
    .collect();
    assert_eq!(found(&out), expected);
    assert_eq!(unresolved(&out), [(line_of(source, "c.get(url)"), "c.get")]);
}

#[test]
fn a_rebound_name_is_the_latest_binding() {
    let pack = r#"return { name = "t", language = "rust", calls = {
        { callee = "*.get", kind = "http", role = "calls", key = "$arg1", method = "$callee",
          receiver = { from = "Client.new" } },
    } }"#;
    let source = r#"fn f() {
    let c = HashMap::new();
    c.get("/before");
    let c = Client::new();
    c.get("/after");
}
"#;
    assert_eq!(found(&sites(source, pack)), ["5 http GET /after"]);
}

/// A pack with a rule of every kind, so that the properties below reach all of the reader.
const EVERY_KIND: &str = r#"return { name = "every", language = "rust",
    calls = {
        { callee = "*.route", kind = "http", role = "serves", key = "$arg1", method = "$arg2.method" },
        { callee = "*.get", kind = "http", role = "calls", key = "$arg1", method = "$callee",
          receiver = { from = "Client.new" } },
        { callee = "env!", kind = "env", role = "calls", key = "$arg1" },
    },
    decorators = {
        { decorator = "get", kind = "http", role = "serves", key = "$arg1", method = "$callee" },
        { decorator = "orm", kind = "table", role = "declares", key = "$table_name" },
    },
    chains = { { root = "settings", kind = "env", role = "calls", segments = 1 } },
    embeds = { { callee = "sqlx.query", lang = "sql" } },
    assigns = { { target = "TABLE", kind = "table", role = "declares", key = "$arg1" } },
}"#;

/// Every pack the properties read with: the built-in ones and one with a rule of every kind.
fn every_pack() -> Vec<Pack> {
    let mut packs = builtin_packs(crate::PACKS).unwrap();
    packs.push(eval_pack("every.lua", EVERY_KIND).unwrap());
    packs
}

/// Pieces of Rust that the packs look at, and pieces that break them.
const FRAGMENTS: &[&str] = &[
    "fn f() {",
    "}",
    "Router::new()",
    ".route(\"/a/{id}\", get(h).post(p))",
    ".nest(\"/api\", r)",
    "web::scope(\"/s\").service(h)",
    "#[get(\"/x\")]",
    "#[route(\"/y\", method = \"GET\", method = \"POST\")]",
    "#[sea_orm(table_name = \"t\")]",
    "#[orm(table_name = T)]",
    "#[arg(long, env = \"E\")]",
    "env::var(\"A\")",
    "env!(\"B\")",
    "sqlx::query_as!(T, \"SELECT 1\", id)",
    "sqlx::query(&sql)",
    "client.get(format!(\"/a/{}\", id))",
    "let client = reqwest::Client::new();",
    "settings.db.url",
    "settings[\"db\"].host()",
    "const TABLE: &str = \"t\";",
    "impl Entity for T {",
    "\"",
    "'",
    "(",
    ")",
    "{",
    "[",
    "]",
    "#[",
    "!",
    "::",
    ".",
    "=",
    ";",
    "\n",
];

proptest! {
    #[test]
    fn reading_any_arrangement_of_fragments_is_total_and_stable(
        parts in prop::collection::vec(prop::sample::select(FRAGMENTS), 0..30),
    ) {
        let packs = every_pack();
        let source = parts.join(" ");
        let lines = LineIndex::new(&source);
        let parse = ast::SourceFile::parse(&source, Edition::CURRENT);
        let first = read(&parse.syntax_node(), &lines, &packs);
        let second = read(&parse.syntax_node(), &lines, &packs);
        let last_line = u32::try_from(source.matches('\n').count()).unwrap() + 1;
        prop_assert!(first.found.iter().all(|found| (1..=last_line).contains(&found.fact.line)));
        prop_assert!(first.unresolved.iter().all(|site| (1..=last_line).contains(&site.line)));
        prop_assert_eq!(found(&first), found(&second));
        prop_assert_eq!(unresolved(&first), unresolved(&second));
    }

    #[test]
    fn reading_arbitrary_text_never_panics(source in ".{0,200}") {
        let packs = every_pack();
        let parse = ast::SourceFile::parse(&source, Edition::CURRENT);
        // Reaching the end is the assertion: a panic in the reader fails the case.
        drop(read(&parse.syntax_node(), &LineIndex::new(&source), &packs));
    }
}

#[test]
fn no_rules_means_no_reading() {
    let out = read(
        &ast::SourceFile::parse("fn f() { env::var(\"A\"); }", Edition::CURRENT).syntax_node(),
        &LineIndex::new("fn f() { env::var(\"A\"); }"),
        &[],
    );
    assert!(out.found.is_empty() && out.embedded.is_empty() && out.unresolved.is_empty());
}
