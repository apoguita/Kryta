# 0017 - A map's row in the client is stated in the pack

Status: accepted (owner, 2026-11-27). The first slice is built and was run in a client by the owner
on 2026-11-27: the hook installed on a table of 898 rows and map 148 was answered with a stated row
(`Kryta.27552.log`).

## The question

The client knows a map by a row of a table compiled into it: the map's kind, campaign, region,
party sizes, world-map place, name id. A server that hosts a map of its own cannot change that row,
so the client shows the stock map's row over the new terrain. Where is a map's row stated, and how
does the client come to use it?

## The decision

A map's row is stated in the map's record, `client_row` in `basic.json`, as any of the row's fields
by name. The login program sends the stated rows over the client link, and `Kryta.dll` replaces the
client's one lookup of that table, so that a stated map is answered with the client's own row with
the stated fields written over it. A field that is not stated keeps the client's value, and a map
that states nothing is the client's to answer.

The owner's words: "the map table, it's in the wrong place, it's unable to be modified easily,
that's why we have the json config files".

## Why this has no source

Rule 0 asks for a source, and there is none: no research project changes what the client holds for
a map. Each puts its map on a stock map id (`docs/port/24-custom-maps.md`, "The map's row in the
client", with the citations). This is therefore this tree's own design, decided by the owner, and
not a port. It is not a message of the game's protocol: nothing the world or login program sends
to the game client changes. It follows the shape of the title hook this tree already has
(`tools/client-dll/src/runtime/title_hooks.rs`, `metadata_detour`), which answers a lookup of the
same kind for rows the client lacks.

What is taken from sources: the row's field names and order (Py4GW Reforged Native,
`include/GW/context/map.h:87-118`), the byte pattern that finds the lookup (the same project's
`offsets/map.json`, `area_info_ref`), and the facts about the table
(`research/Eden/docs/re/area-info-provenance.md`, `research/Rurik/studies/maprows/FINDINGS.md`).
The lookup's address, the table's size and its place in build 39167 were measured here from the
client's file.

## What it does not decide

## A map's name is text in the pack (2026-11-28)

The row holds the id of a text in the client's own files. A map of the server's own states
`name` and `description` as text in `client_row` instead, and `Kryta.dll` replaces the id by the
text where the client hands a coded string to its decoder. The decoder's function, its pattern
and the coded form of text are ported from the owner's Py4GW Reforged Native (owner: "we can port
functionality"); standing in front of the decoder is this tree's own. Step R4 of
`docs/port/24-custom-maps.md`.

## A map of the server's own takes an id from 10000 up (owner, 2026-11-28)

Every one of the client's 898 rows is a map of the game's, so no stock id is free. The owner's
word: "just use 10,000 for starting map id". Such a map states its whole row, and states no
`instance_value`, which the client compares only for its own maps. The client refuses a map id
past its table in several places besides the lookup; `Kryta.dll` hooks each one a run reaches and
leaves such a map out of what that place keeps (step R5 of `docs/port/24-custom-maps.md`, with the
list of the places known). Run by the owner on 2026-11-28 with the test map as map 10000: it
loads, and travel to it and from it works.
- Whether the game's own maps should state their rows in the pack too. Today none does, and each is
  answered by the client.
