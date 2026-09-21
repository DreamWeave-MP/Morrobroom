use std::{
    borrow::Cow,
    collections::{
        hash_map::DefaultHasher,
        {BTreeMap, BTreeSet},
    },
    hash::{Hash, Hasher},
    io::{self, Write},
};

use tes3::esp::{EditorId, ObjectFlags, TES3Object, TypeInfo};

use super::get_object_model_path;

pub mod std_write_fgd;
use std_write_fgd::STDWriteFGD;

pub mod write_fgd_prop;
use write_fgd_prop::WriteFGDProp;

#[allow(dead_code)] // Reserved for grouping records by owning plugin in typed output.
type PluginRecordMap<'a> = BTreeMap<&'a String, BTreeSet<Cow<'a, str>>>;

#[allow(dead_code)] // Shared inventory retained alongside the CLI's typed enum list.
pub static SERIALIZABLE_TYPES: [&'static str; 18] = [
    tes3::esp::Activator::TAG_STR,
    tes3::esp::Alchemy::TAG_STR,
    tes3::esp::Apparatus::TAG_STR,
    tes3::esp::Armor::TAG_STR,
    tes3::esp::Book::TAG_STR,
    tes3::esp::Clothing::TAG_STR,
    tes3::esp::Container::TAG_STR,
    tes3::esp::Door::TAG_STR,
    tes3::esp::Ingredient::TAG_STR,
    tes3::esp::LeveledCreature::TAG_STR,
    tes3::esp::LeveledItem::TAG_STR,
    tes3::esp::Light::TAG_STR,
    tes3::esp::Lockpick::TAG_STR,
    tes3::esp::MiscItem::TAG_STR,
    tes3::esp::Probe::TAG_STR,
    tes3::esp::RepairItem::TAG_STR,
    tes3::esp::Static::TAG_STR,
    tes3::esp::Weapon::TAG_STR,
    // tes3::esp::Creature::TAG_STR,
    // tes3::esp::Npc::TAG_STR,
];

#[allow(dead_code)] // Record-level filter retained for callers that already parsed objects.
pub fn is_serializable_type(object: &TES3Object) -> bool {
    match object {
        TES3Object::Activator(_)
        | TES3Object::Alchemy(_)
        | TES3Object::Apparatus(_)
        | TES3Object::Armor(_)
        | TES3Object::Book(_)
        | TES3Object::Clothing(_)
        | TES3Object::Door(_)
        | TES3Object::Ingredient(_)
        | TES3Object::LeveledCreature(_)
        | TES3Object::LeveledItem(_)
        | TES3Object::Light(_)
        | TES3Object::Lockpick(_)
        | TES3Object::MiscItem(_)
        | TES3Object::Probe(_)
        | TES3Object::RepairItem(_)
        | TES3Object::Static(_)
        | TES3Object::Weapon(_) => true,
        _ => false,
    }
}

pub fn is_serializable_tag(tag: &[u8; 4]) -> bool {
    matches!(
        tag,
        tes3::esp::Activator::TAG
            | tes3::esp::Alchemy::TAG
            | tes3::esp::Apparatus::TAG
            | tes3::esp::Armor::TAG
            | tes3::esp::Book::TAG
            | tes3::esp::Clothing::TAG
            | tes3::esp::Door::TAG
            | tes3::esp::Ingredient::TAG
            | tes3::esp::LeveledCreature::TAG
            | tes3::esp::LeveledItem::TAG
            | tes3::esp::Light::TAG
            | tes3::esp::Lockpick::TAG
            | tes3::esp::MiscItem::TAG
            | tes3::esp::Probe::TAG
            | tes3::esp::RepairItem::TAG
            | tes3::esp::Script::TAG
            | tes3::esp::Static::TAG
            | tes3::esp::Weapon::TAG
    )
}

