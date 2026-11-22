//! Registered operations shared by operator surfaces.
//!
//! Reference: `src/server/game/Chat/ChatCommands/ChatCommand.h` - a command is a name, a handler, a
//! permission and whether a surface may run it - and `ChatCommand.cpp:513-523`, which decides what a
//! surface may ask for. Execution order is `src/server/game/World/World.cpp:1721-1736` at 30a36ec4d:
//! the request is queued and the world update runs it.
//!
//! **One operation is one registered function, called with what the caller supplies.** A caller does
//! not reach a function through a dispatcher arm: it names the operation, supplies the arguments the
//! registration declares and the caller it is, and receives the operation's answer - text, frames, or
//! a transfer to start - or a refusal with a code. Adding an operation is a registration; adding a
//! command that runs an existing operation is a row in `config/Server/commands.json`; neither edits a door
//! (`docs/port/14-operator-surfaces.md`).
//!
//! Every command this tree answers - the chat commands included - is a row in that file naming one of
//! these registrations. There is no enum of commands and no match on one.

use std::collections::BTreeMap;

/// Who is asking. The adapter supplies this; a caller never names itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The kind of surface the request arrived through.
    pub kind: CallerKind,
    /// The identity that surface vouched for; empty when it has none, as the local console has none.
    pub name: String,
}

/// The surfaces a request may arrive through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerKind {
    /// The embedded console: standard input of the world program.
    Console,
    /// The local control endpoint, authenticated by the service credential.
    Api,
    /// An in-game chat line, once a player command calls an operation.
    Chat,
    /// A script, once the Lua runtime exists.
    Lua,
}

impl Caller {
    /// The local console, which is the machine the world runs on.
    pub fn console() -> Self {
        Self {
            kind: CallerKind::Console,
            name: String::new(),
        }
    }

    /// A caller vouched for by the local control endpoint.
    pub fn api(name: &str) -> Self {
        Self {
            kind: CallerKind::Api,
            name: name.to_string(),
        }
    }

    /// A player's chat line: the identity is the player tag the line arrived with.
    pub fn chat(tag: u16) -> Self {
        Self {
            kind: CallerKind::Chat,
            name: tag.to_string(),
        }
    }

    /// The kind's name, for a result that says who asked.
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            CallerKind::Console => "console",
            CallerKind::Api => "api",
            CallerKind::Chat => "chat",
            CallerKind::Lua => "lua",
        }
    }
}

/// The values an operation was called with, in the order its registration declares them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arguments {
    values: Vec<String>,
}

impl Arguments {
    /// No values: what an operation that declares none is called with.
    pub fn none() -> Self {
        Self::default()
    }

    /// Values in declaration order.
    pub fn of(values: Vec<String>) -> Self {
        Self { values }
    }

    /// How many values were supplied.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether none were supplied.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// One value by position.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.values.get(index).map(String::as_str)
    }

    /// Every value, in declaration order.
    pub fn values(&self) -> &[String] {
        &self.values
    }
}

/// Why an operation refused. The code is what a machine reads; the message is what a person reads.
///
/// Reference: the reference renders refusals as text through the handler (`SendErrorMessage`,
/// `Chat.cpp:167`), so the text is the surface's, and the code is this tree's way of telling a machine
/// which refusal it was without parsing that text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// A stable name for the refusal: what a caller branches on.
    pub code: &'static str,
    /// What a person reads. A surface may render it verbatim, and the reference's wording is kept
    /// where the reference has one.
    pub message: String,
}

/// The refusal for a name no surface grants.
pub const UNKNOWN_OPERATION: &str = "unknown_operation";
/// The refusal for a call whose arguments do not match its registration.
pub const INVALID_ARGUMENTS: &str = "invalid_arguments";
/// A refusal the operation itself decides on.
pub const REFUSED: &str = "refused";

