//! Semantic checks that parsing alone cannot make.
//!
//! serde_yaml reports shape errors with the line and column they came from.
//! These are the ones that only mean anything once a document is understood as
//! a contract: a malformed pattern, a condition holding a variable nothing can
//! ever supply, a want that is not concrete. The CLI, the API and the editor
//! all run this pass, so a contract is judged the same way wherever it loads.

use crate::components::{is_builtin, is_open, Behavior, Bindings, Item, Pattern};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    /// The document it is in, e.g. `Agent/host`.
    pub item: String,
    /// Where in that document, e.g. `provides[0].conditions[1]`.
    pub location: String,
    /// The name as written.
    pub name: String,
    pub message: String,
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {}: `{}`: {}",
            self.item, self.location, self.name, self.message
        )
    }
}

impl std::error::Error for SemanticError {}

/// Every semantic problem in a parsed contract, in document order.
pub fn check_items(items: &[Item]) -> Vec<SemanticError> {
    let mut errors = vec![];
    for item in items {
        let name = item.get_name();
        match item {
            Item::Agent(agent) => {
                check_behaviors(&name, "", agent.provides(), agent.wants(), &mut errors)
            }
            // A collective only lists member names; the behaviors it carries
            // belong to those members and are checked on their own documents.
            Item::SuperAgent(_) => {}
            Item::Instance(instance) => {
                check_bindings(&name, instance.get_bindings(), &mut errors);
                check_behaviors(
                    &name,
                    "",
                    instance.get_provides(),
                    instance.get_wants(),
                    &mut errors,
                );
            }
        }
    }
    errors
}

fn check_behaviors(
    item: &str,
    prefix: &str,
    provides: &[Behavior],
    wants: &[Behavior],
    errors: &mut Vec<SemanticError>,
) {
    for (i, promise) in provides.iter().enumerate() {
        let at = format!("{}provides[{}]", prefix, i);
        check_pattern(item, &at, promise.get_name_pattern(), errors);

        // The safety rule: a condition may only use variables the promise
        // name binds, or resolution reaches it with nothing to put there.
        let bound = promise.get_name_pattern().vars();
        let name_at = at.clone();
        let mut report = |at: &str, name: &str, message: String| {
            errors.push(SemanticError {
                item: item.to_string(),
                location: at.to_string(),
                name: name.to_string(),
                message,
            });
        };
        // An open variable is bound by whoever provides a match, so it
        // belongs in what a promise needs, never in what it is.
        for variable in bound.iter().filter(|v| is_open(v)) {
            report(
                &name_at,
                promise.get_name(),
                format!(
                    "`{{{{{}}}}}` is an open variable; a promise name takes its values from whoever asks, so write `{{{{{}}}}}`",
                    variable,
                    &variable[1..]
                ),
            );
        }
        // Which condition opens each name. A value one condition finds is
        // shared with its siblings, which use the name plainly, so a name has
        // exactly one condition that opens it.
        let mut opened: BTreeMap<&str, usize> = BTreeMap::new();
        for (j, condition) in promise.get_condition_patterns().iter().enumerate() {
            let at = format!("{}.conditions[{}]", name_at, j);
            for variable in condition.vars().into_iter().filter(|v| is_open(v)) {
                if bound.contains(&variable[1..]) {
                    report(
                        &at,
                        condition.source(),
                        format!(
                            "`{{{{{}}}}}` is open here but `{{{{{}}}}}` is already named by the promise; use one or the other",
                            variable,
                            &variable[1..]
                        ),
                    );
                }
                if opened.contains_key(&variable[1..]) {
                    report(
                        &at,
                        condition.source(),
                        format!(
                            "`{{{{{}}}}}` is already open in another condition of this promise; to use the value it finds, write `{{{{{}}}}}` here",
                            variable,
                            &variable[1..]
                        ),
                    );
                } else {
                    opened.insert(&variable[1..], j);
                }
            }
        }
        for (j, condition) in promise.get_condition_patterns().iter().enumerate() {
            let at = format!("{}.conditions[{}]", at, j);
            check_pattern(item, &at, condition, errors);
            for variable in condition.vars() {
                // A built-in is supplied when the tracker builds its agents,
                // and an open variable by whoever provides a match; neither
                // comes from the promise name.
                if is_builtin(variable) || is_open(variable) {
                    continue;
                }
                // A sibling that opens the name supplies it.
                if bound.contains(variable) || opened.get(variable).is_some_and(|by| *by != j) {
                    continue;
                }
                let message = if opened.contains_key(variable) {
                    format!(
                        "`{{{{{}}}}}` is opened by this same condition as `{{{{?{}}}}}`; write it one way",
                        variable, variable
                    )
                } else {
                    format!(
                        "`{{{{{}}}}}` is not named by the promise or opened as `{{{{?{}}}}}` by another condition, so nothing supplies a value for it",
                        variable, variable
                    )
                };
                errors.push(SemanticError {
                    item: item.to_string(),
                    location: at.clone(),
                    name: condition.source().clone(),
                    message,
                });
            }
        }
        // Conditions that find a value are taken in whatever order lets each
        // have what it waits on. Two waiting on each other never get a turn.
        let mut known: BTreeSet<&str> = bound.clone();
        let mut pending: Vec<usize> = opened.values().copied().collect();
        pending.sort();
        pending.dedup();
        let conditions = promise.get_condition_patterns();
        while let Some(ready) = pending.iter().position(|j| {
            conditions[*j]
                .vars()
                .into_iter()
                .filter(|v| !is_open(v) && !is_builtin(v) && opened.contains_key(v))
                .all(|v| known.contains(v))
        }) {
            let j = pending.remove(ready);
            known.extend(
                conditions[j]
                    .vars()
                    .into_iter()
                    .filter(|v| is_open(v))
                    .map(|v| &v[1..]),
            );
        }
        for j in pending {
            // one that names its own value is reported above
            if conditions[j]
                .vars()
                .into_iter()
                .any(|v| opened.get(v) == Some(&j))
            {
                continue;
            }
            errors.push(SemanticError {
                item: item.to_string(),
                location: format!("{}.conditions[{}]", at, j),
                name: conditions[j].source().clone(),
                message: String::from(
                    "waits on a value from a condition that waits on this one, so neither can be found first",
                ),
            });
        }
    }

    // Restriction A — that a want is concrete — is not checked here. A want may
    // legitimately carry a variable that an instance's bindings fill in, and
    // this pass sees one document at a time, so it cannot know whether anything
    // instantiates this one. `Tracker::non_ground_wants` answers it once every
    // document is in play.
    for (i, want) in wants.iter().enumerate() {
        let at = format!("{}wants[{}]", prefix, i);
        check_pattern(item, &at, want.get_name_pattern(), errors);
    }
}

