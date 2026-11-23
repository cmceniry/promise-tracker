//! Building a SuperAgent contract out of nodes picked on the overview.

use promise_tracker::components::Item;
use serde::Serialize;
use std::collections::HashSet;

/// The document written for the new card. Its own shape rather than
/// `Item::SuperAgent`, which would also write an empty `comment`.
#[derive(Serialize)]
struct SuperAgentDoc<'a> {
    kind: &'static str,
    name: &'a str,
    agents: &'a [String],
}

/// A SuperAgent contract naming `members`. Serialized rather than formatted,
/// so a node id that YAML would read as something else comes out quoted.
pub fn superagent_yaml(name: &str, members: &[String]) -> String {
    serde_yaml::to_string(&SuperAgentDoc {
        kind: "SuperAgent",
        name,
        agents: members,
    })
    .expect("a document of plain strings always serializes")
}

/// Eight random lowercase letters. `random` yields values in [0, 1), as
/// `Math.random` does; it is passed in so tests can fix the sequence.
pub fn random_name(random: &mut impl FnMut() -> f64) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    (0..8)
        .map(|_| {
            let idx = ((random() * CHARS.len() as f64) as usize).min(CHARS.len() - 1);
            CHARS[idx] as char
        })
        .collect()
}

/// Every agent, SuperAgent and instance name the items declare.
pub fn declared_names(items: &[Item]) -> HashSet<String> {
    items
        .iter()
        .map(|item| match item {
            Item::Agent(a) => a.get_name().clone(),
            Item::SuperAgent(sa) => sa.get_name().clone(),
            Item::Instance(i) => i.get_name().clone(),
        })
        .collect()
}

/// The instance names the items declare.
pub fn instance_names(items: &[Item]) -> HashSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Instance(i) => Some(i.get_name().clone()),
            _ => None,
        })
        .collect()
}

/// The name and YAML for a SuperAgent of `members`, its name clear of
/// `taken` when a few tries find one.
///
/// Instances are refused: the tracker looks a member up among SuperAgents
/// and plain agents only, so a collective listing one would carry nothing of
/// it while the instance went on standing alone.
pub fn superagent_from(
    members: &[String],
    instances: &HashSet<String>,
    taken: &HashSet<String>,
    random: &mut impl FnMut() -> f64,
) -> Result<(String, String), String> {
    let refused: Vec<&str> = members
        .iter()
        .filter(|m| instances.contains(*m))
        .map(String::as_str)
        .collect();
    if !refused.is_empty() {
        return Err(format!(
            "A SuperAgent cannot take in an instance, so this would leave out: {}. Deselect them and try again.",
            refused.join(", ")
        ));
    }
    let mut name = random_name(random);
    for _ in 0..10 {
        if !taken.contains(&name) {
            break;
        }
        name = random_name(random);
    }
    let yaml = superagent_yaml(&name, members);
    Ok((name, yaml))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::validate_contract_content;
    use serde::Deserialize;

    fn items(yaml: &str) -> Vec<Item> {
        serde_yaml::Deserializer::from_str(yaml)
            .map(|d| Item::deserialize(d).expect("parses"))
            .collect()
    }

    /// A fixed sequence standing in for `Math.random`.
    fn sequence(values: &[f64]) -> impl FnMut() -> f64 + '_ {
        let mut i = 0;
        move || {
            let v = values[i % values.len()];
            i += 1;
            v
        }
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn yaml_round_trips_as_one_superagent() {
        // Names YAML would otherwise read as other types or structure.
        let members = strings(&[
            "web",
            "true",
            "123",
            "a: b",
            "- x",
            "#c",
            "{{!self}}",
            "'q\"",
        ]);
        let yaml = superagent_yaml("abcdefgh", &members);
        assert_eq!(validate_contract_content(&yaml), "");
        match items(&yaml).as_slice() {
            [Item::SuperAgent(sa)] => {
                assert_eq!(sa.get_name(), "abcdefgh");
                assert_eq!(sa.get_agent_names(), members);
            }
            other => panic!("expected one SuperAgent, got {:?}", other),
        }
    }

    #[test]
    fn yaml_has_only_kind_name_and_agents() {
        assert_eq!(
            superagent_yaml("abcdefgh", &strings(&["a1", "a2"])),
            "kind: SuperAgent\nname: abcdefgh\nagents:\n- a1\n- a2\n"
        );
    }

    #[test]
    fn random_name_is_eight_lowercase_letters() {
        let name = random_name(&mut sequence(&[0.0, 0.5, 0.999_999]));
        assert_eq!(name, "anzanzan");
        // A generator at its upper edge still lands on a letter.
        assert_eq!(random_name(&mut || 1.0), "zzzzzzzz");
    }

    #[test]
    fn a_taken_name_is_retried() {
        let taken: HashSet<String> = ["aaaaaaaa".to_string()].into();
        // Eight draws of 0.0 make "aaaaaaaa"; the next eight make "bbbbbbbb".
        let mut random = sequence(&[
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.04,
        ]);
        let (name, yaml) =
            superagent_from(&strings(&["a1"]), &HashSet::new(), &taken, &mut random).unwrap();
        assert_eq!(name, "bbbbbbbb");
        assert!(yaml.contains("name: bbbbbbbb"));
    }

    #[test]
    fn instances_are_refused_by_name() {
        let declared = items(
            "kind: Agent\nname: base\n---\nkind: Instance\nname: i1\nbase: Agent/base\n---\nkind: Instance\nname: i2\nbase: base\n",
        );
        let instances = instance_names(&declared);
        let err = superagent_from(
            &strings(&["base", "i1", "i2"]),
            &instances,
            &HashSet::new(),
            &mut || 0.0,
        )
        .unwrap_err();
        assert!(err.contains("i1, i2"), "{}", err);
        assert!(!err.contains("base,"), "{}", err);
    }

    #[test]
    fn declared_names_cover_every_kind() {
        let declared = items(
            "kind: Agent\nname: a\n---\nkind: SuperAgent\nname: s\nagents: [a]\n---\nkind: Instance\nname: i\nbase: a\n",
        );
        let names = declared_names(&declared);
        assert_eq!(names, ["a", "s", "i"].map(String::from).into());
    }
}
