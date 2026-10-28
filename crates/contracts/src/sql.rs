//! SQL files: migrations and queries, and the SQL other connectors find embedded in code.
//!
//! `CREATE`, `ALTER` and `DROP` of a table or view, and `CREATE INDEX … ON`, declare `table <name>`; queries and
//! statements that change rows call the tables they name, leaving out the names of common table expressions.
//! Statements are read one by one, so a statement that does not parse, or has a string or comment that is never closed,
//! is counted and the others still count.
//!
//! The connector always opens a project, even where no file is claimed, because code in any language can embed SQL.

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use gsttt_core::connector::{
    Connector, ConnectorError, Erased, Meta, Parsed, Project, Provide, SourceFile,
};
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::{
    ContractFact, Embedded, FileFacts, Import, Line, ParseStatus, Resolution, UnresolvedReason,
};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    ObjectName, ObjectNamePart, ObjectType, Query, Statement, TableFactor, Visit, Visitor,
};
use sqlparser::dialect::{Dialect, PostgreSqlDialect, dialect_from_str};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer};

use crate::common::{Errors, provide, site};

const VIA: &str = "sql";

/// The dialect statements are read with when the options name none.
const DEFAULT_DIALECT: &str = "postgresql";

/// The names `dialect_from_str` knows, for the message of an unknown one; a test checks each of them.
const DIALECTS: [&str; 16] = [
    "generic",
    "mysql",
    "postgresql",
    "hive",
    "sqlite",
    "snowflake",
    "redshift",
    "mssql",
    "clickhouse",
    "bigquery",
    "ansi",
    "duckdb",
    "databricks",
    "spark",
    "oracle",
    "teradata",
];

/// The connector for SQL.
#[derive(Clone, Copy, Debug)]
pub struct Sql;

/// The connector for SQL.
pub static SQL: Erased<Sql> = Erased(Sql);

/// The options of the `sql` connector.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SqlOptions {
    /// The SQL dialect every statement is read with: `postgresql` (the default), `mysql`, `sqlite`, `mssql`,
    /// `bigquery`, `snowflake`, `redshift`, `clickhouse`, `duckdb`, `hive`, `databricks`, `spark`, `oracle`,
    /// `teradata`, `ansi` or `generic`.
    pub dialect: Option<String>,
}

/// The project of the `sql` connector.
#[derive(Clone, Debug)]
pub struct SqlProject {
    /// The lower-case name of a dialect `dialect_from_str` knows.
    dialect: String,
}

fn claims(path: &RepoPath) -> bool {
    path.extension() == Some("sql")
}

impl Connector for Sql {
    type Options = SqlOptions;
    type Project = SqlProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "sql",
            version: 1,
            summary: "Tables declared by migrations and called by queries, in files and embedded in code",
            extensions: &["sql"],
        }
    }

    fn detect(&self, _: &RepoView) -> bool {
        // Code in any language can embed SQL, so there is always something it may have to read.
        true
    }

    fn open(
        &self,
        _: &RepoView,
        options: &SqlOptions,
        _: &[Pack],
    ) -> Result<Vec<SqlProject>, ConnectorError> {
        // An unknown dialect is a mistake in the configuration whether or not the repository holds SQL.
        let dialect = match &options.dialect {
            None => DEFAULT_DIALECT.to_owned(),
            Some(name) => {
                let lowered = name.trim().to_lowercase();
                if dialect_from_str(&lowered).is_none() {
                    return Err(ConnectorError::Options {
                        connector: "sql".into(),
                        message: format!(
                            "`dialect` is {name:?}, which is not a SQL dialect; expected one of {}",
                            DIALECTS.join(", ")
                        ),
                    });
                }
                lowered
            }
        };
        Ok(vec![SqlProject { dialect }])
    }
}

impl SqlProject {
    fn grammar(&self) -> Box<dyn Dialect> {
        dialect_from_str(&self.dialect).unwrap_or_else(|| Box::new(PostgreSqlDialect {}))
    }
}

impl Project for SqlProject {
    type Extra = ();

