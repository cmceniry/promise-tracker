//! Parameterized names.
//!
//! A behavior name is a [`Pattern`]: literal text interleaved with `{{var}}`
//! placeholders. A pattern with no variables is *ground* and behaves exactly as
//! the plain `String` name did before. Providers may be parameterized; a
//! resolution goal is always ground (Restriction A), so matching is one-sided —
//! a pattern against a concrete name — never general unification.
//!
//! See `docs/design/parameterized-behaviors.md`.

use schemars::gen::SchemaGenerator;
use schemars::schema::Schema;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Variable name to the concrete value it stands for.
pub type Bindings = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Var(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternError {
    /// `{{` with no closing `}}`.
    Unterminated,
    /// `{{}}`.
    EmptyVariable,
    /// A variable name outside `[A-Za-z0-9_-]+`.
    InvalidVariable(String),
    /// `{{a}}{{b}}`, which has no deterministic reading.
    AdjacentVariables(String, String),
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PatternError::Unterminated => write!(f, "`{{{{` with no closing `}}}}`"),
            PatternError::EmptyVariable => write!(f, "empty variable name in `{{{{}}}}`"),
            PatternError::InvalidVariable(v) => write!(
                f,
                "invalid variable name `{}`, expected only letters, digits, `_` or `-`",
                v
            ),
            PatternError::AdjacentVariables(a, b) => write!(
                f,
                "`{{{{{}}}}}` and `{{{{{}}}}}` are adjacent; separate them with literal text",
                a, b
            ),
        }
    }
}

impl std::error::Error for PatternError {}

/// A name, possibly parameterized.
///
/// `source` is the authoritative identity: two patterns are equal, hash alike
/// and order together exactly when their source text matches, which keeps every
/// comparison and sort behaving as it did when names were plain strings.
/// Parsing is deterministic, so equal sources always carry equal segments.
#[derive(Debug, Clone)]
pub struct Pattern {
    source: String,
    segments: Vec<Segment>,
}

fn render(segments: &[Segment]) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            Segment::Literal(l) => out.push_str(l),
            Segment::Var(v) => {
                out.push_str("{{");
                out.push_str(v);
                out.push_str("}}");
            }
        }
    }
    out
}

impl Pattern {
    /// Parse, reporting anything malformed.
    pub fn parse(source: &str) -> Result<Pattern, PatternError> {
        let mut segments: Vec<Segment> = vec![];
        let mut literal = String::new();
        let mut rest = source;

        loop {
            let Some(start) = rest.find("{{") else {
                literal.push_str(rest);
                break;
            };
            literal.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                return Err(PatternError::Unterminated);
            };
            let name = &after[..end];
            if name.is_empty() {
                return Err(PatternError::EmptyVariable);
            }
            if !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(PatternError::InvalidVariable(name.to_string()));
            }
            if literal.is_empty() {
                if let Some(Segment::Var(previous)) = segments.last() {
                    return Err(PatternError::AdjacentVariables(
                        previous.clone(),
                        name.to_string(),
                    ));
                }
            } else {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(Segment::Var(name.to_string()));
            rest = &after[end + 2..];
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Pattern {
            source: source.to_string(),
            segments,
        })
    }

    /// Parse, treating anything malformed as one literal segment.
    ///
    /// Deserialization and the plain constructors use this so that a bad name
    /// loads and behaves exactly as it did before patterns existed — as its own
    /// literal text — instead of failing the whole document. The semantic
    /// validation pass re-parses with [`Pattern::parse`] and reports it there.
    pub fn parse_lossy(source: &str) -> Pattern {
        Pattern::parse(source).unwrap_or_else(|_| Pattern {
            source: source.to_string(),
            segments: vec![Segment::Literal(source.to_string())],
        })
    }

    /// The text this pattern was written as; round-trips through `parse`.
    pub fn source(&self) -> &String {
        &self.source
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// No variables: this names exactly one behavior.
    pub fn is_ground(&self) -> bool {
        !self.segments.iter().any(|s| matches!(s, Segment::Var(_)))
    }

    pub fn vars(&self) -> BTreeSet<&str> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Var(v) => Some(v.as_str()),
                Segment::Literal(_) => None,
            })
            .collect()
    }

    /// Match a concrete name, binding this pattern's variables.
    ///
    /// Leftmost-shortest: each variable takes the smallest binding that lets
    /// the rest of the pattern match, so `{{a}}-{{b}}` against `x-y-z` yields
    /// `a = x`, `b = y-z`. A repeated variable must bind consistently, and a
    /// binding is never empty.
    pub fn match_ground(&self, name: &str) -> Option<Bindings> {
        let mut bindings = Bindings::new();
        if match_from(&self.segments, name, &mut bindings) {
            Some(bindings)
        } else {
            None
        }
    }

    /// Replace bound variables with their values.
    ///
    /// Unbound variables are left in place, so a partially bound pattern is
    /// still a pattern — see partial binding in the design doc. Values are
    /// plain text (Restriction B); a value containing `{{` would not round-trip
    /// and is rejected by validation before it reaches here.
    pub fn substitute(&self, bindings: &Bindings) -> Pattern {
        let mut segments: Vec<Segment> = vec![];
        for segment in &self.segments {
            let next = match segment {
                Segment::Var(v) => match bindings.get(v) {
                    Some(value) => Segment::Literal(value.clone()),
                    None => Segment::Var(v.clone()),
                },
                Segment::Literal(l) => Segment::Literal(l.clone()),
            };
            match (segments.last_mut(), &next) {
                // Substitution can leave two literals side by side; a pattern
                // only ever holds them merged, so that `source` stays canonical.
                (Some(Segment::Literal(previous)), Segment::Literal(addition)) => {
                    previous.push_str(addition)
                }
                _ => segments.push(next),
            }
        }
        segments.retain(|s| !matches!(s, Segment::Literal(l) if l.is_empty()));
        Pattern {
            source: render(&segments),
            segments,
        }
    }
}

