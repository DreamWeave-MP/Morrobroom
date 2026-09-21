use std::{
    cmp::min,
    collections::{BTreeSet, HashMap, HashSet},
    io,
    path::Path,
};

use clap::Parser;
use morrobroom::slipgate::Vector3 as SV3;
use tes3::esp::{self, Cell, EditorId, Header, Plugin, Static, TES3Object};

use morrobroom::{FindLowest, create_workdir, get_prop};

mod broom_args;
use broom_args::{BroomCommand, MorrobroomArgs, default_object_types};

mod brush_ni_node;
use brush_ni_node::BrushNiNode;

mod map_data;
use map_data::MapData;

mod mesh;
use mesh::Mesh;

mod game_object;
mod surfaces;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> io::Result<()> {
    match MorrobroomArgs::parse().command {
        BroomCommand::Compile {
            map_path,
            object_scale,
            output_path,
            no_lightmaps,
        } => compile_map(
            &map_path,
            object_scale,
            output_path.as_deref(),
            !no_lightmaps,
        ),
        BroomCommand::FGD {
            object_scale,
            object_types,
            output_path,
            openmw_config,
        } => {
            let object_types = object_types.unwrap_or_else(|| default_object_types().into());
            let object_type_tags: Vec<&'static str> = object_types
                .iter()
                .map(broom_args::TES3ObjectType::as_str)
                .collect();
            morrobroom::fgd::generate_fgd(
                openmw_config.as_deref(),
                &object_type_tags,
                object_scale,
                &output_path,
            )
            .map_err(|error| io::Error::other(error.to_string()))
        }
    }
}

fn compile_map(
    map_path: &Path,
    object_scale: f32,
    output_path: Option<&Path>,
    lightmaps_enabled: bool,
) -> io::Result<()> {
    let (work_dir, map_dir) = create_workdir(map_path)
        .map_err(|error_string| io::Error::new(io::ErrorKind::InvalidInput, error_string))?;
    let map_name = map_path.to_string_lossy().to_string();
    let map_data = MapData::new(&map_name, lightmaps_enabled);
    assert_eq!(
        map_data.lightmap_geometry().is_some(),
        lightmaps_enabled,
        "lightmap preparation did not honor the compiler option"
    );
    assert!(
        !map_data.geomap.entity_brushes.is_empty(),
        "No brushes found in map! You probably used an apostrophe in worldspawn properties."
    );

    let plugin_path = output_path.map_or_else(
        || {
            let mut path = map_path.to_path_buf();
            path.set_extension("omwaddon");
            path
        },
        Path::to_path_buf,
    );
    let mut plugin = esp::Plugin::from_path(&plugin_path).unwrap_or_default();
    let mut state = CompileState {
        map_data: &map_data,
        work_dir: &work_dir,
        map_dir: &map_dir,
        object_scale,
        cell: None,
        created_objects: Vec::new(),
        processed_base_objects: HashSet::new(),
        used_indices: used_reference_indices(&plugin),
    };
    process_brush_entities(&mut state);
    process_point_entities(&mut state);
    let CompileState {
        cell,
        mut created_objects,
        mut processed_base_objects,
        used_indices: _,
        ..
    } = state;

    if let Some(cell) = cell {
        processed_base_objects.insert(cell.editor_id().to_string());
        created_objects.push(cell.into());
    }
    let point_light_string = format!("{map_dir}-PL");
    plugin.objects.retain(|object| {
        let editor_id = object.editor_id();
        !processed_base_objects.contains(editor_id.as_ref())
            && !editor_id.contains(&point_light_string)
    });
    plugin.objects.extend(created_objects);
    create_header_if_missing(&mut plugin);
    plugin.sort_objects();
    plugin
        .save_path(&plugin_path)
        .unwrap_or_else(|_| panic!("Saving {} failed!", plugin_path.display()));
    println!("Wrote {} to disk successfully.", plugin_path.display());
    Ok(())
}

struct CompileState<'a> {
    map_data: &'a MapData,
    work_dir: &'a Path,
    map_dir: &'a str,
    object_scale: f32,
    cell: Option<Cell>,
    created_objects: Vec<TES3Object>,
    processed_base_objects: HashSet<String>,
    used_indices: BTreeSet<u32>,
}

fn used_reference_indices(plugin: &Plugin) -> BTreeSet<u32> {
    plugin
        .objects_of_type::<Cell>()
        .flat_map(|cell| {
            cell.references
                .keys()
                .filter(|(mast_index, _)| *mast_index == 0)
                .map(|(_, reference_index)| *reference_index)
        })
        .collect()
}

fn process_brush_entities(state: &mut CompileState<'_>) {
    for (entity_id, brushes) in state.map_data.geomap.entity_brushes.iter() {
        process_brush_entity(state, *entity_id, brushes);
    }
}

