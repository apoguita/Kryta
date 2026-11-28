//! `Kryta.dll`: what Kryta adds to the game client.
//!
//! The patched client names this DLL in its import table, so Windows loads it when the client
//! starts - no launcher and no injection (`docs/port/22-client-platform.md`). Everything Kryta adds
//! to the client lives here, as a platform with features on top:
//!
//! | part | module | what it is |
//! |---|---|---|
//! | start-up | `runtime` | `DllMain` starts one thread and returns; the thread does the rest |
//! | server link | `runtime::link` | the DLL's own connection to the server (`kryta-client-link`) |
//! | offsets | [`scanner`], [`patterns`] | places in the client found by byte pattern, from JSON the server sends |
//! | hooks | `runtime::hooks` | detours and call redirects written at the addresses found |
//! | features | [`titles`], `runtime::title_hooks` | custom title text, the first feature |
//! | | [`map_cache`], `runtime::file_hooks` | the maps the server hosts: fetched over the link into one cache, and opened in an archive file's place |
//!
//! The scanner, the offset resolver and the hook layer are ported from Py4GW Reforged Native
//! (`C:\Users\Apo\Py4GW_Reforged_Native` at `e2f5664`), which is the reference for how a DLL finds
//! and hooks this client. Each module names the file it was ported from.
//!
//! The parts that touch the client are `runtime`, built only for the 32-bit Windows client. The
//! rest is safe code that builds and is tested anywhere.

pub mod map_cache;
pub mod map_names;
pub mod map_rows;
pub mod patterns;
pub mod scanner;
pub mod titles;

#[cfg(all(windows, target_arch = "x86"))]
mod runtime;

/// The symbol the patched client imports. Importing one symbol is what makes Windows load this DLL
/// with the client; the function itself does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn kryta_client() {}

#[cfg(test)]
mod client_tests {
    //! The offsets Kryta ships, against a real client file. Ignored unless `KRYTA_GW_EXE` names an
    //! unpatched `Gw.exe`:
    //!
    //! `$env:KRYTA_GW_EXE = 'F:\GW\GW1\Gw.exe'; cargo test -p kryta-client -- --ignored`

    use crate::patterns::{Memory, Patterns};
    use crate::scanner::{Scanner, Section};

    struct NoMemory;

    impl Memory for NoMemory {
        fn read_u32(&self, _: u32) -> Option<u32> {
            None
        }
    }

