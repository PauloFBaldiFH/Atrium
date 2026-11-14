// SPDX-License-Identifier: GPL-3.0-or-later
//! The single writer thread (docs/arquitetura/05-definicoes-de-implementacao.md, §1).

use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::JoinHandle;

use rusqlite::{Connection, Transaction, params};
use serde::Serialize;
use tokio::sync::oneshot;

use crate::{CommandFailed, StoreConfig, StoreError, SubmitError};

/// Applies typed commands to the database. Implemented once per application, by the layer
/// that owns the command enum; the store only runs it on the writer thread.
pub trait CommandHandler: Send + 'static {
    /// The command enum. Serialized into `audit_log`, so replaying the log in `seq` order
    /// reproduces the state.
    type Command: Serialize + Send + 'static;
    /// What a successful command returns to its caller.
    type Output: Send + 'static;
    /// Why the handler may refuse a command (a business rule said no).
    type Rejection: Send + 'static;

    /// Short, stable name of the command, stored in `audit_log.kind`.
    fn kind(command: &Self::Command) -> &'static str;

    /// Applies one command. Everything it writes through `tx` is kept only if it returns
    /// `Ok`; on `Err` the command's savepoint, audit entry included, is rolled back.
    ///
    /// # Errors
    ///
    /// [`CommandFailed::Rejected`] for refusals, [`CommandFailed::Storage`] for SQL errors.
    fn apply(
        &mut self,
        tx: &WriteTx<'_>,
        command: &Self::Command,
    ) -> Result<Self::Output, CommandFailed<Self::Rejection>>;
}

/// The write side of one command: its connection (inside the command's savepoint) and the
/// `audit_log` entry it belongs to.
pub struct WriteTx<'a> {
    connection: &'a Connection,
    audit_seq: i64,
}

impl WriteTx<'_> {
    /// The connection, positioned inside this command's savepoint.
    #[must_use]
    pub fn connection(&self) -> &Connection {
        self.connection
    }

    /// The `audit_log.seq` of the command being applied.
    #[must_use]
    pub fn audit_seq(&self) -> i64 {
        self.audit_seq
    }

    /// Records an effect to run after commit (reindex, notice, ...). It is written in the
    /// same transaction as the change, so it exists if and only if the change does.
    ///
    /// # Errors
    ///
    /// Fails if `payload` cannot be serialized or the insert fails.
    pub fn emit_effect(&self, kind: &str, payload: &impl Serialize) -> Result<(), StoreError> {
        let payload = serde_json::to_string(payload)?;
        self.connection.execute(
            "INSERT INTO effect_outbox (audit_seq, kind, payload) VALUES (?1, ?2, ?3)",
            params![self.audit_seq, kind, payload],
        )?;
        Ok(())
    }
}

type Outcome<H> =
    Result<<H as CommandHandler>::Output, CommandFailed<<H as CommandHandler>::Rejection>>;

/// A queued command waiting for its outcome.
pub struct Pending<H: CommandHandler> {
    reply: oneshot::Receiver<Outcome<H>>,
}

impl<H: CommandHandler> Pending<H> {
    /// Waits for the outcome from async code. The outcome arrives only after the commit.
    ///
    /// # Errors
    ///
    /// The command's failure, or [`CommandFailed::WriterGone`] if the writer stopped.
    pub async fn outcome(self) -> Outcome<H> {
        self.reply.await.unwrap_or(Err(CommandFailed::WriterGone))
    }

    /// Waits for the outcome from a plain thread. Must not be called inside an async
    /// runtime (use [`Pending::outcome`] there).
    ///
    /// # Errors
    ///
    /// The command's failure, or [`CommandFailed::WriterGone`] if the writer stopped.
    pub fn wait(self) -> Outcome<H> {
        self.reply
            .blocking_recv()
            .unwrap_or(Err(CommandFailed::WriterGone))
    }
}

struct Request<H: CommandHandler> {
    command: H::Command,
    /// The staff account the command comes from; `None` for imports and the command line.
    actor: Option<i64>,
    reply: oneshot::Sender<Outcome<H>>,
}

/// Owns the queue sender and the writer thread.
pub(crate) struct WriterHandle<H: CommandHandler> {
    queue: Option<SyncSender<Request<H>>>,
    thread: Option<JoinHandle<()>>,
}

impl<H: CommandHandler> WriterHandle<H> {
    pub(crate) fn start(
        connection: Connection,
        config: &StoreConfig,
        handler: H,
    ) -> Result<Self, StoreError> {
        let (queue, requests) = mpsc::sync_channel(config.queue_capacity.get());
        let max_batch = config.max_batch.get();
        let thread = std::thread::Builder::new()
            .name("elzevir-writer".to_owned())
            .spawn(move || run(connection, handler, &requests, max_batch))
            .map_err(|e| StoreError::Thread(e.to_string()))?;
        Ok(Self {
            queue: Some(queue),
            thread: Some(thread),
        })
    }

