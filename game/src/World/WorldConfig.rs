//! The world's numbers: every threshold an instance, a district or a spawn uses.
//!
//! Reference: the reference's `WorldConfig` - one typed object a program reads its `World.*` keys into,
//! with the game's value as the default and an accessor per key. This is that object for the numbers the
//! owner's rules name (`docs/game-rules.md`), and it is built the way rule 7 requires: a setting is read
//! from the program's file, never compiled in as a law.
//!
//! **R53 (owner):** every number and threshold stays configurable, with the game's value as the default.
//! So each field below carries the value the game uses, and a program that reads a file that states
//! another value uses that one - and says so in its log.

use common::configuration::Config;

/// The numbers a world runs by. One field per rule that names a number.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldConfig {
    /// How many players one district of an outpost holds before the next district is shown (R5). The game
    /// starts at 100; the allotment is tied to a machine's own efficiency, so it is an operator's number.
    pub district_allotment: u32,
    /// The common party cap (R15). Some game modes allow 12 and some instances 25; a map may state its own
    /// cap in its row, and this is the number that applies when it does not.
    pub party_size_cap: u32,
    /// How far around a spawn point a character is placed on entry (R13), in the game's own units.
    pub spawn_scatter: f32,
    /// Whether an instance keeps a clock at all (R33). The game ticks one; an operator may choose not to.
    pub instance_tick: bool,
    /// The number a ping reply carries.
    ///
    /// The working implementation reads it from its own interval file
    /// (`ASURA/config/Server/intervals.json`, `ping_value`: 20, whose note says it is GWPS's number and
    /// has not been verified against this client). It is also the dword the reference's own send log
    /// carried - `0D 00 14 00 00 00` - so 20 is the measured value and an operator may change it.
    pub ping_value: u32,
    /// The interval one connection's simulation is ticked at (R33), in milliseconds.
    ///
    /// The working implementation's own intervals file states **100** (`ASURA/config/Server/intervals.json`,
    /// `tick_interval_ms`: "measured from the live client ... a 0x001E every ~100 ms, median 81"), and its
    /// map clock wakes on that interval whether or not a client is talking
    /// (`ASURA/crates/asura-world/src/instance.rs:394-403`). An operator may run it faster or slower,
    /// which is why it is a setting and not a law.
    pub simulation_tick_ms: u32,
    /// How often the world checks its instance table for copies to dispose (R33, R40), in milliseconds.
    /// The game's own check is not measured; a second is fine-grained enough for a fifteen-minute hold
    /// and cheap enough to be an operator's number rather than a law.
    pub sweep_interval_ms: u32,
    /// The range at which an enemy is simulated (R34), in the game's own units. Outside it, an enemy is not
    /// ticked.
    pub compass_range: f32,
    /// How long an explorable is held for a reconnecting party after a **network** disconnect (R37-R40),
    /// in seconds. A party that leaves by any normal means disposes the instance at once, whatever this
    /// says.
    pub explorable_disconnect_hold: u32,
    /// Whether an outpost that offers a mission presents the "Enter Mission" button (R56).
    pub mission_button: bool,
    /// Seconds between a member's `/resign` and its death (ASURA `travel.json`, `resign_secs`).
    pub resign_secs: u32,
    /// Seconds a leader's party-travel countdown runs (ASURA `travel.json`, `countdown_secs`).
    pub countdown_secs: u32,
    /// A player's maximum health before levels, sent as agent int-property 42 at load.
    ///
    /// `ASURA/config/World/Vanilla/combat.json`, `health_base`: maximum health is this plus
    /// `health_per_level` times the character's level (`docs/52-skills.md`, "A level 1 character now
    /// shows 100 health"). The old fixed 400 was the reference's creation-path number.
    pub agent_health_base: u32,
    /// Maximum health each level adds (`combat.json`, `health_per_level`).
    pub agent_health_per_level: u32,
    /// The energy a player agent is created with, sent as agent int-property 41 at load.
    pub agent_energy: u32,
    /// The energy regeneration a player agent is created with, sent as agent float-property 43. Zero is
    /// what the reference's own capture shows for a fresh agent.
    pub agent_energy_regen: f32,
    /// How fast a player agent walks, sent in its create message and used by the simulation.
    pub agent_speed: f32,
    /// The packed appearance a character with no bits of its own is drawn with.
    ///
    /// An all-zero word is `Profession_None`, which the client cannot build a model from, so the world
    /// states the fallback: profession 1 (Warrior) and hair style 3, which is `0x03100000`
    /// (`ASURA/config/World/Vanilla/character_defaults.json`, `appearance`).
    pub appearance: u32,
    /// How far a client may place itself when it cancels a walk and still be believed, in game units.
    ///
    /// The reference's `MAXIMUM_ALLOWED_CORRECTION` (`GmAgent.c:547-563`), through the working
    /// implementation's own setting (`ASURA/config/Server/movement.json`, `max_allowed_correction`). A
    /// correction farther than this from where the server thought the agent stood is ignored: the agent
    /// stops where the server believes it is and nothing goes on the wire, so the client is never
    /// ordered backwards.
    pub max_allowed_correction: f32,
    /// What the `is_explorable` field of a redirect carries (`0x01A6`), as
    /// `ASURA/crates/asura-world/src/travel.rs:21-69` decides it.
    ///
    /// The working implementation's **default is 0 for every travel**, not the destination's kind: a live
    /// server sends 1 for an explorable and 0 for an outpost, and when ASURA sent the live value an
    /// explorable's load against it waited 5138 ms from the redirect to the players block against 849 ms
    /// at 0 over the same walk (`docs/reference/instance-initialization.md`, "The explorable"). The
    /// faithful value is one setting away, so the setting is ASURA's own three modes: `0`, `1`, `auto`.
    pub travel_explorable: TravelExplorable,
    /// Half the side of a point doorway's box, in map units (`ASURA/config/Server/portals.json`,
    /// `point_box_half_extent`).
    ///
    /// A doorway stated as a wall does not need it: a wall is crossed when the step passes through the
    /// segment. A point doorway - the older shape - is crossed when the step intersects an edge of this
    /// box, and an extent of 0 or less disables point doorways entirely
    /// (`ASURA/crates/asura-world/src/portals.rs:31-64`, `point_half_extent`, `box_crossed`).
    pub point_box_half_extent: f32,
    /// The outpost a fresh character is placed in and has unlocked: the one unlock a character starts
    /// with (R12), and the map its own row names as its start.
    ///
    /// The login's placement names the same fact for a character with no map anywhere
    /// (`authserver.json`, `FallbackOutpost`); a world that wrote a different number here would hand a
    /// new character an unlock the login never sends it to.
    pub starting_outpost: u32,
    /// The address a travelling client is told to reconnect to, as `host:port`.
    ///
    /// The working implementation publishes one (`--public`, `ASURA/crates/asura-world/src/main.rs:73`);
    /// this stack's client dials the address it was launched against, which is the login's port, and the
    /// front door copies that connection to the world (`docs/port/02-wire.md`), so the default is that
    /// address.
    pub public_address: String,
}

