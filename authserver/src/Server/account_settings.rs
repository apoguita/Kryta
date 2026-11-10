//! The account settings block: the 591 bytes a client is handed on every login.
//!
//! A value file, `data/account_settings.conf`, because it is a copied payload rather than something
//! this server constructs. Most of it is not understood, and keeping it byte for byte is the only way
//! to hand the client what it expects: a wrong length here is a desynchronised connection, because the
//! client reads the next message's header out of the middle of this one.

/// Read the block.
///
/// One `data = ` line holding space-separated hexadecimal bytes. A file whose length is not the one
/// the records give is refused rather than sent: a block of the wrong size is worse than no block,
/// because the client cannot recover from it.
pub fn load(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, String> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;

    // The block runs across many lines: the `data` line opens it, the lines after it continue it, and
    // a comment may sit between them.
    let mut bytes = Vec::new();
    let mut collecting = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let words: Vec<&str> = if let Some((key, value)) = line.split_once('=') {
            collecting = key.trim() == "data";
            if !collecting {
                continue;
            }
            value.split_whitespace().collect()
        } else if collecting {
            line.split_whitespace().collect()
        } else {
            continue;
        };

        for word in words {
            let byte = u8::from_str_radix(word, 16)
                .map_err(|_| format!("{}: '{word}' is not a hexadecimal byte", path.display()))?;
            bytes.push(byte);
        }
    }

    if bytes.is_empty() {
        return Err(format!("{}: no 'data' line with bytes", path.display()));
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_block_is_five_hundred_and_ninety_one_bytes() {
        let settings = load("../data/account_settings.conf").expect("the shipped block");
        assert_eq!(settings.len(), 591);
        // The first bytes, which a capture shows and a transposition would move.
        assert_eq!(&settings[..4], &[0x08, 0x00, 0x66, 0x21]);
    }

    #[test]
    fn a_file_with_no_data_line_is_refused() {
        let dir = std::env::temp_dir().join(format!("kryta-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.conf");
        std::fs::write(&path, "# nothing here\n").unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn a_word_that_is_not_hexadecimal_is_refused_with_its_text() {
        let dir = std::env::temp_dir().join(format!("kryta-settings-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.conf");
        std::fs::write(&path, "data = 08 00 zz\n").unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.contains("'zz'"), "{error}");
    }
}
