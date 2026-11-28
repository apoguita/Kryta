//! The areas of the panel, and the one table that registers them.
//!
//! An area is one module here and one script under `tools/manage/ui/areas/`, with the same name.
//! The module declares its routes with `pub fn routes() -> Vec<Route<Manage>>`; the script registers
//! its page with `Kryta.area({...})`. Nothing else lists areas: the router, the page's script lines,
//! the menu and the quick-find palette all follow from this table.

use crate::api::Route;
use crate::state::Manage;

/// One registered area.
pub struct Area {
    /// Its name: the module, the script file, the `#/<id>` page and the `/api/v1/<id>` prefix.
    pub id: &'static str,
    /// The routes it answers.
    pub routes: fn() -> Vec<Route<Manage>>,
    /// Its page script, `tools/manage/ui/areas/<id>.js`.
    pub script: &'static str,
}

/// Declare the area modules and build [`AREAS`] from the same list of names.
macro_rules! areas {
    ($($name:ident),* $(,)?) => {
        $(pub mod $name;)*

        /// Every area, in menu order.
        pub const AREAS: &[Area] = &[$(Area {
            id: stringify!($name),
            routes: $name::routes,
            script: include_str!(concat!("../../ui/areas/", stringify!($name), ".js")),
        }),*];
    };
}

// THE ROUTE TABLE. To add an area: write `areas/<name>.rs` with `pub fn routes()`, write
// `ui/areas/<name>.js`, and add `<name>` to this list. The recipe and a minimal example are in
// `docs/port/23-control-panel.md`, "Adding an area". The tests below fail on an area with no
// route, a route outside its area's prefix, two routes that collide, or a script that does not
// register its area.
areas! {
    overview,
    services,
    flavors,
    console,
    accounts,
    players,
    content,
    navigation,
    settings,
    schema,
    recordings,
}

/// Every route of every area.
pub fn routes() -> Vec<Route<Manage>> {
    AREAS.iter().flat_map(|area| (area.routes)()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{API_PREFIX, collide};

    #[test]
    fn every_area_has_a_route_under_its_own_prefix() {
        for area in AREAS {
            let routes = (area.routes)();
            assert!(!routes.is_empty(), "area {} declares no route", area.id);
            let prefix = format!("{API_PREFIX}{}", area.id);
            for route in routes {
                assert!(
                    route.pattern == prefix || route.pattern.starts_with(&format!("{prefix}/")),
                    "{} is not under {prefix}",
                    route.pattern
                );
                assert!(matches!(route.method, "GET" | "POST"), "{}", route.pattern);
            }
        }
    }

    #[test]
    fn no_two_routes_answer_the_same_address() {
        let routes = routes();
        for (index, left) in routes.iter().enumerate() {
            for right in &routes[index + 1..] {
                assert!(
                    left.method != right.method || !collide(left.pattern, right.pattern),
                    "{} {} collides with {} {}",
                    left.method,
                    left.pattern,
                    right.method,
                    right.pattern
                );
            }
        }
    }

    #[test]
    fn area_names_are_unique_and_each_script_registers_its_area() {
        for (index, area) in AREAS.iter().enumerate() {
            assert!(
                AREAS[index + 1..].iter().all(|other| other.id != area.id),
                "area {} is registered twice",
                area.id
            );
            assert!(
                area.script.contains("Kryta.area({")
                    && area.script.contains(&format!("id: \"{}\"", area.id)),
                "ui/areas/{}.js does not register an area with that id",
                area.id
            );
        }
    }
}