impl Default for WorldConfig {
    /// The game's own numbers, which are what a file that states nothing gets.
    fn default() -> Self {
        Self {
            district_allotment: 100,
            party_size_cap: 8,
            spawn_scatter: 250.0,
            instance_tick: true,
            ping_value: 20,
            simulation_tick_ms: 100,
            sweep_interval_ms: 1000,
            compass_range: 10_000.0,
            explorable_disconnect_hold: 15 * 60,
            mission_button: true,
            resign_secs: 5,
            countdown_secs: 10,
            agent_health_base: 80,
            agent_health_per_level: 20,
            agent_energy: 25,
            agent_energy_regen: 0.0,
            agent_speed: 288.0,
            appearance: 0x0310_0000,
            max_allowed_correction: 250.0,
            travel_explorable: TravelExplorable::Zero,
            point_box_half_extent: 250.0,
            starting_outpost: 148,
            public_address: "127.0.0.1:6112".to_string(),
        }
    }
}

/// How a redirect's `is_explorable` is decided.
///
/// `ASURA/crates/asura-world/src/travel.rs:21-69`: the same three modes, with the same readings and the
/// same default (`Zero`), because that is the value a load against this server actually completes at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelExplorable {
    /// 0 for every travel, whatever the destination is. The default.
    Zero,
    /// 1 for every travel.
    One,
    /// The destination's own kind, which is what a live server sends: 1 for a map of type 1, 4 or 6.
    Auto,
}