    /// A measurement, not a check: where the client's per-map table is and what it holds.
    #[test]
    #[ignore = "needs KRYTA_GW_EXE to name an unpatched Gw.exe"]
    fn measure_the_map_table() {
        let exe = std::env::var("KRYTA_GW_EXE").expect("KRYTA_GW_EXE is not set");
        let file = std::fs::read(&exe).expect("the client file cannot be read");
        let scanner = Scanner::from_file(&file, 0x0040_0000).unwrap();
        println!("{exe}: time stamp {:#x}", scanner.time_date_stamp());

        // imul eax, esi, 0x7c; pop esi; add eax, imm32 (Py4GW Reforged, offsets/map.json).
        let pattern = [0x6B, 0xC6, 0x7C, 0x5E, 0x05];
        let (start, end) = scanner.section_range(Section::Text).unwrap();
        let mut from = start;
        loop {
            let at = scanner.find_in_range(&pattern, None, 0, from, end);
            if at == 0 {
                break;
            }
            let function = scanner.to_function_start(at, 0x100);
            let table = scanner.read_u32(at + 5).unwrap();
            println!("match at {at:#x}, function {function:#x}, table {table:#x}");
            let bytes: Vec<String> = (function..at + 12)
                .map(|address| format!("{:02x}", scanner.read_u32(address).unwrap() as u8))
                .collect();
            println!("  code: {}", bytes.join(" "));
            for map in [0u32, 148, 146, 887, 888, 896, 897] {
                let row: Vec<String> = (0..31)
                    .map(|field| match scanner.read_u32(table + map * 124 + field * 4) {
                        Some(value) => format!("{value:x}"),
                        None => "-".to_string(),
                    })
                    .collect();
                println!("  row {map}: {}", row.join(" "));
            }
            // The rows the client has no map of its own for: those that share one name with many
            // others. Field 29 is `name_id` (`MAP_ROW_FIELDS`, `tools/client-link/src/lib.rs`).
            let count = scanner.read_u32(function + 9).unwrap();
            let field = |map: u32, field: u32| scanner.read_u32(table + map * 124 + field * 4).unwrap();
            let mut by_name: std::collections::BTreeMap<u32, Vec<u32>> = Default::default();
            for map in 0..count {
                by_name.entry(field(map, 29)).or_default().push(map);
            }
            for (name, maps) in by_name.iter().filter(|(_, maps)| maps.len() >= 4) {
                let listed: Vec<String> = maps
                    .iter()
                    .map(|map| format!("{map}(t{:x} f{:x})", field(*map, 3), field(*map, 4)))
                    .collect();
                println!("  name {name:#x} is shared by {} rows: {}", maps.len(), listed.join(" "));
            }
            // With KRYTA_MAP_ROWS=all, one short line for every row.
            if std::env::var("KRYTA_MAP_ROWS").is_ok_and(|rows| rows == "all") {
                for map in 0..count {
                    println!(
                        "  all {map}: campaign {} continent {} region {} type {} flags {:x} thumb {:x} x {} y {} name {:x}",
                        field(map, 0), field(map, 1), field(map, 2), field(map, 3), field(map, 4),
                        field(map, 5), field(map, 16), field(map, 17), field(map, 29)
                    );
                }
            }
            from = at + 1;
        }
    }

    /// A measurement, not a check: the client's bytes at the places `KRYTA_DUMP` names, as
    /// `address:length` pairs in hexadecimal separated by commas, at the file's preferred base
    /// `0x400000` (a crash report's address less its `BaseAddr`, plus `0x400000`).
    #[test]
    #[ignore = "needs KRYTA_GW_EXE to name an unpatched Gw.exe, and KRYTA_DUMP"]
    fn dump_the_clients_bytes() {
        let exe = std::env::var("KRYTA_GW_EXE").expect("KRYTA_GW_EXE is not set");
        let file = std::fs::read(&exe).expect("the client file cannot be read");
        let scanner = Scanner::from_file(&file, 0x0040_0000).unwrap();
        for pair in std::env::var("KRYTA_DUMP").expect("KRYTA_DUMP is not set").split(',') {
            let (address, length) = pair.split_once(':').expect("address:length");
            let address = u32::from_str_radix(address.trim(), 16).unwrap();
            let length = u32::from_str_radix(length.trim(), 16).unwrap();
            for line in (address..address + length).step_by(16) {
                let bytes: Vec<String> = (line..(line + 16).min(address + length))
                    .map(|at| match scanner.read_u32(at) {
                        Some(value) => format!("{:02x}", value as u8),
                        None => "--".to_string(),
                    })
                    .collect();
                println!("  dump {line:08x}  {}", bytes.join(" "));
            }
            println!("  dump --");
        }
    }

    /// A measurement, not a check: where the client raises the assertion `KRYTA_ASSERT` names, as
    /// `file|message`, the start of the function that holds it, and that function's bytes up to
    /// the assertion.
    #[test]
    #[ignore = "needs KRYTA_GW_EXE to name an unpatched Gw.exe, and KRYTA_ASSERT"]
    fn find_an_assertion_of_the_client() {
        let exe = std::env::var("KRYTA_GW_EXE").expect("KRYTA_GW_EXE is not set");
        let file = std::fs::read(&exe).expect("the client file cannot be read");
        let scanner = Scanner::from_file(&file, 0x0040_0000).unwrap();
        let named = std::env::var("KRYTA_ASSERT").expect("KRYTA_ASSERT is not set");
        let (source, message) = named.split_once('|').expect("file|message");
        let at = scanner.find_assertion(source, message, 0, 0);
        assert_ne!(at, 0, "the assertion was not found");
        let function = scanner.to_function_start(at, 0x400);
        println!("  found {message} of {source} at {at:#x}, in the function at {function:#x}");
        for line in (function..at + 0x20).step_by(16) {
            let bytes: Vec<String> = (line..line + 16)
                .map(|address| format!("{:02x}", scanner.read_u32(address).unwrap_or(0) as u8))
                .collect();
            println!("  found {line:08x}  {}", bytes.join(" "));
        }
    }

