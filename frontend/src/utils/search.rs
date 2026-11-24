//! Text normalisation shared by the UI's search fields.

/// The form of a query or a candidate that search fields compare: lowercased,
/// without `{`, `}`, `<` and `>`, and trimmed. Behavior names can show bound
/// values as `<A1>` and variables as `{{doc}}`; dropping that punctuation on
/// both sides lets `A1 reads` find `<A1> reads R1` and `<A1>` find
/// `A1 reads R1`. A key that comes out empty should match everything.
pub fn search_key(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '{' | '}' | '<' | '>'))
        .collect::<String>()
        .trim()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(query: &str, text: &str) -> bool {
        search_key(text).contains(&search_key(query))
    }

    #[test]
    fn strips_braces_and_angle_brackets_and_lowercases() {
        assert_eq!(search_key("<A1> reads R1"), "a1 reads r1");
        assert_eq!(search_key("{{doc}} shows usage"), "doc shows usage");
        assert_eq!(search_key("{{!self}}-Ready"), "!self-ready");
        assert_eq!(search_key("Plain Name"), "plain name");
    }

    #[test]
    fn only_those_four_characters_are_dropped() {
        assert_eq!(search_key("a?b!c[d](e)-f_g.h"), "a?b!c[d](e)-f_g.h");
    }

    #[test]
    fn blank_or_punctuation_only_text_gives_an_empty_key() {
        assert_eq!(search_key(""), "");
        assert_eq!(search_key("   "), "");
        assert_eq!(search_key("<>"), "");
        assert_eq!(search_key(" {{ }} "), "");
    }

    #[test]
    fn query_and_text_meet_whichever_side_has_the_punctuation() {
        assert!(found("A1 reads", "<A1> reads R1"));
        assert!(found("<A1>", "A1 reads R1"));
        assert!(found("<a1> READS", "<A1> reads <R1>"));
        assert!(found("{{doc}}", "{{doc}} shows usage"));
        assert!(found("doc shows", "{{doc}} shows usage"));
        assert!(found("{{doc}} shows", "doc shows usage"));
        assert!(!found("A2", "<A1> reads R1"));
    }
}