impl TravelExplorable {
    /// The word a setting states.
    pub fn parse(text: &str) -> Option<TravelExplorable> {
        match text.trim() {
            "0" => Some(TravelExplorable::Zero),
            "1" => Some(TravelExplorable::One),
            "auto" => Some(TravelExplorable::Auto),
            _ => None,
        }
    }

    /// The value the redirect carries, given the destination's map type.
    ///
    /// `travel.rs:33-42`, `explorable_flag_for`: `auto` answers 1 for map types 1, 4 and 6 and 0 for
    /// everything else.
    pub fn flag(self, map_type: u8) -> u32 {
        match self {
            TravelExplorable::Zero => 0,
            TravelExplorable::One => 1,
            TravelExplorable::Auto => match map_type {
                1 | 4 | 6 => 1,
                _ => 0,
            },
        }
    }

    /// The switch's mode in words, for the startup line: a log always says which value travels.
    ///
    /// `travel.rs:45-51`, `explorable_flag_mode`.
    pub fn mode(self) -> &'static str {
        match self {
            TravelExplorable::Zero => {
                "0 for every travel (default: an explorable is announced as an outpost)"
            }
            TravelExplorable::One => "1 for every travel",
            TravelExplorable::Auto => {
                "auto: the destination's own kind, which is what a live server sends"
            }
        }
    }
}

