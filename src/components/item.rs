use crate::components::agent::Agent;
use crate::components::instance::Instance;
use crate::components::superagent::SuperAgent;
use schemars::JsonSchema;
use serde::de::value::{MapAccessDeserializer, StringDeserializer};
use serde::de::{self, DeserializeSeed, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum Item {
    Agent(Agent),
    SuperAgent(SuperAgent),
    Instance(Instance),
}

impl Item {
    pub fn get_name(&self) -> String {
        match self {
            Item::Agent(agent) => format!("Agent/{}", agent.get_name().clone()),
            Item::SuperAgent(superagent) => format!("SuperAgent/{}", superagent.get_name().clone()),
            Item::Instance(instance) => format!("Instance/{}", instance.get_name().clone()),
        }
    }
}

/// The `kind` discriminator. Deserialized as its own type so that an unknown
/// kind is reported by the underlying deserializer, which lets it attach the
/// location of the offending value.
#[derive(Deserialize)]
enum Kind {
    Agent,
    SuperAgent,
    Instance,
}

/// `Item` is deserialized by hand rather than with `#[serde(tag = "kind")]`.
/// The derived internally tagged enum buffers the whole document into an
/// intermediate value before dispatching on `kind`, which throws away the
/// line/column marks that serde_yaml attaches to errors. Reading `kind` off
/// the map and then handing the remaining entries straight to `Agent` /
/// `SuperAgent` keeps the document streaming, so parse errors keep pointing
/// at the position they came from.
impl<'de> Deserialize<'de> for Item {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(ItemVisitor)
    }
}

struct ItemVisitor;

impl<'de> Visitor<'de> for ItemVisitor {
    type Value = Item;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an Agent, SuperAgent or Instance mapping with a `kind` field")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Item, A::Error>
    where
        A: MapAccess<'de>,
    {
        // Entries seen before `kind` have to be held back and replayed. Only
        // those lose their position; everything from `kind` onward keeps it.
        let mut leading: Vec<(String, serde_yaml::Value)> = vec![];
        let kind = loop {
            match map.next_key::<String>()? {
                Some(key) if key == "kind" => break map.next_value::<Kind>()?,
                Some(key) => {
                    let value = map.next_value::<serde_yaml::Value>()?;
                    leading.push((key, value));
                }
                None => return Err(de::Error::missing_field("kind")),
            }
        };
        let rest = MapAccessDeserializer::new(Kindless {
            leading: leading.into_iter(),
            pending: None,
            inner: map,
        });
        match kind {
            Kind::Agent => Agent::deserialize(rest).map(Item::Agent),
            Kind::SuperAgent => SuperAgent::deserialize(rest).map(Item::SuperAgent),
            Kind::Instance => Instance::deserialize(rest).map(Item::Instance),
        }
    }
}

/// The entries of an Item mapping with the `kind` key removed: first the
/// entries that preceded `kind`, then the rest of the underlying map.
struct Kindless<A> {
    leading: std::vec::IntoIter<(String, serde_yaml::Value)>,
    pending: Option<serde_yaml::Value>,
    inner: A,
}

impl<'de, A> MapAccess<'de> for Kindless<A>
where
    A: MapAccess<'de>,
{
    type Error = A::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if let Some((key, value)) = self.leading.next() {
            self.pending = Some(value);
            return seed
                .deserialize(StringDeserializer::<Self::Error>::new(key))
                .map(Some);
        }
        // Keys still to come are forwarded untouched: routing them through a
        // string deserializer would strip the position the underlying
        // deserializer attaches to key errors such as an unknown field.
        self.inner.next_key_seed(seed)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        match self.pending.take() {
            Some(value) => seed.deserialize(value).map_err(de::Error::custom),
            None => self.inner.next_value_seed(seed),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<Vec<Item>, serde_yaml::Error> {
        serde_yaml::Deserializer::from_str(yaml)
            .map(Item::deserialize)
            .collect()
    }

    fn location(yaml: &str) -> Option<(usize, usize)> {
        let e = parse(yaml).expect_err("expected a parse failure");
        e.location().map(|l| (l.line(), l.column()))
    }

    #[test]
    fn kind_selects_the_variant() {
        let items = parse("kind: Agent\nname: a\n---\nkind: SuperAgent\nname: sa\n").unwrap();
        assert_eq!(
            items.iter().map(Item::get_name).collect::<Vec<String>>(),
            vec!["Agent/a", "SuperAgent/sa"]
        );
    }

    #[test]
    fn kind_may_come_after_other_fields() {
        let items = parse("name: a\nprovides:\n  - name: http\nkind: Agent\n").unwrap();
        assert_eq!(items[0].get_name(), "Agent/a");
    }

    #[test]
    fn instance_is_a_kind() {
        let items = parse("kind: Instance\nname: prod\nbase: SuperAgent/cluster\n").unwrap();
        assert_eq!(items[0].get_name(), "Instance/prod");
    }

    #[test]
    fn missing_kind_is_reported() {
        let e = parse("name: a\n").expect_err("expected a parse failure");
        assert!(e.to_string().contains("missing field `kind`"), "{}", e);
    }

    #[test]
    fn unknown_kind_points_at_the_kind() {
        assert_eq!(location("kind: Machine\nname: a\n"), Some((1, 7)));
    }

    #[test]
    fn wrong_type_points_at_the_value() {
        assert_eq!(location("kind: Agent\nname:\n  - a\n"), Some((3, 3)));
    }

    #[test]
    fn unknown_field_points_at_the_field() {
        assert_eq!(
            location("kind: Agent\nname: a\nwnats:\n  - x\n"),
            Some((3, 1))
        );
    }

    #[test]
    fn nested_error_points_into_the_nesting() {
        assert_eq!(
            location("kind: Agent\nname: a\nprovides:\n  - name: http\n    bogus: 1\n"),
            Some((5, 5))
        );
    }

    #[test]
    fn locations_are_absolute_across_documents() {
        assert_eq!(
            location("kind: Agent\nname: a\n---\nkind: Agent\nname:\n  - b\n"),
            Some((6, 3))
        );
    }

    #[test]
    fn serializes_with_the_kind_tag() {
        let items = parse("kind: Agent\nname: a\n").unwrap();
        let out = serde_yaml::to_string(&items[0]).unwrap();
        assert!(out.starts_with("kind: Agent\n"), "{}", out);
    }
}
