//! Building standalone HTML pages out of a view.
//!
//! An exported page has to stand on its own: no app JS, no API, no localStorage.
//! That means the styling it relies on travels with it, and that text which the
//! live view gets escaped for free by Leptos has to be escaped by hand here.

/// The app's own stylesheet, inlined so an exported page looks like the view it
/// came from. Taken from the source file rather than copied, so the two cannot
/// drift apart.
const APP_CSS: &str = include_str!("../../style/main.css");

/// The handful of Bootstrap rules the views lean on. Bootstrap itself is loaded
/// from a CDN and never lands in `dist/`, and an exported page should not need
/// the network for its own layout, so the used subset is reproduced here.
const BOOTSTRAP_SHIM: &str = "\
.card { border: 1px solid rgba(0,0,0,.175); border-radius: .375rem; background: #fff; }
.card-body { padding: 1rem; }
.text-muted { color: #6c757d; }
.p-3 { padding: 1rem; }
.mb-2 { margin-bottom: .5rem; }
.mb-3 { margin-bottom: 1rem; }
.me-1 { margin-right: .25rem; }
.me-2 { margin-right: .5rem; }
.visually-hidden { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0,0,0,0); }
body { font-family: system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif; margin: 0 1rem; }
.export-header { color: #666; font-size: 0.85em; margin-bottom: 0.75rem; }
.export-header strong { color: #212529; font-size: 1.15em; }";

/// Escape text for inclusion in HTML.
///
/// Agent and behavior names come from user contracts, so they can contain
/// anything. The live view is built through Leptos, which escapes for us; an
/// exported page is assembled as a string and is not.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Collapse a label into something safe to use as a file name.
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_dash = true; // trims leading separators
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// The current time as an ISO 8601 string, for stamping an export.
pub fn generated_at() -> String {
    js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default()
}

/// Wrap a body fragment in a complete, self-contained HTML document.
///
/// `head_extra` is emitted verbatim inside `<head>`, for the rare page that
/// needs a script tag. `body` is emitted verbatim too: callers pass markup, so
/// they own escaping whatever went into it.
pub fn html_document(title: &str, subtitle: &str, head_extra: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"UTF-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
         <title>{title}</title>\n\
         <style>\n{shim}\n{css}\n</style>\n\
         {head_extra}\n\
         </head>\n\
         <body>\n\
         <div class=\"export-header\"><strong>{title}</strong><br>{subtitle}</div>\n\
         {body}\n\
         </body>\n\
         </html>\n",
        title = escape_html(title),
        subtitle = escape_html(subtitle),
        shim = BOOTSTRAP_SHIM,
        css = APP_CSS,
        head_extra = head_extra,
        body = body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_the_characters_that_break_markup() {
        assert_eq!(
            escape_html("a<b> & \"c\" 'd'"),
            "a&lt;b&gt; &amp; &quot;c&quot; &#39;d&#39;"
        );
    }

    #[test]
    fn escaping_leaves_ordinary_text_alone() {
        assert_eq!(escape_html("web server | prod"), "web server | prod");
    }

    #[test]
    fn slugify_makes_a_file_name() {
        assert_eq!(slugify("Simulation A"), "simulation-a");
        assert_eq!(slugify("web server | prod"), "web-server-prod");
        assert_eq!(slugify("---"), "");
    }

    #[test]
    fn document_is_complete_and_self_contained() {
        let doc = html_document("Title", "sub", "", "<p>body</p>");

        assert!(doc.starts_with("<!DOCTYPE html>"));
        assert_eq!(doc.matches("<!DOCTYPE html>").count(), 1);
        assert!(doc.contains("<title>Title</title>"));
        assert!(doc.contains("<p>body</p>"));
        // The stylesheet rides along rather than being linked
        assert!(doc.contains(".contract-text-option"));
        assert!(doc.contains(".card-body"));
        assert!(!doc.contains("<link"));
    }

    #[test]
    fn document_escapes_its_title_and_subtitle() {
        let doc = html_document("a<b>", "x & y", "", "");

        assert!(doc.contains("<title>a&lt;b&gt;</title>"));
        assert!(doc.contains("x &amp; y"));
        assert!(!doc.contains("<title>a<b>"));
    }
}
