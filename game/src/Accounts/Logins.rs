//! One login per account, in the world: which login of each account this world is serving.
//!
//! Only one login per account is ever permitted, and a new login closes any old connection (owner,
//! 2026-11-23; `docs/port/20-open-issues.md` item 10c). The login program closes the login
//! connection it displaced (`authserver/src/Server/logins.rs`); this is the world's half - the
//! gameplay connections of the displaced client.
//!
//! The two programs do not talk to each other, so the world learns of a login the way it learns of
//! everything else from the login program: on the travel row (`game/src/Maps/Travels.rs`). Every
//! login draws a number, later logins a larger one, and every hand-off of that login carries it -
//! the ones the login program writes, and the ones this world writes when the same client travels
//! on. So a gameplay connection arrives knowing which login it belongs to:
//!
//! - the same login as the account's current one: the same client, travelling. Nothing is closed.
//! - a later login: another client took the account. Every connection of the earlier login closes.
//! - an earlier login: a displaced client reaching a map after it was displaced. It is closed.
//!
//! A connection with no travel row carries no login (the creation screen, or a program with no login
//! database) and is left out of this entirely.
//!
//! What a displaced client is sent: nothing, for the reason `authserver/src/Server/logins.rs` gives -
//! no source shows the message that carries retail's notice.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What the connections of one login share: it turns true when a later login took their account.
pub type Displaced = Arc<AtomicBool>;

/// The login each account is being served for.
#[derive(Default)]
pub struct AccountLogins {
    current: Mutex<HashMap<[u8; 16], (u64, Displaced)>>,
}

impl AccountLogins {
    /// No account is being served.
    pub fn new() -> Self {
        Self::default()
    }

    /// A gameplay connection for `account` arrived on a hand-off of login `login`.
    ///
    /// Answers the flag the connection watches. A connection of an earlier login than the account's
    /// current one is answered a flag that is already set.
    pub fn enter(&self, account: [u8; 16], login: u64) -> Displaced {
        let fresh = || Arc::new(AtomicBool::new(false));
        if login == 0 {
            return fresh();
        }
        let Ok(mut current) = self.current.lock() else {
            return fresh();
        };
        match current.get(&account) {
            Some((held, flag)) if *held == login => Arc::clone(flag),
            Some((held, _)) if *held > login => Arc::new(AtomicBool::new(true)),
            _ => {
                let flag = fresh();
                if let Some((_, earlier)) = current.insert(account, (login, Arc::clone(&flag))) {
                    earlier.store(true, Ordering::Release);
                }
                flag
            }
        }
    }
}

/// Whether a later login took the account this flag was answered for.
pub fn displaced(flag: &Displaced) -> bool {
    flag.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: [u8; 16] = [0x11; 16];

    #[test]
    fn a_client_travelling_keeps_its_connections() {
        let logins = AccountLogins::new();
        let first_map = logins.enter(ACCOUNT, 100);
        let second_map = logins.enter(ACCOUNT, 100);
        assert!(!displaced(&first_map));
        assert!(!displaced(&second_map));
    }

    #[test]
    fn a_later_login_closes_every_connection_of_the_earlier_one() {
        let logins = AccountLogins::new();
        let first_map = logins.enter(ACCOUNT, 100);
        let second_map = logins.enter(ACCOUNT, 100);
        let other_account = logins.enter([0x22; 16], 100);

        let newcomer = logins.enter(ACCOUNT, 200);
        assert!(displaced(&first_map));
        assert!(displaced(&second_map));
        assert!(!displaced(&newcomer));
        assert!(!displaced(&other_account), "another account is another matter");
    }

    #[test]
    fn a_displaced_client_reaching_a_map_is_closed_and_displaces_nobody() {
        let logins = AccountLogins::new();
        let newcomer = logins.enter(ACCOUNT, 200);
        let straggler = logins.enter(ACCOUNT, 100);
        assert!(displaced(&straggler));
        assert!(!displaced(&newcomer));
    }

    #[test]
    fn a_connection_with_no_login_is_left_out() {
        let logins = AccountLogins::new();
        let creation = logins.enter(ACCOUNT, 0);
        let played = logins.enter(ACCOUNT, 100);
        let another_creation = logins.enter(ACCOUNT, 0);
        assert!(!displaced(&creation));
        assert!(!displaced(&played));
        assert!(!displaced(&another_creation));
    }
}