fn process_brush_entity(
    state: &mut CompileState<'_>,
    entity_id: morrobroom::slipgate::entity::EntityId,
    brushes: &[morrobroom::slipgate::brush::BrushId],
) {
    let prop_map = state.map_data.get_entity_properties(entity_id);
    let mut mesh = Mesh::from_map(brushes, state.map_data, state.object_scale, entity_id);
    attach_group_nodes(&mut mesh, &prop_map, state.map_data);

    let ref_id = prop_map.get(&"RefId".to_string()).map_or_else(
        || format!("{}-scene-{entity_id}", state.map_dir),
        |id| id[..min(id.len(), 32)].to_string(),
    );
    let mesh_name = prop_map.get(&"Model".to_string()).map_or_else(
        || format!("{}/{ref_id}.nif", state.map_dir),
        |name| (*name).clone(),
    );
    if !state.processed_base_objects.insert(ref_id.clone()) {
        println!("Placing new instance of {ref_id}");
    }

    if !assign_game_object(&mut mesh, &prop_map, &ref_id, &mesh_name, state) {
        return;
    }

    mesh.worldspace_position = Mesh::centroid(&mesh.node_distances) * state.object_scale;
    mesh.mangle = get_prop("mangle", &prop_map)
        .map_or_else(|| get_rotation("0 0 0"), |mangle| get_rotation(mangle));
    if !state.created_objects.contains(&mesh.game_object) {
        let mesh_path = state.work_dir.join("Meshes").join(&mesh_name);
        println!(
            "Saving base object definition & mesh for {ref_id} to plugin as {}",
            mesh_path.display()
        );
        mesh.save(&mesh_path.to_string_lossy().to_string());
        state.created_objects.push(mesh.game_object.clone());
    }
    append_cell_reference(
        &mut state.used_indices,
        &mut state.cell,
        &ref_id,
        mesh.worldspace_position,
        mesh.mangle,
    );
}

fn attach_group_nodes(mesh: &mut Mesh, prop_map: &HashMap<&String, &String>, map_data: &MapData) {
    let Some(group_id) = prop_map.get(&"_tb_id".to_string()) else {
        return;
    };
    let mut ref_instances = 0;
    let mut processed_group_objects = HashSet::new();
    for (entity_id, brushes) in map_data.geomap.entity_brushes.iter() {
        let properties = map_data.get_entity_properties(*entity_id);
        if properties.contains_key(&"_tb_id".to_string())
            || properties.get(&"_tb_group".to_string()) != Some(group_id)
        {
            continue;
        }
        if let Some(ref_id) = properties.get(&"RefId".to_string()) {
            ref_instances += 1;
            if !processed_group_objects.insert((*ref_id).clone()) {
                println!(
                    "We don't have full refId support yet, but this object {ref_id} has appeared in this group {ref_instances} times"
                );
                continue;
            }
            println!("Adding {ref_id} to unique group set.");
        }
        for node in BrushNiNode::from_brushes(brushes, map_data, *entity_id) {
            mesh.attach_node(node, map_data.vfs());
        }
    }
}

fn assign_game_object(
    mesh: &mut Mesh,
    props: &HashMap<&String, &String>,
    ref_id: &str,
    mesh_name: &str,
    state: &mut CompileState<'_>,
) -> bool {
    let Some(classname) = props.get(&"classname".to_string()) else {
        return true;
    };
    match classname.as_str() {
        "world_Activator" => mesh.game_object = game_object::activator(props, ref_id, mesh_name),
        "world_Container" => mesh.game_object = game_object::container(props, ref_id, mesh_name),
        "item_Alchemy" => mesh.game_object = game_object::potion(props, ref_id, mesh_name),
        "item_Apparatus" => mesh.game_object = game_object::apparatus(props, ref_id, mesh_name),
        "item_Armor" => mesh.game_object = game_object::armor(props, ref_id, mesh_name),
        "item_Book" => mesh.game_object = game_object::book(props, ref_id, mesh_name),
        "item_Ingredient" => mesh.game_object = game_object::ingredient(props, ref_id, mesh_name),
        "item_Light" => {
            mesh.game_object = game_object::light(props, state.object_scale, ref_id, mesh_name);
        }
        "item_Misc" => mesh.game_object = game_object::misc(props, ref_id, mesh_name),
        "worldspawn" => {
            let mut local_cell = game_object::cell(props);
            if local_cell.name.is_empty() {
                local_cell.name = state.map_dir.to_string();
            }
            state.processed_base_objects.insert(local_cell.name.clone());
            state.processed_base_objects.insert(ref_id.to_string());
            state.cell = Some(local_cell);
            mesh.game_object = Static {
                id: ref_id.to_string(),
                mesh: mesh_name.to_string(),
                flags: esp::ObjectFlags::default(),
            }
            .into();
        }
        "world_Detail" => {
            state.processed_base_objects.insert(ref_id.to_string());
            mesh.game_object = Static {
                id: ref_id.to_string(),
                mesh: mesh_name.to_string(),
                ..Default::default()
            }
            .into();
        }
        _ => {
            println!("No matching object type found! {classname} requested for {ref_id}");
            return false;
        }
    }
    true
}

