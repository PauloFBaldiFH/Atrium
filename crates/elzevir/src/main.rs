// SPDX-License-Identifier: GPL-3.0-or-later
//! Elzevir entry point: one binary, several roles (`serve`, `staff`, `import`, `export`,
//! and later `worker` and `panel`).
#![expect(
    clippy::print_stderr,
    reason = "the command line reports errors on the terminal"
)]

mod catalog_cli;
mod memory;

use std::io::BufRead;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use elzevir_app::commands::{StaffCommand, WriteCommand, WriteOutput};
use elzevir_app::parallelism::Parallelism;
use elzevir_app::search::{IndexOptions, index_pending};
use elzevir_app::staff::{Permission, hash_password};
use elzevir_search::TantivyIndex;
use elzevir_store::{ElzevirHandler, ElzevirStore, StoreConfig};
use elzevir_web::{AppState, OpacPorts, WebConfig};
use elzevir_web_templates::I18n;

/// Elzevir library system.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serves the OPAC and the staff interface.
    Serve {
        /// Institution data directory (created if missing).
        #[arg(long)]
        data_dir: PathBuf,
        /// Address to listen on.
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: SocketAddr,
        /// Send cookies without `Secure`, for plain-HTTP development only.
        #[arg(long)]
        insecure_cookies: bool,
    },
    /// Manages staff accounts.
    #[command(subcommand)]
    Staff(StaffAction),
    /// Imports a MARC21 file (ISO 2709) into the catalog.
    Import(catalog_cli::ImportArgs),
    /// Exports the catalog as MARC21 (ISO 2709, UTF-8).
    Export(catalog_cli::ExportArgs),
}

#[derive(Subcommand)]
enum StaffAction {
    /// Creates a staff account. The password is read from `ELZEVIR_STAFF_PASSWORD` or, if
    /// unset, from the first line of standard input.
    Add {
        /// Institution data directory.
        #[arg(long)]
        data_dir: PathBuf,
        /// Login name.
        #[arg(long)]
        login: String,
        /// Also grant administration.
        #[arg(long)]
        admin: bool,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Serve {
            data_dir,
            listen,
            insecure_cookies,
        } => serve(&data_dir, listen, insecure_cookies),
        Command::Staff(StaffAction::Add {
            data_dir,
            login,
            admin,
        }) => add_staff(&data_dir, &login, admin),
        Command::Import(args) => catalog_cli::import(&args),
        Command::Export(args) => catalog_cli::export(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("elzevir: {message}");
            ExitCode::FAILURE
        }
    }
}

/// An open data directory: the store, and the lock that keeps other Elzevir processes out
/// while it is open, so that there is only ever one writer (CLAUDE.md, single writer).
/// Fields drop in order, so the store closes before the lock is released.
pub(crate) struct DataDir {
    pub(crate) store: ElzevirStore,
    _lock: std::fs::File,
}

pub(crate) fn open_store(data_dir: &Path) -> Result<DataDir, String> {
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
    let lock_path = data_dir.join("elzevir.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("cannot open {}: {e}", lock_path.display()))?;
    lock.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => format!(
            "another Elzevir process is using {}; stop it first",
            data_dir.display()
        ),
        std::fs::TryLockError::Error(e) => format!("cannot lock {}: {e}", lock_path.display()),
    })?;
    let store = ElzevirStore::open(
        &data_dir.join("elzevir.db"),
        &StoreConfig::default(),
        ElzevirHandler,
    )
    .map_err(|e| e.to_string())?;
    Ok(DataDir { store, _lock: lock })
}

/// How often the indexer looks for new effects in the outbox.
const INDEX_INTERVAL: Duration = Duration::from_secs(1);

/// Effects read per indexing batch.
const INDEX_BATCH: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

/// Opens the search index in `data_dir/index`.
fn open_index(data_dir: &Path) -> Result<(TantivyIndex, Parallelism), String> {
    let directory = data_dir.join("index");
    std::fs::create_dir_all(&directory)
        .map_err(|e| format!("cannot create {}: {e}", directory.display()))?;
    let cores = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    let switch = std::env::var("ELZEVIR_SERIAL").ok();
    let parallelism = Parallelism::from_serial_switch(switch.as_deref(), "index", cores);
    let index = TantivyIndex::open(&directory, parallelism).map_err(|e| e.to_string())?;
    Ok((index, parallelism))
}

/// Keeps the index up to date with the outbox, in its own thread, for as long as the
/// process runs. A failed run is reported and tried again on the next tick: the cursor
/// only moves with a successful commit, so nothing is lost.
fn spawn_indexer(
    store: Arc<ElzevirStore>,
    index: Arc<TantivyIndex>,
    parallelism: Parallelism,
) -> Result<(), String> {
    let options = IndexOptions {
        parallelism,
        batch: INDEX_BATCH,
    };
    std::thread::Builder::new()
        .name("elzevir-indexer".to_owned())
        .spawn(move || {
            loop {
                if let Err(e) = index_pending(&*store, &*store, &*index, &options) {
                    eprintln!("elzevir: indexing failed, retrying: {e}");
                }
                std::thread::sleep(INDEX_INTERVAL);
            }
        })
        .map(drop)
        .map_err(|e| format!("cannot start the indexer: {e}"))
}

fn serve(data_dir: &Path, listen: SocketAddr, insecure_cookies: bool) -> Result<(), String> {
    let DataDir { store, _lock } = open_store(data_dir)?;
    let store = Arc::new(store);
    let (index, parallelism) = open_index(data_dir)?;
    let index = Arc::new(index);
    spawn_indexer(Arc::clone(&store), Arc::clone(&index), parallelism)?;
    let i18n = I18n::english().map_err(|e| e.to_string())?;
    let config = WebConfig {
        secure_cookies: !insecure_cookies,
        ..WebConfig::default()
    };
    let opac = OpacPorts {
        catalog: Arc::clone(&store) as _,
        index,
    };
    let app = elzevir_web::router(AppState::new(
        store,
        Arc::new(memory::AllocatorProbe),
        opac,
        i18n,
        config,
    ));
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(listen)
            .await
            .map_err(|e| format!("cannot listen on {listen}: {e}"))?;
        println!("elzevir: listening on http://{listen}");
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                // If the signal handler cannot be installed, the server simply runs until
                // the process is killed.
                if tokio::signal::ctrl_c().await.is_err() {
                    std::future::pending::<()>().await;
                }
            })
            .await
            .map_err(|e| e.to_string())
    })
}

fn add_staff(data_dir: &Path, login: &str, admin: bool) -> Result<(), String> {
    let password = if let Ok(password) = std::env::var("ELZEVIR_STAFF_PASSWORD") {
        password
    } else {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| format!("cannot read the password: {e}"))?;
        line.trim_end_matches(['\r', '\n']).to_owned()
    };
    if password.chars().count() < 8 {
        return Err("the password must have at least 8 characters".to_owned());
    }
    let mut permissions = [Permission::StaffInterface]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    if admin {
        permissions.insert(Permission::Administration);
    }
    let password_hash = hash_password(&password).map_err(|e| e.to_string())?;
    let data = open_store(data_dir)?;
    let outcome = data
        .store
        .submit(WriteCommand::Staff(StaffCommand::Create {
            login: login.to_owned(),
            password_hash,
            permissions,
        }))
        .map_err(|e| e.to_string())?
        .wait()
        .map_err(|e| e.to_string())?;
    let WriteOutput::StaffCreated(id) = outcome else {
        return Err(format!("unexpected answer from the writer: {outcome:?}"));
    };
    println!("elzevir: staff account `{login}` created (id {})", id.0);
    Ok(())
}
