//! The command table: which names a player or an operator may type, and the operation each one runs.
//!
//! Reference: `docs/decisions/0011-the-command-surface-is-content.md`. The command surface is content,
//! so the rows live in a file (`config/Server/commands.json`) and Rust keeps only the operations. The reference's
//! own dispatch is two functions - `GameSrv_TryHandleGmCommand` (`GmCommands.c`, `/tp`, `/teleport`,
//! `/maps`) and `GameSrv_HandleChatCommand` (`GmChat.c`, `/stuck`) - and the working implementation
//! names that surface once in a Rust table (`ASURA/crates/asura-world/src/chat.rs:126-163`). This is
//! that table, read from a file instead.
//!
//! **A row binds a name to a registered operation, and the binding is checked when the file is read.**
//! There is no enum of commands here and no match on one: the name is resolved through
//! [`Registry`](super::operations::Registry), and a name it does not have - or a parameter list the
//! operation does not declare - is a refused row with a reason, so a typo in the data is reported at
//! start-up rather than answering "Unknown command" to a player who typed a name the file promised.

use common::configuration::json::Document;
use common::logging::{Log, LogLevel};

use super::operations::Registry;

/// One command row: the names it answers to, the operation it runs, and what it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// What a player types, without the slash (`GmCommands.c` and `GmChat.c` take lowercase names).
    pub command: String,
    /// The other names the same command answers to: `GmCommands.c` accepts two names for one command.
    pub also: Vec<String>,
    /// The registered operation it names (`game/src/Chat/Operations.rs`).
    pub operation: String,
    /// The values it takes, in order. Empty means it takes none.
    pub parameters: Vec<String>,
    /// The surfaces that may run it.
    pub surfaces: Vec<Surface>,
}

/// A surface a command may be typed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The in-game chat line.
    Chat,
    /// The embedded console, the local control endpoint and the standalone CLI.
    Console,
}

impl Surface {
    /// The name a row writes for this surface.
    pub fn name(self) -> &'static str {
        match self {
            Surface::Chat => "chat",
            Surface::Console => "console",
        }
    }

    /// The surface a row's name writes, if it writes one this tree has.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "chat" => Some(Surface::Chat),
            "console" => Some(Surface::Console),
            _ => None,
        }
    }
}

impl Command {
    /// Every name this command answers to, its own first.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.command.as_str()).chain(self.also.iter().map(String::as_str))
    }

    /// Whether it takes an argument.
    pub fn takes_argument(&self) -> bool {
        !self.parameters.is_empty()
    }

    /// Whether this row may be run at a surface.
    pub fn serves(&self, surface: Surface) -> bool {
        self.surfaces.contains(&surface)
    }
}

/// Every command a world answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    rows: Vec<Command>,
}

