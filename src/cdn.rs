//! Reading client files off CCP's content CDN.
//!
//! The route is the launcher's:
//!
//! 1. `binaries.eveonline.com/eveclient_<SERVER>.json` names the build a server is running;
//! 2. `binaries.eveonline.com/eveonline_<build>.txt` indexes the application files, one of which is
//!    the resource index;
//! 3. that resource index maps every `res:/` path to a content hash, and
//!    `resources.eveonline.com/<hash>` serves the file.
//!
//! [`Cdn::connect`] walks all three, keeping the index entries under `res:/staticdata/`. The
//! resource index is around 20 MB, so connecting takes a moment; a [`Cdn`] is worth holding on to.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
};

use crate::Error;

const BINARIES: &str = "https://binaries.eveonline.com";
const RESOURCES: &str = "https://resources.eveonline.com";
const STATIC_DATA: &str = "res:/staticdata/";

/// The live server. Others CCP publishes builds for include `sisi`.
pub const TRANQUILITY: &str = "tq";

/// A client static data file, as listed in the client's resource index.
#[derive(Debug, Clone)]
pub struct Resource {
    /// Full resource path, e.g. `res:/staticdata/fighterabilities.static`.
    pub path: String,
    /// Content hash, which is also the path it is served under.
    pub hash: String,
    /// Uncompressed size in bytes.
    pub bytes: u64,
}

impl Resource {
    /// Container name: the file stem, which is what the container is called in a dump.
    pub fn container(&self) -> &str {
        let file = self.path.rsplit('/').next().unwrap_or(&self.path);
        file.rsplit_once('.').map_or(file, |(stem, _)| stem)
    }

    /// Whether this is a `.static` file, and so a candidate for [`Cdn::read_container`].
    ///
    /// Not every `.static` file turns out to hold a cache table - a handful are something else
    /// wearing the same extension - and that is only knowable once the file has been fetched.
    pub fn is_container(&self) -> bool {
        self.path.ends_with(".static")
    }
}

/// A connection to the content CDN for one client build.
pub struct Cdn {
    http: reqwest::blocking::Client,
    build: String,
    /// Readable containers by name, plus everything else under `res:/staticdata/` for [`Cdn::fetch`].
    resources: BTreeMap<String, Resource>,
}

impl Cdn {
    /// Connect to the CDN for whatever build Tranquility is running.
    pub fn connect() -> Result<Self, Error> {
        Self::connect_to(TRANQUILITY)
    }

    /// Connect to the CDN for the build a named server is running, e.g. `tq` or `sisi`.
    pub fn connect_to(server: &str) -> Result<Self, Error> {
        #[derive(serde::Deserialize)]
        struct ClientInfo {
            build: String,
        }

        let http = reqwest::blocking::Client::builder().timeout(None).build()?;
        let info: ClientInfo = http
            .get(format!("{BINARIES}/eveclient_{}.json", server.to_uppercase()))
            .send()?
            .error_for_status()?
            .json()?;
        let app_index = format!("{BINARIES}/eveonline_{}.txt", info.build);
        let res_index = lookup(&http, &app_index, "app:/resfileindex.txt")?;
        let resources = static_data_index(&http, &format!("{BINARIES}/{res_index}"))?;
        Ok(Self {
            http,
            build: info.build,
            resources,
        })
    }

    /// Build the connection is pinned to.
    pub fn build(&self) -> &str {
        &self.build
    }

    /// Every static data resource the client has, readable or not.
    pub fn resources(&self) -> impl Iterator<Item = &Resource> {
        self.resources.values()
    }

    /// Containers that could be read, in name order.
    pub fn containers(&self) -> impl Iterator<Item = &Resource> {
        self.resources.values().filter(|resource| resource.is_container())
    }

    /// Look one container up by name.
    pub fn container(&self, name: &str) -> Option<&Resource> {
        self.resources.get(name).filter(|resource| resource.is_container())
    }

