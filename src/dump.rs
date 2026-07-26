//! Writing the dump.
//!
//! Every container is a "keymap": a single JSON object whose keys are entity IDs. They are written
//! as a stream, entry by entry, so that nothing has to be held whole in memory.

use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{Dump, Error, FileReport, cdn::Cdn, sde::Export, tables};

/// Which client containers to read. [`Containers::All`] is what Phobos does by default; it is
/// around 110 MB, most of it `solarsystemcontent`.
#[derive(Debug, Clone, Copy)]
pub enum Containers<'a> {
    All,
    Named(&'a [&'a str]),
    None,
}

/// Which export tables to write out verbatim. [`Tables::All`] is around 600 MB, most of it map
/// data, so it is worth naming the ones you want.
#[derive(Debug, Clone, Copy)]
pub enum Tables<'a> {
    All,
    Named(&'a [&'a str]),
    None,
}

/// Download the current export and client data, and write a complete dump to `out_dir`.
///
/// Pass `export_zip` to reuse an export already on disk instead of downloading 100 MB again; it is
/// downloaded to that path if it is not there yet.
pub fn write_latest(
    out_dir: &Path,
    export_zip: Option<&Path>,
    containers: Containers<'_>,
    tables: Tables<'_>,
) -> Result<Dump, Error> {
    let zip = match export_zip {
        Some(path) if path.exists() => path.to_owned(),
        Some(path) => {
            crate::sde::download(crate::sde::LATEST_URL, path)?;
            path.to_owned()
        }
        None => {
            let path = std::env::temp_dir().join("eve-online-static-data-latest-jsonl.zip");
            crate::sde::download(crate::sde::LATEST_URL, &path)?;
            path
        }
    };

    let mut export = Export::open(&zip)?;
    let build_number = export.build_number()?;
    let mut files = write_export(&mut export, out_dir)?;
    files.extend(write_tables(&mut export, tables, out_dir)?);

    if !matches!(containers, Containers::None) {
        let cdn = Cdn::connect()?;
        files.extend(write_containers(&cdn, containers, out_dir)?);
    }

    Ok(Dump { build_number, files })
}

/// Write everything derivable from a static data export: `phobos/metadata.json` and the
/// `fsd_built` containers.
pub fn write_export(export: &mut Export, out_dir: &Path) -> Result<Vec<FileReport>, Error> {
    let build_number = export.build_number()?;
    // The build number is what consumers key their caches on, so it is written first: a dump with
    // data but no version is worse than no dump at all.
    let mut files = vec![write_metadata(out_dir, build_number)?];
    files.push(tables::items(export, out_dir)?);
    files.push(tables::groups(export, out_dir)?);
    files.push(tables::item_lists(export, out_dir)?);
    files.push(tables::attrs(export, out_dir)?);
    files.push(tables::effects(export, out_dir)?);
    files.push(tables::buffs(export, out_dir)?);
    files.push(tables::mutas(export, out_dir)?);
    // Item dogma and skill requirements both come out of typeDogma, in one pass over it.
    let (item_dogma, item_skill_reqs) = tables::item_dogma(export, out_dir)?;
    files.push(item_dogma);
    files.push(item_skill_reqs);
    Ok(files)
}

/// Write export tables into `sde`, verbatim: one container per table, keyed by each record's
/// `_key`, with everything else the record carries left exactly as CCP published it.
///
/// This is the part of the export that has no verified client shape to be converted into. Where a
/// table does have one - items, groups, attributes, effects, item dogma, warfare buffs,
/// mutaplasmids - [`write_export`] writes that too, under `fsd_built`.
pub fn write_tables(export: &mut Export, tables: Tables<'_>, out_dir: &Path) -> Result<Vec<FileReport>, Error> {
    let names: Vec<String> = match tables {
        Tables::All => export
            .tables()
            .into_iter()
            .filter(|name| name.ends_with(".jsonl") && name != "_sde.jsonl")
            .collect(),
        Tables::Named(names) => names.iter().map(|name| format!("{name}.jsonl")).collect(),
        Tables::None => return Ok(Vec::new()),
    };

    let mut files = Vec::with_capacity(names.len());
    for table in names {
        let container = table.trim_end_matches(".jsonl").to_lowercase();
        let mut keymap = Keymap::create(out_dir, "sde", &container)?;
        let counts = export.read(&table, |mut record: serde_json::Map<String, serde_json::Value>| {
            // The ID travels in the record on the way in and becomes the map key on the way out.
            let Some(key) = record.remove("_key") else {
                return Ok(());
            };
            let key = match key {
                serde_json::Value::String(key) => key,
                other => other.to_string(),
            };
            keymap.write_json_value(&key, &serde_json::Value::Object(record))
        })?;
        files.push(keymap.finish(counts.skipped)?);
    }
    Ok(files)
}