pub fn tag_to_tag_str(tag: &[u8; 4]) -> &'static str {
    match tag {
        tes3::esp::Activator::TAG => tes3::esp::Activator::TAG_STR,
        tes3::esp::Alchemy::TAG => tes3::esp::Alchemy::TAG_STR,
        tes3::esp::Apparatus::TAG => tes3::esp::Apparatus::TAG_STR,
        tes3::esp::Armor::TAG => tes3::esp::Armor::TAG_STR,
        tes3::esp::Book::TAG => tes3::esp::Book::TAG_STR,
        tes3::esp::Clothing::TAG => tes3::esp::Clothing::TAG_STR,
        tes3::esp::Door::TAG => tes3::esp::Door::TAG_STR,
        tes3::esp::Ingredient::TAG => tes3::esp::Ingredient::TAG_STR,
        tes3::esp::LeveledCreature::TAG => tes3::esp::LeveledCreature::TAG_STR,
        tes3::esp::LeveledItem::TAG => tes3::esp::LeveledItem::TAG_STR,
        tes3::esp::Light::TAG => tes3::esp::Light::TAG_STR,
        tes3::esp::Lockpick::TAG => tes3::esp::Lockpick::TAG_STR,
        tes3::esp::MiscItem::TAG => tes3::esp::MiscItem::TAG_STR,
        tes3::esp::Probe::TAG => tes3::esp::Probe::TAG_STR,
        tes3::esp::RepairItem::TAG => tes3::esp::RepairItem::TAG_STR,
        tes3::esp::Script::TAG => tes3::esp::Script::TAG_STR,
        tes3::esp::Static::TAG => tes3::esp::Static::TAG_STR,
        tes3::esp::Weapon::TAG => tes3::esp::Weapon::TAG_STR,
        _ => unimplemented!("Unimplemented record type in tag_to_tag_str: {tag:?}"),
    }
}

/// Return an FGD‑safe identifier (ASCII, A‑Z a‑z 0‑9 _ only).
pub fn encode_fgd_token<S: AsRef<str>>(input: &S) -> Cow<'_, str> {
    use std::borrow::Cow;
    let bytes = input.as_ref().as_bytes();
    let mut out = String::new();

    for &b in bytes {
        let c = b as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            use std::fmt::Write;
            write!(out, "_x{:02X}_", b).unwrap();
        }
    }
    if out == input.as_ref() {
        Cow::Borrowed(input.as_ref())
    } else {
        Cow::Owned(out)
    }
}

/// Decode back to the original UTF‑8 string.
/// Returns Err on malformed sequence.
#[allow(dead_code)] // Kept as the inverse of encode_fgd_token for round-trip consumers.
pub fn decode_fgd_token<S: AsRef<str>>(input: &S) -> Result<Cow<'_, str>, &'static str> {
    let s = input.as_ref();
    if !s.contains("_x") {
        return Ok(Cow::Borrowed(s));
    }

    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' && i + 4 < bytes.len() && bytes[i + 1] == b'x' && bytes[i + 4] == b'_' {
            let hi = bytes[i + 2];
            let lo = bytes[i + 3];
            let v = (hex_val(hi)? << 4) | hex_val(lo)?;
            out.push(v);
            i += 5;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(Cow::Owned(String::from_utf8(out).map_err(|_| "utf8")?))
}

#[allow(dead_code)] // Used by the retained FGD token decoder.
fn hex_val(b: u8) -> Result<u8, &'static str> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err("bad hex"),
    }
}

/// Generates a unique but deterministic RGB color based on the input ID string.
/// Returns [i32; 3] with all values in the range 0..=255.
fn generate_rgb_from_id(id: &str) -> [i32; 3] {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    let hash = hasher.finish();

    // Split hash into 3 bytes for RGB
    [
        ((hash & 0xFF) as u8) as i32,
        (((hash >> 8) & 0xFF) as u8) as i32,
        (((hash >> 16) & 0xFF) as u8) as i32,
    ]
}

pub fn write_base_class<W: Write, D: std::fmt::Display>(
    fgd_string: &mut W,
    class_name: D,
) -> Result<(), io::Error> {
    writeln!(fgd_string, "@BaseClass = {}\n[", class_name)
}

pub fn write_point_class<W: Write>(
    fgd_string: &mut W,
    classes: &[&str],
    bounds: Option<&[i32; 6]>,
    id: String,
) -> Result<(), io::Error> {
    write!(fgd_string, "@PointClass")?;

    write_fgd_base(fgd_string, classes)?;

    let color = write_fgd_size(bounds, fgd_string)?;

    write_fgd_color(&color, fgd_string)?;

    writeln!(fgd_string, " = {}", encode_fgd_token(&id))?;

    Ok(())
}