    pub(crate) fn submit(
        &self,
        actor: Option<i64>,
        command: H::Command,
    ) -> Result<Pending<H>, SubmitError> {
        let queue = self.queue.as_ref().ok_or(SubmitError::Closed)?;
        let (reply, receiver) = oneshot::channel();
        match queue.try_send(Request {
            command,
            actor,
            reply,
        }) {
            Ok(()) => Ok(Pending { reply: receiver }),
            Err(TrySendError::Full(_)) => Err(SubmitError::Busy),
            Err(TrySendError::Disconnected(_)) => Err(SubmitError::Closed),
        }
    }
}

impl<H: CommandHandler> WriterHandle<H> {
    pub(crate) fn submit_waiting(&self, command: H::Command) -> Result<Pending<H>, SubmitError> {
        let queue = self.queue.as_ref().ok_or(SubmitError::Closed)?;
        let (reply, receiver) = oneshot::channel();
        queue
            .send(Request {
                command,
                actor: None,
                reply,
            })
            .map_err(|_| SubmitError::Closed)?;
        Ok(Pending { reply: receiver })
    }
}

impl<H: CommandHandler> Drop for WriterHandle<H> {
    fn drop(&mut self) {
        // Closing the queue ends the writer loop after it drains what is already queued.
        self.queue = None;
        if let Some(thread) = self.thread.take() {
            // A writer that panicked has already lost its in-flight batch (rolled back by
            // SQLite); there is nothing left to recover here.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "join result only says whether the writer panicked; see comment"
            )]
            let _ = thread.join();
        }
    }
}

/// The writer loop: wait for one command, take whatever else is already queued (up to
/// `max_batch`), commit them together, repeat until the queue closes.
fn run<H: CommandHandler>(
    mut connection: Connection,
    mut handler: H,
    requests: &Receiver<Request<H>>,
    max_batch: usize,
) {
    while let Ok(first) = requests.recv() {
        let mut batch = Vec::with_capacity(max_batch);
        batch.push(first);
        while batch.len() < max_batch {
            match requests.try_recv() {
                Ok(next) => batch.push(next),
                Err(_) => break,
            }
        }
        commit_batch(&mut connection, &mut handler, batch);
    }
    // Best effort: the database is consistent without these; they only shrink the WAL and
    // refresh planner statistics before the file is closed.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "optional maintenance on close; see comment"
    )]
    let _ = connection.execute_batch("PRAGMA optimize; PRAGMA wal_checkpoint(TRUNCATE);");
}

fn commit_batch<H: CommandHandler>(
    connection: &mut Connection,
    handler: &mut H,
    batch: Vec<Request<H>>,
) {
    let mut tx = match connection.transaction() {
        Ok(tx) => tx,
        Err(error) => return fail_all(batch, &StoreError::from(error)),
    };
    let mut outcomes = Vec::with_capacity(batch.len());
    for request in batch {
        let outcome = apply_one(&mut tx, handler, request.actor, &request.command);
        outcomes.push((request.reply, outcome));
    }
    match tx.commit() {
        Ok(()) => {
            for (reply, outcome) in outcomes {
                send(reply, outcome);
            }
        }
        Err(error) => {
            let error = StoreError::from(error);
            for (reply, _) in outcomes {
                send(reply, Err(CommandFailed::Storage(error.clone())));
            }
        }
    }
}

fn apply_one<H: CommandHandler>(
    tx: &mut Transaction<'_>,
    handler: &mut H,
    actor: Option<i64>,
    command: &H::Command,
) -> Outcome<H> {
    let savepoint = tx.savepoint().map_err(StoreError::from)?;
    let json = serde_json::to_string(command).map_err(StoreError::from)?;
    savepoint
        .execute(
            "INSERT INTO audit_log (kind, command, actor) VALUES (?1, ?2, ?3)",
            params![H::kind(command), json, actor],
        )
        .map_err(StoreError::from)?;
    let write = WriteTx {
        connection: &savepoint,
        audit_seq: savepoint.last_insert_rowid(),
    };
    let output = handler.apply(&write, command)?;
    savepoint.commit().map_err(StoreError::from)?;
    Ok(output)
    // On any `?` above, dropping `savepoint` rolls it back.
}

fn fail_all<H: CommandHandler>(batch: Vec<Request<H>>, error: &StoreError) {
    for request in batch {
        send(request.reply, Err(CommandFailed::Storage(error.clone())));
    }
}

fn send<T>(reply: oneshot::Sender<T>, outcome: T) {
    // The caller may have stopped waiting (request cancelled); the command's effect stands
    // regardless, so an undelivered reply is not an error.
    #[expect(
        clippy::let_underscore_must_use,
        reason = "caller may have gone away; see comment"
    )]
    let _ = reply.send(outcome);
}
