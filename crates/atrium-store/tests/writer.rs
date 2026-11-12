// SPDX-License-Identifier: GPL-3.0-or-later
//! Writer-thread behaviour (task 0.6): no `SQLITE_BUSY` under concurrency, the audit log
//! reproduces the state, rejected commands leave no trace, effects ride with their change.
#![expect(
    clippy::unwrap_used,
    reason = "integration test: helper functions fail the test on error"
)]

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use atrium_store::{CommandFailed, CommandHandler, Store, StoreConfig, SubmitError, WriteTx};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Command {
    Add { key: String, by: i64 },
    WriteThenRefuse { key: String },
    Sleep { millis: u64 },
}

#[derive(Debug, PartialEq, Eq)]
struct Refused;

struct Counters;

impl CommandHandler for Counters {
    type Command = Command;
    type Output = i64;
    type Rejection = Refused;

    fn kind(command: &Command) -> &'static str {
        match command {
            Command::Add { .. } => "add",
            Command::WriteThenRefuse { .. } => "write_then_refuse",
            Command::Sleep { .. } => "sleep",
        }
    }

    fn apply(
        &mut self,
        tx: &WriteTx<'_>,
        command: &Command,
    ) -> Result<i64, CommandFailed<Refused>> {
        match command {
            Command::Add { key, by } => {
                let value: i64 = tx
                    .connection()
                    .query_row(
                        "INSERT INTO counters (key, value) VALUES (?1, ?2)
                         ON CONFLICT (key) DO UPDATE SET value = value + excluded.value
                         RETURNING value",
                        params![key, by],
                        |row| row.get(0),
                    )
                    .map_err(|e| CommandFailed::Storage(e.into()))?;
                tx.emit_effect("counter_changed", &(key, value))?;
                Ok(value)
            }
            Command::WriteThenRefuse { key } => {
                tx.connection()
                    .execute(
                        "INSERT INTO counters (key, value) VALUES (?1, 1)",
                        params![key],
                    )
                    .map_err(|e| CommandFailed::Storage(e.into()))?;
                tx.emit_effect("counter_changed", key)?;
                Err(CommandFailed::Rejected(Refused))
            }
            Command::Sleep { millis } => {
                std::thread::sleep(Duration::from_millis(*millis));
                Ok(0)
            }
        }
    }
}

fn open(path: &Path, config: &StoreConfig) -> Store<Counters> {
    let setup = Connection::open(path).unwrap();
    setup
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS counters (key TEXT PRIMARY KEY, value INTEGER NOT NULL) STRICT",
        )
        .unwrap();
    drop(setup);
    Store::open(path, config, Counters).unwrap()
}

fn counters(store: &Store<Counters>) -> BTreeMap<String, i64> {
    store
        .readers()
        .read(|c| {
            let mut stmt = c.prepare("SELECT key, value FROM counters")?;
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<BTreeMap<_, _>, _>>()
        })
        .unwrap()
}

fn audit(store: &Store<Counters>) -> Vec<Command> {
    store
        .readers()
        .read(|c| {
            let mut stmt = c.prepare("SELECT command FROM audit_log ORDER BY seq")?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect()
}

#[test]
fn fifty_concurrent_writers_never_hit_busy_and_the_audit_log_replays_the_state() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(open(&dir.path().join("atrium.db"), &StoreConfig::default()));
    let start = Arc::new(Barrier::new(51));
    let done = Arc::new(AtomicBool::new(false));

    // A reader running the whole time must never see a lock error either.
    let reader = {
        let store = Arc::clone(&store);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut reads = 0_u32;
            while !done.load(Ordering::Relaxed) {
                store
                    .readers()
                    .read(|c| {
                        c.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))
                    })
                    .unwrap();
                reads += 1;
            }
            reads
        })
    };

    let writers: Vec<_> = (0..50_i64)
        .map(|writer| {
            let store = Arc::clone(&store);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                for step in 0..20_i64 {
                    let command = Command::Add {
                        key: format!("k{}", (writer + step) % 7),
                        by: writer * 100 + step,
                    };
                    store.submit(command).unwrap().wait().unwrap();
                }
            })
        })
        .collect();
    start.wait();
    for writer in writers {
        writer.join().unwrap();
    }
    done.store(true, Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0);

    let log = audit(&store);
    assert_eq!(log.len(), 1000);
    let mut replayed = BTreeMap::new();
    for command in log {
        let Command::Add { key, by } = command else {
            panic!("unexpected command in the log: {command:?}");
        };
        *replayed.entry(key).or_insert(0) += by;
    }
    assert_eq!(replayed, counters(&store));
}