/// Write client `.static` containers into `fsd_lite`.
pub fn write_containers(cdn: &Cdn, containers: Containers<'_>, out_dir: &Path) -> Result<Vec<FileReport>, Error> {
    let names: Vec<String> = match containers {
        Containers::All => cdn.containers().map(|res| res.container().to_owned()).collect(),
        Containers::Named(names) => names.iter().map(|name| (*name).to_owned()).collect(),
        Containers::None => return Ok(Vec::new()),
    };

    let asked_for = matches!(containers, Containers::Named(_));
    let mut files = Vec::with_capacity(names.len());
    for name in names {
        // A few .static files are not cache databases at all. Skip them when sweeping everything,
        // the way Phobos does; complain when the caller named them.
        let Some(rows) = cdn.read_container(&name)? else {
            if asked_for {
                return Err(Error::UnreadableContainer(name));
            }
            continue;
        };
        let mut keymap = Keymap::create(out_dir, "fsd_lite", &name)?;
        for (key, value) in &rows {
            // Values are already JSON, so they go through untouched.
            keymap.write_json(key, value)?;
        }
        files.push(keymap.finish(0)?);
    }
    Ok(files)
}

/// Write the file that records which build the dump came from.
pub fn write_metadata(out_dir: &Path, build_number: u32) -> Result<FileReport, Error> {
    #[derive(Serialize)]
    struct Field {
        field_name: &'static str,
        field_value: u32,
    }

    let rel_path = "phobos/metadata.json".to_owned();
    let fields = [Field {
        field_name: "client_build",
        field_value: build_number,
    }];
    let mut writer = BufWriter::new(create_file(out_dir, &rel_path)?);
    serde_json::to_writer(&mut writer, &fields).map_err(|source| Error::Serialize {
        path: rel_path.clone(),
        source,
    })?;
    writer.flush().map_err(|source| Error::Write {
        path: rel_path.clone(),
        source,
    })?;
    Ok(FileReport {
        path: rel_path,
        written: fields.len(),
        skipped: 0,
    })
}

/// Write a container with no entries, for data that is known to be out of reach.
pub fn write_empty(out_dir: &Path, dir: &str, container: &str) -> Result<FileReport, Error> {
    Keymap::create(out_dir, dir, container)?.finish(0)
}

/// A container file, written entry by entry.
pub(crate) struct Keymap {
    rel_path: String,
    writer: BufWriter<fs::File>,
    written: usize,
}

impl Keymap {
    pub(crate) fn create(base: &Path, dir: &str, container: &str) -> Result<Self, Error> {
        let rel_path = format!("{dir}/{container}.json");
        let mut writer = BufWriter::with_capacity(256 * 1024, create_file(base, &rel_path)?);
        write!(writer, "{{").map_err(|source| Error::Write {
            path: rel_path.clone(),
            source,
        })?;
        Ok(Self {
            rel_path,
            writer,
            written: 0,
        })
    }

    pub(crate) fn write(&mut self, key: i32, value: &impl Serialize) -> Result<(), Error> {
        self.key(&key.to_string())?;
        serde_json::to_writer(&mut self.writer, value).map_err(|source| Error::Serialize {
            path: self.rel_path.clone(),
            source,
        })?;
        self.written += 1;
        Ok(())
    }

    /// Write an entry whose value is already JSON.
    pub(crate) fn write_json(&mut self, key: &str, json: &str) -> Result<(), Error> {
        self.key(key)?;
        self.raw(json)?;
        self.written += 1;
        Ok(())
    }

    /// Write an entry under a key that is not necessarily numeric.
    pub(crate) fn write_json_value(&mut self, key: &str, value: &serde_json::Value) -> Result<(), Error> {
        self.key(key)?;
        serde_json::to_writer(&mut self.writer, value).map_err(|source| Error::Serialize {
            path: self.rel_path.clone(),
            source,
        })?;
        self.written += 1;
        Ok(())
    }

    fn key(&mut self, key: &str) -> Result<(), Error> {
        if self.written > 0 {
            self.raw(",")?;
        }
        serde_json::to_writer(&mut self.writer, key).map_err(|source| Error::Serialize {
            path: self.rel_path.clone(),
            source,
        })?;
        self.raw(":")
    }

    fn raw(&mut self, text: &str) -> Result<(), Error> {
        self.writer.write_all(text.as_bytes()).map_err(|source| Error::Write {
            path: self.rel_path.clone(),
            source,
        })
    }

    /// Close the file off and report on it. `skipped` covers entries dropped on the way in, which
    /// the writer never sees.
    pub(crate) fn finish(mut self, skipped: usize) -> Result<FileReport, Error> {
        let result = write!(self.writer, "}}").and_then(|()| self.writer.flush());
        result.map_err(|source| Error::Write {
            path: self.rel_path.clone(),
            source,
        })?;
        Ok(FileReport {
            path: self.rel_path,
            written: self.written,
            skipped,
        })
    }
}

fn create_file(base: &Path, rel_path: &str) -> Result<fs::File, Error> {
    let path: PathBuf = base.join(rel_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::File::create(&path).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })
}
