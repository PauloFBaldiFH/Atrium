// SPDX-License-Identifier: GPL-3.0-or-later
//! Atrium entry point: one binary, several roles (`serve`, `staff`, and later `import`,
//! `worker` and `panel`).
#![expect(
    clippy::print_stderr,
    reason = "the command line reports errors on the terminal"
)]

use std::io::BufRead;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use atrium_app::commands::{StaffCommand, WriteCommand, WriteOutput};
use atrium_app::staff::{Permission, hash_password};
use atrium_store::{AtriumHandler, AtriumStore, StoreConfig};
use atrium_web::{AppState, WebConfig};
use atrium_web_templates::I18n;
use clap::{Parser, Subcommand};

/// Atrium library system.
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
}

#[derive(Subcommand)]
enum StaffAction {
    /// Creates a staff account. The password is read from `ATRIUM_STAFF_PASSWORD` or, if
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
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("atrium: {message}");
            ExitCode::FAILURE
        }
    }
}

fn open_store(data_dir: &Path) -> Result<AtriumStore, String> {
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
    AtriumStore::open(
        &data_dir.join("atrium.db"),
        &StoreConfig::default(),
        AtriumHandler,
    )
    .map_err(|e| e.to_string())
}

fn serve(data_dir: &Path, listen: SocketAddr, insecure_cookies: bool) -> Result<(), String> {
    let store = Arc::new(open_store(data_dir)?);
    let i18n = I18n::english().map_err(|e| e.to_string())?;
    let config = WebConfig {
        secure_cookies: !insecure_cookies,
        ..WebConfig::default()
    };
    let app = atrium_web::router(AppState::new(store, i18n, config));
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(listen)
            .await
            .map_err(|e| format!("cannot listen on {listen}: {e}"))?;
        println!("atrium: listening on http://{listen}");
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
    let password = if let Ok(password) = std::env::var("ATRIUM_STAFF_PASSWORD") {
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
    let store = open_store(data_dir)?;
    let outcome = store
        .submit(WriteCommand::Staff(StaffCommand::Create {
            login: login.to_owned(),
            password_hash,
            permissions,
        }))
        .map_err(|e| e.to_string())?
        .wait()
        .map_err(|e| e.to_string())?;
    let WriteOutput::StaffCreated(id) = outcome;
    println!("atrium: staff account `{login}` created (id {})", id.0);
    Ok(())
}