pub fn write_unplaceable_point_class<W: Write>(
    fgd_string: &mut W,
    classes: &[&str],
    bounds: Option<&[i32; 6]>,
    color: &[i32; 3],
    id: String,
) -> Result<(), io::Error> {
    write!(fgd_string, "@PointClass")?;

    write_fgd_base(fgd_string, classes)?;

    write_fgd_size(bounds, fgd_string)?;

    write_fgd_color(color, fgd_string)?;

    writeln!(fgd_string, " = {id}")?;

    Ok(())
}

pub fn write_light_point_class<W: Write>(
    fgd_string: &mut W,
    classes: &[&str],
    bounds: &[i32; 6],
    color: &[u8; 4],
    id: String,
) -> Result<(), io::Error> {
    write!(fgd_string, "@PointClass")?;

    write_fgd_base(fgd_string, classes)?;

    write_fgd_size(Some(bounds), fgd_string)?;

    write_light_fgd_color(&color, fgd_string)?;

    writeln!(fgd_string, " = {id}")?;

    Ok(())
}

pub fn write_fgd_size<W: Write>(
    bounds: Option<&[i32; 6]>,
    fgd_string: &mut W,
) -> Result<[i32; 3], io::Error> {
    let Some([min_x, min_y, min_z, max_x, max_y, max_z]) = bounds else {
        return Ok([255, 255, 255]);
    };

    write!(
        fgd_string,
        " size({min_x} {min_y} {min_z}, {max_x} {max_y} {max_z})"
    )?;

    let get_color = |(min, max): (i32, i32)| (min.abs() + max.abs()) % 255;

    let (r, g, b) = (
        get_color((*min_x, *max_x)),
        get_color((*min_y, *max_y)),
        get_color((*min_z, *max_z)),
    );

    Ok([r, g, b])
}

pub fn write_fgd_color<W: Write>(color: &[i32; 3], fgd_string: &mut W) -> Result<(), io::Error> {
    let (r, g, b) = (color[0], color[1], color[2]);
    write!(fgd_string, " color({r} {g} {b})")
}

pub fn write_light_fgd_color<W: Write>(
    color: &[u8; 4],
    fgd_string: &mut W,
) -> Result<(), io::Error> {
    let (r, g, b) = (color[0], color[1], color[2]);
    write!(fgd_string, " color({r} {g} {b})")
}

pub fn write_fgd_base<W: Write>(fgd_string: &mut W, classes: &[&str]) -> io::Result<()> {
    if !classes.is_empty() {
        let mut buffer = String::with_capacity(64);

        buffer.push_str(" base(");
        buffer.push_str(&classes.join(",")); // simple, readable
        buffer.push(')');

        write!(fgd_string, "{buffer}")?;
    }

    Ok(())
}

pub fn write_object_flags<W: Write>(fgd_string: &mut W, flags: &ObjectFlags) -> io::Result<()> {
    "ObjectFlags".write_fgd(
        fgd_string,
        "",
        &flags
            .iter_names()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
            .join(" | "),
    )
}

pub fn serialize_objects_as_fgd<W: Write>(
    config_manager: &crate::fgd::ConfigurationManager,
    fgd_string: &mut W,
) -> Result<(), std::io::Error> {
    let set: Vec<&'static str> = config_manager
        .object_types
        .iter()
        .map(|object_type| *object_type)
        .collect();

    write_dictionary(&config_manager.merged_objects, &set, fgd_string)?;

    serialize_base_object_to_fgd(fgd_string)?;

    for (_, (object, parent_plugin)) in &config_manager.merged_objects {
        let bounds = if let Some(path) = get_object_model_path(object) {
            config_manager.get_object_bounds(&path)
        } else {
            None
        };

        match object {
            TES3Object::Script(_) => continue,
            _ => serialize_object_to_fgd(object, bounds, parent_plugin, fgd_string)?,
        }
    }

    Ok(())
}

