//! Command line front end.
//!
//! ```text
//! deimos <out-dir> [options]
//!
//!   --export <zip>        use (or download to) this static data export instead of a temporary one
//!   --containers a,b,c    client containers to read; defaults to all of them
//!   --tables a,b,c        export tables to write out verbatim; defaults to all of them
//!   --no-client           skip the client CDN entirely
//!   --no-tables           skip the verbatim export tables (they are the bulk of the output)
//!   --list                list the client's containers and exit
//! ```

use std::{path::PathBuf, process::ExitCode};

use deimos::{
    cdn::Cdn,
    dump::{self, Containers, Tables},
};

fn main() -> ExitCode {
    let args = match Args::parse() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: deimos <out-dir> [--export <zip>] [--containers a,b,c] [--tables a,b,c] \
                 [--no-client] [--no-tables] [--list]"
            );
            return ExitCode::FAILURE;
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), deimos::Error> {
    if args.list {
        let cdn = Cdn::connect()?;
        println!("client build {}", cdn.build());
        for resource in cdn.resources() {
            let mark = if resource.is_container() { " " } else { "*" };
            println!("{mark} {:<40} {:>9} KiB", resource.container(), resource.bytes / 1024);
        }
        println!("\n* packed with a loader that only exists inside the client, so not readable here");
        return Ok(());
    }

    let container_names = split(args.containers.as_deref());
    let containers = match (args.no_client, args.containers.is_some()) {
        (true, _) => Containers::None,
        (false, true) => Containers::Named(&container_names),
        (false, false) => Containers::All,
    };
    let table_names = split(args.tables.as_deref());
    let tables = match (args.no_tables, args.tables.is_some()) {
        (true, _) => Tables::None,
        (false, true) => Tables::Named(&table_names),
        (false, false) => Tables::All,
    };

    println!("writing dump to {}", args.out_dir.display());
    let dump = dump::write_latest(&args.out_dir, args.export.as_deref(), containers, tables)?;
    for file in &dump.files {
        println!("  {:<45} {:>7} entries", file.path, file.written);
    }
    for file in dump.skipped() {
        println!("  {}: {} entries skipped", file.path, file.skipped);
    }
    println!("EVE build {}", dump.build_number);
    Ok(())
}

fn split(list: Option<&str>) -> Vec<&str> {
    list.map(|list| list.split(',').map(str::trim).filter(|name| !name.is_empty()).collect())
        .unwrap_or_default()
}

struct Args {
    out_dir: PathBuf,
    export: Option<PathBuf>,
    containers: Option<String>,
    tables: Option<String>,
    no_client: bool,
    no_tables: bool,
    list: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut out_dir = None;
        let mut export = None;
        let mut containers = None;
        let mut tables = None;
        let mut no_client = false;
        let mut no_tables = false;
        let mut list = false;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
            match arg.as_str() {
                "--export" => export = Some(PathBuf::from(value("--export")?)),
                "--containers" => containers = Some(value("--containers")?),
                "--tables" => tables = Some(value("--tables")?),
                "--no-client" => no_client = true,
                "--no-tables" => no_tables = true,
                "--list" => list = true,
                "-h" | "--help" => return Err(String::new()),
                other if other.starts_with('-') => return Err(format!("unknown option {other}")),
                other if out_dir.is_none() => out_dir = Some(PathBuf::from(other)),
                other => return Err(format!("unexpected argument {other}")),
            }
        }

        Ok(Self {
            out_dir: match (out_dir, list) {
                (Some(dir), _) => dir,
                (None, true) => PathBuf::new(),
                (None, false) => return Err("an output directory is required".to_owned()),
            },
            export,
            containers,
            tables,
            no_client,
            no_tables,
            list,
        })
    }
}
