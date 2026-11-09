//! The cipher this client scrambles its connection with.
//!
//! Reference: `src/common/Cryptography/` holds the reference's primitives, and this is where this
//! server's belong. The algorithm is **RC4**, which is textbook rather than this project's design:
//! a 256-byte permutation built from the key, then a byte generator that swaps as it walks. It is
//! written out here rather than taken from a crate because the requirement is byte-exactness with
//! one specific cipher, and twenty lines plus the published test vectors pin that down.
//!
//! RC4 is broken as a cipher. It is here because the client speaks it and there is nothing to
//! negotiate - the reason is recorded with the protocol, not decided here.

/// The RC4 keystream state: a permutation of 256 bytes and two indices.
///
/// Reference: the state `arc4_setup` builds and `arc4_crypt` advances.
#[derive(Clone)]
pub struct Arc4 {
    state: [u8; 256],
    x: u8,
    y: u8,
}

impl Arc4 {
    /// Build the state from a key.
    ///
    /// The key schedule: the permutation starts as the identity, then every position is swapped
    /// with one chosen by the running sum of the key's bytes, cycling the key when it runs out.
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty(), "RC4 needs a key");

        let mut state = [0u8; 256];
        for (index, byte) in state.iter_mut().enumerate() {
            *byte = index as u8;
        }

        let mut j: u8 = 0;
        let mut key_index = 0usize;
        for index in 0..256usize {
            if key_index >= key.len() {
                key_index = 0;
            }

            let at_index = state[index];
            j = j.wrapping_add(at_index).wrapping_add(key[key_index]);
            state[index] = state[j as usize];
            state[j as usize] = at_index;
            key_index += 1;
        }

        Self { state, x: 0, y: 0 }
    }

    /// The next keystream byte, advancing the state.
    pub fn keystream_byte(&mut self) -> u8 {
        self.x = self.x.wrapping_add(1);
        let at_x = self.state[self.x as usize];

        self.y = self.y.wrapping_add(at_x);
        let at_y = self.state[self.y as usize];

        self.state[self.x as usize] = at_y;
        self.state[self.y as usize] = at_x;

        self.state[at_x.wrapping_add(at_y) as usize]
    }

    /// Scramble a buffer in place.
    ///
    /// A stream cipher's encryption and decryption are the same operation, so there is one method
    /// and no way to reach for the wrong one.
    pub fn apply(&mut self, buffer: &mut [u8]) {
        for byte in buffer.iter_mut() {
            *byte ^= self.keystream_byte();
        }
    }

    /// The same keystream position, as a second instance.
    ///
    /// One connection has two ciphers keyed identically and advanced separately, one per direction,
    /// which is what the reference's server does when it copies its encrypting context into the
    /// decrypting one at the moment the key is exchanged.
    pub fn fork(&self) -> Self {
        self.clone()
    }
}

impl std::fmt::Debug for Arc4 {
    /// Never print the state: it is the keystream's position, and a trace logs everything.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Arc4(<state hidden>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrambled(key: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let mut cipher = Arc4::new(key);
        let mut buffer = plaintext.to_vec();
        cipher.apply(&mut buffer);
        buffer
    }

    #[test]
    fn the_published_vector_for_the_key_key() {
        // Key "Key", plaintext "Plaintext": the classic vector.
        assert_eq!(
            scrambled(b"Key", b"Plaintext"),
            vec![0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]
        );
    }

    #[test]
    fn the_published_vector_for_the_key_wiki() {
        // Key "Wiki", plaintext "pedia".
        assert_eq!(
            scrambled(b"Wiki", b"pedia"),
            vec![0x10, 0x21, 0xBF, 0x04, 0x20]
        );
    }

    #[test]
    fn the_published_vector_for_a_five_byte_key_of_zero_plaintext() {
        // Key 01 02 03 04 05 over sixteen zero bytes.
        assert_eq!(
            scrambled(&[1, 2, 3, 4, 5], &[0u8; 16]),
            vec![
                0xB2, 0x39, 0x63, 0x05, 0xF0, 0x3D, 0xC0, 0x27, 0xCC, 0xC3, 0x52, 0x4A, 0x0A, 0x11,
                0x18, 0xA8
            ]
        );
    }

    #[test]
    fn applying_the_same_keystream_twice_restores_the_plaintext() {
        let key = [7u8; 20];
        let plaintext = b"a message the client would send";
        let scrambled_once = scrambled(&key, plaintext);

        let mut cipher = Arc4::new(&key);
        let mut buffer = scrambled_once.clone();
        cipher.apply(&mut buffer);

        assert_eq!(&buffer, plaintext);
    }

    #[test]
    fn a_forked_cipher_carries_on_from_the_same_position() {
        let mut first = Arc4::new(b"key");
        let mut second = first.fork();

        let mut a = [0u8; 8];
        let mut b = [0u8; 8];
        first.apply(&mut a);
        second.apply(&mut b);

        assert_eq!(a, b);
    }

    #[test]
    fn a_key_longer_than_the_state_cycles() {
        let long_key = [0xABu8; 300];
        let mut cipher = Arc4::new(&long_key);
        let mut buffer = [0u8; 4];
        cipher.apply(&mut buffer);
        assert_ne!(buffer, [0, 0, 0, 0]);
    }
}