pub fn get_object_type_data(
    object_type: &'static str,
) -> (&'static str, &'static str, &'static str, &'static [u8; 4]) {
    match object_type {
        "STAT" => (
            "StaticList",
            "static_list",
            "List of all static objects in this configuration",
            tes3::esp::Static::TAG,
        ),
        "ACTI" => (
            "ActivatorList",
            "activator_list",
            "List of all activators in this configuration",
            tes3::esp::Activator::TAG,
        ),
        "SCPT" => (
            "ScriptList",
            "script_list",
            "List of all mwscripts in this configuration",
            tes3::esp::Script::TAG,
        ),
        "INGR" => (
            "IngredientList",
            "ingredient_list",
            "List of all ingredients in this configuration",
            tes3::esp::Ingredient::TAG,
        ),
        "LIGH" => (
            "LightList",
            "light_list",
            "List of all lights in this configuration",
            tes3::esp::Light::TAG,
        ),
        "APPA" => (
            "ApparatusList",
            "apparatus_list",
            "List of all alchemy apparatuses in this configuration",
            tes3::esp::Apparatus::TAG,
        ),
        "BOOK" => (
            "BookList",
            "book_list",
            "List of all books in this configuration",
            tes3::esp::Book::TAG,
        ),
        "DOOR" => (
            "DoorList",
            "door_list",
            "List of all doors in this configuration",
            tes3::esp::Door::TAG,
        ),
        "ARMO" => (
            "ArmorList",
            "armor_list",
            "List of all armors in this configuration",
            tes3::esp::Armor::TAG,
        ),
        "WEAP" => (
            "WeaponList",
            "weapon_list",
            "List of all weapons in this configuration",
            tes3::esp::Weapon::TAG,
        ),
        "ALCH" => (
            "PotionList",
            "potion_list",
            "List of all potions in this configuration",
            tes3::esp::Alchemy::TAG,
        ),
        "CONT" => (
            "ContainerList",
            "container_list",
            "List of all containers in this configuration",
            tes3::esp::Container::TAG,
        ),
        "REPA" => (
            "RepairItemList",
            "repairitem_list",
            "List of all repair items in this configuration",
            tes3::esp::RepairItem::TAG,
        ),
        "LOCK" => (
            "LockpickList",
            "lockpick_list",
            "List of all lockpicks in this configuration",
            tes3::esp::Lockpick::TAG,
        ),
        "PROB" => (
            "ProbeList",
            "probe_list",
            "List of all probes in this configuration",
            tes3::esp::Probe::TAG,
        ),
        "LEVC" => (
            "LeveledCreatureList",
            "leveledcreature_list",
            "List of all leveled creatures in this configuration",
            tes3::esp::LeveledCreature::TAG,
        ),
        "LEVI" => (
            "LeveledItemList",
            "leveleditem_list",
            "List of all leveled items in this configuration",
            tes3::esp::LeveledItem::TAG,
        ),
        "CLOT" => (
            "ClothingList",
            "clothing_list",
            "List of all clothing items in this configuration",
            tes3::esp::Clothing::TAG,
        ),
        "MISC" => (
            "MiscList",
            "misc_list",
            "List of all miscellaneous items in this configuration",
            tes3::esp::MiscItem::TAG,
        ),
        _ => unimplemented!(),
    }
}

#[allow(dead_code)] // Retained for per-type FGD files in addition to combined output.
pub fn serialize_typed_objects_as_fgd<W: Write>(
    config_manager: &crate::fgd::ConfigurationManager,
    fgd_string: &mut W,
    object_type: &'static str,
) -> Result<(), std::io::Error> {
    serialize_base_object_to_fgd(fgd_string)?;

    write_dictionary(&config_manager.merged_objects, &[object_type], fgd_string)?;

    let (class_name, list_name, list_desc, object_type) = get_object_type_data(object_type);

    write_choice_list_by_tag(
        &config_manager.merged_objects,
        fgd_string,
        class_name,
        object_type,
        list_name,
        list_desc,
    )?;

    for (_, (object, parent_plugin)) in &config_manager.merged_objects {
        let bounds = if let Some(path) = crate::fgd::get_object_model_path(object) {
            config_manager.get_object_bounds(&path)
        } else {
            None
        };

        if object.tag() == object_type {
            serialize_object_to_fgd(object, bounds, parent_plugin, fgd_string)?
        }
    }

    Ok(())
}