impl Refusal {
    /// One refusal, by code and message.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// What an operation answered: what the surface is to do about it.
///
/// An operation does not send anything itself and does not know which door called it. It says what
/// the answer is, and the door performs it in its own medium - frames on chat, text on the console,
/// JSON over the control endpoint - which is why one implementation serves every surface without a
/// branch per surface anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Text: `wire` is what a client is told, `log` is what the operator's log gets.
    Text {
        /// The lines a client is told.
        wire: Vec<String>,
        /// The lines the log gets.
        log: Vec<String>,
    },
    /// Whole frames the surface sends as they are.
    Frames(Vec<Vec<u8>>),
    /// A transfer the surface starts with its own machinery (`begin_transfer`).
    Transfer {
        /// The destination the caller asked for, as it was typed.
        argument: String,
    },
    /// `/resign`: the session marks the member and schedules its own death.
    Resign,
    /// `/guild <verb>`: the session answers it with its own guild state
    /// (`ASURA/crates/asura-world/src/guild.rs:254-294`, `handle_guild_command`).
    Guild {
        /// The verb, as it was typed.
        argument: String,
    },
    /// `/.exp <total>` sets the character's experience; `/xp` alone reads it
    /// (`ASURA/crates/asura-world/src/chat.rs:612-648`). The session holds the character's progress.
    Experience {
        /// The total typed, or `None` for the reading form.
        argument: Option<String>,
    },
}

impl Outcome {
    /// One line for a surface that only renders text: `wire` joined, log lines dropped.
    pub fn text(line: impl Into<String>) -> Self {
        Self::Text {
            wire: vec![line.into()],
            log: Vec::new(),
        }
    }

    /// Text with both halves.
    pub fn logged(wire: Vec<String>, log: Vec<String>) -> Self {
        Self::Text { wire, log }
    }

    /// What a surface that cannot send frames or start a transfer may render.
    ///
    /// An operation that answers with frames or a transfer is one a session must perform; a console
    /// or an API call is refused rather than answered with nothing (`REFUSED`).
    pub fn into_text(self) -> Result<String, Refusal> {
        match self {
            Outcome::Text { wire, .. } => Ok(wire.join("\n")),
            Outcome::Frames(_)
            | Outcome::Transfer { .. }
            | Outcome::Resign
            | Outcome::Guild { .. }
            | Outcome::Experience { .. } => Err(Refusal::new(
                REFUSED,
                "this operation needs a player session: it answers with frames a client performs",
            )),
        }
    }
}

/// One call: what is being asked, with what, by whom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The registered operation's name.
    pub operation: String,
    /// The values it was called with, in its own declaration order.
    pub arguments: Arguments,
    /// Who is asking.
    pub caller: Caller,
}

/// What an operation is: the facts a door needs before it calls the function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registration {
    /// The stable name a row or an endpoint names.
    pub name: &'static str,
    /// What the operation does, in one line, for help and for a manager page.
    pub help: &'static str,
    /// The values the operation takes, in order. Empty means it takes none.
    pub arguments: &'static [&'static str],
}

