//! The scanner: finding places in the client by what their bytes look like.
//!
//! Ported from Py4GW Reforged Native at `e2f5664`: `src/base/scanner.cpp` (212 lines) and
//! `src/base/file_scanner.cpp` (370 lines). The searches, their order and their edge cases are
//! Reforged's; each function names the one it was ported from.
//!
//! Like Reforged's, it searches the executable's **file**, not the running image, so a hook already
//! written into memory cannot spoil a search, and it answers in addresses of the running image.
//!
//! Where this differs from Reforged, and why:
//!
//! - Reforged maps the file as an image section (`CreateFileMappingW` with `SEC_IMAGE_NO_EXECUTE`).
//!   This reads the file and lays each section out at its virtual size. The bytes are the same -
//!   neither applies relocations - and it needs no Windows call, so the scanner is tested on the
//!   machine that builds it.
//! - Reforged's `FindNthUseOfAddress` searches the file for a running address as it stands, which
//!   only finds anything while the client is loaded at its preferred base. This converts the
//!   address to the preferred base first; at the preferred base the two are the same search.
//! - Reforged's `FunctionFromNearCall` reads the call from live memory. This reads it from the
//!   file, where a relative call holds the same bytes and no hook has replaced it.

/// One of the three sections a search can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// `.text`: the code.
    Text = 0,
    /// `.rdata`: constants and strings.
    RData = 1,
    /// `.data`: writable globals.
    Data = 2,
}

/// A section's bytes and where it sits, relative to the image base.
#[derive(Debug, Clone)]
struct SectionImage {
    /// The section's relative virtual address.
    rva: u32,
    /// The section's bytes, as long as its virtual size.
    bytes: Vec<u8>,
}

/// The client executable, ready to be searched.
#[derive(Debug, Clone)]
pub struct Scanner {
    /// Where the image is loaded in the running process.
    runtime_base: u32,
    /// Where the file says it prefers to be loaded: the base its absolute addresses are written for.
    preferred_base: u32,
    /// `TimeDateStamp` of the file header: which build this is.
    time_date_stamp: u32,
    sections: [Option<SectionImage>; 3],
}

