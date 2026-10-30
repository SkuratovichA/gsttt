//! The arguments of each tool, and the reading of a call's arguments into them.
//!
//! A struct here is the whole contract of a tool's arguments: its fields and their doc comments become the input schema the model reads, and the same struct reads the call. A call that does not fit is an error the model can correct, so the reading names the argument and what it expects instead of passing serde's wording on.

use gsttt_core::text::join_and;
use rmcp::model::JsonObject;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_path_to_error::Segment;

use crate::error::ToolError;
use crate::tools::ToolName;

/// One argument of a tool, as the messages about it need it.
#[derive(Debug)]
pub(crate) struct Field {
    pub(crate) name: &'static str,
    /// Whether a call has to give it.
    pub(crate) required: bool,
    /// What a value looks like, to finish "`name` must be …".
    pub(crate) expects: &'static str,
}

impl Field {
    const fn required(name: &'static str, expects: &'static str) -> Self {
        Self {
            name,
            required: true,
            expects,
        }
    }

    const fn optional(name: &'static str, expects: &'static str) -> Self {
        Self {
            name,
            required: false,
            expects,
        }
    }
}

const PATHS: &str = "a list of paths, for example [\"src/a.ts\"]";
const NAMES: &str = "a list of names, for example [\"getHash\", \"Runner.start\"]";
const PATH: &str = "a path from the repository root, for example \"src/a.ts\"";
const FLAG: &str = "true or false";

/// The arguments of one tool, read from the JSON object of a call.
pub(crate) trait Arguments: DeserializeOwned + JsonSchema + 'static {
    /// The arguments the tool takes. A test pins this list against the schema, so that the two cannot drift.
    const FIELDS: &'static [Field];
}

/// What `impact` is asked about.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImpactArguments {
    /// The files to ask about, as paths from the repository root, such as `src/a.ts`. A path with no file on disk is asked about as deleted, so that the answer can still list what used to import it. Leave it out when `changed` is true.
    #[serde(default)]
    pub(crate) paths: Vec<String>,
    /// Ask about the files that differ from HEAD instead of naming paths: the modified, added and untracked files, and the deleted ones as deleted. Use it before finishing a task.
    #[serde(default)]
    pub(crate) changed: bool,
    /// With `changed`: compare against the best common ancestor of HEAD and this git revision, such as a branch name, instead of against HEAD.
    pub(crate) base: Option<String>,
    /// Stop the walk after this many hops through code from a changed file: the imports, and with `names` the uses of a name. A hop across a contract does not count, so the walk goes on to the clients of every contract it comes to. Leave it out to follow every hop, or, with `names`, to take one: the direct callers.
    pub(crate) depth: Option<u32>,
    /// With `paths`: ask about these names, each a function, class, method or variable that the files declare, instead of about the files as a whole. A method is written `Class.method`. The answer lists the direct callers of the names, the declarations that refer to them, each with the line it starts on, so ask again about a caller to go further. A name that a file does not declare is said so in the answer and reaches nothing. It cannot go with `changed`.
    #[serde(default)]
    pub(crate) names: Vec<String>,
    /// Show every entry of each list, as far as the 25,000 characters of an answer allow. Without it a long list stops at 30 entries. A cut list ends with a line that counts the rest.
    #[serde(default)]
    pub(crate) all: bool,
    /// Whether to list the files tagged as tests, which is what happens when this is left out. False leaves them out of the answer and says how many it left out, to save context when they do not matter; the walk is the same.
    #[serde(default = "on_by_default")]
    pub(crate) tests: bool,
}

/// What a flag that is on unless the call turns it off reads when it is left out.
fn on_by_default() -> bool {
    true
}

impl Arguments for ImpactArguments {
    const FIELDS: &'static [Field] = &[
        Field::optional("paths", PATHS),
        Field::optional("changed", FLAG),
        Field::optional("base", "a git revision, such as a branch name"),
        Field::optional("depth", "a whole number, for example 2"),
        Field::optional("names", NAMES),
        Field::optional("all", FLAG),
        Field::optional("tests", FLAG),
    ];
}

/// What `dependents` and `dependencies` are asked about: one file.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NeighbourArguments {
    /// The file to ask about, as a path from the repository root, for example "src/a.ts".
    pub(crate) path: String,
    /// Show every entry of each list, as far as the 25,000 characters of an answer allow. Without it a long list stops at 30 entries. A cut list ends with a line that counts the rest.
    #[serde(default)]
    pub(crate) all: bool,
}

