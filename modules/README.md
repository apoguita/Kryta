# modules

Out-of-tree code, one directory per module, named `mod-*`. The reference's `modules/` holds
`mod-playerbots`, `mod-ale`, `mod-aoe-loot` and six more.

A module is a crate that links `game` and `scripts` and is listed in `modules/*` of the root
`Cargo.toml`. That member line arrives with the first module, because cargo refuses a glob
that matches nothing.

Nothing is here yet.