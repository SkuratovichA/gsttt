//! The text the gate prints.

use super::rules::{Entry, Judgement, Status};

/// Every scenario of every capability as proven, pending or failing, then the failures with the
/// places to look at, then one line of totals.
pub(super) fn render(judgement: &Judgement) -> String {
    let mut lines: Vec<String> = Vec::new();
    for group in judgement
        .entries
        .chunk_by(|a, b| a.capability == b.capability)
    {
        let Some(first) = group.first() else {
            continue;
        };
        let count = |status: Status| group.iter().filter(|entry| entry.status == status).count();
        lines.push(format!(
            "{}: {} proven, {} pending, {} failing",
            first.capability,
            count(Status::Proven),
            count(Status::Pending),
            count(Status::Failing)
        ));
        lines.extend(group.iter().map(|entry| format!("  {}", describe(entry))));
    }
    if !judgement.failures.is_empty() {
        lines.push(String::new());
        lines.push("failures:".to_owned());
        lines.extend(
            judgement
                .failures
                .iter()
                .map(|failure| format!("  {}: {}", failure.site, failure.message)),
        );
    }
    lines.push(String::new());
    lines.push(summary(judgement));
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// The names of the open changes that can be archived, one per line.
pub(super) fn ready(judgement: &Judgement) -> String {
    let mut text = String::new();
    for name in judgement.ready() {
        text.push_str(name);
        text.push('\n');
    }
    text
}

fn describe(entry: &Entry) -> String {
    let Entry {
        title,
        site,
        proofs,
        ..
    } = entry;
    let proofs = proofs
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match entry.status {
        Status::Proven => format!("proven   {title}  ({site})  proof: {proofs}"),
        Status::Pending => format!(
            "pending  {title}  ({site})  change `{}` still has unticked tasks",
            entry.change().unwrap_or_default()
        ),
        Status::Failing if proofs.is_empty() => format!("failing  {title}  ({site})"),
        Status::Failing => format!("failing  {title}  ({site})  proofs: {proofs}"),
    }
}

fn summary(judgement: &Judgement) -> String {
    let count = |status: Status| {
        judgement
            .entries
            .iter()
            .filter(|entry| entry.status == status)
            .count()
    };
    let totals = format!(
        "{} scenarios, {} proven, {} pending, {} failing",
        judgement.entries.len(),
        count(Status::Proven),
        count(Status::Pending),
        count(Status::Failing)
    );
    if judgement.passed() {
        format!("spec gate passed: {totals}")
    } else {
        format!(
            "spec gate failed with {} problems: {totals}",
            judgement.failures.len()
        )
    }
}
