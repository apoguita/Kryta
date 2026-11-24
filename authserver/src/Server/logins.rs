//! The accounts logged in on this program: one connection per account, ever.
//!
//! Only one login per account is permitted. A new login closes the connection that held the account
//! before it, so only one client has an account in use (owner, 2026-11-23;
//! `docs/port/20-open-issues.md` item 10c). This is the login connection's half of that: each
//! logged-in connection holds its account here, a later login for the same account takes it over,
//! and the earlier connection sees that it has been displaced and closes.
//!
//! What a displaced client is sent: nothing. Retail tells it why - `ENetError` 0x23, "another client
//! has connected using your account" (`research/Headquarter/code/client/error.c:33`,
//! `research/Eden/docs/re/auth-messages-atoc.md`) - but no project and no recording shows the
//! message that carries that code to a client that asked nothing, so the connection is closed
//! without one and the client reports the loss in its own words. OpenTyria, the one source that
//! meets the case, refuses the new login instead and leaves the kick as a note
//! (`research/OpenTyria/code/OpenTyria/AuthSrv.c:792-796`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A connection's hold on its account. It turns true when a later login took the account.
pub type Hold = Arc<AtomicBool>;

/// Which connection holds each logged-in account.
#[derive(Default)]
pub struct Logins {
    held: Mutex<HashMap<[u8; 16], Hold>>,
}

impl Logins {
    /// No account is logged in.
    pub fn new() -> Self {
        Self::default()
    }

    /// A login for `account` on a connection whose earlier hold, if it has one, is `own`.
    ///
    /// The connection that held the account is displaced, unless it is this same connection logging
    /// in again. Answers the hold to keep, and whether another connection was displaced.
    pub fn take(&self, account: [u8; 16], own: Option<&Hold>) -> (Hold, bool) {
        let Ok(mut held) = self.held.lock() else {
            return (Arc::new(AtomicBool::new(false)), false);
        };
        if let (Some(current), Some(own)) = (held.get(&account), own)
            && Arc::ptr_eq(current, own)
        {
            return (Arc::clone(own), false);
        }
        let hold: Hold = Arc::new(AtomicBool::new(false));
        let displaced = held
            .insert(account, Arc::clone(&hold))
            .map(|earlier| earlier.store(true, Ordering::Release))
            .is_some();
        (hold, displaced)
    }

    /// A connection ended: its account is free again, unless a later login already took it.
    pub fn release(&self, account: [u8; 16], hold: &Hold) {
        if let Ok(mut held) = self.held.lock()
            && held
                .get(&account)
                .is_some_and(|current| Arc::ptr_eq(current, hold))
        {
            held.remove(&account);
        }
    }

    /// Whether a connection is logged in to `account` now.
    #[cfg(test)]
    pub fn is_logged_in(&self, account: &[u8; 16]) -> bool {
        self.held
            .lock()
            .is_ok_and(|held| held.contains_key(account))
    }
}

/// Whether a later login took the account this hold was for.
pub fn displaced(hold: &Hold) -> bool {
    hold.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: [u8; 16] = [0x11; 16];

    #[test]
    fn a_new_login_displaces_the_connection_that_held_the_account() {
        let logins = Logins::new();
        let (first, displaced_someone) = logins.take(ACCOUNT, None);
        assert!(!displaced_someone);
        assert!(logins.is_logged_in(&ACCOUNT));

        let (second, displaced_someone) = logins.take(ACCOUNT, None);
        assert!(displaced_someone);
        assert!(displaced(&first));
        assert!(!displaced(&second));

        let (_, displaced_someone) = logins.take([0x22; 16], None);
        assert!(!displaced_someone, "another account is another matter");
        assert!(!displaced(&second));
    }

    #[test]
    fn a_connection_logging_in_again_keeps_its_own_hold() {
        let logins = Logins::new();
        let (hold, _) = logins.take(ACCOUNT, None);
        let (again, displaced_someone) = logins.take(ACCOUNT, Some(&hold));
        assert!(!displaced_someone);
        assert!(Arc::ptr_eq(&hold, &again));
        assert!(!displaced(&hold));
    }

    #[test]
    fn a_displaced_connection_ending_does_not_free_the_account() {
        let logins = Logins::new();
        let (first, _) = logins.take(ACCOUNT, None);
        let (second, _) = logins.take(ACCOUNT, None);
        logins.release(ACCOUNT, &first);
        assert!(logins.is_logged_in(&ACCOUNT), "the later login still holds it");
        logins.release(ACCOUNT, &second);
        assert!(!logins.is_logged_in(&ACCOUNT));
    }
}
