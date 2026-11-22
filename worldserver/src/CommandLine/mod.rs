//! Console input queues commands for the world update.
//!
//! Reference: `src/server/apps/worldserver/CommandLine/CliRunnable.cpp:110-231` (the thread reads a
//! line and queues it), `src/server/game/Chat/Chat.cpp:909` (`CliHandler::ParseCommands`) and
//! `src/server/game/World/World.cpp:1721-1736` (the world update runs the queue) at 30a36ec4d.
//!
//! **The console names no command.** What an operator may type is `config/Server/commands.json` read as rows
//! (`game::chat::commands`), and what a row runs is a registered operation
//! (`game::chat::operations`). This module turns a typed line into a call and prints the answer; it
//! holds no command and no operation of its own.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

use game::chat::commands::{Surface, Table};
use game::chat::operations::{Arguments, Caller, Invocation, Refusal, Registry, UNKNOWN_OPERATION};

/// A call waiting for the world update, and where its answer goes.
pub struct Request {
    /// The call: the operation, its values, and that the console asked.
    pub invocation: Invocation,
    /// The world's answer, or the refusal.
    pub reply: Sender<Result<String, Refusal>>,
}

/// Run pending calls from the world update thread.
pub fn process(receiver: &Receiver<Request>, registry: &Registry, world: &game::world::World) {
    while let Ok(request) = receiver.try_recv() {
        let answer = registry.invoke_text(&request.invocation, world);
        let _ = request.reply.send(answer);
    }
}

/// Start only for an interactive terminal, leaving supervised input to its owner.
pub fn start(sender: SyncSender<Request>, table: Arc<Table>, registry: Arc<Registry>) -> bool {
    if !std::io::stdin().is_terminal() {
        return false;
    }
    std::thread::Builder::new()
        .name("world-console".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            loop {
                print!("Kryta> ");
                let _ = std::io::stdout().flush();
                let mut line = String::new();
                if !matches!(input.read_line(&mut line), Ok(n) if n > 0) {
                    break;
                }
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "help" {
                    println!("{}", help(&table, &registry));
                    continue;
                }
                let invocation = match parse(&table, &registry, line) {
                    Ok(invocation) => invocation,
                    Err(refusal) => {
                        println!("{}", refusal.message);
                        continue;
                    }
                };
                let (reply, answer) = mpsc::channel();
                if sender.send(Request { invocation, reply }).is_err() {
                    break;
                }
                match answer.recv() {
                    Ok(Ok(text)) => println!("{text}"),
                    Ok(Err(refusal)) => println!("{}", refusal.message),
                    Err(_) => break,
                }
            }
        })
        .is_ok()
}

/// Resolve a typed line to a call, without running it.
///
/// `help` and the empty line are the console's own answers, not commands, so they are handled by the
/// caller. A line no row grants is `Unknown command`, the reference's own answer for a name it does
/// not know (`GmChat.c:130-140`).
pub fn parse(table: &Table, registry: &Registry, line: &str) -> Result<Invocation, Refusal> {
    let line = line.trim();
    let Some((row, values)) = table.resolve_console(line) else {
        return Err(Refusal::new(UNKNOWN_OPERATION, "Unknown command"));
    };
    registry.invocation(&row.operation, Arguments::of(values), Caller::console())
}

/// What an operator may type at the console, and what each one does.
pub fn help(table: &Table, registry: &Registry) -> String {
    let mut lines = vec!["help - what this console answers".to_string()];
    for row in table
        .rows()
        .iter()
        .filter(|row| row.serves(Surface::Console))
    {
        let Some(registration) = registry.get(&row.operation) else {
            continue;
        };
        lines.push(format!(
            "{} - {} [{}]",
            row.names().collect::<Vec<_>>().join(", "),
            registration.help,
            row.operation
        ));
    }
    lines.join("\n")
}

/// Run one line synchronously, for a caller that has the world in hand (the tests, and the panel's
/// own checks): parse, invoke, and answer the text a person reads.
pub fn execute(
    table: &Table,
    registry: &Registry,
    world: &game::world::World,
    line: &str,
) -> String {
    let line = line.trim();
    if line.is_empty() {
        return String::new();
    }
    if line == "help" {
        return help(table, registry);
    }
    match parse(table, registry, line)
        .and_then(|invocation| registry.invoke_text(&invocation, world))
    {
        Ok(answer) => answer,
        Err(refusal) => refusal.message,
    }
}

/// The operations this world registers, one a line: what `kryta-cli list` asks for.
///
/// The registry is the world's, so the answer cannot disagree with what the world would accept - the
/// list is not a second table kept beside it.
pub fn list_operations(
    _invocation: &Invocation,
    world: &game::world::World,
    _agent: Option<&game::agents::Agent>,
) -> Result<game::chat::operations::Outcome, Refusal> {
    let mut rows: Vec<String> = world
        .operations()
        .rows()
        .map(|row| {
            let values = if row.arguments.is_empty() {
                "no values".to_string()
            } else {
                row.arguments.join(", ")
            };
            format!("{:<18} {:<44} {values}", row.name, row.help)
        })
        .collect();
    rows.sort();
    Ok(game::chat::operations::Outcome::text(rows.join("\n")))
}