    fn root(&self) -> &'static str {
        ""
    }

    fn matches(&self, path: &RepoPath) -> bool {
        claims(path)
    }

    fn salt(&self) -> u64 {
        // The dialect is an option, and the options are part of the key of every fact.
        0
    }

    fn provides(&self) -> Vec<Provide> {
        vec![
            provide(
                ContractKind::Table,
                Role::Declares,
                &["CREATE, ALTER and DROP of tables and views", "CREATE INDEX"],
                &[
                    "tables created by ORM migrations written in code",
                    "tables created inside stored procedures",
                ],
            ),
            provide(
                ContractKind::Table,
                Role::Calls,
                &["SQL files", "SQL embedded in code"],
                &[
                    "SQL built from strings at run time",
                    "statements the SQL parser does not read",
                ],
            ),
        ]
    }

    fn parse(&self, file: &SourceFile<'_>) -> Parsed<()> {
        let (contracts, status) = read(self.grammar().as_ref(), file.text);
        Parsed::new(FileFacts {
            status,
            contracts,
            ..FileFacts::default()
        })
    }

    fn resolve(&self, _: &RepoPath, _: &Import) -> Resolution {
        Resolution::Unresolved(UnresolvedReason::Unsupported(
            "SQL files have no imports".to_owned(),
        ))
    }

    fn embeds(&self) -> &[&str] {
        &["sql"]
    }

    fn parse_embedded(&self, document: &Embedded) -> Vec<ContractFact> {
        // Lines count from the document's first line; the engine places them in the host file.
        read(self.grammar().as_ref(), &document.text).0
    }
}

/// The sites of a text of SQL, and how well it was read.
///
/// A string, a quoted name or a comment that the tokenizer cannot close fails the statement it is in and no other:
/// the statements before it are whole, and reading goes on after the next semicolon.
fn read(dialect: &dyn Dialect, text: &str) -> (Vec<ContractFact>, ParseStatus) {
    let mut facts = Vec::new();
    let mut errors = Errors::default();
    let mut rest = text;
    // The lines of `text` that come before `rest`.
    let mut before = 0;
    loop {
        let mut tokens = Vec::new();
        let failure = Tokenizer::new(dialect, rest)
            .tokenize_with_location_into_buf_with_mapper(&mut tokens, |token| place(token, before))
            .err();
        let mut groups = statements(tokens);
        // After a tokenizer error the last group is the statement it stopped in, which is not read.
        let unfinished = failure.as_ref().and_then(|_| groups.pop());
        for group in groups {
            read_group(dialect, group, &mut facts, &mut errors);
        }
        let Some(failure) = failure else {
            break;
        };
        errors.note(
            unfinished
                .as_deref()
                .and_then(start_of)
                .unwrap_or_else(|| line_of(failure.location.line.saturating_add(before))),
        );
        let Some((next, skipped)) = after_semicolon(rest, failure.location) else {
            break;
        };
        rest = next;
        before = before.saturating_add(skipped);
    }
    // Source order, the same everywhere; one site per line and key.
    facts.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.key.cmp(&b.key)));
    facts.dedup_by(|a, b| a.line == b.line && a.key == b.key && a.role == b.role);
    (facts, errors.status())
}

/// A token moved down by the lines of the text before the one it was read from.
fn place(mut token: TokenWithSpan, before: u64) -> TokenWithSpan {
    token.span.start.line = token.span.start.line.saturating_add(before);
    token.span.end.line = token.span.end.line.saturating_add(before);
    token
}

/// The line a statement starts on: its first token that is not whitespace or a comment.
fn start_of(group: &[TokenWithSpan]) -> Option<Line> {
    group
        .iter()
        .find(|token| !matches!(token.token, Token::Whitespace(_)))
        .map(|first| line_of(first.span.start.line))
}

/// Reads one statement: its sites, or an error when it does not parse even with its placeholders replaced.
fn read_group(
    dialect: &dyn Dialect,
    group: Vec<TokenWithSpan>,
    facts: &mut Vec<ContractFact>,
    errors: &mut Errors,
) {
    let Some(start) = start_of(&group) else {
        return;
    };
    let base = group
        .first()
        .map_or(start, |token| line_of(token.span.start.line));
    // The text is kept only for a statement that may hold a placeholder this grammar does not read.
    let text: Option<String> = group
        .iter()
        .any(|token| may_start_parameter(&token.token))
        .then(|| group.iter().map(|token| token.token.to_string()).collect());
    let found = parse_group(dialect, group)
        .map(|statement| {
            let mut found = Vec::new();
            read_statement(&statement, start, &mut found);
            found
        })
        .or_else(|| read_with_parameters(dialect, text.as_deref()?, base));
    match found {
        Some(found) => facts.extend(found),
        None => errors.note(start),
    }
}