/// Match `segs` against `input`, shortest binding first, backtracking when a
/// choice fails further along.
fn match_from(segs: &[Segment], input: &str, bindings: &mut Bindings) -> bool {
    let Some((first, tail)) = segs.split_first() else {
        return input.is_empty();
    };

    match first {
        Segment::Literal(literal) => match input.strip_prefix(literal.as_str()) {
            Some(remainder) => match_from(tail, remainder, bindings),
            None => false,
        },
        Segment::Var(name) => {
            // A repeat has to take the value already chosen for it.
            if let Some(bound) = bindings.get(name).cloned() {
                return match input.strip_prefix(bound.as_str()) {
                    Some(remainder) => match_from(tail, remainder, bindings),
                    None => false,
                };
            }
            match tail.first() {
                // Trailing variable: it takes whatever is left, and a binding
                // is never empty.
                None => {
                    if input.is_empty() {
                        return false;
                    }
                    bindings.insert(name.clone(), input.to_string());
                    true
                }
                // Variables are never adjacent, so what follows is a literal.
                // Try each place it could start, nearest first.
                Some(Segment::Literal(next)) => {
                    let mut from = next_boundary(input, 1);
                    while from <= input.len() {
                        let Some(hay) = input.get(from..) else { break };
                        let Some(offset) = hay.find(next.as_str()) else {
                            break;
                        };
                        let split = from + offset;
                        bindings.insert(name.clone(), input[..split].to_string());
                        if match_from(tail, &input[split..], bindings) {
                            return true;
                        }
                        bindings.remove(name);
                        from = next_boundary(input, split + 1);
                    }
                    false
                }
                Some(Segment::Var(_)) => false,
            }
        }
    }
}

fn next_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}
impl Eq for Pattern {}

/// Comparing against plain text is comparing against the source.
impl PartialEq<str> for Pattern {
    fn eq(&self, other: &str) -> bool {
        self.source == other
    }
}
impl PartialEq<&str> for Pattern {
    fn eq(&self, other: &&str) -> bool {
        self.source == *other
    }
}

impl Hash for Pattern {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
    }
}

impl PartialOrd for Pattern {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pattern {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.source.cmp(&other.source)
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.source)
    }
}

impl From<&str> for Pattern {
    fn from(s: &str) -> Pattern {
        Pattern::parse_lossy(s)
    }
}
impl From<String> for Pattern {
    fn from(s: String) -> Pattern {
        Pattern::parse_lossy(&s)
    }
}

/// Serialized as its source text, so the YAML surface is unchanged.
impl Serialize for Pattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Ok(Pattern::parse_lossy(&source))
    }
}

