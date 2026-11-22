//! Schema: each database's own record of the files it has applied, against the files on disk.
//!
//! A database records each schema file it has had in its own `updates` table. This page reads that
//! record and the checkout's files: a file listed as new has not been applied yet; changed means
//! the file was edited after it was applied, which the updater refuses to apply again until an
//! operator resolves it; recorded, not on disk means the database holds a record for a file this
//! checkout does not carry.

use shared::network::http::Response;

use crate::Manage;
use crate::web::{components, layout};

/// The schema page.
pub fn page(manage: &Manage) -> Response {
    let mut sections = String::new();
    for (label, files) in manage.schema_page() {
        let rows = files
            .iter()
            .map(|file| {
                components::row(&[
                    format!("<code>{}</code>", components::escape(&file.name)),
                    components::escape(file.state),
                ])
            })
            .collect::<Vec<_>>();
        sections.push_str(&format!(
            "<section class=\"panel\"><h2>{label}</h2>\
             {table}<p class=\"note\">{} file(s).</p></section>\n",
            files.len(),
            table = components::table(&["file", "state"], &rows),
        ));
    }
    let body = format!(
        "<p class=\"note\">A database records each schema file it has had in its own\
         <code>updates</code> table. A file listed as <em>new</em> has not been applied yet;\
         <em>changed</em> means the file was edited after it was applied, which the updater\
         refuses to apply again until an operator resolves it; <em>recorded, not on disk</em>\
         means the database holds a record for a file this checkout does not carry.</p>\n\
         {sections}"
    );
    layout::page("Schema", "schema", &body)
}