impl Table {
    /// An empty table: the state of a world started without a command file, which answers no command
    /// and says so once, at start-up.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Read the rows.
    ///
    /// A file that is missing is reported and leaves the table empty, the way the creation tables are
    /// handled: the program starts and the log says what it does not have. A file that is *present* but
    /// wrong is an error - a row without a name, a name stated twice, a parameter list the parser cannot
    /// honour, or an operation this server does not have - because a command that cannot work is worse
    /// than one that does not exist.
    pub fn load(
        path: impl AsRef<std::path::Path>,
        log: &Log,
        registry: &Registry,
    ) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            log.out(
                LogLevel::Warn,
                "server.worldserver",
                format!(
                    "No chat command table at {}: no chat command will be answered.",
                    path.display()
                ),
            );
            return Ok(Table::empty());
        }
        let document = Document::read(path).map_err(|error| error.to_string())?;
        let mut table = Table::empty();
        for record in document.records("commands")? {
            let command = record.required("command")?;
            let also = record.strings("also")?.unwrap_or_default();
            let operation = record.required("operation")?;
            let parameters = record.strings("parameters")?.unwrap_or_default();
            let surfaces = record.strings("surfaces")?.unwrap_or_default();

            if command.is_empty() || command.contains('/') {
                return Err(format!(
                    "{}: a command is a name without the slash, and this row's is {command:?}",
                    path.display()
                ));
            }
            if command != command.to_lowercase() {
                return Err(format!(
                    "{}: command {command:?} is not lower case; the client sends a command as typed and the reference's names are lower case",
                    path.display()
                ));
            }
            // The binding is the registry's: a row names an operation, and a name this world does not
            // register is a refused row rather than a command that answers "Unknown command" to a
            // player the file promised.
            let Some(registration) = registry.get(&operation) else {
                let known: Vec<&str> = registry.rows().map(|row| row.name).collect();
                return Err(format!(
                    "{}: command {command:?} runs {operation:?}, which is not an operation this world registers; the operations are {}",
                    path.display(),
                    known.join(", ")
                ));
            };
            if parameters.len() != registration.arguments.len() {
                return Err(format!(
                    "{}: command {command:?} states {} parameter(s) and {operation} declares {} ({}); a row must take what its operation takes",
                    path.display(),
                    parameters.len(),
                    registration.arguments.len(),
                    registration.arguments.join(", ")
                ));
            }
            let surfaces = if surfaces.is_empty() {
                // A row without a surface is a chat command, which is what the reference's own rows
                // are; a console row states its surface.
                vec![Surface::Chat]
            } else {
                let mut resolved = Vec::with_capacity(surfaces.len());
                for name in &surfaces {
                    let Some(surface) = Surface::from_name(name) else {
                        return Err(format!(
                            "{}: command {command:?} names surface {name:?}; the surfaces are chat, console",
                            path.display()
                        ));
                    };
                    resolved.push(surface);
                }
                resolved
            };
            if surfaces.contains(&Surface::Chat) && command.split_whitespace().count() != 1 {
                return Err(format!(
                    "{}: a chat command is one word, and this row's is {command:?}",
                    path.display()
                ));
            }
            if surfaces.contains(&Surface::Chat) && parameters.len() > 1 {
                return Err(format!(
                    "{}: command {command:?} is typed on a chat line, which is split on its first space, so it can take at most one parameter and this row states {}",
                    path.display(),
                    parameters.len()
                ));
            }
            table.rows.push(Command {
                command,
                also,
                operation,
                parameters,
                surfaces,
            });
        }

        // One name, one row: two rows answering `tp` would answer it twice, and which one ran would be
        // the file's order rather than its meaning.
        for (index, row) in table.rows.iter().enumerate() {
            for name in row.names() {
                if let Some(other) = table
                    .rows
                    .iter()
                    .skip(index + 1)
                    .find(|candidate| candidate.names().any(|other| other == name))
                {
                    return Err(format!(
                        "{}: {name:?} is stated by two rows ({:?} and {:?})",
                        path.display(),
                        row.command,
                        other.command
                    ));
                }
            }
        }

        Ok(table)
    }

    /// The row a player's name resolves to, if this world answers it.
    pub fn command(&self, name: &str) -> Option<&Command> {
        self.rows
            .iter()
            .find(|row| row.names().any(|candidate| candidate == name))
    }

    /// The rows, in the file's order.
    pub fn rows(&self) -> &[Command] {
        &self.rows
    }

    /// The chat row a typed line resolves to, and the values it carries.
    ///
    /// The reference splits a chat line on its first space: the name, then one argument if the row
    /// takes one. A name the file grants but with the wrong shape resolves to nothing, which is the
    /// reference's "Unknown command" rather than a partly-run command.
    pub fn resolve_chat(&self, line: &str) -> Option<(&Command, Vec<String>)> {
        let (name, argument) = match line.split_once(' ') {
            Some((name, rest)) => (name, rest.trim_start()),
            None => (line, ""),
        };
        let values: Vec<String> = if argument.is_empty() {
            Vec::new()
        } else {
            vec![argument.to_string()]
        };
        self.rows
            .iter()
            .filter(|row| row.serves(Surface::Chat))
            .find(|row| {
                row.parameters.len() == values.len()
                    && row.names().any(|candidate| candidate == name)
            })
            .map(|row| (row, values))
    }

    /// The console row a typed line resolves to, and the values it carries.
    ///
    /// A console row's name may be more than one word (`server info` is the reference's name for a
    /// console command), so the match is the longest name that prefixes the line. The values are what
    /// remains, split on whitespace; a name that matches with the wrong number of values resolves to
    /// nothing, which is the reference's "Unknown command" rather than a partly-run command.
    pub fn resolve_console(&self, line: &str) -> Option<(&Command, Vec<String>)> {
        let mut best: Option<(&Command, Vec<String>)> = None;
        for row in self.rows.iter().filter(|row| row.serves(Surface::Console)) {
            for name in row.names() {
                let rest = if line == name {
                    ""
                } else if let Some(rest) = line
                    .strip_prefix(name)
                    .and_then(|rest| rest.strip_prefix(' '))
                {
                    rest.trim()
                } else {
                    continue;
                };
                let values: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
                if values.len() != row.parameters.len() {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|(found, _)| found.command.len() < name.len())
                {
                    best = Some((row, values));
                }
            }
        }
        // One row per name is enforced at load, so a match here is unambiguous.
        best
    }

    /// How many commands this world answers.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether it answers none.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::operations::{Registry, register_chat};
    use common::configuration::json::Document;

    /// The registry a world builds: the chat operations plus the two a program registers itself.
    fn registry() -> Registry {
        let mut registry = Registry::new();
        register_chat(&mut registry).expect("the chat operations register");
        registry
            .register(
                crate::chat::operations::Registration {
                    name: "world.status",
                    help: "what this world is running",
                    arguments: &[],
                },
                crate::chat::operations::world_status,
            )
            .expect("the status operation registers");
        registry
            .register(
                crate::chat::operations::Registration {
                    name: "world.exit",
                    help: "stop this world",
                    arguments: &[],
                },
                crate::chat::operations::world_status,
            )
            .expect("the exit operation registers");
        registry
            .register(
                crate::chat::operations::Registration {
                    name: "world.restart",
                    help: "stop this world and exit 2",
                    arguments: &[],
                },
                crate::chat::operations::world_status,
            )
            .expect("the restart operation registers");
        registry
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("kryta-commands-{name}.json"));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn load(text: &str, name: &str, registry: &Registry) -> Result<Table, String> {
        let path = scratch(name);
        std::fs::write(&path, text).unwrap();
        Table::load(&path, &Log::new(), registry)
    }

    /// The file this tree ships, read as a world reads it: one binding style, every row naming an
    /// operation the registry has.
    #[test]
    fn the_shipped_table_binds_its_rows_to_registered_operations() {
        let document =
            Document::read("../config/Server/commands.json").expect("the shipped file parses");
        let records = document
            .records("commands")
            .expect("commands is a record list");
        assert_eq!(records.len(), 10);

        let registry = registry();
        let table = Table::load("../config/Server/commands.json", &Log::new(), &registry)
            .expect("the shipped table reads");
        assert_eq!(table.len(), 10);

        let tp = table.command("tp").expect("/tp is answered");
        assert_eq!(tp.operation, "chat.transfer");
        assert!(tp.takes_argument());
        assert!(tp.serves(Surface::Chat));
        assert!(
            !tp.serves(Surface::Console),
            "a chat row is not a console row"
        );
        assert_eq!(
            table.command("teleport").map(|row| row.operation.as_str()),
            Some("chat.transfer"),
            "the reference accepts two names for one command"
        );
        assert_eq!(table.command("maps").unwrap().operation, "chat.maps");
        assert_eq!(table.command("stuck").unwrap().operation, "chat.stuck");
        assert_eq!(table.command("guild").unwrap().operation, "chat.guild");
        assert_eq!(table.command(".exp").unwrap().operation, "chat.experience");
        assert_eq!(
            table.command("xp").unwrap().operation,
            "chat.experience_read"
        );
        assert!(table.command("say").is_none());

        // The console rows: the reference's own console names, bound the same way.
        for (name, operation) in [
            ("server info", "world.status"),
            ("server exit", "world.exit"),
            ("server restart", "world.restart"),
        ] {
            let row = table.command(name).expect("the console answers it");
            assert_eq!(row.operation, operation);
            assert!(!row.takes_argument());
            assert!(row.serves(Surface::Console));
            assert!(
                !row.serves(Surface::Chat),
                "a console row is not a chat row"
            );
            let (resolved, values) = table
                .resolve_console(name)
                .expect("the console resolves it");
            assert_eq!(resolved.command, name);
            assert!(values.is_empty());
        }

        // The two resolvers keep the surfaces apart.
        assert!(
            table.resolve_console("server info now").is_none(),
            "it takes no values"
        );
        assert!(
            table.resolve_console("tp ascalon").is_none(),
            "a chat row is not console-visible"
        );
        assert!(
            table.resolve_chat("server info").is_none(),
            "a console row is not chat-visible"
        );
        let (row, values) = table
            .resolve_chat("tp ascalon city")
            .expect("a chat row resolves");
        assert_eq!(row.command, "tp");
        assert_eq!(
            values,
            vec!["ascalon city".to_string()],
            "the line after the name is one value"
        );
        assert!(
            table.resolve_chat("tp").is_none(),
            "a row that takes a value needs one"
        );
        assert!(
            table.resolve_chat("maps extra").is_none(),
            "a row that takes none refuses one"
        );
    }

    /// A name the registry does not have is refused when the file is read, and the refusal lists what
    /// this world does register.
    #[test]
    fn a_row_naming_an_unregistered_operation_is_refused() {
        let registry = registry();
        let error = load(
            r#"{"commands":[{"command":"warp","operation":"chat.warp"}]}"#,
            "unknown-operation",
            &registry,
        )
        .unwrap_err();
        assert!(error.contains("chat.warp"), "{error}");
        assert!(
            error.contains("chat.transfer"),
            "the refusal lists what exists: {error}"
        );
    }

    /// A row must take what its operation takes: the count is the registration's, not the file's.
    #[test]
    fn a_parameter_list_the_operation_does_not_declare_is_refused() {
        let registry = registry();
        let error = load(
            r#"{"commands":[{"command":"warp","operation":"chat.maps","parameters":["map"]}]}"#,
            "parameter-mismatch",
            &registry,
        )
        .unwrap_err();
        assert!(error.contains("chat.maps"), "{error}");
        assert!(error.contains("declares 0"), "{error}");
    }

    /// A surface this tree does not have is refused, and a value the chat line cannot carry too.
    #[test]
    fn a_surface_and_a_chat_rows_second_parameter_are_refused() {
        let world_ops = registry();
        assert!(
            load(
                r#"{"commands":[{"command":"x","operation":"chat.maps","surfaces":["telnet"]}]}"#,
                "surface",
                &world_ops,
            )
            .is_err()
        );
        // A row must take what its operation declares: `chat.maps` takes none, so two values are
        // refused by the count before the line-splitting rule could be reached.
        let error = load(
            r#"{"commands":[{"command":"warp","operation":"chat.maps","parameters":["a","b"]}]}"#,
            "two-parameters",
            &world_ops,
        )
        .unwrap_err();
        assert!(error.contains("declares 0"), "{error}");

        // The line-splitting rule itself, for an operation that does declare two values: a chat line
        // is split on its first space and can carry only one, so such a row cannot be a chat row.
        let mut wide = registry();
        wide.register(
            crate::chat::operations::Registration {
                name: "test.two",
                help: "two values",
                arguments: &["a", "b"],
            },
            crate::chat::operations::world_status,
        )
        .unwrap();
        let error = load(
            r#"{"commands":[{"command":"wide","operation":"test.two","parameters":["a","b"]}]}"#,
            "first-space",
            &wide,
        )
        .unwrap_err();
        assert!(error.contains("first space"), "{error}");
    }

    /// Two rows answering one name would answer it twice.
    #[test]
    fn a_name_stated_twice_is_refused() {
        let registry = registry();
        let error = load(
            r#"{"commands":[{"command":"tp","operation":"chat.transfer","parameters":["destination"]},{"command":"go","also":["tp"],"operation":"chat.maps"}]}"#,
            "duplicate-name",
            &registry,
        )
        .unwrap_err();
        assert!(error.contains("stated by two rows"), "{error}");
    }

    /// A missing file is not an error: the world starts, answers no command, and says so.
    #[test]
    fn a_missing_file_leaves_the_table_empty() {
        let table = Table::load(
            "../data/no-such-command-file.json",
            &Log::new(),
            &registry(),
        )
        .unwrap();
        assert!(table.is_empty());
    }

    /// A console row may be more than one word, and the longest name wins.
    #[test]
    fn the_console_resolves_the_longest_name_that_prefixes_the_line() {
        let registry = registry();
        let table = load(
            r#"{"commands":[
                {"command":"server info","operation":"world.status","surfaces":["console"]},
                {"command":"server","operation":"world.status","surfaces":["console"]}
            ]}"#,
            "longest-name",
            &registry,
        )
        .unwrap();
        assert_eq!(
            table.resolve_console("server info").unwrap().0.command,
            "server info"
        );
        assert_eq!(table.resolve_console("server").unwrap().0.command, "server");
    }
}