/// What an operation does: one function of the call, the world, and the caller's own agent if it has one.
pub type Handler = fn(
    &Invocation,
    &crate::world::World,
    Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal>;

struct Entry {
    registration: Registration,
    handler: Handler,
}

/// The operations a world has registered.
///
/// A registry, not a dispatcher: a name that is not registered is refused with `unknown_operation`,
/// and there is no branch a new operation could be spliced into.
#[derive(Default)]
pub struct Registry {
    entries: BTreeMap<&'static str, Entry>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one implementation, refusing a duplicate or empty name.
    pub fn register(&mut self, registration: Registration, handler: Handler) -> Result<(), String> {
        if registration.name.trim().is_empty() {
            return Err("an operation needs a name".into());
        }
        if self.entries.contains_key(registration.name) {
            return Err(format!(
                "operation {:?} is registered twice",
                registration.name
            ));
        }
        self.entries.insert(
            registration.name,
            Entry {
                registration,
                handler,
            },
        );
        Ok(())
    }

    /// What an operation is, if it is registered.
    pub fn get(&self, name: &str) -> Option<Registration> {
        self.entries.get(name).map(|entry| entry.registration)
    }

    /// Whether an operation is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Every registration, by name.
    pub fn rows(&self) -> impl Iterator<Item = Registration> + '_ {
        self.entries.values().map(|entry| entry.registration)
    }

    /// Build a call, checking that the operation exists and that the values match its declaration.
    ///
    /// This is where "unknown operation" and "invalid arguments" are decided, before any surface's
    /// rendering and before the world is touched.
    pub fn invocation(
        &self,
        name: &str,
        arguments: Arguments,
        caller: Caller,
    ) -> Result<Invocation, Refusal> {
        let Some(registration) = self.get(name) else {
            return Err(Refusal::new(
                UNKNOWN_OPERATION,
                format!("unknown operation: {name}"),
            ));
        };
        if arguments.len() != registration.arguments.len() {
            return Err(Refusal::new(
                INVALID_ARGUMENTS,
                format!(
                    "operation {name} takes {} argument(s) ({}), and {} were supplied",
                    registration.arguments.len(),
                    if registration.arguments.is_empty() {
                        "none".to_string()
                    } else {
                        registration.arguments.join(", ")
                    },
                    arguments.len()
                ),
            ));
        }
        Ok(Invocation {
            operation: name.to_string(),
            arguments,
            caller,
        })
    }

    /// Run a call against a world. Called on the world's own thread, never on a listener's.
    pub fn invoke(
        &self,
        invocation: &Invocation,
        world: &crate::world::World,
        agent: Option<&crate::agents::Agent>,
    ) -> Result<Outcome, Refusal> {
        let Some(entry) = self.entries.get(invocation.operation.as_str()) else {
            return Err(Refusal::new(
                UNKNOWN_OPERATION,
                format!("unknown operation: {}", invocation.operation),
            ));
        };
        (entry.handler)(invocation, world, agent)
    }

    /// Run a call for a surface that renders text only, refusing what it cannot perform.
    pub fn invoke_text(
        &self,
        invocation: &Invocation,
        world: &crate::world::World,
    ) -> Result<String, Refusal> {
        self.invoke(invocation, world, None)?.into_text()
    }
}

/// Existing world information, presented to an operator.
pub fn world_status(
    _invocation: &Invocation,
    world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::text(format!(
        "World {}: {} map(s) loaded.",
        world.realm(),
        world.maps().len()
    )))
}

/// Build both outputs of the existing map-list operation.
///
/// ASURA: `crates/asura-world/src/chat.rs:41-102,288-324`.
pub fn map_targets(maps: &[crate::world::MapEntry]) -> crate::chat::MappedMaps {
    crate::chat::maps_answer(maps)
}

/// The alias table, presented to an operator: the table the chat surface only logs.
pub fn list_targets(
    _invocation: &Invocation,
    world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::text(map_targets(world.maps()).table.join("\n")))
}

/// `/maps`: the alias table, the chat half sent and the table logged - the reference's two outputs.
pub fn chat_maps(
    _invocation: &Invocation,
    world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    let answer = map_targets(world.maps());
    Ok(Outcome::logged(answer.chat, answer.table))
}

/// `/tp`: a transfer the session starts with its own machinery (`begin_transfer`).
pub fn chat_transfer(
    invocation: &Invocation,
    _world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::Transfer {
        argument: invocation.arguments.get(0).unwrap_or_default().to_string(),
    })
}

/// `/stuck`: where this agent stands now, as the frame that says so.
///
/// **Not** `AGENT_STOP_MOVING` - that is what `0x8028` answers - and the working implementation sends
/// the position alone (`ASURA/crates/asura-world/src/chat.rs:260-287`); the tick it also broadcasts is
/// the ticker's own, so no second clock is invented.
pub fn chat_stuck(
    invocation: &Invocation,
    _world: &crate::world::World,
    agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    let Some(agent) = agent else {
        return Err(Refusal::new(
            REFUSED,
            format!(
                "{} needs a player session: it answers with the position of the agent that asked",
                invocation.operation
            ),
        ));
    };
    let frame = crate::server::packets::agent_packets::agent_update_position(
        agent.agent_id,
        agent.position,
        agent.plane,
    )
    .1;
    Ok(Outcome::Frames(vec![frame]))
}