/// Why a file could not be read as the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageError(pub String);

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn refuse<T>(reason: &str) -> Result<T, ImageError> {
    Err(ImageError(reason.to_string()))
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

impl Scanner {
    /// Read an executable's file, to be searched as the image loaded at `runtime_base`.
    ///
    /// Port of `FileScanner::CreateFromPath` (`file_scanner.cpp:49-170`): the same checks on the
    /// DOS header, the PE signature, the machine and the section table, and the same three section
    /// names (`FileScanner::FileScanner`, `:177-197`).
    pub fn from_file(file: &[u8], runtime_base: u32) -> Result<Self, ImageError> {
        if u16_at(file, 0) != Some(0x5A4D) {
            return refuse("Invalid IMAGE_DOS_HEADER");
        }
        let Some(pe) = u32_at(file, 0x3C).map(|offset| offset as usize) else {
            return refuse("Not enough bytes for the IMAGE_DOS_HEADER");
        };
        if u32_at(file, pe) != Some(0x0000_4550) {
            return refuse("Not a PE file, invalid signature");
        }
        // IMAGE_FILE_HEADER follows the signature: Machine, NumberOfSections, TimeDateStamp, ...,
        // SizeOfOptionalHeader at +16.
        if u16_at(file, pe + 4) != Some(0x014C) {
            return refuse("Not a 32 bits executable");
        }
        let (Some(count), Some(time_date_stamp), Some(optional_size)) = (
            u16_at(file, pe + 6),
            u32_at(file, pe + 8),
            u16_at(file, pe + 20),
        ) else {
            return refuse("Not enough bytes for the PE_FILE_HEADER");
        };
        let optional = pe + 24;
        // IMAGE_OPTIONAL_HEADER32: ImageBase at +28, SizeOfImage at +56.
        let (Some(preferred_base), Some(image_size)) =
            (u32_at(file, optional + 28), u32_at(file, optional + 56))
        else {
            return refuse("Not enough bytes for the optional header");
        };

        let mut sections: [Option<SectionImage>; 3] = [None, None, None];
        let table = optional + optional_size as usize;
        for index in 0..count as usize {
            // IMAGE_SECTION_HEADER, 40 bytes: Name[8], VirtualSize, VirtualAddress, SizeOfRawData,
            // PointerToRawData.
            let header = table + index * 40;
            let (Some(name), Some(virtual_size), Some(rva), Some(raw_size), Some(raw_offset)) = (
                file.get(header..header + 8),
                u32_at(file, header + 8),
                u32_at(file, header + 12),
                u32_at(file, header + 16),
                u32_at(file, header + 20),
            ) else {
                return refuse("Not enough bytes for the section headers");
            };
            if image_size < rva || image_size - rva < virtual_size {
                return refuse("Not enough bytes for a section");
            }
            let which = match name {
                b".text\0\0\0" => Section::Text,
                b".rdata\0\0" => Section::RData,
                b".data\0\0\0" => Section::Data,
                _ => continue,
            };
            // What an image mapping holds: the raw bytes, cut or zero-filled to the virtual size.
            let mut bytes = vec![0u8; virtual_size as usize];
            let held = raw_size.min(virtual_size) as usize;
            let Some(raw) = file.get(raw_offset as usize..raw_offset as usize + held) else {
                return refuse("Not enough bytes for a section");
            };
            bytes[..held].copy_from_slice(raw);
            sections[which as usize] = Some(SectionImage { rva, bytes });
        }
        if sections[Section::Text as usize].is_none() {
            return refuse("Scanner text section not found.");
        }
        Ok(Self {
            runtime_base,
            preferred_base,
            time_date_stamp,
            sections,
        })
    }

    /// Which build the file is: the `TimeDateStamp` of its header.
    pub fn time_date_stamp(&self) -> u32 {
        self.time_date_stamp
    }

    /// A section's range in the running image, as `(start, end)`.
    ///
    /// Port of `Scanner::GetSectionRange` and the `mem_sections` table (`scanner.cpp:52-70`).
    pub fn section_range(&self, section: Section) -> Option<(u32, u32)> {
        self.sections[section as usize].as_ref().map(|image| {
            let start = self.runtime_base.wrapping_add(image.rva);
            (start, start.wrapping_add(image.bytes.len() as u32))
        })
    }

    /// The byte at a running address, when a known section holds it.
    fn byte(&self, address: i64) -> Option<u8> {
        self.sections.iter().flatten().find_map(|image| {
            let start = i64::from(self.runtime_base.wrapping_add(image.rva));
            let index = address - start;
            (index >= 0)
                .then(|| image.bytes.get(index as usize).copied())
                .flatten()
        })
    }

    /// Whether `pattern` under `mask` sits at `address`.
    fn matches(&self, address: i64, pattern: &[u8], mask: Option<&[u8]>, length: usize) -> bool {
        (0..length).all(|index| {
            let compared = mask.is_none_or(|mask| mask[index] == b'x');
            !compared || self.byte(address + index as i64) == pattern.get(index).copied()
        })
    }

    /// Search a range of running addresses for a pattern. With `start > end` the search runs
    /// backwards from `start`.
    ///
    /// Port of `FileScanner::FindInRange` (`file_scanner.cpp:327-363`): the length is the mask's
    /// (or the pattern's when there is no mask), the end is drawn in by that length in both
    /// directions, the first byte is always compared, and the answer is the match plus `offset`.
    pub fn find_in_range(
        &self,
        pattern: &[u8],
        mask: Option<&[u8]>,
        offset: i32,
        start: u32,
        end: u32,
    ) -> u32 {
        let length = mask.map_or(pattern.len(), <[u8]>::len);
        if length == 0 || pattern.is_empty() {
            return 0;
        }
        let first = pattern[0];
        let start = i64::from(start);
        let end = i64::from(end) - length as i64;
        let hit = |address: i64| {
            self.byte(address) == Some(first) && self.matches(address, pattern, mask, length)
        };
        let found = if start > end {
            let mut address = start;
            loop {
                if address < end {
                    break None;
                }
                if hit(address) {
                    break Some(address);
                }
                address -= 1;
            }
        } else {
            (start..end).find(|&address| hit(address))
        };
        found.map_or(0, |address| (address + i64::from(offset)) as u32)
    }

    /// Search one section for a pattern.
    ///
    /// Port of `Scanner::Find` (`scanner.cpp:101-103`).
    pub fn find(&self, pattern: &[u8], mask: Option<&[u8]>, offset: i32, section: Section) -> u32 {
        match self.section_range(section) {
            Some((start, end)) => self.find_in_range(pattern, mask, offset, start, end),
            None => 0,
        }
    }

    /// Whether an address is inside a section.
    ///
    /// Port of `Scanner::IsValidPtr` (`scanner.cpp:105-109`): both bounds are exclusive.
    pub fn is_valid_ptr(&self, address: u32, section: Section) -> bool {
        self.section_range(section)
            .is_some_and(|(start, end)| address != 0 && address > start && address < end)
    }

    /// The function a call or jump instruction leads to, followed through any jump it lands on.
    ///
    /// Port of `Scanner::FunctionFromNearCall` (`scanner.cpp:111-136`): `E8` and `E9` carry a
    /// 32-bit distance from the next instruction, `EB` a signed 8-bit one.
    pub fn function_from_near_call(&self, call: u32, check_valid_ptr: bool) -> u32 {
        if !self.is_valid_ptr(call, Section::Text) {
            return 0;
        }
        let at = |offset: i64| self.byte(i64::from(call) + offset);
        let function = match at(0) {
            Some(0xE8 | 0xE9) => {
                let (Some(a), Some(b), Some(c), Some(d)) = (at(1), at(2), at(3), at(4)) else {
                    return 0;
                };
                u32::from_le_bytes([a, b, c, d])
                    .wrapping_add(call)
                    .wrapping_add(5)
            }
            Some(0xEB) => {
                let Some(distance) = at(1) else {
                    return 0;
                };
                (distance as i8 as i32 as u32)
                    .wrapping_add(call)
                    .wrapping_add(2)
            }
            _ => return 0,
        };
        if check_valid_ptr && !self.is_valid_ptr(function, Section::Text) {
            return 0;
        }
        match self.function_from_near_call(function, check_valid_ptr) {
            0 => function,
            nested => nested,
        }
    }

    /// The `nth` place a section mentions an address (0 is the first).
    ///
    /// Port of `Scanner::FindNthUseOfAddress` (`scanner.cpp:142-170`). The file holds addresses for
    /// its preferred base, so the address is converted to that base before it is searched for (see
    /// the module's note).
    pub fn find_nth_use_of_address(
        &self,
        address: u32,
        nth: usize,
        offset: i32,
        section: Section,
    ) -> u32 {
        if address == 0 {
            return 0;
        }
        let Some((start, end)) = self.section_range(section) else {
            return 0;
        };
        let in_file = address
            .wrapping_sub(self.runtime_base)
            .wrapping_add(self.preferred_base);
        let pattern = in_file.to_le_bytes();
        let mut found = self.find_in_range(&pattern, Some(b"xxxx"), 0, start, end);
        if found == 0 {
            return 0;
        }
        for _ in 0..nth {
            found = self.find_in_range(&pattern, Some(b"xxxx"), 0, found + 1, end);
            if found == 0 {
                return 0;
            }
        }
        found.wrapping_add(offset as u32)
    }

    /// The `nth` place a section mentions a string of bytes held in `.rdata`.
    ///
    /// Port of `Scanner::FindNthUseOfString(const char*)` (`scanner.cpp:186-198`): the string is
    /// searched for with its terminator, then the search walks back to the start of the string it
    /// was found inside, and that address is the one looked for.
    pub fn find_nth_use_of_string(
        &self,
        text: &[u8],
        nth: usize,
        offset: i32,
        section: Section,
    ) -> u32 {
        let mut pattern = text.to_vec();
        pattern.push(0);
        let found = self.find(&pattern, None, 0, Section::RData);
        if found == 0 {
            return 0;
        }
        let start = self.find_in_range(&[0], Some(b"x"), 1, found, found.wrapping_sub(0x64));
        if start == 0 {
            return 0;
        }
        self.find_nth_use_of_address(start, nth, offset, section)
    }

    /// The `nth` place a section mentions a wide string held in `.rdata`.
    ///
    /// Port of `Scanner::FindNthUseOfString(const wchar_t*)` (`scanner.cpp:172-184`): the pattern is
    /// the characters and one byte of the terminator, as Reforged's mask of `len * 2 + 1` has it.
    pub fn find_nth_use_of_wide_string(
        &self,
        text: &[u16],
        nth: usize,
        offset: i32,
        section: Section,
    ) -> u32 {
        let mut pattern: Vec<u8> = text.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        pattern.push(0);
        let found = self.find(&pattern, None, 0, Section::RData);
        if found == 0 {
            return 0;
        }
        let start =
            self.find_in_range(&[0, 0], Some(b"xx"), 2, found, found.wrapping_sub(0x128));
        if start == 0 {
            return 0;
        }
        self.find_nth_use_of_address(start, nth, offset, section)
    }

    /// The start of the function an address is inside: the nearest `push ebp; mov ebp, esp` at or
    /// before it, within `scan_range` bytes.
    ///
    /// Port of `Scanner::ToFunctionStart` (`scanner.cpp:207-212`).
    pub fn to_function_start(&self, address: u32, scan_range: u32) -> u32 {
        if address == 0 {
            return 0;
        }
        self.find_in_range(
            &[0x55, 0x8B, 0xEC],
            Some(b"xxx"),
            0,
            address,
            address.wrapping_sub(scan_range),
        )
    }

    /// The code that raises one of the client's assertions, found by the assertion's source file,
    /// its message and, when given, its line.
    ///
    /// Port of `FileScanner::FindAssertion` (`file_scanner.cpp:206-325`). The client raises an
    /// assertion with `push line; mov edx, file; mov ecx, message`, so the strings are found in
    /// `.rdata` and the instruction that loads their addresses is searched for in `.text`. A line
    /// that fits a byte is tried as `push imm8` first, then as `push imm32`; with no line only the
    /// two loads are matched. The file name is tried as written, lower-cased, then with each word
    /// capitalised, and when the match is inside a path the search walks back to its drive letter.
    pub fn find_assertion(&self, file: &str, message: &str, line: u32, offset: i32) -> u32 {
        let Some((rdata_start, rdata_end)) = self.section_range(Section::RData) else {
            return 0;
        };
        let terminated = |text: &str| {
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0);
            bytes
        };
        let in_file = |address: u32| {
            address
                .wrapping_sub(self.runtime_base)
                .wrapping_add(self.preferred_base)
                .to_le_bytes()
        };
        let message_pattern = terminated(message);
        let file_patterns = [
            terminated(file),
            terminated(&file.to_ascii_lowercase()),
            terminated(&capitalise_words(file)),
        ];

        // push imm32; mov edx, imm32; mov ecx, imm32
        let mut code = [0x68, 0, 0, 0, 0, 0xBA, 0, 0, 0, 0, 0xB9, 0, 0, 0, 0];
        let mut message_from = rdata_start;
        loop {
            let found_message =
                self.find_in_range(&message_pattern, None, 0, message_from, rdata_end);
            if found_message == 0 {
                return 0;
            }
            message_from = found_message + 1;
            code[11..15].copy_from_slice(&in_file(found_message));

            let mut file_from = rdata_start;
            loop {
                let mut found_file = file_patterns
                    .iter()
                    .map(|pattern| self.find_in_range(pattern, None, 0, file_from, rdata_end))
                    .find(|&found| found != 0)
                    .unwrap_or(0);
                if found_file == 0 {
                    break;
                }
                file_from = found_file + 1;

                if self.byte(i64::from(found_file) + 1) != Some(b':') {
                    found_file = self.find_in_range(
                        b":",
                        Some(b"x"),
                        -1,
                        found_file,
                        found_file.wrapping_sub(128),
                    );
                    if found_file == 0 {
                        break;
                    }
                }
                code[6..10].copy_from_slice(&in_file(found_file));

                let found = if line != 0 {
                    let short = if line & 0xFF == line {
                        // push imm8, then the two loads.
                        code[3] = 0x6A;
                        code[4] = line as u8;
                        self.find(&code[3..], None, offset, Section::Text)
                    } else {
                        0
                    };
                    if short != 0 {
                        short
                    } else {
                        code[0] = 0x68;
                        code[1..5].copy_from_slice(&line.to_le_bytes());
                        self.find(&code, None, offset, Section::Text)
                    }
                } else {
                    self.find(&code[5..], None, offset, Section::Text)
                };
                if found != 0 {
                    return found;
                }
            }
        }
    }
}