impl Arguments for NeighbourArguments {
    const FIELDS: &'static [Field] = &[Field::required("path", PATH), Field::optional("all", FLAG)];
}

/// What `path` is asked about: two files.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RouteArguments {
    /// The file the chain starts at, as a path from the repository root, for example "src/a.ts".
    pub(crate) from: String,
    /// The file the chain ends at, as a path from the repository root, for example "src/b.ts".
    pub(crate) to: String,
    /// Show every file of the chain, as far as the 25,000 characters of an answer allow. Without it a long chain stops at 30 entries. A cut list ends with a line that counts the rest.
    #[serde(default)]
    pub(crate) all: bool,
}

impl Arguments for RouteArguments {
    const FIELDS: &'static [Field] = &[
        Field::required("from", PATH),
        Field::required("to", PATH),
        Field::optional("all", FLAG),
    ];
}

/// What `contract` is asked about.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractArguments {
    /// A contract key such as "http GET /bookings/{}", or part of one, such as "/bookings", which lists the keys that contain it.
    pub(crate) query: String,
    /// Show every entry of each list, as far as the 25,000 characters of an answer allow. Without it a long list stops at 30 entries. A cut list ends with a line that counts the rest.
    #[serde(default)]
    pub(crate) all: bool,
}

impl Arguments for ContractArguments {
    const FIELDS: &'static [Field] = &[
        Field::required(
            "query",
            "text: a contract key such as \"http GET /bookings/{}\", or part of one",
        ),
        Field::optional("all", FLAG),
    ];
}

/// What `coverage` is asked.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CoverageArguments {
    /// Show every entry of each list, as far as the 25,000 characters of an answer allow. Without it a long list stops at 30 entries. A cut list ends with a line that counts the rest.
    #[serde(default)]
    pub(crate) all: bool,
}

impl Arguments for CoverageArguments {
    const FIELDS: &'static [Field] = &[Field::optional("all", FLAG)];
}

/// The arguments of a tool that takes none.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoArguments {}

impl Arguments for NoArguments {
    const FIELDS: &'static [Field] = &[];
}

/// What `impact` asks about: the files that were named, or the files that changed.
#[derive(Debug)]
pub(crate) enum Subject {
    /// These paths, as the model wrote them, and the names they declare that the question is about, none for the files as a whole. Only named files can come with names, because which declarations a diff changes is not derived.
    Paths {
        paths: Vec<String>,
        names: Vec<String>,
    },
    /// The files that differ from HEAD, or from the best common ancestor of HEAD and `base`.
    Changed { base: Option<String> },
}

/// An `impact` call whose arguments agree with each other.
#[derive(Debug)]
pub(crate) struct ImpactCall {
    pub(crate) subject: Subject,
    pub(crate) depth: Option<u32>,
    pub(crate) all: bool,
    pub(crate) tests: bool,
}

impl TryFrom<ImpactArguments> for ImpactCall {
    type Error = ToolError;

    fn try_from(arguments: ImpactArguments) -> Result<Self, ToolError> {
        let ImpactArguments {
            paths,
            changed,
            base,
            depth,
            names,
            all,
            tests,
        } = arguments;
        let subject = match (paths.is_empty(), changed) {
            (false, true) => return Err(ToolError::PathsAndChanged),
            (true, true) if !names.is_empty() => return Err(ToolError::NamesWithChanged),
            (true, _) if !names.is_empty() => return Err(ToolError::NamesWithoutPaths),
            (true, false) => return Err(ToolError::NothingAsked),
            (false, false) if base.is_some() => return Err(ToolError::BaseWithoutChanged),
            (false, false) if names.iter().any(|name| name.trim().is_empty()) => {
                return Err(ToolError::BlankName);
            }
            (false, false) => Subject::Paths { paths, names },
            (true, true) => Subject::Changed { base },
        };
        Ok(Self {
            subject,
            depth,
            all,
            tests,
        })
    }
}

/// A call to a known tool whose arguments were read.
#[derive(Debug)]
pub(crate) enum Call {
    Impact(ImpactCall),
    Dependents(NeighbourArguments),
    Dependencies(NeighbourArguments),
    Path(RouteArguments),
    Contract(ContractArguments),
    Coverage(CoverageArguments),
    IndexStatus,
}

