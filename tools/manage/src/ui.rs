//! The panel's pages: static files under `tools/manage/ui/`, embedded in the binary and served as
//! they are. There is no build step and nothing is fetched from another host.
//!
//! `index.html` is one page for every address (the browser routes on the `#` part). The only thing
//! written into it is one `<script>` line per registered area, from the table in `areas`, so an
//! area's page is loaded by registering the area and by nothing else.

use shared::network::http::Response;

use crate::areas::Area;

const INDEX: &str = include_str!("../ui/index.html");
const STYLE: &str = include_str!("../ui/app.css");
const SHELL: &str = include_str!("../ui/app.js");

/// Where `index.html` receives the areas' script lines.
const AREA_SCRIPTS: &str = "<!-- area scripts -->";

const HTML: &str = "text/html; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const JAVASCRIPT: &str = "text/javascript; charset=utf-8";

/// The static file at `path`, or `None` when the panel has no such file.
pub fn file(path: &str, areas: &[Area]) -> Option<Response> {
    match path {
        "/" | "/index.html" => Some(Response::new(200, HTML, index(areas))),
        "/ui/app.css" => Some(Response::new(200, CSS, STYLE)),
        "/ui/app.js" => Some(Response::new(200, JAVASCRIPT, SHELL)),
        _ => {
            let id = path.strip_prefix("/ui/areas/")?.strip_suffix(".js")?;
            let area = areas.iter().find(|area| area.id == id)?;
            Some(Response::new(200, JAVASCRIPT, area.script))
        }
    }
}

/// `index.html` with one script line per area, in the table's order.
fn index(areas: &[Area]) -> String {
    let scripts: String = areas
        .iter()
        .map(|area| format!("<script src=\"/ui/areas/{}.js\"></script>\n", area.id))
        .collect();
    INDEX.replace(AREA_SCRIPTS, &scripts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::areas::AREAS;

    #[test]
    fn each_static_file_has_its_content_type() {
        let page = file("/", AREAS).expect("the page is served");
        assert_eq!(page.content_type, HTML);
        assert_eq!(file("/ui/app.css", AREAS).expect("css").content_type, CSS);
        assert_eq!(file("/ui/app.js", AREAS).expect("js").content_type, JAVASCRIPT);
        assert!(file("/ui/areas/accounts.js", AREAS).is_some());
        assert!(file("/ui/areas/nothing.js", AREAS).is_none());
        assert!(file("/ui/../Cargo.toml", AREAS).is_none());
        assert!(file("/accounts", AREAS).is_none());
    }

    #[test]
    fn the_page_loads_every_registered_area_and_nothing_from_another_host() {
        let page = String::from_utf8(file("/", AREAS).expect("the page").body).expect("text");
        assert!(INDEX.contains(AREA_SCRIPTS), "index.html keeps its placeholder");
        for area in AREAS {
            assert!(page.contains(&format!("src=\"/ui/areas/{}.js\"", area.id)));
        }
        for text in [INDEX, STYLE, SHELL] {
            assert!(!text.contains("http://") && !text.contains("https://"));
            assert!(text.is_ascii());
        }
    }

    #[test]
    fn the_pages_carry_no_inline_handlers_and_no_eval() {
        let mut sources = vec![INDEX, STYLE, SHELL];
        sources.extend(AREAS.iter().map(|area| area.script));
        for text in sources {
            assert!(text.is_ascii());
            for banned in ["onclick=", "onsubmit=", "eval(", "innerHTML", "document.write"] {
                assert!(!text.contains(banned), "{banned} is not used in the pages");
            }
        }
    }
}
