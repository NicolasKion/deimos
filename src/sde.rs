//! Reading CCP's static data export.
//!
//! The export is a zip of JSONL files, one JSON object per line, each carrying its own ID in a
//! `_key` field. Tables are read as a stream: the biggest (`types.jsonl`) is 150 MB uncompressed
//! and there is no reason to hold it in memory.

use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use serde::de::DeserializeOwned;

use crate::Error;

/// The export CCP keeps current.
pub const LATEST_URL: &str = "https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip";

/// Download an export. Returns the number of bytes written.
///
/// The export is around 100 MB, so this is not quick.
pub fn download(url: &str, dest: &Path) -> Result<u64, Error> {
    let mut response = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    let mut file = File::create(dest).map_err(|source| Error::Write {
        path: dest.display().to_string(),
        source,
    })?;
    response.copy_to(&mut file).map_err(Error::from)
}

/// An opened static data export.
pub struct Export {
    archive: zip::ZipArchive<File>,
}

impl Export {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = File::open(path).map_err(|source| Error::Open {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self {
            archive: zip::ZipArchive::new(file)?,
        })
    }

    /// Download the current export to `dest` and open it.
    pub fn download_latest(dest: &Path) -> Result<Self, Error> {
        download(LATEST_URL, dest)?;
        Self::open(dest)
    }

    /// Table names the export contains, e.g. `types.jsonl`.
    pub fn tables(&self) -> Vec<String> {
        self.archive.file_names().map(str::to_owned).collect()
    }

    /// Hand every record of `table` to `handle`, deserialized into `T`.
    ///
    /// A record that fails to deserialize is counted as skipped rather than failing the read: one
    /// unexpected record should not cost the caller the whole table.
    pub fn read<T, F>(&mut self, table: &str, mut handle: F) -> Result<Counts, Error>
    where
        T: DeserializeOwned,
        F: FnMut(T) -> Result<(), Error>,
    {
        let file = match self.archive.by_name(table) {
            Ok(file) => file,
            Err(zip::result::ZipError::FileNotFound) => return Err(Error::MissingEntry(table.to_owned())),
            Err(err) => return Err(err.into()),
        };
        let mut counts = Counts::default();
        for line in BufReader::with_capacity(256 * 1024, file).lines() {
            let line = line.map_err(|source| Error::Read {
                entry: table.to_owned(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<T>(&line) {
                Ok(record) => {
                    handle(record)?;
                    counts.read += 1;
                }
                Err(_) => counts.skipped += 1,
            }
        }
        Ok(counts)
    }

    /// EVE client build the export was made from.
    pub fn build_number(&mut self) -> Result<u32, Error> {
        #[derive(serde::Deserialize)]
        struct Metadata {
            #[serde(rename = "buildNumber")]
            build_number: Option<u32>,
        }

        let mut build_number = None;
        self.read("_sde.jsonl", |record: Metadata| {
            build_number = build_number.or(record.build_number);
            Ok(())
        })?;
        build_number.ok_or(Error::NoBuildNumber)
    }
}

/// How a table read went.
#[derive(Default, Debug, Clone, Copy)]
pub struct Counts {
    pub read: usize,
    pub skipped: usize,
}