/// Where reading goes on after the tokenizer failed at `at`: the text after the next semicolon, and the number of
/// lines left behind.
fn after_semicolon(text: &str, at: Location) -> Option<(&str, u64)> {
    let from = offset_of(text, at);
    let semicolon = from.saturating_add(text.get(from..)?.find(';')?);
    let (skipped, rest) = text.split_at_checked(semicolon.saturating_add(1))?;
    let lines = skipped.matches('\n').count();
    Some((rest, u64::try_from(lines).unwrap_or(u64::MAX)))
}

/// The byte offset of a tokenizer location, which counts lines by `\n` and columns by characters; the end of the
/// text when the location lies beyond it.
fn offset_of(text: &str, at: Location) -> usize {
    let lines = usize::try_from(at.line)
        .unwrap_or(usize::MAX)
        .saturating_sub(1);
    let columns = usize::try_from(at.column)
        .unwrap_or(usize::MAX)
        .saturating_sub(1);
    let line_start: usize = text.split_inclusive('\n').take(lines).map(str::len).sum();
    text.get(line_start..)
        .and_then(|line| line.char_indices().nth(columns))
        .map_or(text.len(), |(index, _)| line_start.saturating_add(index))
}

fn line_of(line: u64) -> Line {
    u32::try_from(line).unwrap_or(u32::MAX).max(1)
}

/// The statement a group of tokens holds, when it holds exactly one.
fn parse_group(dialect: &dyn Dialect, tokens: Vec<TokenWithSpan>) -> Option<Statement> {
    let mut parser = Parser::new(dialect).with_tokens_with_locations(tokens);
    let statement = parser.parse_statement().ok()?;
    (parser.peek_token().token == Token::EOF).then_some(statement)
}

/// Whether a token may begin a driver placeholder that not every grammar reads: `?` and `:name` are operators or
/// nothing at all in PostgreSQL, and `%s` is a modulo.
fn may_start_parameter(token: &Token) -> bool {
    matches!(
        token,
        Token::Mod | Token::Colon | Token::Question | Token::Placeholder(_)
    )
}

/// The sites of a statement that parses once its driver placeholders stand for a value, which is how the grammars
/// that do not read them are made to. Native syntax is tried first, so `data ? 'key'` stays a PostgreSQL operator
/// whenever the statement reads that way. `base` is the line `text` starts on.
fn read_with_parameters(
    dialect: &dyn Dialect,
    text: &str,
    base: Line,
) -> Option<Vec<ContractFact>> {
    let neutral = neutralise_placeholders(text);
    let tokens = Tokenizer::new(dialect, &neutral)
        .tokenize_with_location()
        .ok()?;
    let statement = parse_group(dialect, tokens)?;
    let mut facts = Vec::new();
    read_statement(&statement, 1, &mut facts);
    // The lines of the new text count from its first line, which is `base` in the text it was made from.
    let shift = base.saturating_sub(1);
    for fact in &mut facts {
        fact.line = fact.line.saturating_add(shift);
    }
    Some(facts)
}

/// The tokens of each statement; a semicolon inside a string, a comment or a dollar-quoted body is part of one
/// token, so splitting on semicolon tokens is safe.
fn statements(tokens: Vec<TokenWithSpan>) -> Vec<Vec<TokenWithSpan>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for token in tokens {
        if token.token == Token::SemiColon {
            groups.push(std::mem::take(&mut current));
        } else {
            current.push(token);
        }
    }
    groups.push(current);
    groups
}

fn read_statement(statement: &Statement, start: Line, facts: &mut Vec<ContractFact>) {
    match statement {
        Statement::CreateTable(create) => {
            declare(&create.name, start, facts);
            if let Some(query) = &create.query {
                call_in(query.as_ref(), start, facts);
            }
        }
        Statement::CreateView(view) => {
            declare(&view.name, start, facts);
            call_in(view.query.as_ref(), start, facts);
        }
        Statement::CreateIndex(index) => declare(&index.table_name, start, facts),
        Statement::AlterTable(alter) => declare(&alter.name, start, facts),
        Statement::Drop {
            object_type: ObjectType::Table | ObjectType::View | ObjectType::MaterializedView,
            names,
            ..
        } => {
            for name in names {
                declare(name, start, facts);
            }
        }
        Statement::Query(_)
        | Statement::Insert(_)
        | Statement::Update(_)
        | Statement::Delete(_)
        | Statement::Merge(_) => call_in(statement, start, facts),
        _ => {}
    }
}

