//! What a run says in its stream: the final message, the tokens, the cost, the turns, and whether the account ran out of usage.

use std::collections::BTreeMap;

use serde_json::Value;

/// How the message that ends a run on the usage limit of the account starts, for each form of the limit Claude Code words. They are read from the Claude Code binary, not seen in a run, since a limit cannot be asked for.
const LIMIT_MESSAGES: [&str; 12] = [
    "You've hit your",
    "You've reached your",
    "You're out of usage credits",
    "Your org is out of usage \u{b7} add funds to continue",
    "Your org is out of usage \u{b7} contact your admin",
    "Your seat type doesn't include usage credits",
    "Your seat type doesn't include usage",
    "Your usage allocation has been disabled by your admin",
    "Your group's usage limit is set to $0",
    "Fable 5 requires usage credits",
    "You're out of extra usage",
    "Your seat type doesn't include extra usage",
];

/// The names the API gives the four counts of a message.
const MESSAGE_KEYS: [&str; 4] = [
    "input_tokens",
    "output_tokens",
    "cache_read_input_tokens",
    "cache_creation_input_tokens",
];

/// The names the final result gives the four counts of a model.
const MODEL_KEYS: [&str; 4] = [
    "inputTokens",
    "outputTokens",
    "cacheReadInputTokens",
    "cacheCreationInputTokens",
];

/// The tokens of the four kinds a run is billed for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Tokens {
    pub(super) input: u64,
    pub(super) output: u64,
    pub(super) cache_read: u64,
    pub(super) cache_write: u64,
}

impl Tokens {
    fn of(counts: Option<&Value>, [input, output, read, write]: [&str; 4]) -> Self {
        let count = |key: &str| {
            counts
                .and_then(|counts| counts.get(key))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        Self {
            input: count(input),
            output: count(output),
            cache_read: count(read),
            cache_write: count(write),
        }
    }

    /// The larger of each count: a message that streams is repeated with the counts it has reached so far.
    fn largest(self, other: Self) -> Self {
        Self {
            input: self.input.max(other.input),
            output: self.output.max(other.output),
            cache_read: self.cache_read.max(other.cache_read),
            cache_write: self.cache_write.max(other.cache_write),
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            input: self.input.saturating_add(other.input),
            output: self.output.saturating_add(other.output),
            cache_read: self.cache_read.saturating_add(other.cache_read),
            cache_write: self.cache_write.saturating_add(other.cache_write),
        }
    }
}

/// How the stream of a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum End {
    /// A final result that is not an error.
    Success,
    /// The run used its turns.
    MaxTurns,
    /// A final result that is an error.
    Failed,
    /// No final result: the process was stopped, or died.
    Unfinished,
}

/// What a stream says about its run.
#[derive(Debug, PartialEq)]
pub(super) struct Observed {
    pub(super) end: End,
    /// The text of the final message, when the run ended in success.
    pub(super) message: Option<String>,
    pub(super) tokens: Tokens,
    /// What Claude Code says the run cost; `None` when it ended without saying.
    pub(super) cost_usd: Option<f64>,
    pub(super) turns: u32,
    /// The account ran out of usage during the run.
    pub(super) usage_limit: bool,
}

