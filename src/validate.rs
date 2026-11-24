//! Semantic checks that parsing alone cannot make.
//!
//! serde_yaml reports shape errors with the line and column they came from.
//! These are the ones that only mean anything once a document is understood as
//! a contract: a malformed pattern, a condition holding a variable nothing can
//! ever supply, a want that is not concrete. The CLI, the API and the editor
//! all run this pass, so a contract is judged the same way wherever it loads.

use crate::components::{is_builtin, legal, Behavior, Bindings, Item, Pattern, PatternError};
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
        // name binds or the promise declares for its conditions to find, or
        // resolution reaches it with nothing to put there.
        let bound = promise.get_name_pattern().vars();
        let declared = promise.get_for_some();
        let name_at = at.clone();
        check_for_some(&name_at, promise, &mut |at: &str, name: &str, message: String| {
            errors.push(SemanticError {
                item: item.to_string(),
                location: at.to_string(),
                name: name.to_string(),
                message,
            });
        });
        for (j, condition) in promise.get_condition_patterns().iter().enumerate() {
            let at = format!("{}.conditions[{}]", at, j);
            check_pattern(item, &at, condition, errors);
            for variable in condition.vars() {
                // A built-in is supplied when the tracker builds its agents.
                if is_builtin(variable)
                    || bound.contains(variable)
                    || declared.iter().any(|d| d == variable)
                {
                    continue;
                }
                errors.push(SemanticError {
                    item: item.to_string(),
                    location: at.clone(),
                    name: condition.source().clone(),
                    message: format!(
                        "`{{{{{}}}}}` is not named by the promise or declared in `forSome`, so nothing supplies a value for it; to have the conditions find it, add `forSome: [{}]`",
                        variable, variable
                    ),
                });
            }
        }
    }

    // Restriction A — that a want is concrete — is not checked here. A want may
    // legitimately carry a variable that an instance's bindings fill in, and
    // this pass sees one document at a time, so it cannot know whether anything
    // instantiates this one. `Tracker::non_ground_wants` answers it once every
    // document is in play.
    for (i, want) in wants.iter().enumerate() {
        let at = format!("{}wants[{}]", prefix, i);
        match Pattern::parse(want.get_name()) {
            // There is no promise here to declare it on.
            Err(PatternError::OpenVariable(variable)) => errors.push(SemanticError {
                item: item.to_string(),
                location: at.clone(),
                name: want.get_name().clone(),
                message: format!(
                    "`{{{{?{v}}}}}` is no longer supported, and a want names one behavior: write the value out, or have a promise declare `forSome: [{v}]` and write `{{{{{v}}}}}` in its conditions",
                    v = variable
                ),
            }),
            _ => check_pattern(item, &at, want.get_name_pattern(), errors),
        }
        if !want.get_for_some().is_empty() {
            errors.push(SemanticError {
                item: item.to_string(),
                location: format!("{}.forSome", at),
                name: want.get_for_some().join(", "),
                message: String::from(
                    "`forSome` belongs on a promise, for its conditions to find values; a want names one behavior",
                ),
            });
        }
    }
}

/// The declaring rules for `forSome`: each name a legal variable, listed
/// once, not one the promise name already has, and used by a condition; and
/// conditions there to use them.
fn check_for_some(
    at: &str,
    promise: &Behavior,
    report: &mut impl FnMut(&str, &str, String),
) {
    let declared = promise.get_for_some();
    if declared.is_empty() {
        return;
    }
    let conditions = promise.get_condition_patterns();
    if conditions.is_empty() {
        report(
            &format!("{}.forSome", at),
            &declared.join(", "),
            String::from(
                "`forSome` declares variables for the promise's conditions to find, and it has no conditions",
            ),
        );
    }
    let in_name = promise.get_name_pattern().vars();
    for (k, variable) in declared.iter().enumerate() {
        let at = format!("{}.forSome[{}]", at, k);
        if !legal(variable) {
            report(
                &at,
                variable,
                String::from("not a legal variable name; expected only letters, digits, `_` or `-`"),
            );
            continue;
        }
        if declared[..k].contains(variable) {
            report(&at, variable, String::from("listed twice in `forSome`"));
            continue;
        }
        if in_name.contains(variable.as_str()) {
            report(
                &at,
                variable,
                format!(
                    "`{{{{{}}}}}` is a variable of the promise name, which takes its value from whoever asks; a `forSome` variable is found by the conditions, so give it another name",
                    variable
                ),
            );
            continue;
        }
        if !conditions.is_empty() && !conditions.iter().any(|c| c.vars().contains(variable.as_str())) {
            report(
                &at,
                variable,
                format!("no condition uses `{{{{{}}}}}`", variable),
            );
        }
    }
}

