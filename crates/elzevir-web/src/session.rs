// SPDX-License-Identifier: GPL-3.0-or-later
//! Staff sessions, held in memory. A restart signs everyone out; persisting sessions comes
//! with the institution lifecycle work (open on demand, close when idle).

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use elzevir_app::staff::StaffIdentity;

use crate::security::random_token;

/// Active staff sessions, keyed by an unguessable id stored in a cookie.
pub struct Sessions {
    active: Mutex<HashMap<String, Session>>,
    idle: Duration,
}

struct Session {
    identity: StaffIdentity,
    csrf: String,
    last_seen: Instant,
}

/// A live session as seen by a request.
#[derive(Debug, Clone)]
pub(crate) struct ActiveSession {
    pub(crate) identity: StaffIdentity,
    pub(crate) csrf: String,
}

impl Sessions {
    /// Sessions expire after `idle` without requests.
    #[must_use]
    pub fn new(idle: Duration) -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
            idle,
        }
    }

    /// Starts a session and returns its id.
    pub(crate) fn start(&self, identity: StaffIdentity) -> Result<String, getrandom::Error> {
        let id = random_token()?;
        let csrf = random_token()?;
        let mut active = self.lock();
        let now = Instant::now();
        active.retain(|_, s| now.duration_since(s.last_seen) < self.idle);
        active.insert(
            id.clone(),
            Session {
                identity,
                csrf,
                last_seen: now,
            },
        );
        Ok(id)
    }

    /// The session with `id`, if it exists and has not expired; refreshes its idle timer.
    pub(crate) fn touch(&self, id: &str) -> Option<ActiveSession> {
        let mut active = self.lock();
        let now = Instant::now();
        let session = active.get_mut(id)?;
        if now.duration_since(session.last_seen) >= self.idle {
            active.remove(id);
            return None;
        }
        session.last_seen = now;
        Some(ActiveSession {
            identity: session.identity.clone(),
            csrf: session.csrf.clone(),
        })
    }

    /// Ends a session.
    pub(crate) fn end(&self, id: &str) {
        self.lock().remove(id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        // A poisoned lock means a request panicked mid-update of one entry; the map is
        // still a valid map, and the worst case is one stale session.
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