/// Reads the lines of a transcript, skipping what is not JSON: the last line of a run that was killed may be half written.
///
/// The counts are those of the result when it has them, summed over the models. Without one, they are those of the messages streamed so far, each counted once at the largest it reached.
pub(super) fn observe(transcript: &str) -> Observed {
    let mut messages: BTreeMap<String, Tokens> = BTreeMap::new();
    let mut result = None;
    let mut usage_limit = false;
    let mut first_message = true;
    for line in transcript.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match string(&event, "type") {
            Some("assistant") => {
                let Some(message) = event.get("message") else {
                    continue;
                };
                if let Some(id) = string(message, "id") {
                    let tokens = Tokens::of(message.get("usage"), MESSAGE_KEYS);
                    messages
                        .entry(id.to_owned())
                        .and_modify(|seen| *seen = seen.largest(tokens))
                        .or_insert(tokens);
                }
                // Only the first message is the account's refusal. A later one that has the same words is the model quoting them, in a file it read or in the answer.
                if std::mem::take(&mut first_message) {
                    usage_limit |= first_text(message).is_some_and(is_limit);
                }
            }
            // `rejected` is a limit that stops the run; `allowed_warning` only says one is near.
            Some("rate_limit_event") => {
                usage_limit |= event
                    .pointer("/rate_limit_info/status")
                    .and_then(Value::as_str)
                    == Some("rejected");
            }
            Some("result") => result = Some(event),
            _ => {}
        }
    }

    let streamed = messages
        .values()
        .fold(Tokens::default(), |sum, tokens| sum.plus(*tokens));
    let streamed_turns = u32::try_from(messages.len()).unwrap_or(u32::MAX);
    let Some(result) = result else {
        return Observed {
            end: End::Unfinished,
            message: None,
            tokens: streamed,
            cost_usd: None,
            turns: streamed_turns,
            usage_limit,
        };
    };

    let is_error = result
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = string(&result, "result");
    usage_limit |= is_error && text.is_some_and(is_limit);
    let end = match string(&result, "subtype") {
        Some("error_max_turns") => End::MaxTurns,
        Some("success") if !is_error => End::Success,
        _ => End::Failed,
    };
    let message = match end {
        End::Success => text.map(str::to_owned),
        End::MaxTurns | End::Failed | End::Unfinished => None,
    };
    let tokens = result
        .get("modelUsage")
        .and_then(Value::as_object)
        .filter(|models| !models.is_empty())
        .map_or(streamed, |models| {
            models.values().fold(Tokens::default(), |sum, usage| {
                sum.plus(Tokens::of(Some(usage), MODEL_KEYS))
            })
        });
    Observed {
        end,
        message,
        tokens,
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
        turns: result
            .get("num_turns")
            .and_then(Value::as_u64)
            .map_or(streamed_turns, |turns| {
                u32::try_from(turns).unwrap_or(u32::MAX)
            }),
        usage_limit,
    }
}

/// The string at `key` of an event.
fn string<'a>(event: &'a Value, key: &str) -> Option<&'a str> {
    event.get(key)?.as_str()
}

/// The text of the first text block of a message.
fn first_text(message: &Value) -> Option<&str> {
    message
        .get("content")?
        .as_array()?
        .iter()
        .find(|block| string(block, "type") == Some("text"))
        .and_then(|block| string(block, "text"))
}

fn is_limit(text: &str) -> bool {
    LIMIT_MESSAGES
        .iter()
        .any(|prefix| text.trim_start().starts_with(prefix))
}

/// Events of a stream, for the tests of this module and of the runs.
#[cfg(test)]
pub(crate) mod events {
    use serde_json::{Value, json};

    pub(crate) fn lines(events: &[Value]) -> String {
        events
            .iter()
            .map(|event| event.to_string() + "\n")
            .collect()
    }