impl WorldConfig {
    /// Read the `World` section of a program's configuration.
    ///
    /// A key the file does not state keeps the game's value, and the read is reported: the program logs
    /// every number it is running by, so an operator can see the file's effect without reading the code.
    pub fn read(config: &Config) -> Result<(Self, Vec<String>), String> {
        let mut settings = Self::default();
        let mut notes = Vec::new();

        let small = |config: &Config, key: &str, default: u32| -> Result<u32, String> {
            let (value, _) = config
                .value::<u32>(&format!("World.{key}"), default)
                .map_err(|error| error.to_string())?;
            Ok(value)
        };

        settings.district_allotment =
            small(config, "DistrictAllotment", settings.district_allotment)?;
        settings.party_size_cap = small(config, "PartySizeCap", settings.party_size_cap)?;
        settings.explorable_disconnect_hold = small(
            config,
            "ExplorableDisconnectHold",
            settings.explorable_disconnect_hold,
        )?;

        settings.spawn_scatter = number(config, "SpawnScatter", settings.spawn_scatter)?;
        settings.compass_range = number(config, "CompassRange", settings.compass_range)?;
        settings.instance_tick = flag(config, "InstanceTick", settings.instance_tick)?;
        settings.simulation_tick_ms =
            small(config, "SimulationTickMs", settings.simulation_tick_ms)?;
        settings.ping_value = small(config, "PingValue", settings.ping_value)?;
        settings.sweep_interval_ms = small(config, "SweepIntervalMs", settings.sweep_interval_ms)?;
        settings.mission_button = flag(config, "MissionButton", settings.mission_button)?;
        settings.resign_secs = small(config, "ResignSecs", settings.resign_secs)?;
        settings.countdown_secs = small(config, "CountdownSecs", settings.countdown_secs)?;

        // The agent a load creates, and the look of a character with no bits of its own. The numbers
        // are the pack's (`ASURA/config/World/Vanilla/combat.json`, `character_defaults.json`):
        // health 80 + 20 per level, energy 25, regeneration 0, speed 288, profession 1 with hair style 3.
        settings.agent_health_base = small(config, "AgentHealthBase", settings.agent_health_base)?;
        settings.agent_health_per_level = small(
            config,
            "AgentHealthPerLevel",
            settings.agent_health_per_level,
        )?;
        settings.agent_energy = small(config, "AgentEnergy", settings.agent_energy)?;
        settings.agent_energy_regen =
            number(config, "AgentEnergyRegen", settings.agent_energy_regen)?;
        settings.agent_speed = number(config, "AgentSpeed", settings.agent_speed)?;
        settings.appearance = small(config, "Appearance", settings.appearance)?;
        settings.max_allowed_correction = number(
            config,
            "MaxAllowedCorrection",
            settings.max_allowed_correction,
        )?;
        // What a redirect's `is_explorable` carries. `travel.rs:33-69` reads the same three words from
        // `ASURA_TRAVEL_EXPLORABLE`; here it is the program's file (rule 7), with the same default.
        let (travel_explorable, _) = config
            .value_string("World.TravelExplorable", "0")
            .map_err(|error| error.to_string())?;
        settings.point_box_half_extent =
            number(config, "PointBoxHalfExtent", settings.point_box_half_extent)?;
        settings.starting_outpost = small(config, "StartingOutpost", settings.starting_outpost)?;
        settings.travel_explorable =
            TravelExplorable::parse(&travel_explorable).ok_or_else(|| {
                format!("World.TravelExplorable is not one of 0, 1, auto: '{travel_explorable}'")
            })?;
        let (public_address, _) = config
            .value_string("World.PublicAddress", &settings.public_address)
            .map_err(|error| error.to_string())?;
        settings.public_address = public_address.trim().to_string();

        notes.push(format!(
            "World: a district holds {} player(s) (R5), a party is capped at {} (R15), a character lands within {:.0} units of its spawn point (R13), an explorable is held {} s for a network disconnect (R37), the compass range is {:.0} (R34), ticking is {} every {} ms, the instance table is swept every {} ms, the mission button is {} (R56).",
            settings.district_allotment,
            settings.party_size_cap,
            settings.spawn_scatter,
            settings.explorable_disconnect_hold,
            settings.compass_range,
            if settings.instance_tick { "on" } else { "off" },
            settings.simulation_tick_ms,
            settings.sweep_interval_ms,
            if settings.mission_button { "shown" } else { "hidden" },
        ));
        notes.push(format!(
            "World: a travel is redirected to {} (World.PublicAddress).",
            settings.public_address,
        ));
        notes.push(format!(
            "World: a travel's is_explorable: {}; a point doorway's box has a half side of {:.0}; a new character starts in outpost {} (R12).",
            settings.travel_explorable.mode(),
            settings.point_box_half_extent,
            settings.starting_outpost,
        ));
        notes.push(format!(
            "World: a player agent is created with {} health plus {} a level, {} energy, {:.1} energy a second and a walking speed of {:.0}; a character with no appearance of its own is drawn as 0x{:08X}.",
            settings.agent_health_base,
            settings.agent_health_per_level,
            settings.agent_energy,
            settings.agent_energy_regen,
            settings.agent_speed,
            settings.appearance,
        ));
        Ok((settings, notes))
    }
}

/// One number, as the file states it.
fn number(config: &Config, key: &str, default: f32) -> Result<f32, String> {
    let (text, _) = config
        .value_string(&format!("World.{key}"), &format!("{default}"))
        .map_err(|error| error.to_string())?;
    text.trim()
        .parse()
        .map_err(|_| format!("World.{key} is not a number: '{text}'"))
}

