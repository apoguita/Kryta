# 0004 - Gameplay connections arrive at the front door

Status: accepted, taken on a measurement, and landed: `authserver/src/Server/front_door.rs` and
`shared/src/Network/Relay.rs`. The owner may overrule it; if he does, this record is corrected rather
than extended.

## The question

After a client is told where to play, it opens a second connection and speaks the game protocol on
it. In AzerothCore that connection dials the world program's own port, because the client is told an
address and follows it: two programs, two ports, and the world program's listener in
`game/Server/WorldSocketMgr` is where a player's session begins.

This client does not follow. Measured on 2026-11-10 against this server, build 39167: the auth
program answered `0x8029` with a `0x0009` naming `127.0.0.1:6113`, a port with a listener on it that
accepted nothing; the gameplay connection arrived on **6112**, the address the client was launched
with, 35 ms later. The older measurement of this client (build 39112) recorded the same thing.

So the reference's two-port arrangement cannot be taken literally for this connection: one address
receives both kinds, and something has to hand the gameplay one on.

## The decision

The auth program is the front door. When a `0x000C0500` connection arrives on its port, it copies the
bytes to the world program's registered address and back, and the world program's own listener is
still where the game protocol lives.

The world address comes from the realm list's row (`realmlist.address` and `port`), which is the same
row the `0x0009` is built from, so the front door and the answer cannot disagree about where a world
is.

## Why this shape and not the alternatives

1. **It is GWPS's own arrangement.** GWPS's auth service (the Guild Wars server ASURA was measured
   against, not AzerothCore) receives both kinds of connection on one port and hands the gameplay one
   to a game server; its `0x0009` names its own address for exactly that reason. Two programs, one
   front door, the game protocol in the game server - this is what GWPS does, not an invention here.
2. **Naming the launch address in `0x0009` and answering gameplay on the auth port fuses the halves.**
   That is the arrangement this project exists to avoid: the auth program would own a world socket, a
   session, a map and a load block, and the separation between the two programs would stop meaning
   anything.
3. **The world program keeps its listener.** `game/Server/` still starts the sockets, so a future
   deployment where a client does follow an address costs one branch in the front door and nothing
   else, and the world program is not a library the auth program calls.
4. **A copy is what is possible across a process boundary.** A socket handle cannot be passed between
   these two processes; the reference passes one between its own. Copying bytes is the crossing that
   does not pretend otherwise, and it leaves everything the client sees identical to the reference's
   sequence.

## What the front door is not

It is a router, not an authority. It does not decode the game protocol, it does not decide who may
play, and it must not: the world program reads the 64-byte version message itself, compares the token
pair against what it was told about the travel, and closes the connection when they do not match. A
front door that started deciding would be the game library in the wrong program.

It reads those 64 bytes only to learn which world the client named, and forwards them byte for byte,
because the world decodes them again and that decode is the authoritative one.

## What it costs

The transport between the two programs is this tree's own: GWPS has a control channel
(`0x000C0FFF` connections and `CtrlMsgId_TransferConnection`) and this does not. The front door
forwards the client's 64 bytes unchanged, and a travel's token pair reaches the world as a `travel` row
the login writes and the world claims (`game/src/Maps/Travels.rs`), rather than by GWPS's control
message. If the control channel is ever ported, this crossing is what it replaces.

Costs accepted: a copy per gameplay connection, a connect timeout of 3 s when a world cannot be
reached, and the world's address being configuration the two programs must agree on rather than
something the control channel negotiates.

## What it does not decide

- Whether the world program's listener binds to the address the realm list publishes, or to all
  interfaces: that is the world program's own configuration (`BindIP`, `WorldServerPort`).
- What a world needs to exist before it can accept a connection. That is the next question, and it is
  about content rather than transport.
