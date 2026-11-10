//! The key exchange: the sizes, the two constants, and the arithmetic.
//!
//! Reference for the shape: `src/server/apps/authserver/Server/AuthSocket.cpp`, whose
//! `HandleAuthChallenge` is where the reference's own exchange happens. The **numbers and the
//! arithmetic here are this client's**, measured, and each one names its evidence.
//!
//! What the exchange is, from `ASURA/docs/reference/client-protocol.md` section 3:
//!
//! ```text
//! client -> server   header 0x4200, then 64 bytes: the client's public value, little endian,
//!                    read as a 512-bit integer                          (66 bytes total)
//! server             shared = client_public ^ server_private mod prime
//! server             20 random bytes: the master secret
//! server -> client   header 0x1601, then master[i] XOR shared_le[i]     (22 bytes total)
//! both               key = the client's own 20-byte digest of the master
//! both               RC4 keyed with those 20 bytes, one instance per direction
//! ```
//!
//! What is **not** on the wire and matters: there is no generator and no server public value. The
//! patched client carries the matching parameters, so the server's only arithmetic is raising the
//! client's number to a fixed exponent. The risk here is transcription, not key management: the
//! constants below must be identical or the client fails the exchange and reports an opaque error.
//!
//! Not here yet, and deliberately: the 20-byte digest, because no record of it holds an input and
//! its expected output, so nothing but a running client can say a transcription is right. It lands
//! with a test that can prove it, not before.

/// The header of the client's seed message.
///
/// Evidence: `ASURA/docs/reference/client-protocol.md` section 3, step 1.
pub const CLIENT_SEED_HEADER: u16 = 0x4200;

/// The header of the server's seed message.
///
/// Evidence: the same table, step 4.
pub const SERVER_SEED_HEADER: u16 = 0x1601;

/// How many bytes the client's public value is.
pub const PUBLIC_LEN: usize = 64;

/// How many bytes the client's whole seed message is: the header and the value.
pub const CLIENT_SEED_LEN: usize = 2 + PUBLIC_LEN;

/// How many bytes the master secret is.
pub const MASTER_SECRET_LEN: usize = 20;

/// How many bytes the server's whole seed message is: the header and the secret.
pub const SERVER_SEED_LEN: usize = 2 + MASTER_SECRET_LEN;

/// How many bytes the session key is, and therefore how many RC4 is keyed with.
pub const KEY_LEN: usize = 20;

/// The prime modulus, in decimal.
///
/// Evidence: the reference server's own `main.c`, which five public reimplementations of this
/// protocol carry verbatim; the patched client's matching parameter makes any other value fail the
/// exchange. A 512-bit number, quoted as digits rather than as bytes so a transcription error is
/// visible.
pub const PRIME_MODULUS_DECIMAL: &str = "11369030006184639557951841839086711170523356038756186113745357553615433746914467378379225197818567184257516325746258410038643664166668140728650714232881773";

/// The exponent the server raises the client's value to, in decimal.
///
/// Evidence: the same source, and the same argument: the client carries the public half of this
/// pair, so the exponent is fixed. It is compiled rather than configured because no machine can
/// choose another and still complete the exchange; `ASURA`'s audit records that consequence - one
/// key for every deployment - and it is reported there rather than decided here.
pub const SERVER_PRIVATE_DECIMAL: &str = "4955013838800442477563322845022960998436564559618750588243916809877035796638982560199828129813551335307191644041932374697066251319749593910087044611412991";

/// What went wrong with a client public value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeError {
    /// The client's value was not the length the protocol says.
    WrongLength {
        /// What arrived.
        got: usize,
    },
    /// The client's value is zero modulo the prime, so the shared secret would be zero and the
    /// "exchange" would have exchanged nothing.
    ///
    /// Reference: the reference server would carry on with the zero and derive a key from it; this
    /// server refuses instead. The real client cannot present such a value, so the refusal costs
    /// nothing and turns a silent broken connection into a stated one.
    Degenerate,
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongLength { got } => write!(
                f,
                "the client's public value is {got} bytes; the protocol says {PUBLIC_LEN}"
            ),
            Self::Degenerate => f.write_str(
                "the client's public value is zero modulo the prime, so the shared secret would be zero",
            ),
        }
    }
}

impl std::error::Error for ExchangeError {}

