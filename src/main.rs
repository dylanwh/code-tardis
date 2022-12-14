use chrono::serde::*;
use chrono::{DateTime, Utc};
use eyre::{eyre, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::read_to_string;

use std::path::{PathBuf, Path};
use clap::{Parser, Subcommand};

static CODE_HISTORY_DIR: &str = "Library/Application Support/Code/User/History";

#[derive(Parser, Debug)]
struct Tardis {
    #[arg(short = 'C', long, default_value = ".")]
    dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List all vscode backup files in current directory
    List {
        #[arg(short, long)]
        verbose: bool,
    },
    Restore {
        /// The files to restore
        #[arg()]
        files: Vec<PathBuf>,
    }

}

#[derive(Debug, Serialize, Deserialize)]
struct CodeHistoryFile {
    dir: PathBuf,
    info: CodeHistoryInfo,
}

impl CodeHistoryFile {
    fn current_file(&self) -> PathBuf {
        PathBuf::from(self.info.resource.path())
    }

    fn backup_files(&self) -> Vec<(DateTime<Utc>, PathBuf)> {
        self.info
            .entries
            .iter()
            .map(|e| (e.timestamp.clone(), self.dir.join(&e.id)))
            .collect()
    }

    fn is_scheme(&self, scheme: &str) -> bool {
        self.info.resource.scheme() == scheme
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CodeHistoryInfo {
    version: u32,
    resource: url::Url,
    entries: Vec<CodeHistoryEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CodeHistoryEntry {
    id: PathBuf,
    #[serde(with = "ts_milliseconds")]
    timestamp: DateTime<Utc>,
}

fn main() -> Result<()> {
    let args: Tardis = Tardis::parse();


    let home_dir = dirs::home_dir().ok_or_else(|| eyre!("Could not find home directory"))?;
    let history_dir = home_dir.join(CODE_HISTORY_DIR);
    let current_dir = args.dir.canonicalize().context("Could not find current directory")?;
    let found_files = walkdir::WalkDir::new(history_dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().ends_with("entries.json"))
        .map(|e| {
            let info = read_to_string(e.path())
                .with_context(|| format!("Could not read file {:?}", e.path()))?;
            let info: CodeHistoryInfo = serde_json::from_str(&info)?;
            let file = CodeHistoryFile {
                dir: e
                    .path()
                    .parent()
                    .ok_or_else(|| eyre!("Could not find parent directory"))?
                    .to_path_buf(),
                    info,
            };
            if file.is_scheme("file") && file.current_file().starts_with(&current_dir) {
                Ok(Some(file))
            } else {
                Ok(None)
            }
        })
        .filter_map(|e| e.transpose())
        .collect::<Result<Vec<_>>>()?;

    match args.command {
        Command::List { verbose } => {
            for file in found_files {
                let current_file = file.current_file().strip_prefix(&current_dir)?.to_path_buf();
                if verbose {
                    for (ts, backup) in file.backup_files() {
                        println!("{}\t{}\t{}", current_file.to_string_lossy(), ts, backup.to_string_lossy());
                    }
                } else {
                    println!("{} ({} backups)", current_file.to_string_lossy(), file.backup_files().len());
                }
            }
        }
        Command::Restore { files: _  } => {
            for history_file in found_files {
                let current_file = history_file.current_file().strip_prefix(&current_dir)?.to_path_buf();
                let (ts, backup_file) = history_file.backup_files().last().cloned().ok_or_else(|| eyre!("No backup files found"))?;
                println!("Restoring {} using {} from {}", current_file.to_string_lossy(), backup_file.to_string_lossy(), ts);
                std::fs::copy(backup_file, current_file)?;
            }
        }
    }

    Ok(())
}

fn to_absolute<P: AsRef<Path>, C: AsRef<Path>>(path: P, current_dir: C) -> PathBuf {
    if path.as_ref().is_absolute() {
        path.as_ref().to_path_buf()
    } else {
        current_dir.as_ref().join(path)
    }
}