fn process_point_entities(state: &mut CompileState<'_>) {
    for entity_id in state.map_data.geomap.point_entities.iter() {
        let prop_map = state.map_data.get_entity_properties(*entity_id);
        let class = prop_map
            .get(&"classname".to_string())
            .expect("All point entities have class names")
            .as_str();
        if class.contains("Light_Point") {
            let ref_id = format!("{}-PL-{}", state.map_dir, state.used_indices.find_lowest());
            let ref_id = ref_id[..min(ref_id.len(), 32)].to_string();
            let radius = class
                .chars()
                .skip_while(|character| !character.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .expect("All point light types should have a radius encoded in their classnames!");
            state.created_objects.push(game_object::point_light(
                &prop_map,
                state.object_scale,
                radius,
                &ref_id,
            ));
            append_cell_reference(
                &mut state.used_indices,
                &mut state.cell,
                &ref_id,
                point_entity_position(state.object_scale, &prop_map),
                [0.0; 3],
            );
        } else if class == "world_CreatureList" {
            let ref_id = required_ref_id(&prop_map, *entity_id, "creature list");
            if state.processed_base_objects.insert(ref_id.clone()) {
                state
                    .created_objects
                    .push(game_object::creature_list(&prop_map, &ref_id));
            }
            append_cell_reference(
                &mut state.used_indices,
                &mut state.cell,
                &ref_id,
                point_entity_position(state.object_scale, &prop_map),
                [0.0; 3],
            );
        } else if class == "world_ItemList" {
            let ref_id = required_ref_id(&prop_map, *entity_id, "item list");
            if state.processed_base_objects.insert(ref_id.clone()) {
                state
                    .created_objects
                    .push(game_object::item_list(&prop_map, &ref_id));
            }
        } else {
            println!("Unidentified point entity class: {class}");
        }
    }
}

fn required_ref_id(
    props: &HashMap<&String, &String>,
    entity_id: morrobroom::slipgate::entity::EntityId,
    kind: &str,
) -> String {
    props
        .get(&"RefId".to_string())
        .map_or_else(
            || panic!("RefIds are mandatory for all point entities, failed on {kind}, entity ID: {entity_id}"),
            |ref_id| ref_id[..min(ref_id.len(), 32)].to_string(),
        )
}

fn point_entity_position(scale_mode: f32, prop_map: &HashMap<&String, &String>) -> SV3 {
    let coords: Vec<f32> = prop_map
        .iter()
        .find(|(key, _)| key.as_str() == "origin")
        .map_or_else(
            || {
                eprintln!("All point entities must have an origin!");
                std::process::exit(256);
            },
            |(_, value)| {
                value
                    .split_whitespace()
                    .map(|coordinate| coordinate.parse().expect("Invalid coordinate"))
                    .collect()
            },
        );
    assert_eq!(coords.len(), 3, "Origin must have exactly 3 coordinates");
    SV3::new(coords[0], coords[1], coords[2]) * scale_mode
}

fn append_cell_reference(
    used_indices: &mut BTreeSet<u32>,
    cell: &mut Option<Cell>,
    ref_id: &str,
    translation: SV3,
    rotation: [f32; 3],
) {
    let lowest_available_index = used_indices.find_lowest();
    if let Some(local_cell) = cell {
        local_cell.references.insert(
            (0, lowest_available_index),
            esp::Reference {
                id: ref_id.to_string(),
                mast_index: 0,
                refr_index: lowest_available_index,
                translation: [translation.x, translation.y, translation.z],
                rotation: [-rotation[0], -rotation[1], -rotation[2]],
                ..Default::default()
            },
        );
        used_indices.insert(lowest_available_index);
    }
}

fn get_rotation(input: &str) -> [f32; 3] {
    let mut angles = [0.0f32; 3];
    for (index, token) in input.split_whitespace().take(3).enumerate() {
        if let Ok(value) = token.parse::<f32>() {
            angles[index] = value.to_radians();
        }
    }
    [angles[2], angles[0], angles[1]]
}

fn create_header_if_missing(plugin: &mut Plugin) {
    if plugin.objects_of_type::<Header>().next().is_none() {
        plugin.objects.push(TES3Object::Header(Header {
            version: 1.3,
            ..Default::default()
        }));
    } else {
        println!(
            "Plugin was found to already have {} header records",
            plugin.objects_of_type::<Header>().count()
        );
    }
}
