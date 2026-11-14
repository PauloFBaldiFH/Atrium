// SPDX-License-Identifier: GPL-3.0-or-later
//! Staff accounts, passwords and permissions (initial RBAC).
//!
//! Passwords are hashed with Argon2id. Login answers the same way, and takes about the
//! same time, whether the login is unknown or the password is wrong.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

/// Internal identifier of a staff account.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct StaffId(pub i64);

/// What a staff member may do. Starts small; grows with each module.
///
/// Koha: `userflags` and `permissions` tables; mapping them is part of the migrator.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// May log in to the staff interface at all.
    StaffInterface,
    /// May change institution settings and staff accounts.
    Administration,
    /// May add and edit records and Items.
    ///
    /// Koha: the `editcatalogue` permission.
    Cataloguing,
}

impl Permission {
    /// Stable name used in storage.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaffInterface => "staff_interface",
            Self::Administration => "administration",
            Self::Cataloguing => "cataloguing",
        }
    }

    /// Parses a stored name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "staff_interface" => Some(Self::StaffInterface),
            "administration" => Some(Self::Administration),
            "cataloguing" => Some(Self::Cataloguing),
            _ => None,
        }
    }
}

/// A stored staff account.
#[derive(Clone, PartialEq, Eq)]
pub struct StaffAccount {
    /// Internal id.
    pub id: StaffId,
    /// Login name, unique per institution.
    pub login: String,
    /// Argon2id hash in PHC string format.
    pub password_hash: String,
    /// Granted permissions.
    pub permissions: BTreeSet<Permission>,
}

impl fmt::Debug for StaffAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaffAccount")
            .field("id", &self.id)
            .field("login", &self.login)
            .field("permissions", &self.permissions)
            .finish_non_exhaustive()
    }
}

/// Port: where staff accounts are looked up.
pub trait StaffDirectory: Send + Sync {
    /// The account with this login, if any.
    ///
    /// # Errors
    ///
    /// Storage failures.
    fn by_login(&self, login: &str) -> Result<Option<StaffAccount>, StaffDirectoryError>;
}

/// A storage failure while looking up staff.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("staff directory unavailable: {0}")]
pub struct StaffDirectoryError(pub String);

/// An authenticated staff member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffIdentity {
    /// Internal id.
    pub id: StaffId,
    /// Login name.
    pub login: String,
    /// Granted permissions.
    pub permissions: BTreeSet<Permission>,
}

impl StaffIdentity {
    /// Whether this staff member holds `permission`.
    #[must_use]
    pub fn can(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

/// Why a login did not succeed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoginRefused {
    /// Unknown login or wrong password; deliberately indistinguishable.
    #[error("invalid login or password")]
    InvalidCredentials,
    /// Correct credentials, but the account may not use the staff interface.
    #[error("this account may not use the staff interface")]
    NotStaff,
    /// The directory failed.
    #[error(transparent)]
    Unavailable(#[from] StaffDirectoryError),
}

/// Hash compared against when the login is unknown, so that response time does not reveal
/// which logins exist.
static DECOY_HASH: LazyLock<Option<String>> = LazyLock::new(|| hash_password("decoy").ok());

/// Checks `login` and `password` and returns the staff identity.
///
/// # Errors
///
/// See [`LoginRefused`].
pub fn log_in(
    directory: &dyn StaffDirectory,
    login: &str,
    password: &str,
) -> Result<StaffIdentity, LoginRefused> {
    let Some(account) = directory.by_login(login)? else {
        if let Some(decoy) = DECOY_HASH.as_deref() {
            // Spend the same effort as a real check; the result is irrelevant.
            let _same_cost = verify_password(password, decoy);
        }
        return Err(LoginRefused::InvalidCredentials);
    };
    if !verify_password(password, &account.password_hash) {
        return Err(LoginRefused::InvalidCredentials);
    }
    if !account.permissions.contains(&Permission::StaffInterface) {
        return Err(LoginRefused::NotStaff);
    }
    Ok(StaffIdentity {
        id: account.id,
        login: account.login,
        permissions: account.permissions,
    })
}

/// Hashes a password with Argon2id (default parameters) and a random salt.
///
/// # Errors
///
/// Fails only if the system random source is unavailable.
pub fn hash_password(password: &str) -> Result<String, PasswordHashError> {
    let hash: PasswordHash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| PasswordHashError(e.to_string()))?;
    Ok(hash.to_string())
}

/// Hashing a password failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot hash password: {0}")]
pub struct PasswordHashError(pub String);

/// Whether `password` matches `hash`. A malformed hash never matches.
#[must_use]
pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneAccount(StaffAccount);

    impl StaffDirectory for OneAccount {
        fn by_login(&self, login: &str) -> Result<Option<StaffAccount>, StaffDirectoryError> {
            Ok((login == self.0.login).then(|| self.0.clone()))
        }
    }

    fn directory(permissions: &[Permission]) -> OneAccount {
        OneAccount(StaffAccount {
            id: StaffId(7),
            login: "ana".to_owned(),
            password_hash: hash_password("correct horse").unwrap(),
            permissions: permissions.iter().copied().collect(),
        })
    }

    #[test]
    fn correct_credentials_log_in() {
        let dir = directory(&[Permission::StaffInterface]);
        let who = log_in(&dir, "ana", "correct horse").unwrap();
        assert_eq!(who.id, StaffId(7));
        assert!(who.can(Permission::StaffInterface));
        assert!(!who.can(Permission::Administration));
    }

    #[test]
    fn wrong_password_and_unknown_login_look_the_same() {
        let dir = directory(&[Permission::StaffInterface]);
        assert_eq!(
            log_in(&dir, "ana", "wrong"),
            Err(LoginRefused::InvalidCredentials)
        );
        assert_eq!(
            log_in(&dir, "nobody", "correct horse"),
            Err(LoginRefused::InvalidCredentials)
        );
    }

    #[test]
    fn accounts_without_staff_permission_are_refused() {
        let dir = directory(&[]);
        assert_eq!(
            log_in(&dir, "ana", "correct horse"),
            Err(LoginRefused::NotStaff)
        );
    }

    #[test]
    fn hashes_are_argon2id_and_salted() {
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert!(a.starts_with("$argon2id$"));
        assert_ne!(a, b);
        assert!(verify_password("same", &a));
        assert!(!verify_password("same", "not a hash"));
    }

    #[test]
    fn permission_names_round_trip() {
        for p in [Permission::StaffInterface, Permission::Administration] {
            assert_eq!(Permission::from_name(p.as_str()), Some(p));
        }
    }

    #[test]
    fn debug_output_hides_the_password_hash() {
        let dir = directory(&[]);
        assert!(!format!("{:?}", dir.0).contains("argon2"));
    }
}