/// Each word's first letter in upper case, as Reforged's `ucwords` (`file_scanner.cpp:33-43`).
fn capitalise_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_is_letter = false;
    for character in text.chars() {
        if character.is_ascii_alphabetic() && !previous_is_letter {
            out.push(character.to_ascii_uppercase());
        } else {
            out.push(character);
        }
        previous_is_letter = character.is_ascii_alphabetic();
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const BASE: u32 = 0x0040_0000;
    pub(crate) const TEXT_RVA: u32 = 0x1000;
    pub(crate) const RDATA_RVA: u32 = 0x3000;

    /// A small 32-bit PE file with a `.text` and a `.rdata` section holding the given bytes.
    pub(crate) fn image(text: &[u8], rdata: &[u8]) -> Vec<u8> {
        let mut file = vec![0u8; 0x400];
        file[0..2].copy_from_slice(&0x5A4Du16.to_le_bytes());
        file[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        let pe = 0x80;
        file[pe..pe + 4].copy_from_slice(&0x4550u32.to_le_bytes());
        file[pe + 4..pe + 6].copy_from_slice(&0x014Cu16.to_le_bytes());
        file[pe + 6..pe + 8].copy_from_slice(&2u16.to_le_bytes());
        file[pe + 8..pe + 12].copy_from_slice(&0x6AE9_51C4u32.to_le_bytes());
        file[pe + 20..pe + 22].copy_from_slice(&0xE0u16.to_le_bytes());
        let optional = pe + 24;
        file[optional + 28..optional + 32].copy_from_slice(&BASE.to_le_bytes());
        file[optional + 56..optional + 60].copy_from_slice(&0x5000u32.to_le_bytes());
        let table = optional + 0xE0;
        let mut raw = 0x400u32;
        for (index, (name, rva, bytes)) in [
            (&b".text\0\0\0"[..], TEXT_RVA, text),
            (&b".rdata\0\0"[..], RDATA_RVA, rdata),
        ]
        .into_iter()
        .enumerate()
        {
            let header = table + index * 40;
            file[header..header + 8].copy_from_slice(name);
            file[header + 8..header + 12].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            file[header + 12..header + 16].copy_from_slice(&rva.to_le_bytes());
            file[header + 16..header + 20].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            file[header + 20..header + 24].copy_from_slice(&raw.to_le_bytes());
            raw += bytes.len() as u32;
        }
        file.extend_from_slice(text);
        file.extend_from_slice(rdata);
        file
    }

    fn scanner(text: &[u8], rdata: &[u8]) -> Scanner {
        Scanner::from_file(&image(text, rdata), BASE).unwrap()
    }

    #[test]
    fn a_file_that_is_not_the_client_is_refused() {
        assert!(Scanner::from_file(b"not an executable", BASE).is_err());
        let mut file = image(&[0x90; 16], &[0; 16]);
        file[0x84] = 0x64; // Machine: not i386
        assert_eq!(
            Scanner::from_file(&file, BASE).err(),
            Some(ImageError("Not a 32 bits executable".to_string()))
        );
    }

    #[test]
    fn a_pattern_is_found_under_its_mask_with_its_offset() {
        // A match that ends on the section's last byte is not found: the search stops one short, as
        // Reforged's does. So the code is followed by one more byte.
        let text = [0x90, 0x55, 0x8B, 0xEC, 0xE8, 0x11, 0x22, 0x33, 0x44, 0xC3, 0xCC];
        let scanner = scanner(&text, &[0; 4]);
        let found = scanner.find(
            &[0x55, 0x8B, 0xEC, 0xE8, 0, 0, 0, 0, 0xC3],
            Some(b"xxxx????x"),
            3,
            Section::Text,
        );
        assert_eq!(found, BASE + TEXT_RVA + 1 + 3);
        assert_eq!(
            scanner.find(&[0x55, 0x8B, 0xED], Some(b"xxx"), 0, Section::Text),
            0
        );
        assert_eq!(scanner.time_date_stamp(), 0x6AE9_51C4);
    }

    #[test]
    fn a_search_runs_backwards_when_start_is_above_end() {
        let text = [0x55, 0x8B, 0xEC, 0x90, 0x90, 0x55, 0x8B, 0xEC, 0x90, 0x90, 0x90];
        let scanner = scanner(&text, &[0; 4]);
        let from = BASE + TEXT_RVA + 9;
        assert_eq!(scanner.to_function_start(from, 0xFF), BASE + TEXT_RVA + 5);
        assert_eq!(
            scanner.to_function_start(BASE + TEXT_RVA + 4, 0xFF),
            BASE + TEXT_RVA
        );
    }

    #[test]
    fn a_near_call_leads_to_its_function_through_a_jump() {
        // +0: nop; +1: call +5 -> +0x0B; +0x0B: jmp +2 (EB) -> +0x0F; +0x0F: push ebp
        let mut text = vec![0x90; 0x20];
        text[1] = 0xE8;
        text[2..6].copy_from_slice(&5u32.to_le_bytes());
        text[0x0B] = 0xEB;
        text[0x0C] = 2;
        text[0x0F] = 0x55;
        let scanner = scanner(&text, &[0; 4]);
        let start = BASE + TEXT_RVA;
        assert_eq!(scanner.function_from_near_call(start + 1, true), start + 0x0F);
        assert_eq!(scanner.function_from_near_call(start + 3, true), 0);
    }

    #[test]
    fn a_use_of_an_address_is_found_at_any_load_base() {
        let rdata_address = BASE + RDATA_RVA + 4;
        let mut text = vec![0x90; 0x20];
        text[3] = 0x68;
        text[4..8].copy_from_slice(&rdata_address.to_le_bytes());
        text[0x10] = 0x68;
        text[0x11..0x15].copy_from_slice(&rdata_address.to_le_bytes());
        let file = image(&text, &[0; 16]);
        for base in [BASE, 0x00EC_0000] {
            let scanner = Scanner::from_file(&file, base).unwrap();
            let running = base + RDATA_RVA + 4;
            assert_eq!(
                scanner.find_nth_use_of_address(running, 0, -1, Section::Text),
                base + TEXT_RVA + 3
            );
            assert_eq!(
                scanner.find_nth_use_of_address(running, 1, 0, Section::Text),
                base + TEXT_RVA + 0x11
            );
            assert_eq!(scanner.find_nth_use_of_address(running, 2, 0, Section::Text), 0);
        }
    }

    #[test]
    fn a_use_of_a_string_is_found_from_inside_the_string() {
        let mut rdata = vec![0u8; 0x40];
        rdata[8..8 + 14].copy_from_slice(b"p:\\code\\Title.");
        rdata[8 + 14..8 + 17].copy_from_slice(b"cpp");
        let string = BASE + RDATA_RVA + 8;
        let mut text = vec![0x90; 0x20];
        text[5] = 0xBA;
        text[6..10].copy_from_slice(&string.to_le_bytes());
        let scanner = scanner(&text, &rdata);
        assert_eq!(
            scanner.find_nth_use_of_string(b"Title.cpp", 0, -1, Section::Text),
            BASE + TEXT_RVA + 5
        );
    }

    #[test]
    fn an_assertion_is_found_by_file_message_and_line() {
        let mut rdata = vec![0u8; 0x80];
        let file = b"P:\\Code\\Gw\\Title.cpp";
        let message = b"titleId < arrsize(s_titles)";
        rdata[0x10..0x10 + file.len()].copy_from_slice(file);
        rdata[0x40..0x40 + message.len()].copy_from_slice(message);
        let file_at = BASE + RDATA_RVA + 0x10;
        let message_at = BASE + RDATA_RVA + 0x40;
        // push 0x51; mov edx, file; mov ecx, message
        let mut text = vec![0x90; 0x30];
        text[0x10] = 0x6A;
        text[0x11] = 0x51;
        text[0x12] = 0xBA;
        text[0x13..0x17].copy_from_slice(&file_at.to_le_bytes());
        text[0x17] = 0xB9;
        text[0x18..0x1C].copy_from_slice(&message_at.to_le_bytes());
        let scanner = scanner(&text, &rdata);
        let expected = BASE + TEXT_RVA + 0x10;
        assert_eq!(
            scanner.find_assertion("Title.cpp", "titleId < arrsize(s_titles)", 0x51, 0),
            expected
        );
        // With no line, the match is the two loads.
        assert_eq!(
            scanner.find_assertion("Title.cpp", "titleId < arrsize(s_titles)", 0, 0),
            expected + 2
        );
        assert_eq!(scanner.find_assertion("Title.cpp", "no such message", 0, 0), 0);
    }
}