    pub(crate) fn assistant(id: &str, output: u64, text: &str) -> Value {
        json!({
            "type": "assistant",
            "message": {
                "id": id,
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": output,
                    "cache_read_input_tokens": 100,
                    "cache_creation_input_tokens": 10
                },
                "content": [{ "type": "text", "text": text }]
            }
        })
    }

    pub(crate) fn result(subtype: &str, is_error: bool, text: &str) -> Value {
        json!({
            "type": "result",
            "subtype": subtype,
            "is_error": is_error,
            "result": text,
            "total_cost_usd": 0.25,
            "num_turns": 4,
            "modelUsage": {
                "claude-a": {
                    "inputTokens": 5, "outputTokens": 6,
                    "cacheReadInputTokens": 7, "cacheCreationInputTokens": 8
                },
                "claude-b": {
                    "inputTokens": 1, "outputTokens": 1,
                    "cacheReadInputTokens": 1, "cacheCreationInputTokens": 1
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::events::{assistant, lines, result};
    use super::*;

    #[test]
    fn the_result_gives_the_message_the_tokens_summed_over_the_models_the_cost_and_the_turns() {
        let observed = observe(&lines(&[
            assistant("m1", 2, "thinking"),
            result("success", false, "done"),
        ]));
        assert_eq!(observed.end, End::Success);
        assert_eq!(observed.message.as_deref(), Some("done"));
        assert_eq!(
            observed.tokens,
            Tokens {
                input: 6,
                output: 7,
                cache_read: 8,
                cache_write: 9
            }
        );
        assert_eq!(observed.cost_usd, Some(0.25));
        assert_eq!(observed.turns, 4);
        assert!(!observed.usage_limit);
    }

    #[test]
    fn a_run_without_a_result_keeps_the_tokens_of_the_messages_it_streamed() {
        // The same message is streamed again as it grows: it counts once, at the largest it reached.
        let transcript = lines(&[
            assistant("m1", 2, "a"),
            assistant("m1", 9, "ab"),
            assistant("m2", 4, "c"),
        ]);
        // The last line was cut in the middle by the kill.
        let observed = observe(&format!("{transcript}{{\"type\":\"assist"));
        assert_eq!(observed.end, End::Unfinished);
        assert_eq!(observed.message, None);
        assert_eq!(
            observed.tokens,
            Tokens {
                input: 6,
                output: 13,
                cache_read: 200,
                cache_write: 20
            }
        );
        assert_eq!((observed.cost_usd, observed.turns), (None, 2));
    }

    #[test]
    fn an_error_result_has_no_message() {
        let observed = observe(&lines(&[result("error_max_turns", true, "")]));
        assert_eq!((observed.end, observed.message), (End::MaxTurns, None));
        let observed = observe(&lines(&[result("error_during_execution", true, "boom")]));
        assert_eq!((observed.end, observed.message), (End::Failed, None));
        assert!(!observed.usage_limit);
    }

    #[test]
    fn a_later_message_that_has_the_words_of_the_limit_is_no_limit() {
        let words = "You've hit your limit \u{b7} resets 5pm";
        // The model read a file, or quotes the message, in the middle of a run that goes on to answer.
        let observed = observe(&lines(&[
            assistant("m1", 1, "reading the docs"),
            assistant("m2", 2, words),
            result("success", false, "done"),
        ]));
        assert!(!observed.usage_limit);
        // The first message is the account's refusal.
        assert!(
            observe(&lines(&[
                assistant("m1", 1, words),
                assistant("m2", 2, "again")
            ]))
            .usage_limit
        );
    }

    #[test]
    fn the_usage_limit_is_read_from_the_message_and_from_a_rejected_rate_limit() {
        let limit = "You've hit your limit \u{b7} resets 5pm";
        assert!(observe(&lines(&[result("success", true, limit)])).usage_limit);
        assert!(observe(&lines(&[assistant("m1", 1, limit)])).usage_limit);
        let rejected = serde_json::json!({
            "type": "rate_limit_event",
            "rate_limit_info": { "status": "rejected" }
        });
        assert!(observe(&lines(&[rejected])).usage_limit);

        let warning = serde_json::json!({
            "type": "rate_limit_event",
            "rate_limit_info": { "status": "allowed_warning" }
        });
        assert!(!observe(&lines(&[warning])).usage_limit);
        // An answer that happens to say it is not a limit.
        assert!(!observe(&lines(&[result("success", false, limit)])).usage_limit);
        let transient = "Server is temporarily limiting requests (not your usage limit)";
        assert!(!observe(&lines(&[result("success", true, transient)])).usage_limit);
    }
}
