# The wire, as this tree knows it

A statement of today: which of this client's wire facts this server uses, where each came from, and
what is not written. It is not a plan and not a specification of its own - when it and a recording
disagree, the recording is right and this file is wrong.

Two rules govern every line here, and both come from `AGENTS.md`:

- The **arrangement** is the reference's: the byte buffer in `shared/src/Packets/`, the opcode table in
  the game library's `Protocol/`, one file per message family in `Packets/`.
- The **contents** are this client's, measured, and each fact names the document or capture that
  recorded it. None of it is Azeroth's protocol.

## The frame

After the handshake every message is a `u16` header and then its fields, with **no length field**:
a header whose bit 15 is set came from the client, and the header selects the field layout.

- Source: `ASURA/docs/reference/client-protocol.md` section 1, which corrects an earlier claim of a
  length field and names the three reference functions that show it.
- Why it matters: a port that writes a length the client does not expect gets error `007` and
  nothing else.

Not written: the opcode table itself. The reference keeps it in `game/Server/Protocol/Opcodes` and
the record of this client's headers reports 194 client-to-server and 488 server-to-client, with the
field layouts measured per message (`client-protocol.md` section 11.7). None of them is in this tree
yet, because the conversation that uses them is not written either.

## The first four bytes of a connection

The client's very first bytes are a bare little-endian `u32`, not a framed message. Decoding them as
one would read `0x0400` as a message id.

| header | what it is | first message size |
|---|---|---|
| `0x000C0400` | an auth connection | 16 |
| `0x000C0500` | a gameplay connection | 64 |
| `0x000C0FFF` | a control connection | 4 |

- Source: `ASURA/docs/reference/client-protocol.md` section 2.
- In this tree: `authserver/src/Authentication/`, whose `ConnectionKind::classify` answers one of the
  three and refuses anything else rather than guessing.
- Proved by: a real socket sending `0x000C0400` and being told what it is, and one sending
  `[EF, BE, AD, DE]` and being refused with its own bytes logged.

## The exchange

```text
client -> server   header 0x4200, then 64 bytes: the client's public value, little endian,
                   read as a 512-bit integer                            (66 bytes total)
server             shared = client_public ^ server_private mod prime   (512-bit prime, 511-bit
                   exponent; neither is chosen by the server)
server             20 random bytes: the master secret
server -> client   header 0x1601, then master[i] XOR shared_le[i]      (22 bytes total)
both               key = the client's own 20-byte digest of the master
both               RC4 keyed with those 20 bytes, one instance per direction, both from offset 0
```

- Source: `ASURA/docs/reference/client-protocol.md` section 3, whose sizes and constants come from
  the reference's handshake and the client's own arithmetic.
- In this tree: `authserver/src/Server/handshake.rs` (the sizes, the two constants as decimal
  digits, `shared_secret`, `server_seed`) and `common/src/Cryptography/Arc4.rs` (the cipher).
- Proved by: three published RC4 vectors; the constants' own bit widths; and a test that plays the
  client against a real listener and recovers the master out of the XOR.

Two deviations, both recorded where they are taken:

- a public value congruent to zero modulo the prime is **refused**, because the shared secret would
  be zero and the exchange would have exchanged nothing. The reference carries on with the zero; the
  real client cannot present one, so the refusal costs nothing.
- the master secret comes from the operating system's entropy, and a machine without any logs that
  instead of silently using zeros.

## What is not written, and why that is not caution

**The 20-byte digest the key is derived with.** It is a fixed, described algorithm - it borrows
SHA-1's initial values and runs a small number of rounds over the 20 input bytes, adding the working
state back onto them - and it is the one wire fact this tree has no way to check:

- no capture here holds a master secret and the digest a client produced from it;
- no published vector exists for it, unlike RC4;
- nothing else in the server depends on its output in a way that would fail loudly.

A transcription could be wrong in one rotation constant and every test in this repository would still
pass. Its only judge is a client, and its failure is an opaque `007` with no diagnostic in it. So it
lands with a check that can prove it, or with the owner's word that it is worth writing unverified.

Until then every connection says so on its own log line: the plaintext half of the exchange is
answered, and everything after it is scrambled with a key this server cannot yet derive.

## Where a fact goes when it is learned

A new message's header, size and layout: this file and the code that uses it, in the same commit,
with the capture or the document section named beside it. A fact with no evidence is a guess wearing
a constant's clothes, and the reference's own history has a list of what that costs.