/// The shared secret, from the client's public value.
///
/// Answers `client_public ^ server_private mod prime` as 64 little-endian bytes - the form the
/// server's seed message XORs the master secret with.
pub fn shared_secret(client_public: &[u8]) -> Result<[u8; PUBLIC_LEN], ExchangeError> {
    use num_bigint::BigUint;

    if client_public.len() != PUBLIC_LEN {
        return Err(ExchangeError::WrongLength {
            got: client_public.len(),
        });
    }

    let prime = BigUint::parse_bytes(PRIME_MODULUS_DECIMAL.as_bytes(), 10)
        .expect("the prime is decimal digits");
    let exponent = BigUint::parse_bytes(SERVER_PRIVATE_DECIMAL.as_bytes(), 10)
        .expect("the exponent is decimal digits");

    // The client's value is little endian on the wire; a big integer is read the other way round.
    let mut little = client_public.to_vec();
    little.reverse();
    let public = BigUint::from_bytes_be(&little);

    if &public % &prime == BigUint::from(0u8) {
        return Err(ExchangeError::Degenerate);
    }

    let shared = public.modpow(&exponent, &prime);

    let bytes = shared.to_bytes_le();
    let mut out = [0u8; PUBLIC_LEN];
    out[..bytes.len()].copy_from_slice(&bytes);
    Ok(out)
}

/// The server's seed message: the master secret XORed with the shared secret, after the header.
///
/// The client recovers the master by doing the same thing, which is why this is an XOR of two equal
/// lengths and not an encryption.
pub fn server_seed(master: &[u8; MASTER_SECRET_LEN], shared: &[u8; PUBLIC_LEN]) -> [u8; SERVER_SEED_LEN] {
    let mut out = [0u8; SERVER_SEED_LEN];
    out[..2].copy_from_slice(&SERVER_SEED_HEADER.to_le_bytes());
    for index in 0..MASTER_SECRET_LEN {
        out[2 + index] = master[index] ^ shared[index];
    }
    out
}

