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

    #[test]
    #[ignore = "needs KRYTA_GW_EXE to name an unpatched Gw.exe"]
    fn every_shipped_offset_resolves_in_the_client() {
        let exe = std::env::var("KRYTA_GW_EXE").expect("KRYTA_GW_EXE is not set");
        let file = std::fs::read(&exe).expect("the client file cannot be read");
        let base = 0x0040_0000;
        let scanner = Scanner::from_file(&file, base).unwrap();

        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/Client/offsets");
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
