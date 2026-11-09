//! Schema versioning: the fetch, the apply, and the record of both.
//!
//! Reference: `src/server/database/Updater/` - `UpdateFetcher.{h,cpp}` and `DBUpdater.{h,cpp}`.
//!
//! Ported by subject, and this is the half that needs no database: which files exist, what state
//! each one is in, the order they are applied in, and how one is read. The other half - asking the
//! database which files it already has, applying the rest, and recording the result - arrives with
//! the connection.
//!
//! Two behaviours here are the reference's and both are load bearing:
//!
//! - **A `.sql` file's name is its order.** The list is sorted by file name, never by the path it
//!   was found under, so a file's name is its place in the sequence.
//! - **A repeated file name is fatal.** Because the name is the order, two files sharing one name
//!   have no defined order, and the reference stops rather than guess.
//!
//! The tree is walked to a fixed depth, which is the reference's own limit.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// How deep the walk goes.
///
/// Reference: `FillFileListRecursively`'s `MAX_DEPTH`.
pub const MAX_DEPTH: u32 = 10;

/// What a file's position in the sequence means.
///
/// Reference: `UpdateFetcher::State`, whose five names are also the strings the reference records
/// in its schema's own history table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UpdateState {
    /// A file shipped with the release.
    Released,
    /// A file this machine's operator added.
    Custom,
    /// A file shipped after the release.
    Pending,
    /// A file from a module.
    Module,
    /// A file that has left the release.
    Archived,
}

impl UpdateState {
    /// The word the history table holds.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Released => "RELEASED",
            Self::Custom => "CUSTOM",
            Self::Pending => "PENDING",
            Self::Module => "MODULE",
            Self::Archived => "ARCHIVED",
        }
    }
}

impl std::str::FromStr for UpdateState {
    type Err = std::convert::Infallible;

    /// Read the word the history table holds.
    ///
    /// Reference: `AppliedFileEntry::StateConvert`, whose fallback is `ARCHIVED` for anything it
    /// does not recognise - which is why this cannot fail.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(match name {
            "RELEASED" => Self::Released,
            "CUSTOM" => Self::Custom,
            "PENDING" => Self::Pending,
            "MODULE" => Self::Module,
            _ => Self::Archived,
        })
    }
}

/// One file in the sequence, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UpdateFile {
    /// The file itself.
    pub path: PathBuf,
    /// What its position means.
    pub state: UpdateState,
}

impl UpdateFile {
    /// The name the order is decided by.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Why a list or a read failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Two files share a name, and the name is the order.
    DuplicateFilename(PathBuf),
    /// The directory named as a source is not one.
    MissingDirectory(PathBuf),
    /// A file could not be read.
    Unreadable(PathBuf),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateFilename(path) => write!(
                f,
                "Duplicate filename \"{}\" occurred. Because updates are ordered by their filenames, every name needs to be unique!",
                path.display()
            ),
            Self::MissingDirectory(path) => {
                write!(f, "the update directory '{}' does not exist", path.display())
            }
            Self::Unreadable(path) => write!(
                f,
                "Failed to open the sql update \"{}\" for reading!",
                path.display()
            ),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Every `.sql` file under a directory, in the order the reference applies them.
///
/// Reference: `FillFileListRecursively` into a `LocaleFileStorage`, which is a set ordered by
/// `PathCompare` - the file name, nothing else.
pub fn collect_files(root: impl AsRef<Path>, state: UpdateState) -> Result<Vec<UpdateFile>, UpdateError> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Err(UpdateError::MissingDirectory(root.to_path_buf()));
    }

    let mut found: Vec<UpdateFile> = Vec::new();
    walk(root, state, 0, &mut found)?;

    found.sort_by_key(UpdateFile::name);

    // Because the order is the name, a repeated name is refused rather than sequenced.
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for file in &found {
        if seen.insert(file.name(), file.path.clone()).is_some() {
            return Err(UpdateError::DuplicateFilename(file.path.clone()));
        }
    }

    Ok(found)
}

