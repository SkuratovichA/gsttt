//! SQL files: migrations and queries, and the SQL other connectors find embedded in code.
//!
//! `CREATE`, `ALTER` and `DROP` of a table or view, and `CREATE INDEX … ON`, declare `table <name>`; queries and
//! statements that change rows call the tables they name, leaving out the names of common table expressions.
//! Statements are read one by one, so a statement that does not parse is counted and the others still count.

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
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use crate::common::{Errors, any_claimed, provide, salt_of, shift_to_host, site, syntax_failure};
use crate::tree::SyntaxError;

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

    fn detect(&self, repo: &RepoView) -> bool {
        any_claimed(repo, claims)
    }

    fn open(
        &self,
        repo: &RepoView,
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
        Ok(if any_claimed(repo, claims) {
            vec![SqlProject { dialect }]
        } else {
            Vec::new()
        })
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
        salt_of(&[&self.dialect])
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
        match read(self.grammar().as_ref(), file.text) {
            Ok((contracts, status)) => Parsed::new(FileFacts {
                status,
                contracts,
                ..FileFacts::default()
            }),
            Err(error) => Parsed::new(syntax_failure(error.line, error.message)),
        }
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
        // The pipeline hands lines back as they are, so the offset to the host file is this connector's job.
        read(self.grammar().as_ref(), &document.text).map_or_else(
            |_| Vec::new(),
            |(facts, _)| shift_to_host(facts, document.line),
        )
    }
}

/// The sites of a text of SQL, and how well it was read.
fn read(
    dialect: &dyn Dialect,
    text: &str,
) -> Result<(Vec<ContractFact>, ParseStatus), SyntaxError> {
    let neutral = neutralise_placeholders(text);
    let tokens = Tokenizer::new(dialect, &neutral)
        .tokenize_with_location()
        .map_err(|error| SyntaxError {
            line: u32::try_from(error.location.line)
                .unwrap_or(u32::MAX)
                .max(1),
            message: error.message,
        })?;
    let mut facts = Vec::new();
    let mut errors = Errors::default();
    for group in statements(tokens) {
        let Some(first) = group
            .iter()
            .find(|token| !matches!(token.token, Token::Whitespace(_)))
        else {
            continue;
        };
        let start = u32::try_from(first.span.start.line)
            .unwrap_or(u32::MAX)
            .max(1);
        let mut parser = Parser::new(dialect).with_tokens_with_locations(group);
        match parser.parse_statement() {
            Ok(statement) if parser.peek_token().token == Token::EOF => {
                read_statement(&statement, start, &mut facts);
            }
            _ => errors.note(start),
        }
    }
    // Source order, the same everywhere; one site per line and key.
    facts.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.key.cmp(&b.key)));
    facts.dedup_by(|a, b| a.line == b.line && a.key == b.key && a.role == b.role);
    Ok((facts, errors.status()))
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

/// Replaces the placeholders of the common drivers, `%s`, `%(name)s` and `:name`, with `?`, which the grammar
/// reads as a parameter, as it does `$1` and `?` themselves. Nothing else changes, newlines included, so lines
/// stay where they are. The `::` of a cast and a colon that continues a word, as in `'10:30'`, are left alone.
fn neutralise_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut previous = '\n';
    while let Some(c) = chars.next() {
        let replaced = match c {
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
            out.push('?');
            previous = '?';
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

    fn calls(dialect: &str, text: &str) -> Vec<(Line, String, Role)> {
        let dialect = dialect_from_str(dialect).unwrap();
        read(dialect.as_ref(), text)
            .unwrap()
            .0
            .into_iter()
            .map(|fact| (fact.line, fact.key.to_string(), fact.role))
            .collect()
    }

    #[test]
    fn every_listed_dialect_is_known() {
        for name in DIALECTS {
            assert!(dialect_from_str(name).is_some(), "{name}");
        }
    }

    #[test]
    fn placeholders_become_one_marker_and_casts_stay() {
        assert_eq!(
            neutralise_placeholders(
                "a = %s AND b = %(user_id)s AND c = :id AND d::int AND t = '10:30'"
            ),
            "a = ? AND b = ? AND c = ? AND d::int AND t = '10:30'"
        );
        assert_eq!(neutralise_placeholders("100%"), "100%");
        assert_eq!(neutralise_placeholders("%(x"), "%(x");
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

    proptest! {
        #[test]
        fn neutralising_keeps_every_line_where_it_was(text in "\\PC*\n?\\PC*") {
            let neutral = neutralise_placeholders(&text);
            prop_assert_eq!(neutral.matches('\n').count(), text.matches('\n').count());
        }

        #[test]
        fn arbitrary_text_never_panics_and_keeps_lines_in_range(text in "[ -~\n]*") {
            let lines = u32::try_from(text.lines().count().max(1)).unwrap_or(u32::MAX);
            if let Ok((facts, _)) = read(&PostgreSqlDialect {}, &text) {
                for fact in facts {
                    prop_assert!(fact.line >= 1 && fact.line <= lines);
                }
            }
        }
    }
}