impl Call {
    /// Reads the arguments of a call to `tool`.
    ///
    /// # Errors
    ///
    /// The argument that is unknown, missing, of the wrong type, or in conflict with another, as a sentence that names it.
    pub(crate) fn read(tool: ToolName, arguments: Option<JsonObject>) -> Result<Self, ToolError> {
        Ok(match tool {
            ToolName::Impact => Self::Impact(read::<ImpactArguments>(tool, arguments)?.try_into()?),
            ToolName::Dependents => Self::Dependents(read(tool, arguments)?),
            ToolName::Dependencies => Self::Dependencies(read(tool, arguments)?),
            ToolName::Path => Self::Path(read(tool, arguments)?),
            ToolName::Contract => Self::Contract(read(tool, arguments)?),
            ToolName::Coverage => Self::Coverage(read(tool, arguments)?),
            ToolName::IndexStatus => {
                read::<NoArguments>(tool, arguments)?;
                Self::IndexStatus
            }
        })
    }
}

/// Reads the arguments of a call into `A`.
///
/// The names are checked before serde reads the values, because serde's errors about a name or a missing field carry no path, and the model needs to be told which argument to correct.
fn read<A: Arguments>(tool: ToolName, arguments: Option<JsonObject>) -> Result<A, ToolError> {
    let object = arguments.unwrap_or_default();
    if let Some(unknown) = object
        .keys()
        .find(|name| !A::FIELDS.iter().any(|field| field.name == name.as_str()))
    {
        return Err(ToolError::UnknownArgument {
            tool,
            argument: unknown.clone(),
            takes: takes(A::FIELDS),
        });
    }
    if let Some(field) = A::FIELDS
        .iter()
        .find(|field| field.required && !object.contains_key(field.name))
    {
        return Err(ToolError::MissingArgument {
            tool,
            argument: field.name,
            expects: field.expects,
        });
    }
    serde_path_to_error::deserialize(Value::Object(object)).map_err(|error| {
        let culprit = error.path().iter().find_map(|segment| match segment {
            Segment::Map { key } => A::FIELDS.iter().find(|field| field.name == key),
            _ => None,
        });
        match culprit {
            Some(field) => ToolError::WrongArgument {
                tool,
                argument: field.name,
                expects: field.expects,
            },
            None => ToolError::Unreadable {
                tool,
                detail: error.into_inner().to_string(),
            },
        }
    })
}

