//! Who the portal says an account is, read from a value file.
//!
//! This is this project's own and it is temporary: a real identity store is a schema decision and
//! the login database has no account table. Until that decision, `data/accounts.conf` holds the pairs
//! a client can log in with, one per line:
//!
//! ```text
//! player@example.com = 0F5A2C1E-1111-2222-3333-444455556666 ; AAAAAAAA-0000-1111-2222-333344445555
//! ```
//!
//! The login name on the left, then the `user_id` and the `token` the client will carry. A file
//! nobody set up is not an error: it means no account can log in, and the portal says so with the
//! reference's own answer rather than inventing an identity.

use std::collections::BTreeMap;

use super::{Account, Accounts};

/// The accounts a client may log in as.
#[derive(Debug, Default, Clone)]
pub struct FileAccounts {
    entries: BTreeMap<String, Account>,
}

impl FileAccounts {
    /// Read the file. A missing file is an empty set, not a failure: it is the shape of a machine
    /// nobody has set up yet, and the portal's refusals already say what that means.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };

        let mut entries = BTreeMap::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((login_name, rest)) = line.split_once('=') else {
                return Err(format!(
                    "{}: line {} is not 'login_name = user_id ; token'",
                    path.display(),
                    index + 1
                ));
            };
            let Some((user_id, token)) = rest.split_once(';') else {
                return Err(format!(
                    "{}: line {} states a user id but no token",
                    path.display(),
                    index + 1
                ));
            };

            entries.insert(
                login_name.trim().to_string(),
                Account {
                    user_id: user_id.trim().to_string(),
                    token: token.trim().to_string(),
                },
            );
        }

        Ok(Self { entries })
    }

    /// The account behind a login name.
    pub fn get(&self, login_name: &str) -> Option<&Account> {
        self.entries.get(login_name)
    }

    /// How many accounts the file states.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the file states no account at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Accounts for FileAccounts {
    fn resolve(&self, login_name: &str) -> Option<Account> {
        self.entries.get(login_name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kryta-portal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("accounts.conf");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn an_account_file_is_read_and_resolved() {
        let path = temp_file(
            "one",
            "# a comment\nplayer@example.com = USER-1 ; TOKEN-1\n",
        );
        let accounts = FileAccounts::load(&path).unwrap();

        assert_eq!(accounts.len(), 1);
        let account = accounts.resolve("player@example.com").unwrap();
        assert_eq!(account.user_id, "USER-1");
        assert_eq!(account.token, "TOKEN-1");
        assert!(accounts.resolve("nobody@example.com").is_none());
    }

    #[test]
    fn a_missing_file_is_an_empty_set_rather_than_a_failure() {
        let dir = std::env::temp_dir().join(format!("kryta-portal-missing-{}", std::process::id()));
        let accounts = FileAccounts::load(dir.join("nothing.conf")).unwrap();
        assert!(accounts.is_empty());
    }

    #[test]
    fn a_line_without_a_token_is_refused_with_its_number() {
        let path = temp_file("bad", "player@example.com = USER-1\n");
        let error = FileAccounts::load(&path).unwrap_err();
        assert!(error.contains("line 1"), "{error}");
    }
}
