//! `game/Pack/` - the `Gw.dat` reader: the client's asset archive, and what is inside it.
//!
//! Reference: no directory of its own. OpenTyria reads this file with `FaArchive.c` and
//! `FaCompress.c`, two files of its one source folder (`code/OpenTyria/`) - but this tree's `common/`
//! is the library both programs link and a 3.6 GB archive is nobody's common dependency, so the reader
//! lives in the game library as the `Pack` noun.
//!
//! What it is for: a map's geometry - the path graph a walk is answered from - is a chunk inside one of
//! the archive's entries, and a map's file id (the number `0x0196` carries) is the way in. `Gwdat.rs` is
//! the index that turns a file id into an entry, and `FaCompress.rs` is the decoder that turns a
//! compressed entry into bytes (`FaCompress.c`'s `FaDecompress`, with its six static tables in
//! `FaCompressTables.rs`). Together they are the archive reader OpenTyria's `FaArchive.c` is; what is
//! built on top of them - the map blob's importer, the path tables and the finder - is `game/Terrain/`.

#[path = "FaCompress.rs"]
pub mod fa_compress;

#[path = "FaCompressTables.rs"]
pub mod fa_compress_tables;

#[path = "Gwdat.rs"]
pub mod gwdat;

pub use fa_compress::{DecompressError, decompress};
pub use gwdat::{Entry, GwDat, Header, MAGIC};