fn walk(
    path: &Path,
    state: UpdateState,
    depth: u32,
    found: &mut Vec<UpdateFile>,
) -> Result<(), UpdateError> {
    let entries = std::fs::read_dir(path).map_err(|_| UpdateError::MissingDirectory(path.to_path_buf()))?;

    for entry in entries.flatten() {
        let entry_path = entry.path();

        if entry_path.is_dir() {
            if depth < MAX_DEPTH {
                walk(&entry_path, state, depth + 1, found)?;
            }
            continue;
        }

        if entry_path.extension().is_some_and(|extension| extension == "sql") {
            found.push(UpdateFile {
                path: entry_path,
                state,
            });
        }
    }

    Ok(())
}

/// One update file's text.
///
/// Reference: `ReadSQLUpdate`, which reads the whole file and treats a failure as fatal: the
/// reference stops the server rather than continue with a database it could not bring up to date.
pub fn read_sql_update(path: impl AsRef<Path>) -> Result<String, UpdateError> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|_| UpdateError::Unreadable(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-updater-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, relative: &str, text: &str) {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn only_sql_files_are_collected() {
        let dir = temp_dir("only-sql");
        write(&dir, "a.sql", "SELECT 1;");
        write(&dir, "b.txt", "not sql");
        write(&dir, "c.sql.bak", "not sql either");

        let files = collect_files(&dir, UpdateState::Pending).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name(), "a.sql");
    }

    #[test]
    fn the_walk_reaches_into_subdirectories() {
        let dir = temp_dir("nested");
        write(&dir, "one.sql", "-- one");
        write(&dir, "deeper/two.sql", "-- two");

        let files = collect_files(&dir, UpdateState::Archived).unwrap();
        assert_eq!(
            files.iter().map(UpdateFile::name).collect::<Vec<_>>(),
            vec!["one.sql", "two.sql"]
        );
        assert!(files.iter().all(|file| file.state == UpdateState::Archived));
    }

    #[test]
    fn the_order_is_the_file_name_and_not_the_path() {
        let dir = temp_dir("order");
        // The paths sort the other way round: z/ comes after a/ lexicographically only by name.
        write(&dir, "z/01-first.sql", "");
        write(&dir, "a/02-second.sql", "");

        let files = collect_files(&dir, UpdateState::Pending).unwrap();
        assert_eq!(
            files.iter().map(UpdateFile::name).collect::<Vec<_>>(),
            vec!["01-first.sql", "02-second.sql"]
        );
    }

    #[test]
    fn a_repeated_file_name_is_refused() {
        let dir = temp_dir("duplicate");
        write(&dir, "a/same.sql", "");
        write(&dir, "b/same.sql", "");

        let error = collect_files(&dir, UpdateState::Pending).unwrap_err();
        assert!(matches!(error, UpdateError::DuplicateFilename(_)));
        assert!(error.to_string().contains("every name needs to be unique"));
    }

    #[test]
    fn a_missing_directory_is_refused() {
        let dir = temp_dir("missing");
        let error = collect_files(dir.join("nope"), UpdateState::Pending).unwrap_err();
        assert!(matches!(error, UpdateError::MissingDirectory(_)));
    }

    #[test]
    fn a_file_is_read_whole() {
        let dir = temp_dir("read");
        write(&dir, "a.sql", "CREATE TABLE t (id INT);\n-- done\n");

        let text = read_sql_update(dir.join("a.sql")).unwrap();
        assert_eq!(text, "CREATE TABLE t (id INT);\n-- done\n");
    }

    #[test]
    fn a_file_that_is_not_there_is_an_error() {
        let dir = temp_dir("unreadable");
        let error = read_sql_update(dir.join("gone.sql")).unwrap_err();
        assert!(matches!(error, UpdateError::Unreadable(_)));
    }

    #[test]
    fn the_state_words_round_trip_and_anything_else_is_archived() {
        for state in [
            UpdateState::Released,
            UpdateState::Custom,
            UpdateState::Pending,
            UpdateState::Module,
            UpdateState::Archived,
        ] {
            let read: UpdateState = state.as_str().parse().unwrap();
            assert_eq!(read, state);
        }
        let unknown: UpdateState = "WHATEVER".parse().unwrap();
        assert_eq!(unknown, UpdateState::Archived);
    }
}
