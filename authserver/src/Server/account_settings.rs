//! The account settings block: the 591 bytes a login is answered with when the account has never
//! uploaded settings of its own (`account.settings` is NULL; `data/sql/base/db_auth/account_settings.sql`).
//!
//! **A protocol payload, not a configuration option.** Nobody configures it: it is what this client
//! expects, byte for byte, and the reference builds the same message out of its own structs. It is a
//! constant here for that reason - a server holds what its client demands, an operator holds what it
//! chooses (`docs/port/04-configuration.md`).
//!
//! Most of it is not understood. Keeping it byte for byte is the only way to hand the client what it
//! expects: a wrong length here is a desynchronised connection, because the client reads the next
//! message's header out of the middle of this one.
//!
//! To refresh it: capture the block a live client accepts, replace the literal below, rebuild. That is
//! the cost of a protocol change, and it is the same cost the reference pays.

/// The block, as a capture prints it.
///
/// Source: the reference's own send trace of one login (`recordings/gwps/world/load-stream.log`),
/// written up in `docs/port/02-wire.md`. One hex string rather than a byte array, so comparing it with
/// a trace is a text comparison.
const BLOCK_HEX: &str = concat!(
    "08 00 66 21 00 10 38 43 7f de 68 8e f5 93 ff ff 00 c1 3f 00 00 00 08 42 01 5e 00 02 00 80 33 18 00 81 03 00 00 00 4b 0a",
    "00 02 00 1b 00 d7 00 42 00 42 00 73 0f 00 07 0c 09 0b 01 02 03 04 05 06 07 08 0a 00 00 76 14 00 08 00 00 00 ef 03 01 00",
    "00 08 80 dc 00 df aa 0b 20 00 02 01 77 f0 01 10 3b 38 03 c0 ff 7d 99 01 fb bf fa bb ff 00 00 00 0d 00 00 a1 01 00 00 cb",
    "01 53 a4 01 96 00 00 96 03 ef 00 29 02 f8 00 26 02 51 6d 01 2d 02 6c 35 59 2f 00 1c 02 00 bd 51 d8 00 b0 01 48 0c 51 e1",
    "00 c3 01 3a 0c 51 82 00 00 01 57 0d 06 c8 02 5e 03 de 00 1d 02 60 00 00 00 00 06 15 06 54 01 0c 02 de 00 06 5e 01 c1 01",
    "fb ff 1a 02 06 05 03 ff 01 a3 00 77 02 06 f7 01 44 02 94 00 a0 02 46 f1 00 00 00 42 00 27 f3 f0 6f 00 8c 02 06 9a 00 19",
    "02 27 00 1c 02 46 de 02 00 00 59 00 60 00 00 00 00 06 6c 03 8e 02 29 00 81 03 06 24 03 00 00 ed 00 00 00 06 03 05 00 00",
    "e2 00 00 00 06 02 01 13 03 2f 00 2f 03 31 57 b5 b7 00 25 00 01 10 01 12 02 22 ff 14 00 06 86 03 00 00 0f 01 00 00 27 1f",
    "fa 90 00 c8 00 06 eb 03 ee 01 b0 00 84 02 60 00 00 00 00 05 99 00 f4 01 b0 00 64 00 47 1f 01 de 01 4b 64 60 00 00 00 00",
    "06 a8 02 1f 02 84 00 22 02 60 00 00 00 00 06 d5 04 00 00 b0 01 00 00 06 d7 04 00 00 26 02 00 00 06 d6 04 00 00 9a 02 00",
    "00 06 20 01 11 02 2d 00 52 02 60 00 00 00 00 06 b8 03 d2 01 64 00 71 02 60 00 00 00 00 06 b8 03 ea 01 64 00 38 02 05 dd",
    "00 d2 01 82 00 d0 00 47 ee 00 2c 01 70 fa 60 00 00 00 00 06 76 04 00 00 fa 00 00 00 46 29 03 00 00 5a 00 60 00 00 00 00",
    "60 00 00 00 00 60 00 00 00 00 60 00 00 00 00 60 00 00 00 00 46 22 02 00 00 3b 00 60 00 00 00 00 60 00 00 00 00 06 35 04",
    "00 00 97 00 00 00 06 23 01 00 00 7b 01 00 00 06 26 01 00 00 db 00 00 00 06 2e 01 00 00 a9 01 00 00 06 24 01 00 00 3c 02",
    "00 00 60 00 00 00 00 60 00 00 00 00 60 00 00 00 00 60 00 00 00 00 0d 11 02 5a 00 d6 01 1c 00",
);

/// How many bytes the block must be, or the client reads the next header out of the middle of it.
///
/// Evidence: the reference's reply is a 599-byte wire frame - two of header, four of request id, two
/// of count, then this (`docs/port/02-wire.md`).
pub const LEN: usize = 591;

/// The block, decoded.
///
/// An empty slice means the literal above is not hexadecimal, which the tests catch rather than a
/// client: a zero-length settings message is a connection the client cannot recover.
pub fn block() -> Vec<u8> {
    common::utilities::hex_bytes(BLOCK_HEX).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_is_five_hundred_and_ninety_one_bytes() {
        let settings = block();
        assert_eq!(settings.len(), LEN);
        // The first bytes, which a capture shows and a transposition would move.
        assert_eq!(&settings[..4], &[0x08, 0x00, 0x66, 0x21]);
    }

    #[test]
    fn the_literal_decodes_rather_than_falling_back_to_nothing() {
        assert!(!block().is_empty());
        assert!(common::utilities::hex_bytes(BLOCK_HEX).is_ok());
    }
}