/// A bound value becomes literal text inside a name, so it has to read as
/// literal text: one containing `{{` would be re-read as a variable and the
/// name would stop round-tripping through its own source.
fn check_bindings(item: &str, bindings: &Bindings, errors: &mut Vec<SemanticError>) {
    for (variable, value) in bindings {
        if !legal(variable) {
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
    fn open_variables_are_rejected_with_the_rewrite() {
        let found = check(
            "kind: Agent
name: o1
provides:
  - name: \"configured for {{agent}}\"
    conditions:
      - \"{{?doc}} shows usage by {{agent}}\"
  - name: \"{{?svc}} is up\"
wants:
  - name: \"{{?svc}} grants read\"
",
        );
        assert_eq!(found.len(), 3, "{:#?}", found);
        assert_eq!(found[0].location, "provides[0].conditions[0]");
        assert_eq!(
            found[0].message,
            "`{{?doc}}` is no longer supported: declare `forSome: [doc]` on the promise and write `{{doc}}`"
        );
        assert_eq!(found[1].location, "provides[1]");
        assert!(found[1].message.contains("declare `forSome: [svc]`"));
        assert_eq!(found[2].location, "wants[0]");
        assert!(
            found[2].message.contains("a want names one behavior")
                && found[2].message.contains("`forSome: [svc]`")
                && found[2].message.contains("`{{svc}}`"),
            "{}",
            found[2].message
        );
    }

    #[test]
    fn a_value_conditions_share_is_declared_once() {
        // What a sibling's `{{?x}}` supplied is now declared on the promise.
        assert!(check(
            "kind: Agent
name: o1
provides:
  - name: reviewed
    forSome: [doc]
    conditions:
      - \"{{doc}} is approved\"
      - \"{{doc}} shows usage\"
  - name: \"placed for {{team}}\"
    forSome: [region, replica]
    conditions:
      - \"{{region}} hosts {{team}}\"
      - \"{{replica}} runs in {{region}}\"
      - \"{{replica}} is healthy\"
"
        )
        .is_empty());
    }

    #[test]
    fn a_shared_value_needs_to_be_declared() {
        let found = messages(
            "kind: Agent
name: o1
provides:
  - name: reviewed
    conditions:
      - \"{{doc}} is approved\"
      - \"{{doc}} shows usage\"
  - name: \"copy in {{region}}\"
    forSome: [region]
    conditions:
      - \"{{region}} replica is healthy\"
",
        );
        assert_eq!(found.len(), 3, "{:#?}", found);
        assert!(found[0].contains("provides[0].conditions[0]") && found[0].contains("nothing supplies"));
        assert!(found[1].contains("provides[0].conditions[1]") && found[1].contains("forSome: [doc]"));
        // a name variable cannot also be found by the conditions
        assert!(found[2].contains("provides[1].forSome[0]") && found[2].contains("variable of the promise name"));
    }

    #[test]
    fn a_declared_variable_may_be_used_by_the_conditions() {
        assert!(check(
            "kind: Agent
name: B
provides:
  - name: \"{{appname}} gateway is configured\"
    forSome: [kc]
    conditions:
      - \"{{appname}} deploys to {{kc}}\"
      - \"{{kc}} deploys {{appname}} gateway\"
  - name: configured
    forSome: [doc, reviewer]
    conditions:
      - \"{{doc}} shows usage\"
      - \"{{reviewer}} signed off on {{doc}}\"
"
        )
        .is_empty());
    }

    #[test]
    fn each_declaring_rule_is_enforced() {
        let found = messages(
            "kind: Agent
name: o1
provides:
  - name: no-conditions
    forSome: [x]
  - name: \"{{app}} is up\"
    forSome: [app, \"a b\", kc, kc, unused]
    conditions:
      - \"{{app}} runs on {{kc}}\"
      - \"{{typo}} is healthy\"
wants:
  - name: w1
    forSome: [x]
",
        );
        assert_eq!(found.len(), 7, "{:#?}", found);
        assert!(found[0].contains("provides[0].forSome:") && found[0].contains("no conditions"));
        assert!(found[1].contains("provides[1].forSome[0]") && found[1].contains("variable of the promise name"));
        assert!(found[2].contains("provides[1].forSome[1]") && found[2].contains("not a legal variable name"));
        assert!(found[3].contains("provides[1].forSome[3]") && found[3].contains("listed twice"));
        assert!(found[4].contains("provides[1].forSome[4]") && found[4].contains("no condition uses"));
        assert!(
            found[5].contains("provides[1].conditions[1]")
                && found[5].contains("not named by the promise or declared in `forSome`")
                && found[5].contains("forSome: [typo]"),
            "{}",
            found[5]
        );
        assert!(found[6].contains("wants[0].forSome") && found[6].contains("belongs on a promise"));
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
