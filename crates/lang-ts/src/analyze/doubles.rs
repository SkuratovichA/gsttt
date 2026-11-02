//! The injection tokens a test file replaces with a stand-in, found before the walk: `{ provide: ConfigService, useValue: stub }` and `.overrideProvider(ConfigService)`.

use std::collections::BTreeSet;

use gsttt_core::fact::FileTags;
use gsttt_core::rules::Pack;
use oxc_ast::ast::{
    CallExpression, Expression, MemberExpression, ObjectExpression, ObjectPropertyKind, Program,
};
use oxc_ast_visit::{Visit, walk};

use crate::shape::{dotted_name, property_str};

/// The properties of a provider that say what stands in for the token.
const STAND_INS: [&str; 4] = ["useValue", "useFactory", "useClass", "useExisting"];

/// The method of a testing module that replaces a provider.
const OVERRIDE: &str = "overrideProvider";

/// The tokens of the rules of `packs` that the test file `program`, written as `text`, provides a stand-in for. A file that is no test keeps its reads, because a stand-in there may be how production reads its settings.
pub(super) fn collect<'p>(
    program: &Program<'_>,
    text: &str,
    packs: &'p [Pack],
    tags: FileTags,
) -> BTreeSet<&'p str> {
    if !tags.contains(FileTags::TEST) {
        return BTreeSet::new();
    }
    let wanted: BTreeSet<&'p str> = packs
        .iter()
        .flat_map(|pack| &pack.calls)
        .filter_map(|rule| rule.test_double.as_deref())
        .filter(|token| text.contains(token))
        .collect();
    if wanted.is_empty() {
        return BTreeSet::new();
    }
    let mut collector = Collector {
        wanted,
        found: BTreeSet::new(),
    };
    collector.visit_program(program);
    collector.found
}

struct Collector<'p> {
    wanted: BTreeSet<&'p str>,
    found: BTreeSet<&'p str>,
}

impl<'p> Collector<'p> {
    /// The wanted token that `expr` names, as an identifier or the last segment of a chain. A string is another Nest token and names nothing.
    fn token(&self, expr: &Expression<'_>) -> Option<&'p str> {
        let name = dotted_name(expr)?;
        let last = name.rsplit('.').next().unwrap_or(&name);
        self.wanted.get(last).copied()
    }
}

impl<'a> Visit<'a> for Collector<'_> {
    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        let mut token = None;
        let mut stands_in = false;
        for property in &it.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                continue;
            };
            match property_str(&property.key) {
                Some("provide") => token = self.token(&property.value),
                Some(name) => stands_in |= STAND_INS.contains(&name),
                None => {}
            }
        }
        if let (Some(token), true) = (token, stands_in) {
            self.found.insert(token);
        }
        walk::walk_object_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let overrides = it
            .callee
            .get_member_expr()
            .and_then(MemberExpression::static_property_name)
            .is_some_and(|name| name == OVERRIDE);
        if overrides
            && let Some(token) = it
                .arguments
                .first()
                .and_then(|argument| argument.as_expression())
                .and_then(|argument| self.token(argument))
        {
            self.found.insert(token);
        }
        walk::walk_call_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    use super::*;

    fn pack() -> Vec<Pack> {
        let rule = r#"
name = "fixture"
language = "ts"

[[calls]]
callee = "configService.get"
kind = "env"
role = "calls"
key = "$arg1"
test_double = "ConfigService"
"#;
        vec![gsttt_config::read_pack("fixture.toml", rule).unwrap()]
    }

    /// Whether `code`, as a test file, provides a stand-in for `ConfigService`.
    fn doubled(code: &str) -> bool {
        let packs = pack();
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        !collect(&parsed.program, code, &packs, FileTags::TEST).is_empty()
    }

    #[test]
    fn a_stand_in_is_found() {
        for code in [
            "x = { provide: ConfigService, useValue: stub }",
            "x = { provide: ConfigService, useFactory: () => stub }",
            "x = { useClass: Fake, provide: ConfigService }",
            "x = { provide: ConfigService, useExisting: Other }",
            "x = { provide: config.ConfigService, useValue: stub }",
            "x = { provide: ConfigService as never, useValue: stub }",
            "x = { 'provide': ConfigService, useValue: stub }",
            "Test.createTestingModule({}).overrideProvider(ConfigService).useValue(stub)",
            "const m = { providers: [{ provide: ConfigService, useValue: stub }] }",
        ] {
            assert!(doubled(code), "{code}");
        }
    }

    #[test]
    fn no_stand_in_is_found_in_what_only_uses_the_service() {
        for code in [
            "x = { provide: ConfigService }",
            "x = { provide: 'ConfigService', useValue: stub }",
            "x = { provide: BillingService, useValue: stub }",
            "x = { providers: [ConfigService] }",
            "x = { provide: ConfigService, ...rest }",
            "foo.overrideProvider()",
            "foo.overrideProvider(BillingService)",
            "overrideProvider(ConfigService)",
            "x = { useValue: ConfigService }",
        ] {
            assert!(!doubled(code), "{code}");
        }
    }

    #[test]
    fn a_file_that_is_no_test_has_no_stand_in() {
        let packs = pack();
        let code = "x = { provide: ConfigService, useValue: stub }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        assert!(collect(&parsed.program, code, &packs, FileTags::NONE).is_empty());
    }
}
