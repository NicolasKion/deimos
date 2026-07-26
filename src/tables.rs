//! Turning static data export tables into the containers the client would have held.
//!
//! The export and the client carry the same data under mostly the same names, so most of this is
//! renaming, plus two shape differences: the client writes booleans as `0`/`1`, and uses JSON
//! objects where the export uses lists carrying an inner `_key`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, FileReport, dump::Keymap, sde::Export};

/// `requiredSkillN` / `requiredSkillNLevel` attribute pairs, in slot order. The client keeps skill
/// requirements as their own container as well as on item attributes; the export only has the
/// attributes, so the container gets rebuilt from them here.
const SKILL_REQ_ATTRS: [(i32, i32); 6] = [
    (182, 277),
    (183, 278),
    (184, 279),
    (1285, 1286),
    (1289, 1287),
    (1290, 1288),
];

pub(crate) fn items(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(rename = "groupID")]
        group_id: i32,
        // Absent whenever zero, which is most items for most of these.
        #[serde(default)]
        capacity: f64,
        #[serde(default)]
        mass: f64,
        #[serde(default)]
        radius: f64,
        #[serde(default)]
        volume: f64,
    }

    #[derive(Serialize)]
    struct Out {
        #[serde(rename = "groupID")]
        group_id: i32,
        capacity: f64,
        mass: f64,
        radius: f64,
        volume: f64,
    }

    let mut keymap = Keymap::create(out, "fsd_built", "types")?;
    let counts = export.read("types.jsonl", |record: In| {
        keymap.write(
            record.key,
            &Out {
                group_id: record.group_id,
                capacity: record.capacity,
                mass: record.mass,
                radius: record.radius,
                volume: record.volume,
            },
        )
    })?;
    keymap.finish(counts.skipped)
}

pub(crate) fn groups(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(rename = "categoryID")]
        category_id: i32,
    }

    #[derive(Serialize)]
    struct Out {
        #[serde(rename = "categoryID")]
        category_id: i32,
    }

    let mut keymap = Keymap::create(out, "fsd_built", "groups")?;
    let counts = export.read("groups.jsonl", |record: In| {
        keymap.write(
            record.key,
            &Out {
                category_id: record.category_id,
            },
        )
    })?;
    keymap.finish(counts.skipped)
}

pub(crate) fn item_lists(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize, Serialize)]
    struct List {
        #[serde(rename = "includedTypeIDs", default, skip_serializing_if = "Vec::is_empty")]
        included_items: Vec<i32>,
        #[serde(rename = "includedGroupIDs", default, skip_serializing_if = "Vec::is_empty")]
        included_groups: Vec<i32>,
        #[serde(rename = "includedCategoryIDs", default, skip_serializing_if = "Vec::is_empty")]
        included_cats: Vec<i32>,
        #[serde(rename = "excludedTypeIDs", default, skip_serializing_if = "Vec::is_empty")]
        excluded_items: Vec<i32>,
        #[serde(rename = "excludedGroupIDs", default, skip_serializing_if = "Vec::is_empty")]
        excluded_groups: Vec<i32>,
        #[serde(rename = "excludedCategoryIDs", default, skip_serializing_if = "Vec::is_empty")]
        excluded_cats: Vec<i32>,
    }

    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(flatten)]
        list: List,
    }

    let mut keymap = Keymap::create(out, "fsd_built", "typelist")?;
    let counts = export.read("typeLists.jsonl", |record: In| keymap.write(record.key, &record.list))?;
    keymap.finish(counts.skipped)
}

pub(crate) fn attrs(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(default)]
        stackable: bool,
        #[serde(rename = "highIsGood", default)]
        high_is_good: bool,
        #[serde(rename = "defaultValue", default)]
        default_value: f64,
        #[serde(rename = "minAttributeID", default)]
        min_attr: Option<i32>,
        #[serde(rename = "maxAttributeID", default)]
        max_attr: Option<i32>,
        #[serde(rename = "unitID", default)]
        unit: Option<i32>,
    }

    // Every optional key is written even when null: the reader on the other side does not treat
    // them as defaultable.
    #[derive(Serialize)]
    struct Out {
        stackable: u8,
        #[serde(rename = "highIsGood")]
        high_is_good: u8,
        #[serde(rename = "defaultValue")]
        default_value: f64,
        #[serde(rename = "minAttributeID")]
        min_attr: Option<i32>,
        #[serde(rename = "maxAttributeID")]
        max_attr: Option<i32>,
        #[serde(rename = "unitID")]
        unit: Option<i32>,
    }

    let mut keymap = Keymap::create(out, "fsd_built", "dogmaattributes")?;
    let counts = export.read("dogmaAttributes.jsonl", |record: In| {
        keymap.write(
            record.key,
            &Out {
                stackable: record.stackable.into(),
                high_is_good: record.high_is_good.into(),
                default_value: record.default_value,
                min_attr: record.min_attr,
                max_attr: record.max_attr,
                unit: record.unit,
            },
        )
    })?;
    keymap.finish(counts.skipped)
}

