// SPDX-License-Identifier: GPL-3.0-or-later
//! The `import` and `export` roles (docs/specs/catalogo/linha-de-comando.md).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use elzevir_app::catalog::{CatalogCommand, CatalogRefusal};
use elzevir_app::commands::{WriteCommand, WriteRefusal};
use elzevir_app::export::{ExportOptions, export_marc21, item_codes};
use elzevir_app::import::{ImportOptions, ImportReport, Parallelism, import_marc21};
use elzevir_app::matching::{self, Matching, OnMatch, OnNoMatch};
use elzevir_domain::catalog::MatchRule;
use elzevir_marc::Limits;
use elzevir_store::{CommandFailed, ElzevirStore};

use crate::open_store;

/// `elzevir import`.
#[derive(Args)]
pub(crate) struct ImportArgs {
    /// Institution data directory (created if missing).
    #[arg(long)]
    data_dir: PathBuf,
    /// Rule that finds a stored record equal to an imported one; without it every record
    /// is stored.
    #[arg(long = "match", value_enum)]
    rule: Option<Rule>,
    /// What to do when a record matches a stored one.
    #[arg(long, value_enum, default_value = "ignore", requires = "rule")]
    on_match: MatchAction,
    /// What to do when a record matches nothing.
    #[arg(long, value_enum, default_value = "create-new", requires = "rule")]
    on_no_match: NoMatchAction,
    /// What to do with the Items of each record.
    #[arg(long, value_enum, default_value = "always-add", requires = "rule")]
    items: ItemAction,
    /// Create the branches and item types the file refers to that do not exist yet,
    /// with their codes as names.
    #[arg(long)]
    create_codes: bool,
    /// The MARC21 file (ISO 2709).
    file: PathBuf,
}

/// `elzevir export`.
#[derive(Args)]
pub(crate) struct ExportArgs {
    /// Institution data directory.
    #[arg(long)]
    data_dir: PathBuf,
    /// Where to write the MARC21 file; standard output when absent.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Rule {
    Isbn,
    Issn,
    KohaBiblionumber,
}

#[derive(Clone, Copy, ValueEnum)]
enum MatchAction {
    Ignore,
    CreateNew,
    Replace,
}

#[derive(Clone, Copy, ValueEnum)]
enum NoMatchAction {
    CreateNew,
    Ignore,
}

#[derive(Clone, Copy, ValueEnum)]
enum ItemAction {
    AlwaysAdd,
    AddOnlyForMatches,
    AddOnlyForNew,
    Replace,
    Ignore,
}

impl ImportArgs {
    fn matching(&self) -> Option<Matching> {
        let rule = match self.rule? {
            Rule::Isbn => MatchRule::Isbn,
            Rule::Issn => MatchRule::Issn,
            Rule::KohaBiblionumber => MatchRule::KohaBiblionumber,
        };
        Some(Matching {
            rule,
            on_match: match self.on_match {
                MatchAction::Ignore => OnMatch::Ignore,
                MatchAction::CreateNew => OnMatch::CreateNew,
                MatchAction::Replace => OnMatch::Replace,
            },
            on_no_match: match self.on_no_match {
                NoMatchAction::CreateNew => OnNoMatch::CreateNew,
                NoMatchAction::Ignore => OnNoMatch::Ignore,
            },
            items: match self.items {
                ItemAction::AlwaysAdd => matching::ItemAction::AlwaysAdd,
                ItemAction::AddOnlyForMatches => matching::ItemAction::AddOnlyForMatches,
                ItemAction::AddOnlyForNew => matching::ItemAction::AddOnlyForNew,
                ItemAction::Replace => matching::ItemAction::Replace,
                ItemAction::Ignore => matching::ItemAction::Ignore,
            },
        })
    }
}

fn open_input(path: &Path) -> Result<BufReader<File>, String> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))
}

pub(crate) fn import(args: &ImportArgs) -> Result<(), String> {
    let data = open_store(&args.data_dir)?;
    if args.create_codes {
        create_codes(&data.store, &args.file)?;
    }
    let cores = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    let switch = std::env::var("ELZEVIR_SERIAL").ok();
    let options = ImportOptions {
        parallelism: Parallelism::from_serial_switch(switch.as_deref(), "import", cores),
        matching: args.matching(),
        ..ImportOptions::default()
    };
    let report =
        import_marc21(open_input(&args.file)?, &data.store, &options).map_err(|e| e.to_string())?;
    print_import(&report);
    Ok(())
}

/// Creates the branches and item types of the file that are missing.
fn create_codes(store: &ElzevirStore, file: &Path) -> Result<(), String> {
    let codes = item_codes(open_input(file)?, Limits::default())
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let branches = codes
        .branches
        .into_iter()
        .map(|code| CatalogCommand::CreateBranch {
            name: code.clone(),
            code,
        });
    let item_types = codes
        .item_types
        .into_iter()
        .map(|code| CatalogCommand::CreateItemType {
            description: code.clone(),
            code,
        });
    let mut created = 0_u32;
    for command in branches.chain(item_types) {
        let answer = store
            .submit(WriteCommand::Catalog(command))
            .map_err(|e| e.to_string())?
            .wait();
        match answer {
            Ok(_) => created = created.saturating_add(1),
            // Already there: nothing to create.
            Err(CommandFailed::Rejected(WriteRefusal::Catalog(CatalogRefusal::CodeTaken(_)))) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    println!("elzevir: {created} branch and item type codes created");
    Ok(())
}

fn print_import(report: &ImportReport) {
    for failed in &report.failures {
        eprintln!(
            "elzevir: record {} (byte {}) not imported: {}",
            failed.index.saturating_add(1),
            failed.offset,
            failed.failure
        );
    }
    for failed in &report.item_failures {
        eprintln!(
            "elzevir: record {}, item {} not stored: {}",
            failed.index.saturating_add(1),
            failed.position.saturating_add(1),
            failed.refusal
        );
    }
    println!(
        "elzevir: {} records read, {} stored, {} matched, {} replaced, {} skipped, {} failed",
        report.read,
        report.imported,
        report.matched.len(),
        report.replaced,
        report.skipped,
        report.failures.len()
    );
    println!(
        "elzevir: {} items stored, {} replaced, {} refused",
        report.items,
        report.replaced_items,
        report.item_failures.len()
    );
}

pub(crate) fn export(args: &ExportArgs) -> Result<(), String> {
    let data = open_store(&args.data_dir)?;
    let options = ExportOptions::default();
    // `export_marc21` flushes what it wrote, so nothing is left in the buffers.
    let report = if let Some(path) = &args.output {
        let file =
            File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        export_marc21(&data.store, &mut BufWriter::new(file), &options)
    } else {
        export_marc21(
            &data.store,
            &mut BufWriter::new(std::io::stdout().lock()),
            &options,
        )
    }
    .map_err(|e| e.to_string())?;
    for failed in &report.failures {
        eprintln!(
            "elzevir: record {} not exported: {}",
            failed.instance.0, failed.failure
        );
    }
    eprintln!(
        "elzevir: {} records exported, {} failed",
        report.written,
        report.failures.len()
    );
    Ok(())
}
