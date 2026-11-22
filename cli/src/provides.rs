use clap::Parser;
use promise_tracker::marks::Marks;
use promise_tracker::query::{self, AgentProvides, ConditionStatus, QueryOptions};
use std::process;

#[derive(Parser)]
pub struct Parameters {
    /// The file(s) and dir(s) to check
    #[clap(short, long = "file")]
    files: Vec<String>,

    /// Show SuperAgent members as agents of their own
    #[clap(long)]
    unfold: bool,

    /// Leave out promises the agent makes only to itself
    #[clap(long)]
    fold_self: bool,

    /// Show each conditional provide's conditions, held or broken
    #[clap(short, long)]
    conditions: bool,

    /// Wrap values a variable was replaced with in `<>`: the agent's own
    /// name where it wrote `{{!self}}`, an Instance binding, a value a
    /// condition took from a sibling's `{{?x}}`
    #[clap(long)]
    mark_bound: bool,

    /// Leave out the agent's promises that are not kept, where something
    /// else keeps the same behavior; one nobody keeps is still listed
    #[clap(long)]
    fold_alternatives: bool,

    /// The agent name to show actions for
    agent: String,
}

pub fn command(parameters: &Parameters) {
    let tracker = cli::load_tracker(&parameters.files, parameters.unfold);
    let options = QueryOptions {
        fold_self: parameters.fold_self,
        bundle: false,
        fold_alternatives: parameters.fold_alternatives,
    };
    let Some(provides) = query::agent_provides(&tracker, &parameters.agent, options) else {
        println!("Agent {} not found", parameters.agent);
        process::exit(1);
    };
    // Display only: the rows keep the plain names.
    let marks = parameters.mark_bound.then(|| Marks::new(&tracker));
    for line in lines(provides, marks.as_ref(), parameters.conditions) {
        println!("{}", line);
    }
}

/// What the command prints for an agent's provides, a line each.
///
/// With `marks`, names are shown as this agent has them: a promise as that
/// one promise is written, a condition as that promise has it — so a
/// condition one promise writes out is not marked because another promise
/// of the agent reaches the same text through a variable.
fn lines(provides: AgentProvides, marks: Option<&Marks>, with_conditions: bool) -> Vec<String> {
    let agent = provides.agent.as_str();
    let shown = |name: &String| -> String {
        match marks {
            Some(marks) => marks.made(agent, name, name),
            None => name.clone(),
        }
    };
    let needed = |promise: &String, name: &String| -> String {
        match marks {
            Some(marks) => marks.condition(agent, promise, name),
            None => name.clone(),
        }
    };
    let mut out: Vec<String> = vec![];
    if !with_conditions {
        // One line per behavior and per pattern, as the CLI always printed.
        let mut behaviors: Vec<&String> = provides.provides.iter().map(|r| &r.behavior).collect();
        behaviors.dedup();
        for behavior in behaviors {
            out.push(shown(behavior));
        }
        let mut patterns: Vec<&String> = provides.patterns.iter().map(|r| &r.pattern).collect();
        patterns.dedup();
        for pattern in patterns {
            out.push(format!("{}\t(pattern)", shown(pattern)));
        }
        return out;
    }
    for row in &provides.provides {
        if row.conditions.is_empty() {
            out.push(shown(&row.behavior));
        } else {
            let marked: Vec<ConditionStatus> = row
                .conditions
                .iter()
                .map(|c| ConditionStatus {
                    behavior: needed(&row.behavior, &c.behavior),
                    held: c.held,
                })
                .collect();
            out.push(format!(
                "{}\tif {}",
                shown(&row.behavior),
                conditions(&marked)
            ));
        }
    }
    for row in &provides.patterns {
        if row.conditions.is_empty() {
            out.push(format!("{}\t(pattern)", shown(&row.pattern)));
        } else {
            out.push(format!(
                "{}\t(pattern) if {}",
                shown(&row.pattern),
                row.conditions
                    .iter()
                    .map(|c| needed(&row.pattern, c))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    out
}

/// `c1 (held), c2 (broken)`
pub fn conditions(conditions: &[ConditionStatus]) -> String {
    conditions
        .iter()
        .map(|c| {
            format!(
                "{} ({})",
                c.behavior,
                if c.held { "held" } else { "broken" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use promise_tracker::components::Item;
    use promise_tracker::Tracker;

    fn tracker(yaml: &str) -> Tracker {
        let mut t = Tracker::new();
        for doc in serde_yaml::Deserializer::from_str(yaml) {
            let item: Item = serde::Deserialize::deserialize(doc).unwrap();
            t.add_item(item);
        }
        t
    }

    fn printed(t: &Tracker, agent: &str, mark: bool, with_conditions: bool) -> Vec<String> {
        let provides = query::agent_provides(t, agent, QueryOptions::default()).unwrap();
        let marks = mark.then(|| Marks::new(t));
        lines(provides, marks.as_ref(), with_conditions)
    }

    /// x keeps p1 on a condition written out and p2 on an open one that
    /// matches the same behavior; p3 names x itself.
    const TWO_WAYS: &str = r#"
kind: Agent
name: x
provides:
- name: p1
  conditions: ["é1 is up"]
- name: p2
  conditions: ["{{?w}} is up"]
- name: p3
  conditions: ["{{!self}} is fed"]
- name: "{{who}} is ready"
  conditions: ["{{!self}} serves {{who}}"]
---
kind: Agent
name: "é1"
provides:
- name: "é1 is up"
"#;

    #[test]
    fn a_condition_written_out_stays_plain_beside_one_that_matched_it() {
        let t = tracker(TWO_WAYS);
        assert_eq!(
            printed(&t, "x", true, true),
            vec![
                "p1\tif \u{e9}1 is up (held)",
                "p2\tif {{?w}} is up (held)",
                "p3\tif <x> is fed (broken)",
                "{{who}} is ready\t(pattern) if <x> serves {{who}}",
            ]
        );
        assert_eq!(
            printed(&t, "x", false, true),
            vec![
                "p1\tif \u{e9}1 is up (held)",
                "p2\tif {{?w}} is up (held)",
                "p3\tif x is fed (broken)",
                "{{who}} is ready\t(pattern) if x serves {{who}}",
            ]
        );
        // What p2's open condition matched is still marked, as p2's.
        let marks = Marks::new(&t);
        assert_eq!(marks.condition("x", "p2", "\u{e9}1 is up"), "<\u{e9}1> is up");
    }

    #[test]
    fn without_conditions_it_is_a_line_per_name() {
        let t = tracker(TWO_WAYS);
        for mark in [false, true] {
            assert_eq!(
                printed(&t, "x", mark, false),
                vec!["p1", "p2", "p3", "{{who}} is ready\t(pattern)"]
            );
        }
        // Its promise is written out: x finding it with `{{?w}}` is x's
        // doing, and does not mark the provider's own list.
        assert_eq!(printed(&t, "\u{e9}1", true, false), vec!["\u{e9}1 is up"]);
        assert_eq!(printed(&t, "\u{e9}1", false, false), vec!["\u{e9}1 is up"]);
    }
}