/// `/resign`: the session performs it (mark, delay, death, defeat).
pub fn chat_resign(
    _invocation: &Invocation,
    _world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::Resign)
}

/// `/guild <verb>`: the session performs it (`ASURA/crates/asura-world/src/chat.rs:536-540`). Not the
/// reference's: ASURA's owner's stand-in for the Guild Registrar (2026-11-06).
pub fn chat_guild(
    invocation: &Invocation,
    _world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::Guild {
        argument: invocation.arguments.get(0).unwrap_or_default().to_string(),
    })
}

/// `/.exp <total>`: preforged's development command, as ASURA has it (`chat.rs:612-621`): set the total
/// and let the curve answer the level.
pub fn chat_experience(
    invocation: &Invocation,
    _world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::Experience {
        argument: Some(invocation.arguments.get(0).unwrap_or_default().to_string()),
    })
}

/// `/xp`: preforged's reading command, as ASURA has it (`chat.rs:622-631`): the total and the level.
pub fn chat_experience_read(
    _invocation: &Invocation,
    _world: &crate::world::World,
    _agent: Option<&crate::agents::Agent>,
) -> Result<Outcome, Refusal> {
    Ok(Outcome::Experience { argument: None })
}

/// Register the operations the chat surface's own rows name (`config/Server/commands.json`).
///
/// They live here, beside the world's own, so a program names them once and every surface - chat,
/// console, API, and Lua when it arrives - reaches the same function.
pub fn register_chat(registry: &mut Registry) -> Result<(), String> {
    registry.register(
        Registration {
            name: "chat.transfer",
            help: "a server transfer to the map the argument names",
            arguments: &["destination"],
        },
        chat_transfer,
    )?;
    registry.register(
        Registration {
            name: "chat.maps",
            help: "the alias table: sent to the client, logged for the operator",
            arguments: &[],
        },
        chat_maps,
    )?;
    registry.register(
        Registration {
            name: "chat.stuck",
            help: "broadcast where the asking agent stands",
            arguments: &[],
        },
        chat_stuck,
    )?;
    registry.register(
        Registration {
            name: "chat.resign",
            help: "resign from the party: die after the pack's delay, then the party is defeated",
            arguments: &[],
        },
        chat_resign,
    )?;
    registry.register(
        Registration {
            name: "chat.guild",
            help: "`/guild create` opens the Guild Registration window (0x00C4, 0x0084, 0x00CA, 0x00C3 {8, 0})",
            arguments: &["verb"],
        },
        chat_guild,
    )?;
    registry.register(
        Registration {
            name: "chat.experience",
            help: "set total experience; the level follows the pack's curve, and the bar moves by 0x00EE or the progress record 0x00E9 is restated",
            arguments: &["total"],
        },
        chat_experience,
    )?;
    registry.register(
        Registration {
            name: "chat.experience_read",
            help: "a server line with the character's total experience and level",
            arguments: &[],
        },
        chat_experience_read,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::Agent;

    fn registry() -> Registry {
        let mut registry = Registry::new();
        registry
            .register(
                Registration {
                    name: "world.status",
                    help: "what this world is running",
                    arguments: &[],
                },
                world_status,
            )
            .unwrap();
        registry
    }

    fn agent() -> Agent {
        Agent {
            agent_id: 7,
            position: (10.0, 20.0),
            plane: 0,
            facing: (1.0, 0.0),
            last_report: None,
            speed_base: 288.0,
            motion: Default::default(),
            move_type: 0,
            move_speed: 0.0,
            move_direction: (1.0, 0.0),
        }
    }

    #[test]
    fn registration_refuses_a_duplicate_or_empty_name() {
        let mut registry = Registry::new();
        let registration = Registration {
            name: "world.status",
            help: "",
            arguments: &[],
        };
        registry.register(registration, world_status).unwrap();
        assert!(
            registry.register(registration, world_status).is_err(),
            "a name is registered once"
        );
        let unnamed = Registration {
            name: "  ",
            help: "",
            arguments: &[],
        };
        assert!(
            Registry::new().register(unnamed, world_status).is_err(),
            "a name is required"
        );
    }

    #[test]
    fn an_unregistered_name_is_refused_by_code_not_by_parsing_text() {
        let registry = registry();
        let refusal = registry
            .invocation("nope", Arguments::none(), Caller::console())
            .unwrap_err();
        assert_eq!(refusal.code, UNKNOWN_OPERATION);
        assert_eq!(refusal.message, "unknown operation: nope");
    }

    #[test]
    fn the_argument_count_is_checked_against_the_registration() {
        let registry = registry();
        let refusal = registry
            .invocation(
                "world.status",
                Arguments::of(vec!["/".into()]),
                Caller::console(),
            )
            .unwrap_err();
        assert_eq!(refusal.code, INVALID_ARGUMENTS);
        assert!(
            refusal.message.contains("takes 0 argument(s)"),
            "{}",
            refusal.message
        );
        assert!(
            registry
                .invocation("world.status", Arguments::none(), Caller::console())
                .is_ok()
        );
    }

    #[test]
    fn the_caller_reaches_the_operation_and_the_text_surface_renders_its_answer() {
        let registry = registry();
        let world = crate::world::World::new(7);
        let invocation = registry
            .invocation("world.status", Arguments::none(), Caller::api("manage"))
            .unwrap();
        assert_eq!(
            registry.invoke_text(&invocation, &world).unwrap(),
            "World 7: 0 map(s) loaded."
        );
    }

    #[test]
    fn the_chat_operations_are_registrations_and_answer_with_what_a_session_performs() {
        let mut registry = Registry::new();
        register_chat(&mut registry).unwrap();
        let world = crate::world::World::new(7);

        let transfer = registry
            .invocation(
                "chat.transfer",
                Arguments::of(vec!["ascalon".into()]),
                Caller::chat(1),
            )
            .unwrap();
        assert_eq!(
            registry.invoke(&transfer, &world, None).unwrap(),
            Outcome::Transfer {
                argument: "ascalon".into()
            }
        );

        let stuck = agent();
        let invocation = registry
            .invocation("chat.stuck", Arguments::none(), Caller::chat(1))
            .unwrap();
        match registry.invoke(&invocation, &world, Some(&stuck)).unwrap() {
            Outcome::Frames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(
                    &frames[0][..2],
                    &crate::server::packets::agent_packets::AGENT_UPDATE_POSITION.to_le_bytes(),
                    "the agent update position frame"
                );
            }
            other => panic!("stuck answers with a frame, not {other:?}"),
        }

        // A surface that cannot perform a frame is refused, not answered with nothing.
        let refusal = registry.invoke_text(&invocation, &world).unwrap_err();
        assert_eq!(refusal.code, REFUSED);
        assert!(
            refusal.message.contains("needs a player session"),
            "{}",
            refusal.message
        );
    }

    #[test]
    fn the_map_list_answers_with_the_chat_half_and_the_logged_table() {
        let mut registry = Registry::new();
        register_chat(&mut registry).unwrap();
        let world = crate::world::World::new(7);
        let invocation = registry
            .invocation("chat.maps", Arguments::none(), Caller::chat(1))
            .unwrap();
        match registry.invoke(&invocation, &world, None).unwrap() {
            Outcome::Text { wire, log } => {
                assert!(
                    !wire.is_empty(),
                    "an empty world still says how many aliases it has: {wire:?}"
                );
                assert!(!log.is_empty(), "the table is logged");
            }
            other => panic!("maps answers with text, not {other:?}"),
        }
    }
}
