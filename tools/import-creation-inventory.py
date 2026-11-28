"""Import the default inventory and the profession starting sets into Kryta's data tree.

The two sources are ASURA's data files, which are the reference's own numbers:

  * ``crates/asura-store/src/state/inventory/seed.rs`` - the six bags and nine items the reference
    creates for a character it loads;
  * ``crates/asura-world/src/default_equipment.rs`` - what a new character wears, per campaign and
    profession, read out of the reference's ``GetDefaultEquipments`` (``GmDefaultArmors.c``).

Both are Rust literals, so this script reads them rather than asking anyone to retype 200 items. It
writes two record lists that Kryta's own configuration reader loads:

  * ``config/flavors/Vanilla/World/creation/inventory.json`` - the bags, and the items that are not part of a profession's set;
  * ``config/flavors/Vanilla/World/creation/equipment.json`` - one set per (campaign, profession).

Usage: ``python tools/import-creation-inventory.py [path-to-ASURA]``
"""

import io
import json
import os
import re
import sys

DEFAULT_ASURA = r"C:\Users\Apo\Documents\GitHub\ASURA"

BAGS_SOURCE = "crates/asura-store/src/state/inventory/seed.rs"
ITEMS_SOURCE = "crates/asura-store/src/state/inventory/seed.rs"
EQUIPMENT_SOURCE = "crates/asura-world/src/default_equipment.rs"


def numbers(text):
    """Every integer literal in a Rust slice, in order: `&[0x21A8, 0xD157]` -> [8600, 53591]."""
    return [int(value, 0) for value in re.findall(r"0[xX][0-9A-Fa-f]+|\d+", text)]


def fields(text):
    """A struct literal's body as a dict of field -> raw value text.

    Split on **top-level** commas only: a field's value may itself hold commas, and `name` and
    `modifiers` are slices - `&[0x21A8, 0xD157, 0xB58F, 0x166F]`. Splitting on every comma cut
    those lists to their first element, which truncated every item's name and modifiers on the
    wire and made `CREATE_NAMED_ITEM` differ from the reference's.
    """
    found = {}
    depth = 0
    current = ""
    parts = []
    for character in text:
        if character in "[({":
            depth += 1
        elif character in "])}":
            depth -= 1
        if character == "," and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += character
    parts.append(current)
    for part in parts:
        if ":" not in part:
            continue
        key, value = part.split(":", 1)
        found[key.strip()] = value.strip()
    return found


def structs(text, name):
    """Every `Name { ... }` literal in `text`, as a dict of field -> raw value text."""
    return [
        fields(match.group(1))
        for match in re.finditer(name + r"\s*\{([^}]*)\}", text, re.S)
    ]


def item_record(fields):
    """One item, from a `SeedItem` or an `ItemSpec` literal, in the field order the wire uses."""
    def number(key, default=0):
        value = fields.get(key, str(default))
        value = re.sub(r"^Some\((.*)\)$", r"\1", value)
        return int(value, 0) if value not in ("None", "") else None

    name = numbers(fields.get("name", "&[]"))
    modifiers = numbers(fields.get("modifiers", "&[]"))

    record = {
        "file_id": number("file_id"),
        "item_type": number("item_type"),
        "dye_tint": number("dye_tint"),
        "dye_colors": number("dye_colors"),
        "materials": number("materials"),
        "unk1": number("unk1"),
        "flags": number("flags"),
        "value": number("value"),
        "model_id": number("model_id"),
        "quantity": number("quantity"),
        "profession": number("profession"),
        "name": name,
        "modifiers": modifiers,
    }
    if "id" in fields:
        record["id"] = number("id")
    if "bag_id" in fields:
        record["bag_id"] = number("bag_id")
        record["slot"] = number("slot")
    return record


def main():
    asura = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_ASURA
    if not os.path.isdir(asura):
        print("no ASURA checkout at", asura)
        return 1

    with io.open(os.path.join(asura, BAGS_SOURCE), encoding="utf-8") as handle:
        seed = handle.read()
    bags_text = seed.split("pub const BAGS")[1].split("];")[0]
    items_text = seed.split("pub const ITEMS")[1].split("];")[0]

    bags = []
    for fields in structs(bags_text, "SeedBag"):
        bags.append(
            {
                "bag_id": int(fields["bag_id"], 0),
                "model": int(fields["bag_model_id"], 0),
                "type": int(fields["bag_type"], 0),
                "slots": int(fields["slot_count"], 0),
                "assoc_item": int(fields["assoc_item"], 0),
            }
        )

    items = [item_record(fields) for fields in structs(items_text, "SeedItem")]

    inventory = {
        "note": "inventory.json - the bags and the items a new character's client is given, imported from ASURA's seed rows by tools/import-creation-inventory.py.",
        "information": {
            "what a row means": "a bag is a row of the client's inventory model: its id, its model, its kind and its slot count. An item carries the file id the client draws, its kind, its dyes, its flags and its name as UTF-16 units.",
            "where the numbers come from": "ASURA crates/asura-store/src/state/inventory/seed.rs, whose rows came from a capture of the reference at its parity point.",
            "why items and not nothing": "a client given no items has no inventory model, and it dies drawing the character it has just made.",
        },
        "bags": bags,
        "items": items,
    }

    with io.open(os.path.join(asura, EQUIPMENT_SOURCE), encoding="utf-8") as handle:
        equipment_text = handle.read()
    body = equipment_text.split("pub const EQUIPMENT")[1]

    sets = []
    for match in re.finditer(r"\(\s*(\d+),\s*(\d+),\s*&\[(.*?)\]\s*,?\s*\)", body, re.S):
        campaign = int(match.group(1))
        profession = int(match.group(2))
        specs = [item_record(fields) for fields in structs(match.group(3), "ItemSpec")]
        if specs:
            sets.append({"campaign": campaign, "profession": profession, "items": specs})

    equipment = {
        "note": "equipment.json - what a new character wears per campaign and profession, imported from ASURA's generated table by tools/import-creation-inventory.py.",
        "information": {
            "what a row means": "a set is one (campaign, profession) pair and the items the creation screen puts on a character of it. The item fields are the wire's own order, which is what GAME_SMSG_CREATE_NAMED_ITEM writes.",
            "where the numbers come from": "the reference's GetDefaultEquipments (GmDefaultArmors.c), generated into ASURA crates/asura-world/src/default_equipment.rs.",
        },
        "sets": sets,
    }

    with io.open("config/flavors/Vanilla/World/creation/inventory.json", "w", encoding="utf-8", newline="\n") as handle:
        json.dump(inventory, handle, indent=2)
        handle.write("\n")
    with io.open("config/flavors/Vanilla/World/creation/equipment.json", "w", encoding="utf-8", newline="\n") as handle:
        json.dump(equipment, handle, indent=2)
        handle.write("\n")

    print("bags:", len(bags), "items:", len(items), "sets:", len(sets),
          "set items:", sum(len(entry["items"]) for entry in sets))
    return 0


if __name__ == "__main__":
    sys.exit(main())