/// The table an object name stands for: its last part, which drops the schema, without quotes, and the line it is
/// written on when the tokens carry one.
fn table_of(name: &ObjectName) -> Option<(&str, Option<Line>)> {
    let ObjectNamePart::Identifier(ident) = name.0.last()? else {
        return None;
    };
    let line = u32::try_from(ident.span.start.line)
        .ok()
        .filter(|line| *line > 0);
    Some((ident.value.as_str(), line))
}

fn declare(name: &ObjectName, start: Line, facts: &mut Vec<ContractFact>) {
    if let Some((table, line)) = table_of(name) {
        facts.push(site(
            ContractKind::Table,
            table,
            Role::Declares,
            line.unwrap_or(start),
            VIA,
        ));
    }
}

/// Calls every table that a statement or query reads from or writes to.
fn call_in<N: Visit + ?Sized>(node: &N, start: Line, facts: &mut Vec<ContractFact>) {
    let mut relations = Relations::default();
    let _ = node.visit(&mut relations);
    for (table, line) in relations.found {
        facts.push(site(
            ContractKind::Table,
            &table,
            Role::Calls,
            line.unwrap_or(start),
            VIA,
        ));
    }
}

/// Collects the tables of a query: its relations without the common table expressions it defines and without
/// the table functions it calls.
#[derive(Default)]
struct Relations {
    found: Vec<(String, Option<Line>)>,
    ctes: BTreeSet<String>,
    functions: BTreeSet<String>,
}

impl Visitor for Relations {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        // A query is visited before its body, so the names a `WITH` defines are known when the body is read.
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.ctes.insert(cte.alias.name.value.to_lowercase());
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        // `FROM generate_series(1, 3)` is a table factor with arguments, visited before the name it holds.
        if let TableFactor::Table {
            name,
            args: Some(_),
            ..
        } = factor
            && let Some((function, _)) = table_of(name)
        {
            self.functions.insert(function.to_lowercase());
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        if let Some((table, line)) = table_of(relation) {
            let lowered = table.to_lowercase();
            let is_cte = relation.0.len() == 1 && self.ctes.contains(&lowered);
            if !is_cte && !self.functions.contains(&lowered) {
                self.found.push((table.to_owned(), line));
            }
        }
        ControlFlow::Continue(())
    }
}

/// What a placeholder is replaced with: a value that every grammar reads wherever an operand stands, a list of
/// one included (`IN %s`).
const PARAMETER: &str = "(0)";