    /// A measurement, not a check: every place the client's code compares a register with the
    /// number of rows of its map table, and the assertion that follows it when there is one
    /// (`push <line>; mov edx, <file>; mov ecx, <expression>`).
    #[test]
    #[ignore = "needs KRYTA_GW_EXE to name an unpatched Gw.exe"]
    fn measure_the_map_count_checks() {
        let exe = std::env::var("KRYTA_GW_EXE").expect("KRYTA_GW_EXE is not set");
        let file = std::fs::read(&exe).expect("the client file cannot be read");
        let scanner = Scanner::from_file(&file, 0x0040_0000).unwrap();
        let rows: u32 = 0x382;
        let byte = |at: u32| scanner.read_u32(at).map(|value| value as u8).unwrap_or(0);
        let text = |at: u32| -> String {
            (0..80).map(|i| byte(at + i)).take_while(|b| *b != 0).map(char::from).collect()
        };
        let (start, end) = scanner.section_range(Section::Text).unwrap();
        let mut at = start;
        while at + 4 < end {
            if scanner.read_u32(at) == Some(rows) {
                // cmp r32, imm32 is 81 F8..FF imm, and cmp eax, imm32 is 3D imm.
                let opcode = if byte(at - 2) == 0x81 && byte(at - 1) >= 0xF8 {
                    format!("cmp r{}", byte(at - 1) & 7)
                } else if byte(at - 1) == 0x3D {
                    "cmp eax".to_string()
                } else {
                    format!("{:02x} {:02x}", byte(at - 2), byte(at - 1))
                };
                // The assertion, when one follows within sixteen bytes: 68 line BA file B9 expr.
                let mut assertion = String::new();
                for next in at + 4..at + 20 {
                    if byte(next) == 0x68 && byte(next + 5) == 0xBA && byte(next + 10) == 0xB9 {
                        let line = scanner.read_u32(next + 1).unwrap();
                        let file = text(scanner.read_u32(next + 6).unwrap());
                        let expression = text(scanner.read_u32(next + 11).unwrap());
                        assertion = format!("{expression} at {file}({line})");
                        break;
                    }
                }
                let function = scanner.to_function_start(at, 0x400);
                println!("  check {at:#x} in {function:#x}: {opcode}  {assertion}");
            }
            at += 1;
        }
    }

    #[test]
    #[ignore = "needs KRYTA_GW_EXE to name an unpatched Gw.exe"]
    fn every_shipped_offset_resolves_in_the_client() {
        let exe = std::env::var("KRYTA_GW_EXE").expect("KRYTA_GW_EXE is not set");
        let file = std::fs::read(&exe).expect("the client file cannot be read");
        let base = 0x0040_0000;
        let scanner = Scanner::from_file(&file, base).unwrap();

        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/flavors/Vanilla/Client/offsets");
        let mut patterns = Patterns::new();
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "json") {
                let text = std::fs::read_to_string(&path).unwrap();
                patterns.load(&serde_json::from_str(&text).unwrap()).unwrap();
            }
        }

        let names = patterns.resolver_names();
        assert!(!names.is_empty(), "no resolver under {}", directory.display());
        for name in names {
            let result = patterns.resolve(&name, &scanner, &NoMemory);
            let address = result
                .value
                .unwrap_or_else(|| panic!("{name}: {} {:?}", result.message, result.trace));
            assert!(
                scanner.is_valid_ptr(address, Section::Text),
                "{name} resolved outside the code: {address:#x}"
            );
            println!("{name} = {address:#x}");
        }
    }
}
