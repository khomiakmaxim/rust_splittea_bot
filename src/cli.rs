use clap::Parser;
use directories::BaseDirs;
use once_cell::sync::Lazy;
use std::{ffi::OsString, path::PathBuf};

pub static CLI: Lazy<Cli> = Lazy::new(parse_args);

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(
        short,
        long,
        env = "SPLITTEA_DB",
        value_name = "FILE",
        help = "Path to the SQLite database file (tries to create if not exists)",
        default_value = get_default_database_file()
    )]
    pub database: PathBuf,
    #[arg(short, long, value_name = "BOT TOKEN", env = "BOT_TOKEN")]
    pub token: String,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}

fn get_default_database_file() -> OsString {
    let db_name = "splitea_db.sqlite";
    if cfg!(target_os = "android") {
        db_name.into()
    } else {
        match BaseDirs::new() {
            Some(base_dirs) => base_dirs.data_dir().join(db_name).into(),
            None => db_name.into(),
        }
    }
}
