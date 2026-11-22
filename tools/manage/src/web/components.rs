//! The HTML pieces every page is built from.
//!
//! A page is written here, not templated: no engine, no asset pipeline. Each function returns a
//! fragment of markup, and the caller escapes the data it puts into one - a name, an id, a token is
//! data, never markup.

use std::fmt::Display;

/// Escape text for HTML: it is data being shown, never markup being trusted.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// A quiet block of explanation or state, already-escaped markup.
pub fn note(inner: &str) -> String {
    format!("<p class=\"note\">{inner}</p>")
}

/// One table: its header cells, then the rows a caller built.
pub fn table(headers: &[&str], rows: &[String]) -> String {
    let head = headers
        .iter()
        .map(|header| format!("<th>{header}</th>"))
        .collect::<String>();
    format!(
        "<table><thead><tr>{head}</tr></thead><tbody>{}</tbody></table>",
        rows.join("")
    )
}

/// One row of a table, from its already-escaped cells.
pub fn row(cells: &[String]) -> String {
    let cells = cells
        .iter()
        .map(|cell| format!("<td>{cell}</td>"))
        .collect::<String>();
    format!("<tr>{cells}</tr>")
}

/// An account's or character's state, as a pill: `active` is the only two-value state a page shows.
pub fn badge(active: bool) -> String {
    let state = if active { "active" } else { "inactive" };
    format!("<span class=\"badge {state}\">{state}</span>")
}

/// One metric card: a value and what it counts.
pub fn metric(value: impl Display, label: &str) -> String {
    format!("<div class=\"metric\"><strong>{value}</strong><span>{label}</span></div>")
}

/// The trail above a page: `(href, label)` pairs, the last one the current page.
pub fn crumbs(items: &[(&str, &str)]) -> String {
    let mut out = String::from("<p class=\"crumbs\">");
    let last = items.len().saturating_sub(1);
    for (index, (href, label)) in items.iter().enumerate() {
        if index > 0 {
            out.push_str("<span class=\"sep\">/</span>");
        }
        if index == last {
            out.push_str(&format!("<span class=\"here\">{label}</span>"));
        } else {
            out.push_str(&format!("<a href=\"{href}\">{label}</a>"));
        }
    }
    out.push_str("</p>");
    out
}

/// One section tab: a link when the section exists, a muted label when it does not yet.
pub fn tab(href: Option<&str>, label: &str, current: bool) -> String {
    match href {
        Some(href) => {
            let class = if current { " class=\"current\"" } else { "" };
            format!("<a{class} href=\"{href}\">{label}</a>")
        }
        None => format!("<span>{label}</span>"),
    }
}

/// The section tabs under an account or a character, from the tabs a caller built.
pub fn section_nav(tabs: &[String]) -> String {
    format!(
        "<nav class=\"section-nav\" aria-label=\"Sections\">{}</nav>",
        tabs.join("")
    )
}

/// A labelled field whose control is already-built markup.
pub fn field(label: &str, control: &str) -> String {
    format!("<label>{label}{control}</label>")
}

/// A text input.
pub fn text_input(name: &str, value: &str) -> String {
    format!(
        "<input type=\"text\" name=\"{name}\" value=\"{}\">",
        escape(value)
    )
}

/// A text input with a placeholder and no value.
pub fn text_input_placeholder(name: &str, placeholder: &str) -> String {
    format!(
        "<input type=\"text\" name=\"{name}\" placeholder=\"{}\">",
        escape(placeholder)
    )
}

/// A number input between two bounds.
pub fn number_input(name: &str, value: impl Display, min: i64, max: i64) -> String {
    format!("<input type=\"number\" name=\"{name}\" min=\"{min}\" max=\"{max}\" value=\"{value}\">")
}

/// A hidden form field.
pub fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{name}\" value=\"{}\">",
        escape(value)
    )
}

/// A button that submits the form it sits in.
pub fn submit(label: &str) -> String {
    format!("<button type=\"submit\">{label}</button>")
}

/// A button that submits explicitly, for forms with several.
pub fn submit_class(label: &str, class: &str) -> String {
    format!("<button class=\"{class}\" type=\"submit\">{label}</button>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_escaped_and_markup_is_not() {
        assert_eq!(escape("<b>A & B</b>"), "&lt;b&gt;A &amp; B&lt;/b&gt;");
        assert_eq!(badge(true), "<span class=\"badge active\">active</span>");
        assert_eq!(
            badge(false),
            "<span class=\"badge inactive\">inactive</span>"
        );
    }

    #[test]
    fn a_trail_marks_only_its_last_place_as_current() {
        let trail = crumbs(&[("/players", "Players"), ("/players/2", "player")]);
        assert!(trail.contains("<a href=\"/players\">Players</a>"));
        assert!(trail.contains("<span class=\"here\">player</span>"));
        assert!(!trail.contains("<a href=\"/players/2\">"));
    }

    #[test]
    fn a_tab_without_a_href_is_a_quiet_label() {
        assert_eq!(
            tab(None, "Shared storage", false),
            "<span>Shared storage</span>"
        );
        assert_eq!(
            tab(Some("/x"), "Overview", true),
            "<a class=\"current\" href=\"/x\">Overview</a>"
        );
    }

    #[test]
    fn a_table_keeps_its_rows_in_order() {
        let table = table(
            &["name", "state"],
            &[row(&["A".to_string(), "active".to_string()])],
        );
        assert!(table.contains("<th>name</th><th>state</th>"));
        assert!(table.contains("<td>A</td><td>active</td>"));
    }
}
