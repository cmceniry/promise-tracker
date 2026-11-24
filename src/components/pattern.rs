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

/// The reserved variable an agent names itself by, in both its spellings.
///
/// `!` opens the built-in namespace: it is not a legal character in an
/// ordinary variable, so no name an author picks can collide with one. A
/// built-in is never bound by matching a goal; [`self_bindings`] supplies it
/// when the tracker builds its working agents.
pub const SELF_VARS: [&str; 2] = ["!self", "!SELF"];

/// The bindings that replace `{{!self}}` with `name`.
pub fn self_bindings(name: &str) -> Bindings {
    SELF_VARS
        .iter()
        .map(|v| (v.to_string(), name.to_string()))
        .collect()
}

/// Is this variable open — written `{{?x}}`, and bound by whoever provides a
/// match rather than by the promise name?
pub fn is_open(variable: &str) -> bool {
    variable.starts_with('?')
}

/// Is this variable name one of the reserved built-ins?
pub fn is_builtin(variable: &str) -> bool {
    variable.starts_with('!')
}

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
    /// `{{!x}}` where `x` is not a built-in.
    UnknownBuiltin(String),
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PatternError::Unterminated => write!(f, "`{{{{` with no closing `}}}}`"),
            PatternError::EmptyVariable => write!(f, "empty variable name in `{{{{}}}}`"),
            PatternError::InvalidVariable(v) => write!(
                f,
                "invalid variable name `{}`, expected only letters, digits, `_` or `-` (after a leading `?` for an open variable)",
                v
            ),
            PatternError::AdjacentVariables(a, b) => write!(
                f,
                "`{{{{{}}}}}` and `{{{{{}}}}}` are adjacent; separate them with literal text",
                a, b
            ),
            PatternError::UnknownBuiltin(v) => write!(
                f,
                "unknown built-in `{{{{{}}}}}`; the only one is `{{{{!self}}}}`",
                v
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
    /// Byte ranges of `source` that a substitution filled in, in order. For
    /// display only ([`Pattern::marked`]); never part of the identity.
    bound: Vec<(usize, usize)>,
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
            if is_builtin(name) {
                if !SELF_VARS.contains(&name) {
                    return Err(PatternError::UnknownBuiltin(name.to_string()));
                }
            } else if name == "?"
                || !name
                    .strip_prefix('?')
                    .unwrap_or(name)
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
            bound: vec![],
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
            bound: vec![],
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

    /// Does this name hold an open variable, `{{?x}}`?
    pub fn has_open(&self) -> bool {
        self.vars().into_iter().any(is_open)
    }

    /// Does this name hold a variable that something has to supply before it
    /// can be resolved? An open variable does not count: resolution itself
    /// finds its value.
    pub fn needs_binding(&self) -> bool {
        self.vars().into_iter().any(|v| !is_open(v))
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

    /// Match this pattern against another one, each side's variables taking
    /// values from the other: two-sided matching.
    ///
    /// The two sides have separate namespaces: `{{x}}` here and `{{x}}` in
    /// `other` are different variables. A variable on either side takes the
    /// other side's literal text, or lines up with exactly one of the other
    /// side's variables; `{{kc}} deploys sampleapp gateway` against
    /// `C deploys {{appname}} gateway` gives `kc = C`, `appname = sampleapp`.
    /// The conventions are [`Pattern::match_ground`]'s — leftmost-shortest, a
    /// binding never empty, a repeated variable agreeing — and with `other`
    /// concrete this is `match_ground` exactly.
    ///
    /// Not matched (yet): a variable taking the other side's variable plus
    /// text (`{{kc}}` against `C-{{zone}}`), and a variable lining up with one
    /// that is already lined up with a different variable.
    ///
    /// With variables on both sides, the wording has to correspond as well
    /// ([`Wording`]): `{{replica}} is healthy` does not match `placed for
    /// {{team}}`, though `placed for is healthy` is an instance of both.
    pub fn unify(&self, other: &Pattern) -> Option<Unifier> {
        let left = tokens(&self.segments);
        let right = tokens(&other.segments);
        let mut unifier = Unifier {
            wording: Wording::new(self, other),
            ..Unifier::default()
        };
        if unify_from(&left, &right, &mut unifier) {
            Some(unifier)
        } else {
            None
        }
    }

    /// The same name with its variables renamed in order of first
    /// appearance (`{{_0}}`, `{{_1}}`, ...), and the renaming used, from the
    /// names as written to the new ones. Two names that differ only in what
    /// their variables are called come out the same.
    pub fn canonical(&self) -> (Pattern, BTreeMap<String, String>) {
        let mut renaming: BTreeMap<String, String> = BTreeMap::new();
        let segments: Vec<Segment> = self
            .segments
            .iter()
            .map(|s| match s {
                Segment::Var(v) => {
                    let next = format!("_{}", renaming.len());
                    Segment::Var(renaming.entry(v.clone()).or_insert(next).clone())
                }
                literal => literal.clone(),
            })
            .collect();
        (
            Pattern {
                source: render(&segments),
                segments,
                bound: vec![],
            },
            renaming,
        )
    }

    /// Replace bound variables with their values.
    ///
    /// Unbound variables are left in place, so a partially bound pattern is
    /// still a pattern — see partial binding in the design doc. Values are
    /// plain text (Restriction B); a value containing `{{` would not round-trip
    /// and is rejected by validation before it reaches here.
    pub fn substitute(&self, bindings: &Bindings) -> Pattern {
        let mut segments: Vec<Segment> = vec![];
        let mut bound: Vec<(usize, usize)> = vec![];
        // Where this segment starts in the text as it was, and as it will be.
        let mut old_at = 0;
        let mut new_at = 0;
        for segment in &self.segments {
            let next = match segment {
                Segment::Var(v) => {
                    let written = v.len() + 4;
                    old_at += written;
                    match bindings.get(v) {
                        Some(value) => {
                            if !value.is_empty() {
                                bound.push((new_at, new_at + value.len()));
                            }
                            new_at += value.len();
                            Segment::Literal(value.clone())
                        }
                        None => {
                            new_at += written;
                            Segment::Var(v.clone())
                        }
                    }
                }
                Segment::Literal(l) => {
                    // What an earlier substitution filled in sits inside
                    // literal text, and moves with it.
                    for (start, end) in &self.bound {
                        if *start >= old_at && *end <= old_at + l.len() {
                            bound.push((start - old_at + new_at, end - old_at + new_at));
                        }
                    }
                    old_at += l.len();
                    new_at += l.len();
                    Segment::Literal(l.clone())
                }
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
            bound,
        }
    }

    /// The same name with some of its variables called something else:
    /// `{{k}} is up` with `k` renamed to `k_2` is `{{k_2}} is up`. What a
    /// substitution filled in earlier moves with the text around it.
    pub fn rename_vars(&self, renaming: &BTreeMap<String, String>) -> Pattern {
        let mut segments: Vec<Segment> = vec![];
        let mut bound: Vec<(usize, usize)> = vec![];
        let mut old_at = 0;
        let mut new_at = 0;
        for segment in &self.segments {
            match segment {
                Segment::Var(v) => {
                    let renamed = renaming.get(v).unwrap_or(v);
                    old_at += v.len() + 4;
                    new_at += renamed.len() + 4;
                    segments.push(Segment::Var(renamed.clone()));
                }
                Segment::Literal(l) => {
                    for (start, end) in &self.bound {
                        if *start >= old_at && *end <= old_at + l.len() {
                            bound.push((start - old_at + new_at, end - old_at + new_at));
                        }
                    }
                    old_at += l.len();
                    new_at += l.len();
                    segments.push(Segment::Literal(l.clone()));
                }
            }
        }
        Pattern {
            source: render(&segments),
            segments,
            bound,
        }
    }

    /// Replace `{{!self}}` with the name of the agent that wrote it.
    pub fn bind_self(&self, name: &str) -> Pattern {
        self.substitute(&self_bindings(name))
    }

    /// Did a substitution fill in any part of this name?
    pub fn has_bound(&self) -> bool {
        !self.bound.is_empty()
    }

    /// The byte ranges of the source a substitution filled in, in order.
    pub fn bound(&self) -> &[(usize, usize)] {
        &self.bound
    }

    /// The source with every substituted value wrapped in `<>`, to show what
    /// was written as a variable. For display only: `<A1> reads R1` and
    /// `A1 reads R1` are the same name.
    pub fn marked(&self) -> String {
        mark_spans(&self.source, &self.bound)
    }

    /// Where this pattern's variables, and whatever was substituted into it
    /// earlier, fall in a concrete `name` it matches: `{{who}} backs up`
    /// against `b1 backs up` gives the range of `b1`. None when it does not
    /// match.
    pub fn spans_on(&self, name: &str) -> Option<Vec<(usize, usize)>> {
        let bindings = self.match_ground(name)?;
        Some(self.substitute(&bindings).bound)
    }
}

/// `text` with each of `spans` wrapped in `<>`. For display only.
///
/// Spans are byte ranges. One that falls outside the text, splits a
/// character or runs into the one before it is left unmarked rather than
/// trusted.
pub fn mark_spans(text: &str, spans: &[(usize, usize)]) -> String {
    mark_spans_with(text, spans, "<", ">")
}

/// [`mark_spans`] with the marks spelled some other way, for a renderer that
/// reads `<` and `>` as markup.
pub fn mark_spans_with(text: &str, spans: &[(usize, usize)], open: &str, close: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (start, end) in spans {
        let (start, end) = (*start, *end);
        if start < at
            || end <= start
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        out.push_str(&text[at..start]);
        out.push_str(open);
        out.push_str(&text[start..end]);
        out.push_str(close);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// Two sets of spans over the same text as one: in order, each once, with
/// spans that overlap run together. Spans that only touch stay apart, so two
/// values side by side still read as two.
pub fn merge_spans(a: &[(usize, usize)], b: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut all: Vec<(usize, usize)> = a.iter().chain(b.iter()).copied().collect();
    all.sort();
    let mut out: Vec<(usize, usize)> = vec![];
    for (start, end) in all {
        match out.last_mut() {
            Some(last) if start < last.1 || (start, end) == *last => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
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

/// What two names matched against each other bound ([`Pattern::unify`]):
/// each side's variables, by name, with the part of the other side each one
/// took, as that side's segments.
///
/// Today a binding is always one literal (`kc = C`) or one variable (lined up
/// with it, and recorded on both sides). Holding segments rather than either
/// of those leaves room for a variable that takes the other side's variable
/// plus text.
#[derive(Debug, Clone, Default)]
pub struct Unifier {
    /// The left side's (`self`'s) variables.
    pub left: BTreeMap<String, Vec<Segment>>,
    /// The right side's (`other`'s) variables.
    pub right: BTreeMap<String, Vec<Segment>>,
    /// The wording rule's bookkeeping while matching; no part of the result.
    wording: Wording,
}

impl PartialEq for Unifier {
    fn eq(&self, other: &Self) -> bool {
        self.left == other.left && self.right == other.right
    }
}
impl Eq for Unifier {}

/// The "wording must correspond" rule of two-sided matching, kept here in
/// one place so that it can be changed or dropped without touching anything
/// else (`docs/design/self-and-open-variables.md` §5).
///
/// It applies only when both sides hold variables; with either side
/// concrete, matching is `match_ground`'s. Each literal segment's wording is
/// its characters outside what a substitution filled in (`Pattern::bound`).
/// A segment whose wording holds a letter or digit must have at least one
/// such character matched against literal text of the other side, not taken
/// by one of the other side's variables. Segments without one (a separator,
/// or nothing but a substituted value) are exempt. A match failing this is no
/// match, and matching goes on to the next way of matching.
#[derive(Debug, Clone, Default)]
struct Wording {
    applies: bool,
    left: SideWording,
    right: SideWording,
}

#[derive(Debug, Clone, Default)]
struct SideWording {
    /// For each token, the wording segment it is a letter or digit of.
    segment_of: Vec<Option<usize>>,
    /// For each wording segment, how many of its letters and digits are
    /// matched against the other side's literal text so far.
    matched: Vec<usize>,
}

impl SideWording {
    fn new(pattern: &Pattern) -> SideWording {
        let mut segment_of = vec![];
        let mut segments = 0;
        let mut at = 0;
        for segment in &pattern.segments {
            match segment {
                Segment::Var(v) => {
                    segment_of.push(None);
                    at += v.len() + 4;
                }
                Segment::Literal(l) => {
                    let mut wording = false;
                    for c in l.chars() {
                        let filled_in = pattern.bound.iter().any(|(s, e)| *s <= at && at < *e);
                        if c.is_alphanumeric() && !filled_in {
                            wording = true;
                            segment_of.push(Some(segments));
                        } else {
                            segment_of.push(None);
                        }
                        at += c.len_utf8();
                    }
                    if wording {
                        segments += 1;
                    }
                }
            }
        }
        SideWording {
            segment_of,
            matched: vec![0; segments],
        }
    }

    /// The token `remaining` tokens from the end.
    fn at(&self, remaining: usize) -> Option<usize> {
        self.segment_of[self.segment_of.len() - remaining]
    }

    fn corresponds(&self) -> bool {
        self.matched.iter().all(|n| *n > 0)
    }
}

impl Wording {
    fn new(left: &Pattern, right: &Pattern) -> Wording {
        Wording {
            applies: !left.is_ground() && !right.is_ground(),
            left: SideWording::new(left),
            right: SideWording::new(right),
        }
    }

    /// Two literal characters, `left_remaining` and `right_remaining` tokens
    /// from the end of their sides, matched against each other (`by` 1), or
    /// that match undone (`by` -1).
    fn literal_match(&mut self, left_remaining: usize, right_remaining: usize, undo: bool) {
        if !self.applies {
            return;
        }
        for (side, remaining) in [(&mut self.left, left_remaining), (&mut self.right, right_remaining)] {
            if let Some(segment) = side.at(remaining) {
                if undo {
                    side.matched[segment] -= 1;
                } else {
                    side.matched[segment] += 1;
                }
            }
        }
    }

    fn corresponds(&self) -> bool {
        !self.applies || (self.left.corresponds() && self.right.corresponds())
    }
}

impl Unifier {
    /// The left side's variables that took literal text, with that text.
    pub fn left_values(&self) -> Bindings {
        literal_values(&self.left)
    }

    /// The right side's variables that took literal text, with that text.
    pub fn right_values(&self) -> Bindings {
        literal_values(&self.right)
    }

    /// Each left variable lined up with a right variable, as (left, right).
    pub fn aligned(&self) -> Vec<(String, String)> {
        self.left
            .iter()
            .filter_map(|(variable, took)| match took.as_slice() {
                [Segment::Var(other)] => Some((variable.clone(), other.clone())),
                _ => None,
            })
            .collect()
    }
}

fn literal_values(side: &BTreeMap<String, Vec<Segment>>) -> Bindings {
    side.iter()
        .filter_map(|(variable, took)| match took.as_slice() {
            [Segment::Literal(text)] => Some((variable.clone(), text.clone())),
            _ => None,
        })
        .collect()
}

/// One character of literal text, or a variable, of one side of a
/// two-sided match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Char(char),
    Var(&'a str),
}

fn tokens(segments: &[Segment]) -> Vec<Token<'_>> {
    let mut out = vec![];
    for segment in segments {
        match segment {
            Segment::Literal(l) => out.extend(l.chars().map(Token::Char)),
            Segment::Var(v) => out.push(Token::Var(v)),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Unifier {
    fn side(&mut self, side: Side) -> &mut BTreeMap<String, Vec<Segment>> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}

/// Two-sided matching, one token at a time. A variable on the left is
/// settled first, then one on the right; two characters must be equal.
fn unify_from(left: &[Token], right: &[Token], unifier: &mut Unifier) -> bool {
    match (left.first(), right.first()) {
        (None, None) => unifier.wording.corresponds(),
        (Some(Token::Var(v)), _) => bind_from(Side::Left, v, &left[1..], right, unifier),
        (_, Some(Token::Var(v))) => bind_from(Side::Right, v, &right[1..], left, unifier),
        (Some(Token::Char(a)), Some(Token::Char(b))) => {
            if a != b {
                return false;
            }
            let (l, r) = (left.len(), right.len());
            unifier.wording.literal_match(l, r, false);
            if unify_from(&left[1..], &right[1..], unifier) {
                return true;
            }
            unifier.wording.literal_match(l, r, true);
            false
        }
        _ => false,
    }
}

/// Settle `variable`, from `side`, against the start of `other`, then match
/// what follows: `rest` on the variable's side, the remainder of `other` on
/// the other.
fn bind_from(
    side: Side,
    variable: &str,
    rest: &[Token],
    other: &[Token],
    unifier: &mut Unifier,
) -> bool {
    let opposite = match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    };
    let go_on = |rest: &[Token], other: &[Token], unifier: &mut Unifier| match side {
        Side::Left => unify_from(rest, other, unifier),
        Side::Right => unify_from(other, rest, unifier),
    };
    let chars = |text: &str| text.chars().map(Token::Char).collect::<Vec<_>>();

    // A repeat has to take what was chosen for it before.
    if let Some(held) = unifier.side(side).get(variable).cloned() {
        return match held.as_slice() {
            [Segment::Literal(text)] => match other.first() {
                // The other side's variable meets a value: it takes the same.
                Some(Token::Var(theirs)) => {
                    let theirs = theirs.to_string();
                    match unifier.side(opposite).get(&theirs).cloned() {
                        None => {
                            unifier.side(opposite).insert(theirs.clone(), held);
                            if go_on(rest, &other[1..], unifier) {
                                return true;
                            }
                            unifier.side(opposite).remove(&theirs);
                            false
                        }
                        Some(same) => same == held && go_on(rest, &other[1..], unifier),
                    }
                }
                _ => {
                    let text = chars(text);
                    other.starts_with(&text) && go_on(rest, &other[text.len()..], unifier)
                }
            },
            [Segment::Var(lined_up)] => {
                other.first() == Some(&Token::Var(lined_up.as_str()))
                    && go_on(rest, &other[1..], unifier)
            }
            _ => false,
        };
    }

    match other.first() {
        None => false,
        // Two variables meet: they line up, unless the other one is settled
        // already, when this one takes what it took.
        Some(Token::Var(theirs)) => {
            let theirs = theirs.to_string();
            match unifier.side(opposite).get(&theirs).cloned() {
                None => {
                    unifier
                        .side(side)
                        .insert(variable.to_string(), vec![Segment::Var(theirs.clone())]);
                    unifier
                        .side(opposite)
                        .insert(theirs.clone(), vec![Segment::Var(variable.to_string())]);
                    if go_on(rest, &other[1..], unifier) {
                        return true;
                    }
                    unifier.side(side).remove(variable);
                    unifier.side(opposite).remove(&theirs);
                    false
                }
                Some(held) => match held.as_slice() {
                    [Segment::Literal(_)] => {
                        unifier.side(side).insert(variable.to_string(), held);
                        if go_on(rest, &other[1..], unifier) {
                            return true;
                        }
                        unifier.side(side).remove(variable);
                        false
                    }
                    // Lined up with another variable already.
                    _ => false,
                },
            }
        }
        // Literal text, as far as the next variable on the other side: try
        // each length, shortest first. Never empty.
        Some(Token::Char(_)) => {
            let available = other
                .iter()
                .take_while(|t| matches!(t, Token::Char(_)))
                .count();
            for take in 1..=available {
                // Only a length after which the variable's own side can go
                // on: its next character has to be the next one there.
                match (rest.first(), other.get(take)) {
                    (None, Some(_)) => continue,
                    (Some(Token::Char(mine)), Some(Token::Char(theirs))) if mine != theirs => {
                        continue
                    }
                    (Some(Token::Char(_)), None) => continue,
                    _ => {}
                }
                let text: String = other[..take]
                    .iter()
                    .map(|t| match t {
                        Token::Char(c) => *c,
                        Token::Var(_) => unreachable!(),
                    })
                    .collect();
                unifier
                    .side(side)
                    .insert(variable.to_string(), vec![Segment::Literal(text)]);
                if go_on(rest, &other[take..], unifier) {
                    return true;
                }
                unifier.side(side).remove(variable);
            }
            false
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
    fn self_is_a_reserved_variable_in_either_spelling() {
        for source in ["{{!self}} reads R1", "{{!SELF}} reads R1"] {
            let p = Pattern::parse(source).unwrap();
            assert!(!p.is_ground());
            assert_eq!(p.source(), source);
            let out = p.bind_self("A1");
            assert!(out.is_ground());
            assert_eq!(out.source(), "A1 reads R1");
        }
    }

    #[test]
    fn an_unknown_builtin_is_an_error() {
        assert_eq!(
            Pattern::parse("{{!slef}} reads R1"),
            Err(PatternError::UnknownBuiltin("!slef".into()))
        );
        assert_eq!(
            Pattern::parse("{{!Self}}"),
            Err(PatternError::UnknownBuiltin("!Self".into()))
        );
        // `!` anywhere but the front is just a bad variable name
        assert_eq!(
            Pattern::parse("{{a!}}"),
            Err(PatternError::InvalidVariable("a!".into()))
        );
    }

    #[test]
    fn bind_self_leaves_other_variables_alone() {
        let p = Pattern::parse("{{!self}} allows {{agent}}").unwrap();
        let out = p.bind_self("S1");
        assert_eq!(out.source(), "S1 allows {{agent}}");
        assert_eq!(out.vars().into_iter().collect::<Vec<_>>(), vec!["agent"]);
        // and a name without it is untouched
        let plain = Pattern::parse("a/{{x}}").unwrap();
        assert_eq!(plain.bind_self("S1"), plain);
    }

    #[test]
    fn marked_wraps_what_substitution_filled_in() {
        let p = Pattern::parse("{{!self}} reads R1").unwrap();
        assert!(!p.has_bound());
        assert_eq!(p.marked(), "{{!self}} reads R1");

        let out = p.bind_self("A1");
        assert!(out.has_bound());
        assert_eq!(out.marked(), "<A1> reads R1");
        // identity is still the plain text
        assert_eq!(out, Pattern::parse("A1 reads R1").unwrap());
        assert!(!Pattern::parse("A1 reads R1").unwrap().has_bound());
    }

    #[test]
    fn marks_survive_a_later_substitution() {
        let p = Pattern::parse("x/{{env}}/{{!self}}/{{tenant}}/y").unwrap();
        let first = p.substitute(&binds(&[("env", "prod")]));
        assert_eq!(first.marked(), "x/<prod>/{{!self}}/{{tenant}}/y");
        let second = first.bind_self("db");
        assert_eq!(second.marked(), "x/<prod>/<db>/{{tenant}}/y");
        let third = second.substitute(&binds(&[("tenant", "t1")]));
        assert_eq!(third.source(), "x/prod/db/t1/y");
        assert_eq!(third.marked(), "x/<prod>/<db>/<t1>/y");
    }

    #[test]
    fn spans_on_finds_what_the_variables_matched() {
        let p = Pattern::parse("{{who}} backs up {{what}}").unwrap();
        assert_eq!(p.spans_on("b1 backs up db"), Some(vec![(0, 2), (12, 14)]));
        assert_eq!(p.spans_on("b1 restores db"), None);
        // What was substituted earlier is carried onto the match.
        let open = Pattern::parse("{{app}} has role {{?role}}")
            .unwrap()
            .substitute(&Bindings::from([("app".to_string(), "é1".to_string())]));
        let spans = open.spans_on("é1 has role admin").unwrap();
        assert_eq!(
            mark_spans("é1 has role admin", &spans),
            "<é1> has role <admin>"
        );
        // A written-out name has nothing to find.
        assert_eq!(
            Pattern::parse("b1 backs up")
                .unwrap()
                .spans_on("b1 backs up"),
            Some(vec![])
        );
    }

    #[test]
    fn mark_spans_skips_what_it_cannot_trust() {
        // Out of range, splitting a character, overlapping, empty.
        assert_eq!(mark_spans("ab", &[(1, 9)]), "ab");
        assert_eq!(mark_spans("é/x", &[(1, 3)]), "é/x");
        assert_eq!(mark_spans("abcd", &[(0, 2), (1, 3)]), "<ab>cd");
        assert_eq!(mark_spans("abcd", &[(2, 2)]), "abcd");
        assert_eq!(
            mark_spans_with("abcd", &[(1, 2)], "#lt;", "#gt;"),
            "a#lt;b#gt;cd"
        );
    }

    #[test]
    fn merge_spans_joins_overlaps_and_keeps_neighbors_apart() {
        assert_eq!(merge_spans(&[(0, 2)], &[(0, 2)]), vec![(0, 2)]);
        assert_eq!(merge_spans(&[(0, 3)], &[(2, 5)]), vec![(0, 5)]);
        assert_eq!(merge_spans(&[(4, 6)], &[(0, 2)]), vec![(0, 2), (4, 6)]);
        assert_eq!(merge_spans(&[(0, 2)], &[(2, 4)]), vec![(0, 2), (2, 4)]);
        assert_eq!(merge_spans(&[], &[]), vec![]);
    }

    #[test]
    fn marks_respect_character_boundaries() {
        let p = Pattern::parse("é/{{a}}/ü").unwrap();
        let out = p.substitute(&binds(&[("a", "日本")]));
        assert_eq!(out.marked(), "é/<日本>/ü");
    }

    #[test]
    fn open_variables_parse_and_match_like_any_other() {
        let p = Pattern::parse("{{?doc}} shows usage of R1 by {{agent}}").unwrap();
        assert!(p.has_open());
        assert!(p.needs_binding());
        assert_eq!(p.source(), "{{?doc}} shows usage of R1 by {{agent}}");

        // once the promise's own variable is filled in, only the open one is left
        let goal = p.substitute(&binds(&[("agent", "A1")]));
        assert_eq!(goal.source(), "{{?doc}} shows usage of R1 by A1");
        assert!(goal.has_open());
        assert!(!goal.needs_binding());
        assert_eq!(
            goal.match_ground("D1 shows usage of R1 by A1"),
            Some(binds(&[("?doc", "D1")]))
        );
        assert_eq!(goal.match_ground("D1 shows usage of R1 by A2"), None);

        assert!(!Pattern::parse("plain/{{x}}").unwrap().has_open());
        assert_eq!(
            Pattern::parse("{{?}}"),
            Err(PatternError::InvalidVariable("?".into()))
        );
        assert_eq!(
            Pattern::parse("{{?a b}}"),
            Err(PatternError::InvalidVariable("?a b".into()))
        );
        assert_eq!(
            Pattern::parse("{{a?}}"),
            Err(PatternError::InvalidVariable("a?".into()))
        );
    }

    fn literal(text: &str) -> Vec<Segment> {
        vec![Segment::Literal(text.to_string())]
    }

    fn var(name: &str) -> Vec<Segment> {
        vec![Segment::Var(name.to_string())]
    }

    #[test]
    fn unify_fills_each_side_from_the_other() {
        let goal = Pattern::parse("{{kc}} deploys sampleapp gateway").unwrap();
        let promise = Pattern::parse("C deploys {{appname}} gateway").unwrap();
        let u = goal.unify(&promise).unwrap();
        assert_eq!(u.left_values(), binds(&[("kc", "C")]));
        assert_eq!(u.right_values(), binds(&[("appname", "sampleapp")]));
        assert!(u.aligned().is_empty());
        // and the other way round
        let u = promise.unify(&goal).unwrap();
        assert_eq!(u.left_values(), binds(&[("appname", "sampleapp")]));
        assert_eq!(u.right_values(), binds(&[("kc", "C")]));
    }

    #[test]
    fn unify_keeps_the_two_namespaces_apart() {
        // `kc` on each side is its own variable; here the two line up.
        let goal = Pattern::parse("sampleapp deploys to {{kc}}").unwrap();
        let promise = Pattern::parse("{{appname}} deploys to {{kc}}").unwrap();
        let u = goal.unify(&promise).unwrap();
        assert_eq!(u.left, BTreeMap::from([("kc".to_string(), var("kc"))]));
        assert_eq!(
            u.right,
            BTreeMap::from([
                ("appname".to_string(), literal("sampleapp")),
                ("kc".to_string(), var("kc")),
            ])
        );
        assert_eq!(u.aligned(), vec![("kc".to_string(), "kc".to_string())]);

        // Different names line up just the same.
        let u = Pattern::parse("{{x}} is up")
            .unwrap()
            .unify(&Pattern::parse("{{host}} is up").unwrap())
            .unwrap();
        assert_eq!(u.aligned(), vec![("x".to_string(), "host".to_string())]);
    }

    #[test]
    fn unify_agrees_with_match_ground_when_one_side_is_concrete() {
        let cases = [
            ("process-execution", "process-execution"),
            ("process-execution", "something-else"),
            ("", ""),
            ("", "x"),
            ("process-execution/{{process}}", "process-execution/p1"),
            ("process-execution/{{process}}", "process-execution/"),
            ("process-execution/{{process}}", "other/p1"),
            ("{{a}}-{{b}}", "x-y-z"),
            ("{{p}}-{{p}}", "x-x"),
            ("{{p}}-{{p}}", "x-y"),
            ("{{a}}-{{a}}", "x-y-x-y"),
            ("{{p}}", ""),
            ("a{{p}}", "a"),
            ("{{p}}a", "a"),
            ("{{a}}-{{b}}", "héllo-wörld"),
            ("{{a}}", "日本語"),
            ("{{who}} backs up {{what}}", "b1 backs up db"),
            ("{{who}} backs up {{what}}", "b1 restores db"),
            ("x/{{a}}/y/{{b}}", "x/1/y/2/y/3"),
            ("{{a}} and {{b}} and {{a}}", "p and q and p"),
            ("{{a}} and {{b}} and {{a}}", "p and q and r"),
        ];
        for (pattern, ground) in cases {
            let pattern = Pattern::parse(pattern).unwrap();
            let concrete = Pattern::parse(ground).unwrap();
            let expected = pattern.match_ground(ground);
            assert_eq!(
                pattern.unify(&concrete).map(|u| u.left_values()),
                expected,
                "{} against {}",
                pattern,
                ground
            );
            assert_eq!(
                concrete.unify(&pattern).map(|u| u.right_values()),
                expected,
                "{} against {}, reversed",
                pattern,
                ground
            );
            // nothing lines up with a concrete name
            if let Some(u) = pattern.unify(&concrete) {
                assert!(u.aligned().is_empty() && u.right.is_empty());
            }
        }
    }

    #[test]
    fn unify_is_leftmost_shortest_on_both_sides() {
        let u = Pattern::parse("{{a}} on {{b}}")
            .unwrap()
            .unify(&Pattern::parse("x on y on {{c}}").unwrap())
            .unwrap();
        // `a = x` leaves `b` to take `y on ` and `{{c}}` together, which is
        // not matched; the shortest `a` that works is `x on y`, and `b`
        // lines up with `c`.
        assert_eq!(u.left_values(), binds(&[("a", "x on y")]));
        assert_eq!(u.aligned(), vec![("b".to_string(), "c".to_string())]);
        assert_eq!(u.right.get("c"), Some(&var("b")));
        assert!(Pattern::parse("{{a}}")
            .unwrap()
            .unify(&Pattern::parse("{{b}}").unwrap())
            .is_some());
    }

    #[test]
    fn unify_repeats_must_agree_across_the_sides() {
        // `x` takes `a`, so `p`, meeting the second `x`, takes `a` too.
        let u = Pattern::parse("{{x}} and {{x}}")
            .unwrap()
            .unify(&Pattern::parse("a and {{p}}").unwrap())
            .unwrap();
        assert_eq!(u.left_values(), binds(&[("x", "a")]));
        assert_eq!(u.right_values(), binds(&[("p", "a")]));
        // `p` took `a` and then meets `b`
        assert_eq!(
            Pattern::parse("a and b")
                .unwrap()
                .unify(&Pattern::parse("{{p}} and {{p}}").unwrap()),
            None
        );
        // a variable lined up with one variable stays lined up with it
        let u = Pattern::parse("{{x}} and {{x}}")
            .unwrap()
            .unify(&Pattern::parse("{{p}} and {{p}}").unwrap())
            .unwrap();
        assert_eq!(u.aligned(), vec![("x".to_string(), "p".to_string())]);
        assert_eq!(
            Pattern::parse("{{x}} and {{x}}")
                .unwrap()
                .unify(&Pattern::parse("{{p}} and {{q}}").unwrap()),
            None
        );
    }

    #[test]
    fn unify_never_binds_empty_or_a_variable_plus_text() {
        let unify = |a: &str, b: &str| {
            Pattern::parse(a)
                .unwrap()
                .unify(&Pattern::parse(b).unwrap())
        };
        assert_eq!(unify("a{{x}}", "a"), None);
        assert_eq!(unify("a{{x}}b", "a{{y}}"), None);
        // deferred: `kc` would have to take `C-` and `{{zone}}` together
        assert_eq!(unify("{{kc}}", "C-{{zone}}"), None);
        assert_eq!(unify("C-{{zone}}", "{{kc}}"), None);
        // but a variable with text on both sides of it is fine
        let u = unify("{{kc}} in east", "C in {{zone}}").unwrap();
        assert_eq!(u.left_values(), binds(&[("kc", "C")]));
        assert_eq!(u.right_values(), binds(&[("zone", "east")]));
    }

    #[test]
    fn rename_vars_keeps_what_was_filled_in() {
        let p = Pattern::parse("{{a}} on {{k}} by {{a}}")
            .unwrap()
            .substitute(&binds(&[("a", "web")]));
        assert_eq!(p.marked(), "<web> on {{k}} by <web>");
        let out = p.rename_vars(&BTreeMap::from([("k".to_string(), "k_2".to_string())]));
        assert_eq!(out.source(), "web on {{k_2}} by web");
        assert_eq!(out.marked(), "<web> on {{k_2}} by <web>");
        assert_eq!(Pattern::parse(out.source()).unwrap(), out);
    }

    #[test]
    fn unify_needs_the_wording_to_correspond() {
        let unify = |a: &str, b: &str| {
            Pattern::parse(a)
                .unwrap()
                .unify(&Pattern::parse(b).unwrap())
        };
        // `placed for is healthy` is an instance of both, but neither side's
        // wording is matched by the other's.
        assert_eq!(unify("{{replica}} is healthy", "placed for {{team}}"), None);
        assert_eq!(
            unify("sampleapp deploys to {{kc}}", "{{appname}} gateway is configured"),
            None
        );
        assert_eq!(
            unify("{{kc}} deploys {{appname}} gateway", "{{appname}} deploys to {{kc}}"),
            None
        );
        // Wording that corresponds still matches.
        let u = unify("{{kc}} deploys sampleapp gateway", "C deploys {{appname}} gateway").unwrap();
        assert_eq!(u.left_values(), binds(&[("kc", "C")]));
        assert_eq!(u.right_values(), binds(&[("appname", "sampleapp")]));
        // A substituted value is not wording: `db` came from `{{!self}}`, so
        // `{{who}}` may take it, and `db/` is left with only a separator.
        let promise = Pattern::parse("{{!self}}/{{env}} reads R1").unwrap().bind_self("db");
        assert_eq!(promise.source(), "db/{{env}} reads R1");
        let u = Pattern::parse("{{who}}/prod reads R1").unwrap().unify(&promise).unwrap();
        assert_eq!(u.left_values(), binds(&[("who", "db")]));
        assert_eq!(u.right_values(), binds(&[("env", "prod")]));
        // Written out by hand, `db` is wording, and nothing matches it.
        assert_eq!(unify("{{who}}/prod reads R1", "db/{{env}} reads R1"), None);
        // The known limit: a hand-written value right before a variable is
        // wording, so a goal variable cannot take `us-east` from it.
        assert_eq!(
            unify("{{region}} replica is healthy", "us-east {{thing}} is healthy"),
            None
        );
        // With either side concrete, the rule does not apply.
        assert_eq!(
            Pattern::parse("{{replica}} is healthy")
                .unwrap()
                .unify(&Pattern::parse("placed for is healthy").unwrap())
                .map(|u| u.left_values()),
            Some(binds(&[("replica", "placed for")]))
        );
    }

    #[test]
    fn canonical_names_variables_by_order_of_appearance() {
        let (a, renaming) = Pattern::parse("{{kc}} deploys {{app}} for {{kc}}")
            .unwrap()
            .canonical();
        assert_eq!(a.source(), "{{_0}} deploys {{_1}} for {{_0}}");
        assert_eq!(
            renaming,
            BTreeMap::from([
                ("kc".to_string(), "_0".to_string()),
                ("app".to_string(), "_1".to_string())
            ])
        );
        let (b, _) = Pattern::parse("{{x}} deploys {{y}} for {{x}}")
            .unwrap()
            .canonical();
        assert_eq!(a, b);
        assert_eq!(Pattern::parse("plain").unwrap().canonical().0.source(), "plain");
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