/// A bound value becomes literal text inside a name, so it has to read as
/// literal text: one containing `{{` would be re-read as a variable and the
/// name would stop round-tripping through its own source.
fn check_bindings(item: &str, bindings: &Bindings, errors: &mut Vec<SemanticError>) {
    for (variable, value) in bindings {
        if variable.is_empty()
            || !variable
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            errors.push(SemanticError {
                item: item.to_string(),
                location: format!("bindings.{}", variable),
                name: variable.clone(),
                message: String::from(
                    "not a usable variable name; expected only letters, digits, `_` or `-`",
                ),
            });
        }
        if value.contains("{{") {
            errors.push(SemanticError {
                item: item.to_string(),
                location: format!("bindings.{}", variable),
                name: value.clone(),
                message: String::from("a bound value is plain text and may not contain `{{`"),
            });
        }
    }
}

fn check_pattern(item: &str, at: &str, pattern: &Pattern, errors: &mut Vec<SemanticError>) {
    if let Err(e) = Pattern::parse(pattern.source()) {
        errors.push(SemanticError {
            item: item.to_string(),
            location: at.to_string(),
            name: pattern.source().clone(),
            message: e.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn check(yaml: &str) -> Vec<SemanticError> {
        let items: Vec<Item> = serde_yaml::Deserializer::from_str(yaml)
            .map(Item::deserialize)
            .collect::<Result<_, _>>()
            .expect("test contract should parse");
        check_items(&items)
    }

    fn messages(yaml: &str) -> Vec<String> {
        check(yaml).iter().map(|e| e.to_string()).collect()
    }

    #[test]
    fn a_well_formed_contract_reports_nothing() {
        assert!(check(
            "kind: Agent
name: host
provides:
  - name: process-execution/{{process}}
    conditions:
      - binary-installed/{{process}}
wants:
  - name: power
"
        )
        .is_empty());
    }

    #[test]
    fn self_needs_no_binder() {
        assert!(check(
            "kind: Agent
name: s1
provides:
  - name: \"{{agent}} reads R1\"
    conditions:
      - \"{{!self}} configured for {{agent}}\"
  - name: \"{{!SELF}} is up\"
wants:
  - name: \"{{!self}} has power\"
"
        )
        .is_empty());
    }

    #[test]
    fn an_unknown_builtin_is_reported() {
        let found = check(
            "kind: Agent
name: a1
wants:
  - name: \"{{!slef}} reads R1\"
",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].location, "wants[0]");
        assert!(found[0].message.contains("unknown built-in"));
    }

    #[test]
    fn self_cannot_be_supplied_by_a_binding() {
        let found = check(
            "kind: Instance
name: i1
base: a1
bindings:
  \"!self\": other
  env: \"{{!self}}\"
",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].location, "bindings.!self");
        assert_eq!(found[1].location, "bindings.env");
    }

    #[test]
    fn open_variables_are_allowed_in_conditions_and_wants() {
        assert!(check(
            "kind: Agent
name: o1
provides:
  - name: \"configured for {{agent}}\"
    conditions:
      - \"{{?doc}} shows usage by {{agent}}\"
      - \"{{?reviewer}} signed off\"
wants:
  - name: \"{{?svc}} grants read\"
"
        )
        .is_empty());
    }

    #[test]
    fn shapes_reserved_for_binding_direction_are_rejected() {
        let found = messages(
            "kind: Agent
name: o1
provides:
  - name: \"{{?svc}} is up\"
  - name: configured
    conditions:
      - \"{{?doc}} shows usage\"
      - \"{{?doc}} is approved\"
  - name: \"copy in {{region}}\"
    conditions:
      - \"{{?region}} replica is healthy\"
",
        );
        assert_eq!(found.len(), 3, "{:#?}", found);
        assert!(found[0].contains("provides[0]") && found[0].contains("open variable"));
        assert!(found[1].contains("provides[1].conditions[1]") && found[1].contains("already open"));
        assert!(found[2].contains("provides[2].conditions[0]") && found[2].contains("already named by the promise"));
    }

    #[test]
    fn a_value_one_condition_finds_may_be_used_by_its_siblings() {
        assert!(check(
            "kind: Agent
name: o1
provides:
  - name: reviewed
    conditions:
      - \"{{doc}} is approved\"
      - \"{{?doc}} shows usage\"
  - name: \"placed for {{team}}\"
    conditions:
      - \"{{?region}} hosts {{team}}\"
      - \"{{?replica}} runs in {{region}}\"
      - \"{{replica}} is healthy\"
"
        )
        .is_empty());
    }

    #[test]
    fn a_shared_value_needs_a_condition_that_can_find_it() {
        let found = messages(
            "kind: Agent
name: o1
provides:
  - name: reviewed
    conditions:
      - \"{{doc}} is approved\"
  - name: mirrored
    conditions:
      - \"{{?doc}} copies {{doc}}\"
  - name: paired
    conditions:
      - \"{{?a}} follows {{b}}\"
      - \"{{?b}} follows {{a}}\"
",
        );
        assert_eq!(found.len(), 4, "{:#?}", found);
        assert!(found[0].contains("provides[0].conditions[0]") && found[0].contains("nothing supplies"));
        assert!(found[1].contains("provides[1].conditions[0]") && found[1].contains("this same condition"));
        assert!(found[2].contains("provides[2].conditions[0]") && found[2].contains("waits on a value"));
        assert!(found[3].contains("provides[2].conditions[1]") && found[3].contains("waits on a value"));
    }

    #[test]
    fn plain_contracts_are_untouched() {
        assert!(check(
            "kind: Agent
name: a1
provides:
  - name: b1
    conditions:
      - b2
wants:
  - name: b3
"
        )
        .is_empty());
    }

    #[test]
    fn malformed_patterns_are_reported_with_their_place() {
        let found = check(
            "kind: Agent
name: host
provides:
  - name: a{{b
",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item, "Agent/host");
        assert_eq!(found[0].location, "provides[0]");
        assert_eq!(found[0].name, "a{{b");
        assert!(
            found[0].message.contains("no closing"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn adjacent_variables_are_reported() {
        let found = check(
            "kind: Agent
name: host
provides:
  - name: \"{{a}}{{b}}\"
",
        );
        assert_eq!(found.len(), 1);
        assert!(
            found[0].message.contains("adjacent"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn a_condition_variable_must_be_bound_by_the_promise() {
        let found = check(
            "kind: Agent
name: host
provides:
  - name: run
    conditions:
      - binary/{{process}}
",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].location, "provides[0].conditions[0]");
        assert!(
            found[0].message.contains("not named by the promise"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn a_parameterized_want_is_left_to_the_tracker() {
        // An instance's bindings may well ground it, and one document cannot
        // see whether anything instantiates it.
        assert!(check(
            "kind: Agent
name: p1
wants:
  - name: process-execution/{{process}}
"
        )
        .is_empty());
    }

    #[test]
    fn instance_documents_are_checked_too() {
        let found = messages(
            "kind: Instance
name: i1
base: SuperAgent/sa1
provides:
  - name: run
    conditions:
      - thing/{{v}}
",
        );
        assert_eq!(found.len(), 1);
        assert!(
            found[0].starts_with("Instance/i1 provides[0].conditions[0]:"),
            "{}",
            found[0]
        );
    }

    #[test]
    fn a_bound_value_must_be_plain_text() {
        let found = check(
            "kind: Instance
name: i1
base: sa1
bindings:
  env: \"{{nested}}\"
",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].location, "bindings.env");
        assert!(
            found[0].message.contains("may not contain"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn every_problem_is_reported_not_just_the_first() {
        assert_eq!(
            check(
                "kind: Agent
name: host
provides:
  - name: run
    conditions:
      - a/{{x}}
      - b/{{y}}
"
            )
            .len(),
            2
        );
    }
}