#[test]
fn a_refused_command_leaves_no_trace_and_does_not_affect_its_batch() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir.path().join("atrium.db"), &StoreConfig::default());

    // Queued back to back so the writer takes them as one batch.
    let before = store
        .submit(Command::Add {
            key: "a".into(),
            by: 1,
        })
        .unwrap();
    let refused = store
        .submit(Command::WriteThenRefuse {
            key: "ghost".into(),
        })
        .unwrap();
    let after = store
        .submit(Command::Add {
            key: "b".into(),
            by: 2,
        })
        .unwrap();

    assert_eq!(before.wait(), Ok(1));
    assert_eq!(refused.wait(), Err(CommandFailed::Rejected(Refused)));
    assert_eq!(after.wait(), Ok(2));

    assert_eq!(
        counters(&store),
        BTreeMap::from([("a".to_owned(), 1), ("b".to_owned(), 2)])
    );
    assert_eq!(audit(&store).len(), 2);
    let effects: i64 = store
        .readers()
        .read(|c| c.query_row("SELECT count(*) FROM effect_outbox", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(effects, 2);
}

#[test]
fn effects_point_at_the_command_that_caused_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir.path().join("atrium.db"), &StoreConfig::default());
    store
        .submit(Command::Add {
            key: "x".into(),
            by: 5,
        })
        .unwrap()
        .wait()
        .unwrap();

    let (kind, payload, command): (String, String, String) = store
        .readers()
        .read(|c| {
            c.query_row(
                "SELECT e.kind, e.payload, a.command
                 FROM effect_outbox e JOIN audit_log a ON a.seq = e.audit_seq",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .unwrap();
    assert_eq!(kind, "counter_changed");
    assert_eq!(payload, r#"["x",5]"#);
    assert_eq!(command, r#"{"Add":{"key":"x","by":5}}"#);
}

#[test]
fn a_full_queue_answers_busy_instead_of_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let config = StoreConfig {
        queue_capacity: NonZeroUsize::MIN,
        ..StoreConfig::default()
    };
    let store = open(&dir.path().join("atrium.db"), &config);

    let results: Vec<_> = (0..3)
        .map(|_| store.submit(Command::Sleep { millis: 300 }))
        .collect();
    assert!(
        results.iter().any(|r| matches!(r, Err(SubmitError::Busy))),
        "with one slot and a slow writer, at least one submit must be refused"
    );
    for pending in results.into_iter().flatten() {
        assert_eq!(pending.wait(), Ok(0));
    }
}

#[test]
fn closing_the_store_finishes_queued_commands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atrium.db");
    let store = open(&path, &StoreConfig::default());
    let pending: Vec<_> = (1..=10)
        .map(|by| {
            store
                .submit(Command::Add {
                    key: "n".into(),
                    by,
                })
                .unwrap()
        })
        .collect();
    drop(store);

    let last = pending.into_iter().map(|p| p.wait().unwrap()).max();
    assert_eq!(last, Some(55));
    let reopened = open(&path, &StoreConfig::default());
    assert_eq!(counters(&reopened)["n"], 55);
}

#[tokio::test(flavor = "current_thread")]
async fn outcomes_can_be_awaited_from_async_code() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir.path().join("atrium.db"), &StoreConfig::default());
    let pending = store
        .submit(Command::Add {
            key: "async".into(),
            by: 3,
        })
        .unwrap();
    assert_eq!(pending.outcome().await, Ok(3));
}