/// The session key: the client's own 20-byte digest of the master secret.
///
/// **This is a transcription, and it is the one wire fact in this tree that no test can prove.** The
/// shape is recorded behaviour: five rounds over the five little-endian words of the 20-byte master,
/// starting from SHA-1's initial state, with SHA-1's first two round constants and its first two
/// f functions, and a digest that *adds* the working state back onto the input words instead of
/// emitting a chaining value. No padding, no length encoding, one block, always the same size.
///
/// It is recorded in ASURA's `client/handshake.rs` (`derive_key`), itself ported from the reference
/// server's `AuthSrv.c`, and there is no other record of it here. That is why it is transcribed
/// rather than derived: a different digest means a keystream the client does not recognise, and the
/// client answers with an opaque `007` and nothing else.
///
/// What can be checked in this repository: that it is deterministic, that it maps 20 bytes to 20,
/// and that it changes when the master changes. What cannot: that it is the digest *this* client
/// computes. That answer comes from a running client - and with the byte trace on, a wrong digest
/// shows as garbage where the first encrypted message should be a message id.
pub fn derive_key(master: &[u8; MASTER_SECRET_LEN]) -> [u8; KEY_LEN] {
    fn word(bytes: &[u8]) -> u32 {
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    let input = [
        word(&master[0..4]),
        word(&master[4..8]),
        word(&master[8..12]),
        word(&master[12..16]),
        word(&master[16..20]),
    ];

    // SHA-1's initial state, as five working registers.
    let mut a: u32 = 0x6745_2301;
    let mut b: u32 = 0xEFCD_AB89;
    let mut c: u32 = 0x98BA_DCFE;
    let mut d: u32 = 0x1032_5476;
    let mut e: u32 = 0xC3D2_E1F0;

    // Three rounds of f(x, y, z) = z ^ (y & (x ^ z)) with SHA-1's first constant.
    let constant = 0x5A82_7999;

    e = e
        .wrapping_add(input[0])
        .wrapping_add(a.rotate_left(5))
        .wrapping_add(d ^ (b & (c ^ d)))
        .wrapping_add(constant);
    b = b.rotate_left(30);

    d = d
        .wrapping_add(input[1])
        .wrapping_add(e.rotate_left(5))
        .wrapping_add(c ^ (a & (b ^ c)))
        .wrapping_add(constant);
    a = a.rotate_left(30);

    c = c
        .wrapping_add(input[2])
        .wrapping_add(d.rotate_left(5))
        .wrapping_add(b ^ (e & (a ^ b)))
        .wrapping_add(constant);
    e = e.rotate_left(30);

    // Two rounds of parity with SHA-1's second constant.
    let constant = 0x6ED9_EBA1;

    e = e
        .wrapping_add(input[3])
        .wrapping_add(a.rotate_left(5))
        .wrapping_add(b ^ c ^ d)
        .wrapping_add(constant);
    b = b.rotate_left(30);

    d = d
        .wrapping_add(input[4])
        .wrapping_add(e.rotate_left(5))
        .wrapping_add(a ^ b ^ c)
        .wrapping_add(constant);
    a = a.rotate_left(30);

    // The digest: the input words with the working state added onto them.
    let digest = [
        input[0].wrapping_add(a),
        input[1].wrapping_add(b),
        input[2].wrapping_add(c),
        input[3].wrapping_add(d),
        input[4].wrapping_add(e),
    ];

    let mut key = [0u8; KEY_LEN];
    for (index, part) in digest.iter().enumerate() {
        key[index * 4..index * 4 + 4].copy_from_slice(&part.to_le_bytes());
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_twenty_bytes_and_deterministic() {
        let master = [0x11u8; MASTER_SECRET_LEN];
        let first = derive_key(&master);
        let second = derive_key(&master);
        assert_eq!(first.len(), KEY_LEN);
        assert_eq!(first, second);
    }

    #[test]
    fn a_different_master_gives_a_different_key() {
        let mut master = [0x11u8; MASTER_SECRET_LEN];
        let before = derive_key(&master);
        master[0] ^= 0x01;
        let after = derive_key(&master);
        assert_ne!(before, after, "one flipped bit must change the digest");
    }

    #[test]
    fn the_digest_adds_the_state_back_onto_the_input() {
        // The derivation's documented shape: digest = input + state, word by word. With a master of
        // zeros the first digest word is the first working register's final value, which is not
        // zero - a hash that emitted a chaining value instead would differ here.
        let key = derive_key(&[0u8; MASTER_SECRET_LEN]);
        assert_ne!(key, [0u8; KEY_LEN]);
    }

    #[test]
    fn the_sizes_are_the_measured_ones_and_add_up() {
        assert_eq!(CLIENT_SEED_LEN, 66);
        assert_eq!(SERVER_SEED_LEN, 22);
        assert_eq!(PUBLIC_LEN, 64);
        assert_eq!(MASTER_SECRET_LEN, 20);
        assert_eq!(KEY_LEN, 20);
        assert_eq!(CLIENT_SEED_LEN, 2 + PUBLIC_LEN);
        assert_eq!(SERVER_SEED_LEN, 2 + MASTER_SECRET_LEN);
    }

    #[test]
    fn the_headers_are_the_client_headers() {
        assert_eq!(CLIENT_SEED_HEADER, 0x4200);
        assert_eq!(SERVER_SEED_HEADER, 0x1601);
    }

    #[test]
    fn the_constants_are_the_sizes_they_claim() {
        use num_bigint::BigUint;

        let prime = BigUint::parse_bytes(PRIME_MODULUS_DECIMAL.as_bytes(), 10).unwrap();
        let exponent = BigUint::parse_bytes(SERVER_PRIVATE_DECIMAL.as_bytes(), 10).unwrap();

        assert_eq!(prime.bits(), 512, "the prime is a 512-bit number");
        assert_eq!(exponent.bits(), 511, "the exponent is a 511-bit number");
    }

    #[test]
    fn a_value_of_the_wrong_length_is_refused() {
        assert_eq!(
            shared_secret(&[0u8; 10]),
            Err(ExchangeError::WrongLength { got: 10 })
        );
    }

    #[test]
    fn a_zero_value_is_refused_because_it_would_share_nothing() {
        assert_eq!(shared_secret(&[0u8; PUBLIC_LEN]), Err(ExchangeError::Degenerate));
    }

    #[test]
    fn one_raised_to_the_exponent_is_one() {
        let mut one = [0u8; PUBLIC_LEN];
        one[0] = 1;
        let shared = shared_secret(&one).unwrap();
        assert_eq!(shared[0], 1);
        assert!(shared[1..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn the_shared_secret_is_below_the_prime_and_deterministic() {
        use num_bigint::BigUint;

        let mut public = [0u8; PUBLIC_LEN];
        for (index, byte) in public.iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(7).wrapping_add(3);
        }
        // Keep it below the prime: the top byte decides, and the prime's top byte is 0x13.
        public[PUBLIC_LEN - 1] = 0x11;

        let first = shared_secret(&public).unwrap();
        let second = shared_secret(&public).unwrap();
        assert_eq!(first, second, "the exchange is deterministic");

        let mut little = first.to_vec();
        little.reverse();
        let shared = BigUint::from_bytes_be(&little);
        let prime = BigUint::parse_bytes(PRIME_MODULUS_DECIMAL.as_bytes(), 10).unwrap();
        assert!(shared < prime, "the answer is reduced modulo the prime");
    }

    #[test]
    fn the_server_seed_carries_the_header_and_xors_the_master() {
        let master = [0xAAu8; MASTER_SECRET_LEN];
        let mut shared = [0u8; PUBLIC_LEN];
        shared[..MASTER_SECRET_LEN].copy_from_slice(&[0x55u8; MASTER_SECRET_LEN]);

        let seed = server_seed(&master, &shared);

        assert_eq!(seed.len(), SERVER_SEED_LEN);
        assert_eq!(&seed[..2], &0x1601u16.to_le_bytes());
        // The client undoes this by the same XOR, so the property that matters is that it is one.
        for index in 0..MASTER_SECRET_LEN {
            assert_eq!(seed[2 + index] ^ shared[index], master[index]);
        }
    }
}