/// The arguments a tool takes, as the end of a sentence.
fn takes(fields: &[Field]) -> String {
    if fields.is_empty() {
        return "it takes no arguments".to_owned();
    }
    let names: Vec<String> = fields
        .iter()
        .map(|field| format!("`{}`", field.name))
        .collect();
    format!("it takes {}", join_and(&names))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use proptest::prelude::*;
    use rmcp::handler::server::common::schema_for_input;
    use rmcp::model::object;
    use serde_json::json;

    use super::*;

    /// The names a schema lists as properties, and those it lists as required.
    fn schema_names<A: Arguments>() -> (BTreeSet<String>, BTreeSet<String>) {
        let schema =
            schema_for_input::<A>().expect("a struct of named fields has an object schema");
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default();
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        (properties, required)
    }

    fn assert_fields_match_schema<A: Arguments>() {
        let (properties, required) = schema_names::<A>();
        let fields: BTreeSet<String> = A::FIELDS
            .iter()
            .map(|field| field.name.to_owned())
            .collect();
        let needed: BTreeSet<String> = A::FIELDS
            .iter()
            .filter(|field| field.required)
            .map(|field| field.name.to_owned())
            .collect();
        assert_eq!(
            fields, properties,
            "the arguments the messages know are not the ones the schema offers"
        );
        assert_eq!(
            needed, required,
            "the arguments the messages require are not the ones the schema requires"
        );
    }

    #[test]
    fn the_fields_the_messages_know_are_the_properties_of_the_schema() {
        assert_fields_match_schema::<ImpactArguments>();
        assert_fields_match_schema::<NeighbourArguments>();
        assert_fields_match_schema::<RouteArguments>();
        assert_fields_match_schema::<ContractArguments>();
        assert_fields_match_schema::<CoverageArguments>();
        assert_fields_match_schema::<NoArguments>();
    }

    #[test]
    fn a_tool_without_required_arguments_reads_a_call_with_no_arguments_at_all() {
        assert!(matches!(
            Call::read(ToolName::Coverage, None),
            Ok(Call::Coverage(_))
        ));
        assert!(matches!(
            Call::read(ToolName::IndexStatus, None),
            Ok(Call::IndexStatus)
        ));
        assert!(matches!(
            Call::read(ToolName::Dependents, None),
            Err(ToolError::MissingArgument {
                argument: "path",
                ..
            })
        ));
    }

    #[test]
    fn null_stands_for_an_optional_argument_that_was_left_out() {
        let call = Call::read(
            ToolName::Impact,
            Some(object(
                json!({ "changed": true, "base": null, "depth": null }),
            )),
        );

        let Ok(Call::Impact(ImpactCall {
            subject: Subject::Changed { base: None },
            depth: None,
            all: false,
            tests: true,
        })) = call
        else {
            panic!("expected changed files with no base and no depth, got {call:?}");
        };
    }

    #[test]
    fn names_go_with_the_paths_they_are_declared_in() {
        let read = |arguments: Value| Call::read(ToolName::Impact, Some(object(arguments)));

        let Ok(Call::Impact(ImpactCall {
            subject: Subject::Paths { paths, names },
            ..
        })) = read(json!({ "paths": ["a.ts"], "names": ["foo", "Runner.start"] }))
        else {
            panic!("expected named paths with names");
        };
        assert_eq!(
            (paths, names),
            (
                vec!["a.ts".to_owned()],
                vec!["foo".to_owned(), "Runner.start".to_owned()]
            )
        );
        assert!(matches!(
            read(json!({ "changed": true, "names": ["foo"] })),
            Err(ToolError::NamesWithChanged)
        ));
        assert!(matches!(
            read(json!({ "names": ["foo"] })),
            Err(ToolError::NamesWithoutPaths)
        ));
        assert!(matches!(
            read(json!({ "paths": ["a.ts"], "changed": true, "names": ["foo"] })),
            Err(ToolError::PathsAndChanged)
        ));
    }

    /// Any JSON value, for the arguments of a client that sends what it likes.
    fn any_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::from),
            any::<i64>().prop_map(Value::from),
            any::<f64>()
                .prop_map(|number| serde_json::Number::from_f64(number)
                    .map_or(Value::Null, Value::Number)),
            "[ -~]{0,8}".prop_map(Value::from),
        ];
        leaf.prop_recursive(3, 24, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
                prop::collection::btree_map("[a-z]{0,4}", inner, 0..4)
                    .prop_map(|entries| Value::Object(entries.into_iter().collect())),
            ]
        })
    }

    /// A value that is none of a boolean.
    fn not_a_flag() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<i64>().prop_map(Value::from),
            "[a-z]{0,6}".prop_map(Value::from),
            Just(Value::Null),
            Just(json!([])),
            Just(json!({})),
        ]
    }

    /// A value that is neither a string nor the null that leaves an optional argument out.
    fn not_text() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<i64>().prop_map(Value::from),
            any::<bool>().prop_map(Value::from),
            Just(json!([])),
            Just(json!({})),
        ]
    }

    /// A value that is neither a whole number that fits in 32 bits nor the null that leaves `depth` out.
    fn not_a_count() -> impl Strategy<Value = Value> {
        prop_oneof![
            (i64::MIN..0).prop_map(Value::from),
            ((u64::from(u32::MAX) + 1)..=u64::MAX).prop_map(Value::from),
            (0.0f64..1000.0).prop_map(Value::from),
            "[0-9]{1,3}".prop_map(Value::from),
            any::<bool>().prop_map(Value::from),
            Just(json!([])),
            Just(json!({})),
        ]
    }

    /// A value that is not a list of strings.
    fn not_paths() -> impl Strategy<Value = Value> {
        prop_oneof![
            any::<i64>().prop_map(Value::from),
            "[a-z/.]{0,8}".prop_map(Value::from),
            any::<bool>().prop_map(Value::from),
            Just(Value::Null),
            Just(json!({})),
            not_text().prop_map(|element| json!(["src/a.ts", element])),
            Just(json!([null])),
        ]
    }

    /// One argument of `impact` with a value of the wrong type for it.
    fn mistyped() -> impl Strategy<Value = (&'static str, Value)> {
        prop_oneof![
            not_paths().prop_map(|value| ("paths", value)),
            not_a_flag().prop_map(|value| ("changed", value)),
            not_text().prop_map(|value| ("base", value)),
            not_a_count().prop_map(|value| ("depth", value)),
            not_paths().prop_map(|value| ("names", value)),
            not_a_flag().prop_map(|value| ("all", value)),
            not_a_flag().prop_map(|value| ("tests", value)),
        ]
    }

    proptest! {
        #[test]
        fn whatever_a_client_sends_the_reading_ends_in_a_call_or_a_sentence_that_leads_with_the_tool(
            entries in prop::collection::btree_map(
                prop_oneof![
                    Just("paths".to_owned()),
                    Just("changed".to_owned()),
                    Just("base".to_owned()),
                    Just("depth".to_owned()),
                    Just("names".to_owned()),
                    Just("all".to_owned()),
                    Just("tests".to_owned()),
                    "[a-z]{1,6}",
                ],
                any_json(),
                0..7,
            ),
        ) {
            let arguments: JsonObject = entries.into_iter().collect();

            if let Err(error) = Call::read(ToolName::Impact, Some(arguments)) {
                prop_assert!(error.to_string().starts_with("impact: "), "{error}");
            }
        }

        #[test]
        fn well_typed_arguments_conflict_exactly_when_the_rules_say(
            paths in prop::collection::vec("[a-z/.]{1,12}", 0..4),
            changed in any::<bool>(),
            base in proptest::option::of("[a-z]{1,8}"),
            depth in proptest::option::of(any::<u32>()),
            names in prop::collection::vec(prop_oneof!["[A-Za-z_.#]{1,12}", "[ \t]{0,3}"], 0..3),
            all in any::<bool>(),
            tests in any::<bool>(),
        ) {
            let arguments = object(json!({
                "paths": paths, "changed": changed, "base": base, "depth": depth, "names": names, "all": all, "tests": tests,
            }));

            let read = Call::read(ToolName::Impact, Some(arguments));
            let blank = names.iter().any(|name| name.chars().all(|letter| matches!(letter, ' ' | '\t')));

            // A blank name is the last rule, so each of the others wins over it when it applies.
            match (paths.is_empty(), changed, base.is_some(), names.is_empty(), blank) {
                (false, true, ..) => prop_assert!(matches!(read, Err(ToolError::PathsAndChanged)), "{read:?}"),
                (true, true, _, false, _) => prop_assert!(matches!(read, Err(ToolError::NamesWithChanged)), "{read:?}"),
                (true, false, _, false, _) => prop_assert!(matches!(read, Err(ToolError::NamesWithoutPaths)), "{read:?}"),
                (true, false, ..) => prop_assert!(matches!(read, Err(ToolError::NothingAsked)), "{read:?}"),
                (false, false, true, ..) => {
                    prop_assert!(matches!(read, Err(ToolError::BaseWithoutChanged)), "{read:?}");
                }
                (false, false, false, _, true) => {
                    prop_assert!(matches!(read, Err(ToolError::BlankName)), "{read:?}");
                }
                (false, false, false, _, false) => {
                    let Ok(Call::Impact(call)) = read else {
                        return Err(TestCaseError::fail(format!("expected named paths, got {read:?}")));
                    };
                    let Subject::Paths { paths: named, names: asked } = &call.subject else {
                        return Err(TestCaseError::fail(format!("expected named paths, got {:?}", call.subject)));
                    };
                    prop_assert_eq!(named, &paths);
                    prop_assert_eq!(asked, &names);
                    prop_assert_eq!((call.depth, call.all, call.tests), (depth, all, tests));
                }
                (true, true, ..) => {
                    let Ok(Call::Impact(call)) = read else {
                        return Err(TestCaseError::fail(format!("expected changed files, got {read:?}")));
                    };
                    let Subject::Changed { base: asked } = &call.subject else {
                        return Err(TestCaseError::fail(format!("expected changed files, got {:?}", call.subject)));
                    };
                    prop_assert_eq!(asked, &base);
                    prop_assert_eq!((call.depth, call.all, call.tests), (depth, all, tests));
                }
            }
        }

        #[test]
        fn a_value_of_the_wrong_type_is_blamed_on_its_own_argument((name, value) in mistyped()) {
            let mut arguments = object(json!({ "changed": true }));
            arguments.insert(name.to_owned(), value);

            let error = Call::read(ToolName::Impact, Some(arguments)).expect_err("the value has the wrong type");

            prop_assert!(
                matches!(&error, ToolError::WrongArgument { argument, .. } if *argument == name),
                "{error:?}"
            );
        }
    }
}
