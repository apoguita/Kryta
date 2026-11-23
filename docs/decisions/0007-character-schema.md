# 0007 - A character is a row in the character database

Status: accepted. It is this tree's own design: the reference's `characters` table is Azeroth's, and a
Tyrian character is a different animal. The owner may correct it, and a correction replaces this text.

## The question

The creation screen's confirm produces a character. Where does one live, and what does a row hold?

## The decision

A character is a row in `kryta_characters.characters`, created by
`data/sql/base/db_characters/characters.sql`. The columns are exactly the fields the messages this tree
already sends read, and nothing else:

| column | read by |
|---|---|
| `uuid`, `name` | the character list (`0x0007`) and the confirm's answer (`0x0188`) |
| `account_uuid` | the list, which asks for one account's characters, and the confirm, which knows the account from the gameplay connection's version message |
| `appearance` | the packed word the confirm carries and `0x0007`'s settings block repeats |
| `primary_profession`, `secondary_profession`, `unlocked_professions` | the profession messages |
| `campaign`, `level`, `is_pvp`, `helm_status` | the packed word in `0x0007`'s settings block |
| `gold`, `last_outpost`, `experience` | the gold and faction messages, and the settings block's outpost |

A column nobody loads does not exist yet. Skills, the unlocked-skill bitmap, the four completion
bitmaps, per-slot armour dyes, hero flags and inventory each arrive with the message that reads them.

## Why

1. **The reference's table is the wrong animal.** `race`, `class`, `playerBytes`, `equipmentCache`,
   `at_login` are WoW's; a Tyrian character has a profession pair, a campaign and an outpost. Copying the
   reference's columns would be copying Azeroth's content into a game that does not have it.
2. **The schema follows the messages.** The character list is the only reader today, and the blob it
   sends is `CharacterSettings_FromDbCharacter` (`GmChar.c`) - a 62-byte record of outpost, appearance,
   campaign, level, PvP flag, secondary profession, helm and up to five armour pieces. Every one of those
   has a column here; the pieces do not, because they are the character's item rows (`item_instance`).
3. **`account_uuid` is what makes a list an account's.** The login asks for the characters of the account
   it just resolved; the column is the join.

## What it costs

- A schema designed here rather than ported, which is the first time this tree has done that for content.
  It is recorded for that reason.
- A character's `uuid` is generated at the confirm and stored as text: the wire carries 16 bytes
  little-endian, and the text form is what a human reads in a query.
- The armour pieces are not columns. They are a bag's worth of `item_instance` rows, written at the
  confirm, and the settings block derives them, which is where the reference derives them too.

## What it does not decide

How a character enters a map: that is the entry rule in `game/src/Agents/Entry.rs`, over the world
pack's spawn points and arrivals.
