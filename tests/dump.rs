//! Checks against the live data.
//!
//! The CDN tests talk to CCP's servers, which is the whole point of the crate, so they need a
//! network connection. The export test additionally needs a static data export, which is a 100 MB
//! download; point `DEIMOS_SDE_ZIP` at one to run it, or leave it unset and it will say so and
//! pass.

use std::{collections::BTreeMap, path::PathBuf};

use deimos::{
    cdn::Cdn,
    dump::{self, Containers, Tables},
    sde::Export,
};

#[test]
fn client_containers_are_readable() {
    let cdn = Cdn::connect().expect("could not reach the content CDN");
    assert!(
        cdn.build().parse::<u32>().is_ok(),
        "build {} is not a number",
        cdn.build()
    );

    let readable: Vec<&str> = cdn.containers().map(|resource| resource.container()).collect();
    for expected in ["fighterabilities", "fighterabilitiesbytype", "dbuffcollections"] {
        assert!(readable.contains(&expected), "no {expected} container, only {readable:?}");
    }

    // Packed containers are listed but not offered as readable, rather than quietly dropped.
    let packed = cdn
        .resources()
        .find(|resource| resource.container() == "spacecomponentsbytype")
        .expect("spacecomponentsbytype is not listed at all");
    assert!(!packed.is_container(), "spacecomponentsbytype claims to be a container");
    assert!(
        cdn.container("spacecomponentsbytype").is_none(),
        "a packed container was handed out as readable"
    );

    // Values are JSON in the shape the client holds them.
    let abilities = cdn
        .read_container("fighterabilities")
        .expect("could not read abilities")
        .expect("fighterabilities is not a cache database");
    assert!(abilities.len() > 20, "only {} fighter abilities", abilities.len());
    for (key, value) in &abilities {
        assert!(key.parse::<i32>().is_ok(), "ability key {key} is not an ID");
        let parsed: serde_json::Value = serde_json::from_str(value).expect("ability value is not JSON");
        assert!(
            parsed.get("disallowInHighSec").is_some_and(serde_json::Value::is_boolean),
            "ability {key} has no disallowInHighSec flag: {value}"
        );
    }
    println!("build {}, {} candidate containers", cdn.build(), readable.len());
}

#[test]
fn export_and_client_agree_on_warfare_buffs() {
    let Some(zip) = std::env::var_os("DEIMOS_SDE_ZIP").map(PathBuf::from) else {
        println!("DEIMOS_SDE_ZIP is not set, skipping - see the module docs");
        return;
    };

    // The export and the client both describe warfare buffs, by completely different routes, so
    // one is a check on the other.
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("export-dump");
    let mut export = Export::open(&zip).expect("could not open the export");
    let files = dump::write_export(&mut export, &out_dir).expect("could not write the dump");
    assert!(
        files.iter().all(|file| file.skipped == 0),
        "entries were dropped: {:?}",
        files.iter().filter(|file| file.skipped > 0).collect::<Vec<_>>()
    );

    let from_export: BTreeMap<String, serde_json::Value> = serde_json::from_reader(
        std::fs::File::open(deimos::container_path(&out_dir, "fsd_lite", "dbuffcollections")).unwrap(),
    )
    .expect("the written container is not JSON");

    let cdn = Cdn::connect().expect("could not reach the content CDN");
    let from_client = cdn
        .read_container("dbuffcollections")
        .expect("could not read buffs")
        .expect("dbuffcollections is not a cache database");
    assert_eq!(from_client.len(), from_export.len(), "the two sources disagree on count");

    // The client carries editor fields the export does not, so compare what the export claims to
    // describe, treating an absent modifier list as an empty one.
    let lists = [
        "itemModifiers",
        "locationModifiers",
        "locationGroupModifiers",
        "locationRequiredSkillModifiers",
    ];
    for (key, value) in &from_client {
        let client: serde_json::Value = serde_json::from_str(value).unwrap();
        let ours = from_export.get(key).unwrap_or_else(|| panic!("buff {key} is missing"));
        for field in ["aggregateMode", "operationName"] {
            assert_eq!(ours.get(field), client.get(field), "buff {key} differs on {field}");
        }
        for field in lists {
            let empty = serde_json::json!([]);
            let ours = ours.get(field).unwrap_or(&empty);
            let theirs = client.get(field).unwrap_or(&empty);
            assert_eq!(ours, theirs, "buff {key} differs on {field}");
        }
    }
    println!("{} warfare buffs agree across both sources", from_client.len());
}

#[test]
fn named_containers_land_where_they_belong() {
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("client-dump");
    let cdn = Cdn::connect().expect("could not reach the content CDN");
    let files = dump::write_containers(&cdn, Containers::Named(&["fighterabilitiesbytype"]), &out_dir)
        .expect("could not write containers");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "fsd_lite/fighterabilitiesbytype.json");
    assert!(files[0].written > 50, "only {} entries", files[0].written);

    let written: BTreeMap<String, serde_json::Value> = serde_json::from_reader(
        std::fs::File::open(deimos::container_path(&out_dir, "fsd_lite", "fighterabilitiesbytype")).unwrap(),
    )
    .expect("the written container is not JSON");
    let (_, first) = written.iter().next().expect("nothing was written");
    assert!(
        first.get("abilitySlot0").is_some(),
        "an item with fighter abilities has no ability slots: {first}"
    );
}

#[test]
fn every_export_table_is_written() {
    let Some(zip) = std::env::var_os("DEIMOS_SDE_ZIP").map(PathBuf::from) else {
        println!("DEIMOS_SDE_ZIP is not set, skipping - see the module docs");
        return;
    };

    // Nothing in the export should go unwritten, since the point of the verbatim pass is that a
    // consumer never has to go back to the zip.
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("table-dump");
    let mut export = Export::open(&zip).expect("could not open the export");
    let names = ["dogmaUnits", "metaGroups", "marketGroups"];
    let files = dump::write_tables(&mut export, Tables::Named(&names), &out_dir).expect("could not write tables");

    assert_eq!(files.len(), names.len());
    for (file, name) in files.iter().zip(names) {
        assert_eq!(file.path, format!("sde/{}.json", name.to_lowercase()));
        assert!(file.written > 0, "{} came out empty", file.path);
        assert_eq!(file.skipped, 0, "{} dropped entries", file.path);
    }

    let expected: usize = export
        .tables()
        .iter()
        .filter(|name| name.ends_with(".jsonl") && *name != "_sde.jsonl")
        .count();
    let written = dump::write_tables(&mut export, Tables::All, &out_dir)
        .expect("could not write every table")
        .len();
    assert_eq!(written, expected, "not every export table was written");
    println!("{written} export tables written verbatim");
}
