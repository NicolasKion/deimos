//! EVE Online static data as JSON, over the network.
//!
//! [Phobos](https://github.com/pyfa-org/Phobos) dumps the game's static data to JSON by reading an
//! installed client. Deimos does the same job without one: it reads CCP's published static data
//! export and the client files on CCP's content CDN, and writes a dump in the same shape.
//!
//! # The two sources
//!
//! **The static data export** is a zip of JSONL files CCP publishes for third parties. It carries
//! items, groups, categories, attributes, effects with their modifiers, item dogma, warfare buffs
//! and mutaplasmids - the bulk of what anyone needs.
//!
//! **The content CDN** serves the client's own files. Client static data comes in two formats:
//! `.static` files are SQLite databases holding a `cache(key, value)` table of JSON, which is what
//! [`cdn::Cdn`] reads; `.fsdbinary` files are a packed format whose decoder ships as a per-file
//! Windows CPython extension inside the client, which is how Phobos reads them and which cannot be
//! done from a plain library. So the `.static` containers are available here and the `.fsdbinary`
//! ones are not - most importantly `spacecomponentsbytype`, which has no counterpart in the export
//! either.
//!
//! # Output layout
//!
//! [`dump`] writes the directory layout Phobos writes, so that consumers of a Phobos dump can read
//! this one:
//!
//! ```text
//! <out>/phobos/metadata.json          build the data came from
//! <out>/fsd_lite/<container>.json     client .static containers, verbatim
//! <out>/fsd_built/<container>.json    export tables converted into the client's shape
//! <out>/sde/<table>.json              every export table, verbatim
//! ```
//!
//! The first three are Phobos's layout, so anything that reads a Phobos dump reads this one.
//! `fsd_built` only holds containers whose client shape is known, which is a fraction of what a
//! client has; `sde` is where the rest of the export lands, in the export's own shape rather than a
//! guessed one.
//!
//! Every file is a JSON object keyed by entity ID.
//!
//! # Example
//!
//! ```no_run
//! use deimos::dump::{self, Containers, Tables};
//!
//! # fn main() -> Result<(), deimos::Error> {
//! let dump = dump::write_latest("./eve-data".as_ref(), None, Containers::All, Tables::All)?;
//! println!("EVE build {}", dump.build_number);
//! # Ok(()) }
//! ```

use std::path::Path;

pub mod cdn;
pub mod dump;
pub mod sde;
mod tables;

/// One written file.
#[derive(Debug, Clone)]
pub struct FileReport {
    /// Path of the file, relative to the dump directory.
    pub path: String,
    /// Entries written.
    pub written: usize,
    /// Entries dropped because they could not be read.
    pub skipped: usize,
}

/// What a dump run produced.
#[derive(Debug, Clone)]
pub struct Dump {
    /// Build the data belongs to.
    pub build_number: u32,
    pub files: Vec<FileReport>,
}

impl Dump {
    /// Files that dropped entries. Empty when everything came across.
    pub fn skipped(&self) -> impl Iterator<Item = &FileReport> {
        self.files.iter().filter(|file| file.skipped > 0)
    }
}

/// Path helper for callers that want to point a reader at a container.
pub fn container_path(out_dir: &Path, dir: &str, container: &str) -> std::path::PathBuf {
    out_dir.join(dir).join(format!("{container}.json"))
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to open static data export {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("static data export is unreadable: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("static data export has no \"{0}\" entry")]
    MissingEntry(String),
    #[error("failed to read \"{entry}\": {source}")]
    Read {
        entry: String,
        #[source]
        source: std::io::Error,
    },
    #[error("static data export does not state a build number")]
    NoBuildNumber,
    #[error("failed to write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize {path}: {source}")]
    Serialize {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("download failed: {0}")]
    Download(#[from] reqwest::Error),
    #[error("the client's file index has no \"{0}\"")]
    MissingClientFile(String),
    #[error("the client has no \"{0}\" container")]
    MissingContainer(String),
    #[error("client container \"{0}\" is not a cache database")]
    UnreadableContainer(String),
    #[error("failed to read client container {container}: {source}")]
    ClientData {
        container: String,
        #[source]
        source: rusqlite::Error,
    },
}