pub(crate) fn effects(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(rename = "effectCategoryID", default)]
        category: i32,
        #[serde(rename = "isAssistance", default)]
        is_assistance: bool,
        #[serde(rename = "isOffensive", default)]
        is_offensive: bool,
        #[serde(rename = "dischargeAttributeID", default)]
        discharge_attr: Option<i32>,
        #[serde(rename = "durationAttributeID", default)]
        duration_attr: Option<i32>,
        #[serde(rename = "rangeAttributeID", default)]
        range_attr: Option<i32>,
        #[serde(rename = "falloffAttributeID", default)]
        falloff_attr: Option<i32>,
        #[serde(rename = "trackingSpeedAttributeID", default)]
        tracking_attr: Option<i32>,
        #[serde(rename = "fittingUsageChanceAttributeID", default)]
        usage_chance_attr: Option<i32>,
        #[serde(rename = "resistanceAttributeID", default)]
        resist_attr: Option<i32>,
        // Modifiers are a list of maps of `func` plus whatever arguments that func takes, and are
        // passed through untouched - the engine is the one that knows the funcs.
        #[serde(rename = "modifierInfo", default)]
        mods: Vec<serde_json::Value>,
    }

    #[derive(Serialize)]
    struct Out {
        #[serde(rename = "effectCategory")]
        category: i32,
        #[serde(rename = "isAssistance")]
        is_assistance: u8,
        #[serde(rename = "isOffensive")]
        is_offensive: u8,
        #[serde(rename = "dischargeAttributeID")]
        discharge_attr: Option<i32>,
        #[serde(rename = "durationAttributeID")]
        duration_attr: Option<i32>,
        #[serde(rename = "rangeAttributeID")]
        range_attr: Option<i32>,
        #[serde(rename = "falloffAttributeID")]
        falloff_attr: Option<i32>,
        #[serde(rename = "trackingSpeedAttributeID")]
        tracking_attr: Option<i32>,
        #[serde(rename = "fittingUsageChanceAttributeID")]
        usage_chance_attr: Option<i32>,
        #[serde(rename = "resistanceAttributeID")]
        resist_attr: Option<i32>,
        #[serde(rename = "modifierInfo")]
        mods: Vec<serde_json::Value>,
    }

    let mut keymap = Keymap::create(out, "fsd_built", "dogmaeffects")?;
    let counts = export.read("dogmaEffects.jsonl", |record: In| {
        keymap.write(
            record.key,
            &Out {
                category: record.category,
                is_assistance: record.is_assistance.into(),
                is_offensive: record.is_offensive.into(),
                discharge_attr: record.discharge_attr,
                duration_attr: record.duration_attr,
                range_attr: record.range_attr,
                falloff_attr: record.falloff_attr,
                tracking_attr: record.tracking_attr,
                usage_chance_attr: record.usage_chance_attr,
                resist_attr: record.resist_attr,
                mods: record.mods,
            },
        )
    })?;
    keymap.finish(counts.skipped)
}

/// Item attributes and effects, plus the skill requirements derived from the attributes.
pub(crate) fn item_dogma(export: &mut Export, out: &std::path::Path) -> Result<(FileReport, FileReport), Error> {
    #[derive(Deserialize, Serialize)]
    struct Attr {
        #[serde(rename = "attributeID")]
        attr_id: i32,
        value: f64,
    }

    #[derive(Deserialize)]
    struct InEffect {
        #[serde(rename = "effectID")]
        effect_id: i32,
        #[serde(rename = "isDefault", default)]
        is_default: bool,
    }

    #[derive(Serialize)]
    struct OutEffect {
        #[serde(rename = "effectID")]
        effect_id: i32,
        #[serde(rename = "isDefault")]
        is_default: u8,
    }

    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(rename = "dogmaAttributes", default)]
        attrs: Vec<Attr>,
        #[serde(rename = "dogmaEffects", default)]
        effects: Vec<InEffect>,
    }

    #[derive(Serialize)]
    struct Out {
        #[serde(rename = "dogmaAttributes")]
        attrs: Vec<Attr>,
        #[serde(rename = "dogmaEffects")]
        effects: Vec<OutEffect>,
    }

    let mut dogma = Keymap::create(out, "fsd_built", "typedogma")?;
    let mut skill_reqs = Keymap::create(out, "fsd_built", "requiredskillsfortypes")?;
    let counts = export.read("typeDogma.jsonl", |record: In| {
        let value = |attr_id: i32| {
            record
                .attrs
                .iter()
                .find(|attr| attr.attr_id == attr_id)
                .map(|attr| attr.value)
        };
        let reqs: BTreeMap<i32, i32> = SKILL_REQ_ATTRS
            .iter()
            .filter_map(|&(skill_attr, level_attr)| Some((value(skill_attr)?, value(level_attr)?)))
            .map(|(skill, level)| (skill as i32, level as i32))
            .collect();
        if !reqs.is_empty() {
            skill_reqs.write(record.key, &reqs)?;
        }
        dogma.write(
            record.key,
            &Out {
                attrs: record.attrs,
                effects: record
                    .effects
                    .into_iter()
                    .map(|effect| OutEffect {
                        effect_id: effect.effect_id,
                        is_default: effect.is_default.into(),
                    })
                    .collect(),
            },
        )
    })?;
    Ok((dogma.finish(counts.skipped)?, skill_reqs.finish(0)?))
}