/// Ask the world to stop normally: the reference's `server exit` (`cs_server.cpp:89`,
/// `HandleServerExitCommand`), which stops the world and leaves the exit code `0`.
pub fn world_exit(
    _invocation: &Invocation,
    world: &game::world::World,
    _agent: Option<&game::agents::Agent>,
) -> Result<game::chat::operations::Outcome, Refusal> {
    world.request_stop(game::world::ExitCode::Normal);
    Ok(game::chat::operations::Outcome::text("Stopping the world."))
}

/// Ask the world to stop so that a supervisor starts it again: the reference's `server restart`
/// (`cs_server.cpp:68`), whose exit code `2` its own `main` documents for "restarter"
/// (`Main.cpp:434-439`). The world does not restart itself; the supervisor reads the code.
pub fn world_restart(
    _invocation: &Invocation,
    world: &game::world::World,
    _agent: Option<&game::agents::Agent>,
) -> Result<game::chat::operations::Outcome, Refusal> {
    world.request_stop(game::world::ExitCode::Restart);
    Ok(game::chat::operations::Outcome::text(
        "Restarting: this process stops and the supervisor starts it again.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry a world builds: the chat operations the file's rows name, plus the world's own.
    fn registry() -> Registry {
        let mut registry = Registry::new();
        game::chat::operations::register_chat(&mut registry).unwrap();
        registry
            .register(
                game::chat::operations::Registration {
                    name: "world.status",
                    help: "what this world is running",
                    arguments: &[],
                },
                game::chat::operations::world_status,
            )
            .unwrap();
        registry
            .register(
                game::chat::operations::Registration {
                    name: "ListTargets",
                    help: "the alias table",
                    arguments: &[],
                },
                game::chat::operations::list_targets,
            )
            .unwrap();
        registry
            .register(
                game::chat::operations::Registration {
                    name: "world.exit",
                    help: "stop this world",
                    arguments: &[],
                },
                world_exit,
            )
            .unwrap();
        registry
            .register(
                game::chat::operations::Registration {
                    name: "world.restart",
                    help: "stop this world and exit 2",
                    arguments: &[],
                },
                world_restart,
            )
            .unwrap();
        registry
    }

    fn table() -> Table {
        Table::load(
            "../config/Server/commands.json",
            &common::logging::Log::new(),
            &registry(),
        )
        .unwrap()
    }

    #[test]
    fn the_console_answers_the_row_the_file_grants() {
        let world = game::world::World::new(7);
        assert_eq!(
            execute(&table(), &registry(), &world, "server info"),
            "World 7: 0 map(s) loaded."
        );
    }

    #[test]
    fn a_name_no_row_grants_is_the_references_unknown_command() {
        let world = game::world::World::new(7);
        assert_eq!(
            execute(&table(), &registry(), &world, "nope"),
            "Unknown command"
        );
        // A registered operation with no row is not reachable: rows grant names, not the registry.
        assert_eq!(
            execute(&table(), &registry(), &world, "@world.status"),
            "Unknown command"
        );
    }

    #[test]
    fn a_chat_row_is_not_console_visible() {
        let error = parse(&table(), &registry(), "tp ascalon").unwrap_err();
        assert_eq!(error.code, UNKNOWN_OPERATION);
        assert_eq!(error.message, "Unknown command");
    }

    #[test]
    fn a_row_takes_exactly_the_values_it_declares() {
        let table = table();
        let registry = registry();
        assert!(parse(&table, &registry, "server info now").is_err());
        let invocation = parse(&table, &registry, "server info").unwrap();
        assert_eq!(invocation.operation, "world.status");
        assert!(invocation.arguments.is_empty());
        assert_eq!(
            invocation.caller.kind,
            game::chat::operations::CallerKind::Console
        );
    }

    #[test]
    fn help_lists_the_console_rows_and_nothing_else() {
        let text = help(&table(), &registry());
        assert!(text.contains("server info"), "{text}");
        assert!(text.contains("world.status"), "{text}");
        assert!(
            !text.contains("maps"),
            "a chat row is not a console command: {text}"
        );
        assert!(!text.contains("stuck"), "{text}");
    }

    #[test]
    fn a_queued_call_is_run_on_the_world_thread_and_answers_the_caller() {
        let world = game::world::World::new(7);
        let registry = registry();
        let invocation = registry
            .invocation("world.status", Arguments::none(), Caller::console())
            .unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        let (reply, answer) = mpsc::channel();
        sender.send(Request { invocation, reply }).unwrap();
        assert!(answer.try_recv().is_err());
        process(&receiver, &registry, &world);
        assert_eq!(answer.recv().unwrap().unwrap(), "World 7: 0 map(s) loaded.");
    }
}
