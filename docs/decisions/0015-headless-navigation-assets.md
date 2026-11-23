# 0015 - Headless navigation assets

## Status

Accepted - 2026-11-19.

## Decision

Kryta ships the extracted `data/path-graphs.bin` cache and `worldserver` reads only that portable
`ASURAPG1` file. It never opens `Gw.dat` and does not configure an archive path.

The cache is not a declaration that every map in the game must be pathable. It binds every world map
whose file id is present; an omitted stock or custom map has no graph and follows the existing
`NoGraph` path. This keeps future custom maps independent of the client archive.

The configuration manager owns inspection and explicit selected-map extraction. Its manager-only
`MapArchiveFile` is an offline import source, and its `PathGraphsFile` is the cache it updates. The
manager exposes `GET /api/navigation/maps` and `POST /api/navigation/dump`; neither operation belongs
to the world-server control endpoint.

## Reference and reason

ASURA's `dump_path_graphs` in `crates/asura-worldserver/src/main.rs:571-665` defines the portable
export. Its ordinary server startup may fall back to the archive at lines 366-391. Kryta differs here
because the owner requires a headless server separated from an installed game client.