pub(crate) fn buffs(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize, Serialize)]
    struct AttrMod {
        #[serde(rename = "dogmaAttributeID")]
        attr_id: i32,
    }

    #[derive(Deserialize, Serialize)]
    struct GroupMod {
        #[serde(rename = "dogmaAttributeID")]
        attr_id: i32,
        #[serde(rename = "groupID")]
        group_id: i32,
    }

    #[derive(Deserialize, Serialize)]
    struct SkillMod {
        #[serde(rename = "dogmaAttributeID")]
        attr_id: i32,
        #[serde(rename = "skillID")]
        skill_id: i32,
    }

    #[derive(Deserialize, Serialize)]
    struct Buff {
        #[serde(rename = "aggregateMode")]
        aggregate_mode: String,
        #[serde(rename = "operationName")]
        operation: String,
        #[serde(rename = "itemModifiers", default, skip_serializing_if = "Vec::is_empty")]
        item_mods: Vec<AttrMod>,
        #[serde(rename = "locationModifiers", default, skip_serializing_if = "Vec::is_empty")]
        loc_mods: Vec<AttrMod>,
        #[serde(rename = "locationGroupModifiers", default, skip_serializing_if = "Vec::is_empty")]
        loc_group_mods: Vec<GroupMod>,
        #[serde(
            rename = "locationRequiredSkillModifiers",
            default,
            skip_serializing_if = "Vec::is_empty"
        )]
        loc_skill_mods: Vec<SkillMod>,
    }

    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(flatten)]
        buff: Buff,
    }

    let mut keymap = Keymap::create(out, "fsd_lite", "dbuffcollections")?;
    let counts = export.read("dbuffCollections.jsonl", |record: In| {
        keymap.write(record.key, &record.buff)
    })?;
    keymap.finish(counts.skipped)
}

pub(crate) fn mutas(export: &mut Export, out: &std::path::Path) -> Result<FileReport, Error> {
    #[derive(Deserialize, Serialize)]
    struct Mapping {
        #[serde(rename = "applicableTypes")]
        applicable_items: Vec<i32>,
        #[serde(rename = "resultingType")]
        result_item: i32,
    }

    #[derive(Deserialize)]
    struct InRoll {
        #[serde(rename = "_key")]
        attr_id: i32,
        min: f64,
        max: f64,
    }

    #[derive(Serialize)]
    struct Roll {
        min: f64,
        max: f64,
    }

    #[derive(Deserialize)]
    struct In {
        #[serde(rename = "_key")]
        key: i32,
        #[serde(rename = "inputOutputMapping", default)]
        mappings: Vec<Mapping>,
        #[serde(rename = "attributeIDs", default)]
        rolls: Vec<InRoll>,
    }

    #[derive(Serialize)]
    struct Out {
        #[serde(rename = "inputOutputMapping")]
        mappings: Vec<Mapping>,
        // A list keyed by attribute ID on the way in, an object keyed by it on the way out.
        #[serde(rename = "attributeIDs")]
        rolls: BTreeMap<i32, Roll>,
    }

    let mut keymap = Keymap::create(out, "fsd_built", "dynamicitemattributes")?;
    let counts = export.read("dynamicItemAttributes.jsonl", |record: In| {
        keymap.write(
            record.key,
            &Out {
                mappings: record.mappings,
                rolls: record
                    .rolls
                    .into_iter()
                    .map(|roll| {
                        (
                            roll.attr_id,
                            Roll {
                                min: roll.min,
                                max: roll.max,
                            },
                        )
                    })
                    .collect(),
            },
        )
    })?;
    keymap.finish(counts.skipped)
}