pub fn serialize_object_to_fgd<W: Write>(
    object: &TES3Object,
    bounds: Option<&[i32; 6]>,
    parent_plugin: &String,
    fgd_string: &mut W,
) -> Result<(), io::Error> {
    match object {
        TES3Object::Static(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Activator(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Ingredient(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Light(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Alchemy(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Apparatus(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Book(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Weapon(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Clothing(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Armor(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Container(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Door(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::LeveledCreature(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::LeveledItem(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Lockpick(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::Probe(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::RepairItem(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        TES3Object::MiscItem(record) => {
            record.write_fgd(fgd_string, parent_plugin, bounds)?;
        }
        // TES3Object::Creature(record) => {
        //     record.write_fgd(fgd_string, parent_plugin, bounds)?;
        // }
        // TES3Object::Npc(record) => {
        //     record.write_fgd(fgd_string, parent_plugin, bounds)?;
        // }
        TES3Object::Script(_) => {}
        _ => unimplemented!(
            "Unimplemented record type in serialize_object_to_fgd: {}",
            object.tag_str()
        ),
    };

    Ok(())
}

pub fn serialize_base_object_to_fgd<W: Write>(fgd_string: &mut W) -> Result<(), std::io::Error> {
    write_base_class(fgd_string, "world_Base")?;

    writeln!(
        fgd_string,
        "    mangle (string) : \"Object Rotation\" : \"0 0 0\"\n]\n"
    )
}

/// Write a choice list that groups TES3 records by owner plug‑in and displays them in an FGD
/// `choices` block.
///
/// The list syntax produced looks like:
///
/// ```fgd
///   ItemId(choices) : "Item list" : "" =
///   [
///       "example_id" : "myplugin.esp"
///       ...
///   ]
/// ```
pub fn write_choice_list_by_tag<W: io::Write>(
    merged_objects: &BTreeMap<String, (TES3Object, String)>,
    out: &mut W,
    class_name: &str,
    object_tag: &[u8; 4],
    list_name: &str,
    list_desc: &str,
) -> io::Result<()> {
    // Write base class declaration
    write_base_class(out, class_name)?;

    writeln!(
        out,
        "    {list_name}(choices) : \"{list_desc}\" : \"\" =\n    ["
    )?;

    // Build: plugin -> set of ids
    let mut by_plugin: BTreeMap<&str, BTreeSet<Cow<'_, str>>> = BTreeMap::new();

    for (object, owner_plugin) in merged_objects.values() {
        if object.tag() == object_tag {
            let id = object.editor_id_ascii_lowercase(); // Cow<'_, str>
            by_plugin
                .entry(owner_plugin.as_str())
                .or_default()
                .insert(id);
        }
    }

    // Emit each id
    for (plugin, ids) in by_plugin {
        for id in ids {
            writeln!(out, "        \"{id}\" : \"{plugin}\"")?;
        }
    }

    writeln!(out, "    ]\n]\n")
}

pub fn write_dictionary<W: Write>(
    merged_objects: &BTreeMap<String, (TES3Object, String)>,
    classes: &[&'static str],
    fgd_string: &mut W,
) -> Result<(), io::Error> {
    let mut dictionary_class_list: Vec<&'static str> = Vec::new();

    classes
        .into_iter()
        .map(|class| get_object_type_data(class))
        .try_for_each(|(class_name, list_name, list_desc, object_type)| {
            dictionary_class_list.push(class_name);

            write_choice_list_by_tag(
                merged_objects,
                fgd_string,
                class_name,
                object_type,
                list_name,
                list_desc,
            )
        })?;

    writeln!(
        fgd_string,
        "@PointClass base({}) size(-32 -32 -32, 32 32 32) color(255 0 255) = tool_Dictionary : \"Global dictionary of all non-referenceable records in your openmw installation.\" []\n",
        dictionary_class_list.join(", ")
    )?;

    Ok(())
}