/// Replaces the placeholders of the common drivers, `?`, `?1`, `%s`, `%(name)s` and `:name`, with a value.
/// Nothing else changes, newlines included, so lines stay where they are. The `::` of a cast and a colon that
/// continues a word, as in `'10:30'`, are left alone.
fn neutralise_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut previous = '\n';
    while let Some(c) = chars.next() {
        let replaced = match c {
            '?' => {
                while chars.next_if(char::is_ascii_digit).is_some() {}
                true
            }
            '%' if chars.next_if_eq(&'s').is_some() => true,
            '%' if chars.peek() == Some(&'(') => {
                // `%(name)s`: look ahead on a copy, and only consume when the whole shape is there.
                let mut ahead = chars.clone();
                ahead.next();
                let mut named = false;
                while ahead
                    .next_if(|c| c.is_alphanumeric() || *c == '_')
                    .is_some()
                {
                    named = true;
                }
                if named && ahead.next_if_eq(&')').is_some() && ahead.next_if_eq(&'s').is_some() {
                    chars = ahead;
                    true
                } else {
                    false
                }
            }
            ':' if previous != ':'
                && !(previous.is_alphanumeric() || previous == '_')
                && chars
                    .peek()
                    .is_some_and(|next| next.is_alphanumeric() || *next == '_') =>
            {
                while chars
                    .next_if(|next| next.is_alphanumeric() || *next == '_')
                    .is_some()
                {}
                true
            }
            _ => false,
        };
        if replaced {
            out.push_str(PARAMETER);
            previous = ')';
        } else {
            out.push(c);
            previous = c;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn read_in(dialect: &str, text: &str) -> (Vec<(Line, String, Role)>, ParseStatus) {
        let dialect = dialect_from_str(dialect).unwrap();
        let (facts, status) = read(dialect.as_ref(), text);
        let sites = facts
            .into_iter()
            .map(|fact| (fact.line, fact.key.to_string(), fact.role))
            .collect();
        (sites, status)
    }

    fn calls(dialect: &str, text: &str) -> Vec<(Line, String, Role)> {
        read_in(dialect, text).0
    }

    fn call(line: Line, table: &str) -> (Line, String, Role) {
        (line, format!("table {table}"), Role::Calls)
    }

    fn recovered(errors: u32, first_error_line: Line) -> ParseStatus {
        ParseStatus::Recovered {
            errors,
            first_error_line,
        }
    }

    #[test]
    fn every_listed_dialect_is_known() {
        for name in DIALECTS {
            assert!(dialect_from_str(name).is_some(), "{name}");
        }
    }

    #[test]
    fn placeholders_become_a_value_and_casts_stay() {
        assert_eq!(
            neutralise_placeholders(
                "a = %s AND b = %(user_id)s AND c = :id AND d::int AND t = '10:30' AND e = ?2"
            ),
            "a = (0) AND b = (0) AND c = (0) AND d::int AND t = '10:30' AND e = (0)"
        );
        assert_eq!(neutralise_placeholders("100%"), "100%");
        assert_eq!(neutralise_placeholders("%(x"), "%(x");
    }

    #[test]
    fn native_syntax_is_tried_before_placeholders() {
        // The jsonb operator `?` reads as itself in PostgreSQL; the same character is a placeholder in a statement
        // that does not parse with it.
        let operator = calls("postgresql", "SELECT 1 FROM docs WHERE data ? 'key';");
        assert_eq!(operator, vec![(1, "table docs".to_owned(), Role::Calls)]);
        let placeholder = calls("postgresql", "SELECT 1 FROM docs WHERE id = ?;");
        assert_eq!(placeholder, vec![(1, "table docs".to_owned(), Role::Calls)]);
    }

    #[test]
    fn a_statement_with_placeholders_keeps_its_own_line() {
        let text = "SELECT 1 FROM a;\n\n\nSELECT 1 FROM b WHERE id = :id;\n";
        assert_eq!(
            calls("postgresql", text),
            vec![
                (1, "table a".to_owned(), Role::Calls),
                (4, "table b".to_owned(), Role::Calls),
            ]
        );
    }

    #[test]
    fn table_functions_are_not_tables() {
        let facts = calls(
            "postgresql",
            "SELECT * FROM generate_series(1, 3) AS g JOIN items ON true;",
        );
        let keys: Vec<&str> = facts.iter().map(|(_, key, _)| key.as_str()).collect();
        assert_eq!(keys, vec!["table items"]);
    }

    #[test]
    fn writes_and_views_call_the_tables_they_use() {
        let text = "INSERT INTO audit SELECT * FROM bookings;\nUPDATE items SET a = 1;\nDELETE FROM carts;\nCREATE VIEW v AS SELECT * FROM prices;\nDROP TABLE IF EXISTS old_things;\nCREATE INDEX idx ON bookings (id);\n";
        let got: Vec<(Line, String, Role)> = calls("postgresql", text);
        let expected = vec![
            (1, "table audit".to_owned(), Role::Calls),
            (1, "table bookings".to_owned(), Role::Calls),
            (2, "table items".to_owned(), Role::Calls),
            (3, "table carts".to_owned(), Role::Calls),
            (4, "table prices".to_owned(), Role::Calls),
            (4, "table v".to_owned(), Role::Declares),
            (5, "table old_things".to_owned(), Role::Declares),
            (6, "table bookings".to_owned(), Role::Declares),
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn an_unterminated_string_costs_only_its_statement() {
        let text = "SELECT * FROM a;\nSELECT 'oops FROM b;\nSELECT * FROM c;";
        assert_eq!(
            read_in("postgresql", text),
            (vec![call(1, "a"), call(3, "c")], recovered(1, 2))
        );
    }

    #[test]
    fn an_unterminated_comment_ends_the_file() {
        // Nothing after the opening of the comment is a statement, so only the statements before it are read.
        let text = "SELECT * FROM a;\nSELECT 1 /* oops\nSELECT * FROM b;";
        assert_eq!(
            read_in("postgresql", text),
            (vec![call(1, "a")], recovered(1, 2))
        );
    }

    #[test]
    fn every_statement_the_tokenizer_stops_in_is_counted() {
        let text = "SELECT 'x FROM a;\nSELECT * FROM b;\nSELECT \"y FROM c;\nSELECT * FROM d;";
        assert_eq!(
            read_in("postgresql", text),
            (vec![call(2, "b"), call(4, "d")], recovered(2, 1))
        );
    }

    #[test]
    fn a_statement_across_lines_is_counted_at_its_first_line() {
        let text = "SELECT *\nFROM a\nWHERE x = 'oops;\nSELECT * FROM b;";
        assert_eq!(
            read_in("postgresql", text),
            (vec![call(4, "b")], recovered(1, 1))
        );
    }

    #[test]
    fn columns_are_counted_in_characters_when_reading_resumes() {
        // The quote is the 14th character and the 18th byte of the line; counting bytes would resume too late.
        let text = "SELECT 'ü€' FROM a; SELECT 'ü€ FROM b; SELECT * FROM c;";
        assert_eq!(
            read_in("postgresql", text),
            (vec![call(1, "a"), call(1, "c")], recovered(1, 1))
        );
    }

    #[test]
    fn a_text_that_is_one_open_string_has_no_sites() {
        assert_eq!(read_in("postgresql", "'oops"), (vec![], recovered(1, 1)));
        assert_eq!(
            read_in("postgresql", "\n\n  -- c\n'oops"),
            (vec![], recovered(1, 4))
        );
    }

    #[test]
    fn a_tokenizer_location_is_a_line_of_newlines_and_a_column_of_characters() {
        let text = "ab\nüx\ny";
        assert_eq!(offset_of(text, Location::new(1, 1)), 0);
        assert_eq!(offset_of(text, Location::new(2, 2)), 5);
        assert_eq!(offset_of(text, Location::new(3, 1)), 7);
        assert_eq!(offset_of(text, Location::new(9, 9)), text.len());
        assert_eq!(
            after_semicolon("a;\nb;c", Location::new(1, 1)),
            Some(("\nb;c", 0))
        );
        assert_eq!(
            after_semicolon("a;\nb;c", Location::new(2, 1)),
            Some(("c", 1))
        );
        assert_eq!(after_semicolon("a;\nb", Location::new(2, 1)), None);
    }

    proptest! {
        #[test]
        fn neutralising_keeps_every_line_where_it_was(text in "\\PC*\n?\\PC*") {
            let neutral = neutralise_placeholders(&text);
            prop_assert_eq!(neutral.matches('\n').count(), text.matches('\n').count());
        }

        #[test]
        fn arbitrary_text_never_panics_and_keeps_lines_in_range(text in "[ -~\n]*") {
            let lines = u32::try_from(text.matches('\n').count() + 1).unwrap_or(u32::MAX);
            for fact in read(&PostgreSqlDialect {}, &text).0 {
                prop_assert!(fact.line >= 1 && fact.line <= lines);
            }
        }

        #[test]
        fn broken_quoting_never_takes_the_statements_before_it(
            pieces in prop::collection::vec(
                prop::sample::select(vec![
                    "SELECT * FROM t", ";", "'", "\"", "`", "/*", "*/", "--", "$$", "\n", " ", "x", "é",
                ]),
                0..24,
            ),
        ) {
            let tail = pieces.concat();
            let text = format!("SELECT * FROM kept;\n{tail}");
            let lines = u32::try_from(text.matches('\n').count() + 1).unwrap_or(u32::MAX);
            let (facts, _) = read(&PostgreSqlDialect {}, &text);
            prop_assert!(facts.iter().any(|fact| fact.line == 1 && fact.key.to_string() == "table kept"));
            prop_assert!(facts.iter().all(|fact| (1..=lines).contains(&fact.line)));
        }
    }
}
