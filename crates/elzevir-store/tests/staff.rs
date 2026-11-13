// SPDX-License-Identifier: GPL-3.0-or-later
//! Staff accounts through the real command handler and directory.
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use elzevir_app::commands::{StaffCommand, WriteCommand, WriteOutput, WriteRefusal};
use elzevir_app::staff::{LoginRefused, Permission, StaffDirectory, hash_password, log_in};
use elzevir_store::{CommandFailed, ElzevirHandler, ElzevirStore, StoreConfig};

fn create(login: &str, password: &str, permissions: &[Permission]) -> WriteCommand {
    WriteCommand::Staff(StaffCommand::Create {
        login: login.to_owned(),
        password_hash: hash_password(password).unwrap(),
        permissions: permissions.iter().copied().collect(),
    })
}

fn open(dir: &tempfile::TempDir) -> ElzevirStore {
    ElzevirStore::open(
        &dir.path().join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .unwrap()
}

#[test]
fn created_staff_can_log_in_and_logins_are_unique() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);

    let created = store
        .submit(create("ana", "s3cret", &[Permission::StaffInterface]))
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(created, WriteOutput::StaffCreated(_)));

    let duplicate = store.submit(create("ana", "other", &[])).unwrap().wait();
    assert_eq!(
        duplicate,
        Err(CommandFailed::Rejected(WriteRefusal::LoginTaken(
            "ana".to_owned()
        )))
    );

    let account = store.by_login("ana").unwrap().unwrap();
    assert_eq!(account.permissions, [Permission::StaffInterface].into());
    assert!(store.by_login("nobody").unwrap().is_none());

    let who = log_in(&store, "ana", "s3cret").unwrap();
    assert_eq!(who.login, "ana");
    assert_eq!(
        log_in(&store, "ana", "other"),
        Err(LoginRefused::InvalidCredentials)
    );
}

#[test]
fn the_audit_log_never_holds_a_plain_password() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store
        .submit(create(
            "bia",
            "plain-text-password",
            &[Permission::StaffInterface],
        ))
        .unwrap()
        .wait()
        .unwrap();
    let logged: String = store
        .readers()
        .read(|c| c.query_row("SELECT command FROM audit_log", [], |r| r.get(0)))
        .unwrap();
    assert!(logged.contains("staff_interface"));
    assert!(logged.contains("$argon2id$"));
    assert!(!logged.contains("plain-text-password"));
}