/// A plain string in the schema, so `cli schema` output does not change.
impl JsonSchema for Pattern {
    fn schema_name() -> String {
        String::schema_name()
    }
    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        String::json_schema(gen)
    }
    fn is_referenceable() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binds(pairs: &[(&str, &str)]) -> Bindings {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn ground_patterns_are_plain_names() {
        let p = Pattern::parse("process-execution").unwrap();
        assert!(p.is_ground());
        assert_eq!(p.source(), "process-execution");
        assert_eq!(
            p.segments(),
            &[Segment::Literal("process-execution".into())]
        );
        assert!(p.vars().is_empty());
        assert_eq!(p.match_ground("process-execution"), Some(Bindings::new()));
        assert_eq!(p.match_ground("something-else"), None);
    }

    #[test]
    fn empty_name_is_ground_and_matches_only_itself() {
        let p = Pattern::parse("").unwrap();
        assert!(p.is_ground());
        assert_eq!(p.segments(), &[]);
        assert_eq!(p.match_ground(""), Some(Bindings::new()));
        assert_eq!(p.match_ground("x"), None);
    }

    #[test]
    fn parse_splits_literals_and_variables() {
        let p = Pattern::parse("run/{{process}}/now").unwrap();
        assert!(!p.is_ground());
        assert_eq!(
            p.segments(),
            &[
                Segment::Literal("run/".into()),
                Segment::Var("process".into()),
                Segment::Literal("/now".into()),
            ]
        );
        assert_eq!(p.vars().into_iter().collect::<Vec<_>>(), vec!["process"]);
    }

    #[test]
    fn source_round_trips() {
        for source in [
            "b1",
            "run/{{p}}",
            "{{p}}/run",
            "{{p}}",
            "a-{{x}}-b-{{y}}-c",
            "{{p}}-{{p}}",
        ] {
            let p = Pattern::parse(source).unwrap();
            assert_eq!(p.source(), source);
            assert_eq!(render(p.segments()), source);
        }
    }

    #[test]
    fn a_lone_brace_pair_is_literal() {
        // Only `{{` opens a variable; stray braces are ordinary text.
        let p = Pattern::parse("a{b}c").unwrap();
        assert!(p.is_ground());
        assert_eq!(p.match_ground("a{b}c"), Some(Bindings::new()));
    }

    #[test]
    fn parse_errors() {
        assert_eq!(Pattern::parse("a{{b"), Err(PatternError::Unterminated));
        assert_eq!(Pattern::parse("a{{}}b"), Err(PatternError::EmptyVariable));
        assert_eq!(
            Pattern::parse("a{{b c}}"),
            Err(PatternError::InvalidVariable("b c".into()))
        );
        assert_eq!(
            Pattern::parse("{{a}}{{b}}"),
            Err(PatternError::AdjacentVariables("a".into(), "b".into()))
        );
        // A separator, however thin, makes it readable again.
        assert!(Pattern::parse("{{a}}-{{b}}").is_ok());
    }

    #[test]
    fn lossy_parse_keeps_a_bad_name_as_literal_text() {
        let p = Pattern::parse_lossy("a{{b");
        assert!(p.is_ground());
        assert_eq!(p.source(), "a{{b");
        assert_eq!(p.match_ground("a{{b"), Some(Bindings::new()));
        // and it still reports as malformed when validation asks
        assert!(Pattern::parse(p.source()).is_err());
    }

    #[test]
    fn match_binds_a_single_variable() {
        let p = Pattern::parse("process-execution/{{process}}").unwrap();
        assert_eq!(
            p.match_ground("process-execution/p1"),
            Some(binds(&[("process", "p1")]))
        );
        assert_eq!(p.match_ground("process-execution/"), None);
        assert_eq!(p.match_ground("other/p1"), None);
    }

    #[test]
    fn match_is_leftmost_shortest() {
        let p = Pattern::parse("{{a}}-{{b}}").unwrap();
        assert_eq!(
            p.match_ground("x-y-z"),
            Some(binds(&[("a", "x"), ("b", "y-z")]))
        );
    }

    #[test]
    fn repeated_variables_must_agree() {
        let p = Pattern::parse("{{p}}-{{p}}").unwrap();
        assert_eq!(p.match_ground("x-x"), Some(binds(&[("p", "x")])));
        assert_eq!(p.match_ground("x-y"), None);
    }

    #[test]
    fn matching_backtracks_when_the_shortest_choice_fails() {
        // Shortest-first would take a = "x", leaving "y-x-y" for the repeat.
        // Only a = "x-y" satisfies both occurrences.
        let p = Pattern::parse("{{a}}-{{a}}").unwrap();
        assert_eq!(p.match_ground("x-y-x-y"), Some(binds(&[("a", "x-y")])));
    }

    #[test]
    fn bindings_are_never_empty() {
        assert_eq!(Pattern::parse("{{p}}").unwrap().match_ground(""), None);
        assert_eq!(Pattern::parse("a{{p}}").unwrap().match_ground("a"), None);
        assert_eq!(Pattern::parse("{{p}}a").unwrap().match_ground("a"), None);
    }

    #[test]
    fn matching_respects_character_boundaries() {
        let p = Pattern::parse("{{a}}-{{b}}").unwrap();
        assert_eq!(
            p.match_ground("héllo-wörld"),
            Some(binds(&[("a", "héllo"), ("b", "wörld")]))
        );
        assert_eq!(
            Pattern::parse("{{a}}").unwrap().match_ground("日本語"),
            Some(binds(&[("a", "日本語")]))
        );
    }

    #[test]
    fn substitute_grounds_a_pattern() {
        let p = Pattern::parse("binary-installed/{{process}}").unwrap();
        let out = p.substitute(&binds(&[("process", "p1")]));
        assert!(out.is_ground());
        assert_eq!(out.source(), "binary-installed/p1");
        assert_eq!(
            out.segments(),
            &[Segment::Literal("binary-installed/p1".into())]
        );
    }

    #[test]
    fn substitute_leaves_unbound_variables_in_place() {
        let p = Pattern::parse("kube-api/{{env}}/{{tenant}}").unwrap();
        let out = p.substitute(&binds(&[("env", "prod")]));
        assert!(!out.is_ground());
        assert_eq!(out.source(), "kube-api/prod/{{tenant}}");
        assert_eq!(out.vars().into_iter().collect::<Vec<_>>(), vec!["tenant"]);
        // and the result is itself a well-formed pattern
        assert_eq!(Pattern::parse(out.source()).unwrap(), out);
    }

    #[test]
    fn substitute_ignores_variables_it_has_no_value_for() {
        let p = Pattern::parse("a/{{x}}").unwrap();
        assert_eq!(p.substitute(&binds(&[("unrelated", "v")])), p);
        assert_eq!(p.substitute(&Bindings::new()), p);
    }

    #[test]
    fn substitute_merges_neighbouring_literals() {
        // "a" + "b" + "c" has to collapse into one segment, or `source` would
        // stop being canonical for the value it represents.
        let p = Pattern::parse("a{{x}}c").unwrap();
        let out = p.substitute(&binds(&[("x", "b")]));
        assert_eq!(out.segments(), &[Segment::Literal("abc".into())]);
        assert_eq!(out, Pattern::parse("abc").unwrap());
    }

    #[test]
    fn substitute_drops_an_empty_value() {
        let p = Pattern::parse("a{{x}}b").unwrap();
        let out = p.substitute(&binds(&[("x", "")]));
        assert_eq!(out.segments(), &[Segment::Literal("ab".into())]);
    }

    #[test]
    fn match_then_substitute_is_a_round_trip() {
        let promise = Pattern::parse("process-execution/{{process}}").unwrap();
        let condition = Pattern::parse("binary-installed/{{process}}").unwrap();
        let bindings = promise.match_ground("process-execution/p2").unwrap();
        assert_eq!(
            condition.substitute(&bindings).source(),
            "binary-installed/p2"
        );
    }

    #[test]
    fn identity_follows_the_source_text() {
        let a = Pattern::parse("b1").unwrap();
        let b = Pattern::parse("b1").unwrap();
        let c = Pattern::parse("b2").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a < c);

        // sorting matches what plain string names did
        let mut sorted = vec![
            Pattern::from("b3"),
            Pattern::from("b1"),
            Pattern::from("{{v}}"),
            Pattern::from("b2"),
        ];
        sorted.sort();
        assert_eq!(
            sorted
                .iter()
                .map(|p| p.source().as_str())
                .collect::<Vec<_>>(),
            vec!["b1", "b2", "b3", "{{v}}"]
        );

        use std::collections::HashSet;
        let set: HashSet<Pattern> = [Pattern::from("b1"), Pattern::from("b1")].into();
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn serde_uses_the_bare_string() {
        let p = Pattern::parse("run/{{p}}").unwrap();
        let yaml = serde_yaml::to_string(&p).unwrap();
        assert_eq!(yaml, "run/{{p}}\n");
        let back: Pattern = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, p);
        assert!(!back.is_ground());
    }
}