    /// Read a container: its rows, as (key, raw JSON value) in the order stored.
    ///
    /// `None` means the file exists but is not a cache database, which a few `.static` files are
    /// not. Values are handed back as raw JSON rather than parsed, since most callers are writing
    /// them straight back out and parsing would only cost.
    pub fn read_container(&self, name: &str) -> Result<Option<Vec<(String, String)>>, Error> {
        let resource = self
            .container(name)
            .ok_or_else(|| Error::MissingContainer(name.to_owned()))?;
        let bytes = self.fetch(&resource.path)?;

        // rusqlite wants a path, and containers are small enough that a trip through a temporary
        // file is cheaper than anything clever.
        let scratch = std::env::temp_dir().join(format!("deimos-{}-{name}.static", self.build));
        std::fs::write(&scratch, &bytes).map_err(|source| Error::Write {
            path: scratch.display().to_string(),
            source,
        })?;
        let rows = read_cache_table(&scratch, name);
        let _ = std::fs::remove_file(&scratch);
        rows
    }

    /// Fetch a static data resource by its `res:/` path.
    pub fn fetch(&self, res_path: &str) -> Result<Vec<u8>, Error> {
        let hash = self
            .resources
            .values()
            .find(|resource| resource.path == res_path)
            .map(|resource| &resource.hash)
            .ok_or_else(|| Error::MissingClientFile(res_path.to_owned()))?;
        Ok(self
            .http
            .get(format!("{RESOURCES}/{hash}"))
            .send()?
            .error_for_status()?
            .bytes()?
            .to_vec())
    }
}

/// Rows of the `cache` table a container holds, or `None` if this file has no such table.
fn read_cache_table(db_path: &std::path::Path, container: &str) -> Result<Option<Vec<(String, String)>>, Error> {
    let fail = |source| Error::ClientData {
        container: container.to_owned(),
        source,
    };
    let Ok(db) = rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Ok(None);
    };
    let Ok(mut statement) = db.prepare("select key, value from cache") else {
        return Ok(None);
    };
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(fail)?;
    rows.collect::<Result<Vec<_>, _>>().map(Some).map_err(fail)
}

/// Every `res:/staticdata/` entry in the resource index, keyed by container name.
fn static_data_index(http: &reqwest::blocking::Client, url: &str) -> Result<BTreeMap<String, Resource>, Error> {
    let mut resources = BTreeMap::new();
    for_each_index_line(http, url, |path, hash, bytes| {
        if let Some(bytes) = bytes.filter(|_| path.starts_with(STATIC_DATA)) {
            let resource = Resource {
                path: path.to_owned(),
                hash: hash.to_owned(),
                bytes,
            };
            resources.insert(resource.container().to_owned(), resource);
        }
        true
    })?;
    Ok(resources)
}

/// Content hash of one path in an index, stopping as soon as it turns up.
fn lookup(http: &reqwest::blocking::Client, url: &str, path: &str) -> Result<String, Error> {
    let mut found = None;
    for_each_index_line(http, url, |line_path, hash, _| {
        if line_path != path {
            return true;
        }
        found = Some(hash.to_owned());
        false
    })?;
    found.ok_or_else(|| Error::MissingClientFile(path.to_owned()))
}

/// Walk an index file - CSV lines of `path,hash,md5,size,compressed size` - as it downloads,
/// stopping early when `handle` returns false. Indexes run to tens of megabytes.
fn for_each_index_line(
    http: &reqwest::blocking::Client,
    url: &str,
    mut handle: impl FnMut(&str, &str, Option<u64>) -> bool,
) -> Result<(), Error> {
    let response = http.get(url).send()?.error_for_status()?;
    for line in BufReader::with_capacity(256 * 1024, response).lines() {
        let line = line.map_err(|source| Error::Read {
            entry: url.to_owned(),
            source,
        })?;
        let mut fields = line.split(',');
        let (Some(path), Some(hash)) = (fields.next(), fields.next()) else {
            continue;
        };
        let bytes = fields.nth(1).and_then(|size| size.parse().ok());
        if !handle(path, hash, bytes) {
            break;
        }
    }
    Ok(())
}
