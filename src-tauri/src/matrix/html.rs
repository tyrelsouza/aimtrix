//! Cleaning message HTML (`formatted_body`), both what arrives and what we
//! send, so windows can show it as-is.
//!
//! Keeps the formatting tags the Matrix spec suggests and drops everything
//! else: scripts, styles, event handlers, unknown attributes, and links that
//! aren't web, mail or Matrix links. Colors survive only as `#rrggbb`.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use ammonia::{Builder, UrlRelative};

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let mut builder = Builder::empty();
    builder
        .tags(HashSet::from([
            "font",
            "del",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "blockquote",
            "p",
            "a",
            "ul",
            "ol",
            "sup",
            "sub",
            "li",
            "b",
            "i",
            "u",
            "strong",
            "em",
            "s",
            "strike",
            "code",
            "hr",
            "br",
            "div",
            "pre",
            "span",
        ]))
        .tag_attributes(HashMap::from([
            (
                "font",
                HashSet::from(["color", "data-mx-color", "data-mx-bg-color"]),
            ),
            (
                "span",
                HashSet::from(["data-mx-color", "data-mx-bg-color", "data-mx-spoiler"]),
            ),
            ("a", HashSet::from(["href"])),
            ("ol", HashSet::from(["start"])),
        ]))
        .generic_attributes(HashSet::new())
        .url_schemes(HashSet::from(["https", "http", "mailto", "matrix"]))
        .url_relative(UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        // Removed with their contents. `mx-reply` is the quoted message a
        // reply carries for clients that don't show replies.
        .clean_content_tags(HashSet::from(["script", "style", "mx-reply"]))
        .attribute_filter(|_element, attribute, value| match attribute {
            "color" | "data-mx-color" | "data-mx-bg-color" => {
                is_hex_color(value).then_some(Cow::Borrowed(value))
            }
            _ => Some(Cow::Borrowed(value)),
        });
    builder
});

/// `html` with only safe formatting left.
pub fn sanitize(html: &str) -> String {
    SANITIZER.clean(html).to_string()
}

/// `#rrggbb`. Anything else could smuggle CSS into the style it becomes.
fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_basic_formatting() {
        let html = "<b>bold</b> <i>it</i> <u>under</u> <em>e</em> <strong>s</strong><br>next";
        assert_eq!(sanitize(html), html);
    }

    #[test]
    fn keeps_valid_colors() {
        let html = r##"<font color="#ff0000">red</font> <span data-mx-color="#00FF00" data-mx-bg-color="#000000">green</span>"##;
        assert_eq!(sanitize(html), html);
    }

    #[test]
    fn drops_colors_that_are_not_hex() {
        assert_eq!(
            sanitize(r#"<font color="red;background:url(x)">x</font>"#),
            "<font>x</font>"
        );
        assert_eq!(
            sanitize(r#"<span data-mx-color="expression(alert(1))">x</span>"#),
            "<span>x</span>"
        );
    }

    #[test]
    fn strips_scripts_and_styles_with_their_contents() {
        assert_eq!(
            sanitize("hi<script>alert('x')</script><style>p{}</style> there"),
            "hi there"
        );
    }

    #[test]
    fn strips_event_handlers_and_unknown_attributes() {
        assert_eq!(
            sanitize(r#"<b onclick="alert(1)" style="color:red" class="x">b</b>"#),
            "<b>b</b>"
        );
    }

    #[test]
    fn strips_unknown_tags_but_keeps_their_text() {
        assert_eq!(
            sanitize(
                r#"<img src="x" onerror="alert(1)">a<iframe src="x"></iframe><marquee>b</marquee>"#
            ),
            "ab"
        );
    }

    #[test]
    fn removes_reply_fallbacks() {
        assert_eq!(
            sanitize("<mx-reply><blockquote>quoted</blockquote></mx-reply>answer"),
            "answer"
        );
    }

    #[test]
    fn keeps_only_safe_links() {
        assert_eq!(
            sanitize(r#"<a href="https://example.org">ok</a>"#),
            r#"<a href="https://example.org" rel="noopener noreferrer">ok</a>"#
        );
        assert_eq!(
            sanitize(r#"<a href="javascript:alert(1)">bad</a>"#),
            r#"<a rel="noopener noreferrer">bad</a>"#
        );
        assert_eq!(
            sanitize(r#"<a href="/relative">rel</a>"#),
            r#"<a rel="noopener noreferrer">rel</a>"#
        );
    }

    #[test]
    fn escapes_text() {
        assert_eq!(sanitize("1 < 2 & 3"), "1 &lt; 2 &amp; 3");
    }
}
