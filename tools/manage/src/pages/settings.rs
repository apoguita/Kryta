//! Settings: each program's own JSON file, read here and never written.
//!
//! The values are the file's, not the programs' effective ones: a program reads its file at
//! start-up and applies the environment over it, and the file is what an operator shares. The
//! passwords in a connection string are masked by the reader itself, so a page is safe to paste
//! into an issue.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The settings page.
pub fn page(manage: &Manage) -> Response {
    let (world_path, auth_path) = manage.config_paths();
    let body = format!(
        "<p class=\"note\">Each program's own JSON, read here and never written: an operator\
         shares the file, and the program reads it at start-up. Values are the file's; an\
         environment variable overrides a key at the program's start, not here.</p>\n\
         <h2>the world program</h2>\n{}\n\
         <h2>the login program</h2>\n{}\n",
        table_for(world_path),
        table_for(auth_path)
    );
    layout::page("Settings", "settings", &body)
}

/// One program's file, as a table of its keys.
fn table_for(path: Option<&str>) -> String {
    let Some(path) = path else {
        return components::note(
            "No settings file is named in manage.json. Add WorldConfigFile or AuthConfigFile to\
             show one.",
        );
    };
    let config = match common::configuration::Config::load_json(
        path,
        false,
        common::configuration::Policy::default(),
    ) {
        Ok(config) => config,
        Err(error) => {
            return components::note(&format!(
                "<code>{}</code> cannot be read: {}",
                components::escape(path),
                components::escape(&error.to_string())
            ));
        }
    };
    let mut rows: Vec<(String, String)> = config.describe();
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let rows = rows
        .iter()
        .map(|(key, value)| {
            components::row(&[
                format!("<code>{}</code>", components::escape(key)),
                format!("<code>{}</code>", components::escape(value)),
            ])
        })
        .collect::<Vec<_>>();
    format!(
        "<p class=\"note\"><code>{}</code> - {} key(s).</p>\n{}",
        components::escape(path),
        rows.len(),
        components::table(&["key", "value"], &rows)
    )
}