/// One flag, as the file states it: the reader's own `1`/`0`.
fn flag(config: &Config, key: &str, default: bool) -> Result<bool, String> {
    let (value, _) = config
        .value_bool(&format!("World.{key}"), default)
        .map_err(|error| error.to_string())?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::configuration::Policy;

    fn config(text: &str) -> Config {
        Config::parse(
            text,
            std::path::PathBuf::from("test.conf"),
            Policy::default(),
        )
    }

    #[test]
    fn the_defaults_are_the_numbers_the_ledger_names() {
        let settings = WorldConfig::default();
        assert_eq!(settings.district_allotment, 100, "R5");
        assert_eq!(settings.party_size_cap, 8, "R15");
        assert_eq!(settings.spawn_scatter, 250.0, "R13");
        assert!(settings.instance_tick, "R33");
        assert_eq!(
            settings.simulation_tick_ms, 100,
            "R33's cadence, ASURA's own intervals file"
        );
        assert_eq!(
            settings.sweep_interval_ms, 1000,
            "R33/R40's sweep, a second"
        );
        assert_eq!(settings.compass_range, 10_000.0, "R34");
        assert_eq!(settings.explorable_disconnect_hold, 900, "R37");
        assert!(settings.mission_button, "R56");
    }

    #[test]
    fn a_file_that_states_nothing_keeps_the_games_numbers() {
        let (settings, notes) = WorldConfig::read(&config("")).expect("reads");
        assert_eq!(settings, WorldConfig::default());
        assert_eq!(
            notes.len(),
            4,
            "the read is reported: the rules, the redirect address, the travel's flag, then the agent"
        );
    }

    #[test]
    fn a_file_changes_a_number_and_the_read_says_which() {
        let (settings, notes) = WorldConfig::read(&config(
            "World.DistrictAllotment = 40\nWorld.SpawnScatter = 100\nWorld.InstanceTick = 0\n",
        ))
        .expect("reads");
        assert_eq!(settings.district_allotment, 40);
        assert_eq!(settings.spawn_scatter, 100.0);
        assert!(!settings.instance_tick);
        assert_eq!(
            settings.party_size_cap, 8,
            "the keys the file does not state keep the game's value"
        );
        assert!(
            notes[0].contains("40"),
            "the log line carries the numbers in force"
        );
    }

    #[test]
    fn a_number_that_is_not_a_number_is_refused_with_its_key() {
        let error = WorldConfig::read(&config("World.SpawnScatter = near\n")).expect_err("refused");
        assert!(error.contains("World.SpawnScatter"), "{error}");
    }

    /// `ASURA/crates/asura-world/src/travel.rs:33-69`: three modes with three readings, the default
    /// answering 0 for every destination and `auto` giving 1 to map types 1, 4 and 6 only.
    #[test]
    fn the_travels_explorable_flag_is_asuras_three_modes() {
        let zero = TravelExplorable::parse("0").unwrap();
        let one = TravelExplorable::parse("1").unwrap();
        let auto = TravelExplorable::parse("auto").unwrap();
        assert_eq!(
            zero.flag(1),
            0,
            "the default announces an explorable as an outpost"
        );
        assert_eq!(one.flag(0), 1, "one for every travel");
        assert_eq!(auto.flag(1), 1, "auto: an explorable");
        assert_eq!(auto.flag(4), 1, "auto: 4 is an explorable type");
        assert_eq!(auto.flag(6), 1, "auto: 6 is an explorable type");
        assert_eq!(auto.flag(0), 0, "auto: an outpost");
        assert_eq!(
            auto.flag(11),
            0,
            "auto: character creation is not an explorable"
        );
        assert!(
            TravelExplorable::parse("2").is_none(),
            "only the three words"
        );
        assert!(
            zero.mode().contains("default"),
            "the mode names itself for the log"
        );
    }

    #[test]
    fn a_file_states_the_travels_explorable_flag_or_is_refused() {
        let (settings, notes) =
            WorldConfig::read(&config("World.TravelExplorable = auto\n")).expect("reads");
        assert_eq!(settings.travel_explorable, TravelExplorable::Auto);
        assert!(
            notes.iter().any(|note| note.contains("is_explorable")),
            "the read is reported: {notes:?}"
        );
        let error =
            WorldConfig::read(&config("World.TravelExplorable = maybe\n")).expect_err("refused");
        assert!(error.contains("World.TravelExplorable"), "{error}");
    }
}
