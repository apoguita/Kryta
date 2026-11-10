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

## The file check

Before the version message, on the same socket, the client asks which game files it should have:
**21 bytes in, 32 bytes out**. It compares the whole answer, and treats a wrong or missing one as
fatal.

| | |
|---|---|
| the question | 21 bytes, `01 00 00 00 00 F1 00 10 00 01` then eleven zeros |
| the answer | 32 bytes: header `0x02F1`, length `0x0020`, then six file ids in tight consecutive order with a zero among them |

- Source of the shape: this client's protocol, recorded in `ASURA/docs/reference/client-protocol.md`
  section 7. The reference server has no equivalent - its client does not ask this - which is why
  `game/FileCheck/` exists in this tree's naming.
- In this tree: `data/file_check.conf` holds both byte strings as values, and
  `authserver/src/Server/file_check.rs` reads them and does the comparison. The bytes are not
  compiled in because they change when the client's own files change.
- Proved by: the shipped file's lengths (21 and 32) and header; a real socket sending the question
  and then the preamble on the same connection; and a question that only starts like it, which is
  refused rather than answered.
- What the ids are not: a rule. They are a client-build fact, refreshed by capturing the reply a
  live client accepts - never by arithmetic on the previous set.

A socket that does not answer this leaves the client at "Connecting" with nothing to report, which
is exactly what this server did to the client before the check existed.
## The portal, before the login message

The client is launched with `-portal 127.0.0.1`, speaks plain HTTP to `localhost:6601`, and takes the
pair its login message will carry from four endpoints. Between "the client reached the auth port" and
"the client can log in" this is the step, and it is a different protocol on a different port.

| # | request | answer |
|---|---|---|
| 1 | `GET /Spawned/WebGate/session/create.xml` | `<Reply><Session>UUID</Session></Reply>` |
| 2 | `POST /Spawned/WebGate/users/login.xml` | the `UserId`/`LoginName`/`Provider` block |
| 3 | `POST /Spawned/WebGate/my_account/game_accounts.xml` | a one-row array for the game code |
| 4 | `POST /Spawned/WebGate/my_account/token.xml` | `<Reply><Token>UUID</Token></Reply>` |

The session comes from call 1 and travels in `Authorization: Arena <session>`. The refusals are the
reference's own: a missing or unknown session is `401`, a provider that is not `Portal` is
`400 Unsupported provider`, and a login name nobody knows is `400 who are you?`.

- Source: `ASURA/docs/reference/client-protocol.md` section 4, measured with a real client at build
  39112, where all four answered `200` and the pair they returned is what the login message carried.
- In this tree: `game/src/Portal/`, which is one of the directories the naming adds because the
  reference has no equivalent. The arrangement is a request-to-response function with no socket in
  it, so the same code answers a test and a program.
- Not decided here: who an account is. `Accounts` is supplied by the caller, and the login database
  still has no account table - that is the identity question, and it is the owner's.
- Not built: the HTTP listener on 6601, and which program hosts it. `game/Portal/` is the code's home
  by this tree's naming; a third program is not in the naming, and the reference has two.
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

## The digest the connection is scrambled with, and what will prove it

After the server seed, both sides derive a 20-byte key from the master and run RC4 with it, one
instance per direction, both from offset zero.

- Source of the shape: recorded behaviour - five rounds over the five little-endian words of the
  master, from SHA-1's initial state, with its first two round constants and two f functions, and a
  digest that adds the working state back onto the input words. Recorded in
  `ASURA/docs/reference/client-protocol.md` section 3 and implemented there.
- In this tree: `authserver/src/Server/handshake.rs` (`derive_key`) and `common/src/Cryptography/`
  (`Arc4`). It is the one wire fact here that is **transcribed rather than derived**, because there
  is no other record of it and nothing in this repository can prove it: no capture holds a master
  and its digest, there is no published vector, and no other path would fail loudly if a rotation
  constant were wrong.
- What *is* checkable here: that it maps 20 bytes to 20, deterministically, and that a changed
  master changes the digest; and that a message encrypted with the derived key is read back through
  it as its own id.

**The client is the proof, and the log now says what it thinks.** The first thing the client sends
after the exchange is its login message, `0x8038`. This server decrypts the first bytes of it and
reports one of two things:

| the log line | what it means |
|---|---|
| `first encrypted message is 0x8038 ... the key derivation matches what this client expects` | the derivation is right, and the conversation can be built on it |
| `first encrypted message reads as 0xXXXX ... Either the key derivation does not match this client, or the message is not the one expected first` | the derivation, or the framing after it, is wrong - and the trace has the bytes to say which |

That is the whole reason the byte trace exists: a wrong digest is not a silent failure any more, it
is a log line with the bytes beside it.

## Where a fact goes when it is learned

A new message's header, size and layout: this file and the code that uses it, in the same commit,
with the capture or the document section named beside it. A fact with no evidence is a guess wearing
a constant's clothes, and the reference's own history has a list of what that costs.
