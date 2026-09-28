use std::{
    cmp::min,
    collections::{BTreeSet, HashMap, HashSet},
    io,
    path::Path,
};

use clap::Parser;
use morrobroom::slipgate::{Vector3 as SV3, entity::EntityId};
use nalgebra::{Rotation3, Vector3};
use tes3::esp::{self, Cell, EditorId, Header, Plugin, Static, TES3Object};

use morrobroom::{FindLowest, get_prop, lightmap_bake};

mod broom_args;
use broom_args::{BroomCommand, MorrobroomArgs, default_object_types};

mod brush_ni_node;

mod map_data;
use map_data::MapData;

mod mesh;
use mesh::{Mesh, NifStructuralKind, NifStructuralMarker};

mod game_object;
mod surfaces;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> io::Result<()> {
    match MorrobroomArgs::parse().command {
        BroomCommand::Compile {
            map_path,
            object_scale,
            openmw_config,
            output_path,
            output_dir,
            no_lightmaps,
        } => compile_map(
            &map_path,
            object_scale,
            openmw_config.as_deref(),
            output_path.as_deref(),
            output_dir.as_deref(),
            !no_lightmaps,
        ),
        BroomCommand::Nif2Map {
            inputs,
            recursive,
            output_dir,
            shell_thickness,
            fallback_thickness,
            fallback,
            skip_material,
            texture_roots,
            include_collision,
            overwrite,
            dry_run,
            verbose,
            no_validate,
            max_brushes,
        } => morrobroom::nif2map::run(&morrobroom::nif2map::Options {
            inputs,
            recursive,
            output_dir,
            shell_thickness,
            fallback_thickness,
            fallback,
            skip_material,
            texture_roots,
            include_collision,
            overwrite,
            dry_run,
            verbose,
            validate: !no_validate,
            max_brushes,
        }),
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
            let output_path = output_path
                .map_or_else(morrobroom::fgd::default_catalog_output_path, Ok)
                .map_err(|error| io::Error::other(error.to_string()))?;
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
    openmw_config_path: Option<&Path>,
    output_path: Option<&Path>,
    output_dir: Option<&Path>,
    lightmaps_enabled: bool,
) -> io::Result<()> {
    let workdir_result = output_dir.map_or_else(
        || morrobroom::create_workdir(map_path),
        |output_dir| morrobroom::create_workdir_at(map_path, output_dir),
    );
    let (work_dir, map_dir) = workdir_result
        .map_err(|error_string| io::Error::new(io::ErrorKind::InvalidInput, error_string))?;
    let map_name = map_path.to_string_lossy().to_string();
    let openmw_config = match openmw_config_path {
        Some(path) => openmw_config::OpenMWConfiguration::new(Some(path.to_path_buf())),
        None => openmw_config::OpenMWConfiguration::from_env_or_user_config(),
    }
    .map_err(|error| io::Error::other(format!("failed to load OpenMW configuration: {error}")))?;
    let map_data = MapData::new(&map_name, lightmaps_enabled, &openmw_config);
    assert_eq!(
        map_data.lightmap_geometry().is_some(),
        lightmaps_enabled,
        "lightmap preparation did not honor the compiler option"
    );
    assert!(
        !map_data.geomap.entity_brushes.is_empty(),
        "No brushes found in map!"
    );
    if let Some(lightmap) = map_data.lightmap() {
        let path = work_dir
            .join("Textures")
            .join(&map_dir)
            .join("lightmap.dds");
        lightmap_bake::write_dds(&path, lightmap)?;
    }

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
    process_brush_entities(&mut state)?;
    process_point_entities(&mut state);
    let CompileState {
        cell,
        mut created_objects,
        mut processed_base_objects,
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

fn process_brush_entities(state: &mut CompileState<'_>) -> io::Result<()> {
    for (entity_id, brushes) in state.map_data.geomap.entity_brushes.iter() {
        process_brush_entity(state, *entity_id, brushes)?;
    }
    Ok(())
}

fn process_brush_entity(
    state: &mut CompileState<'_>,
    entity_id: morrobroom::slipgate::entity::EntityId,
    brushes: &[morrobroom::slipgate::brush::BrushId],
) -> io::Result<()> {
    let prop_map = state.map_data.get_entity_properties(entity_id);
    let classname = prop_map
        .get(&"classname".to_string())
        .map_or("", |value| value.as_str());
    if classname == "func_group" && brushes.is_empty() {
        return Ok(());
    }
    let mut mesh = Mesh::from_map(brushes, state.map_data, state.object_scale, entity_id);

    let ref_id = prop_map.get(&"ESM3_RefId".to_string()).map_or_else(
        || format!("{}-scene-{entity_id}", state.map_dir),
        |id| id[..min(id.len(), 32)].to_string(),
    );
    let mesh_name = prop_map.get(&"ESM3_Model".to_string()).map_or_else(
        || format!("{}/{ref_id}.nif", state.map_dir),
        |name| (*name).clone(),
    );
    mesh.mangle = get_prop("mangle", &prop_map)
        .map_or_else(|| get_rotation("0 0 0"), |mangle| get_rotation(mangle));
    let group_id = if classname == "func_group" {
        prop_map.get(&"_tb_id".to_string()).copied()
    } else {
        prop_map.get(&"_tb_group".to_string()).copied()
    };
    let markers = nif_markers_for_entity(group_id, &prop_map, &state.map_data.geomap)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    mesh.attach_nif_semantics(
        &markers,
        get_prop("Nif_Target", &prop_map).map(String::as_str),
    );
    if !state.processed_base_objects.insert(ref_id.clone()) {
        println!("Placing new instance of {ref_id}");
    }

    if !assign_game_object(&mut mesh, &prop_map, &ref_id, &mesh_name, state) {
        return Ok(());
    }

    mesh.worldspace_position = Mesh::centroid(&mesh.node_distances) * state.object_scale;
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
    Ok(())
}

fn nif_markers_for_group(
    initial_group_id: Option<&String>,
    geomap: &morrobroom::slipgate::GeoMap,
) -> Result<Vec<NifStructuralMarker>, String> {
    let mut group_id = initial_group_id.cloned();
    let mut visited = HashSet::new();
    let mut group_markers = Vec::new();

    while let Some(current_group_id) = group_id {
        if !visited.insert(current_group_id.clone()) {
            return Err(format!(
                "TrenchBroom group cycle detected at group {current_group_id}"
            ));
        }

        let mut markers = Vec::new();
        for entity_id in geomap.point_entities.iter() {
            let properties = entity_properties(geomap, *entity_id);
            if properties.get(&"_tb_group".to_string()) != Some(&&current_group_id) {
                continue;
            }
            if let Some(marker) = structural_marker(&properties) {
                markers.push(marker);
            }
        }
        group_markers.push(markers);

        let group_entity = (0..geomap.entity_properties.len())
            .map(EntityId)
            .find(|entity_id| {
                let properties = entity_properties(geomap, *entity_id);
                properties.get(&"_tb_id".to_string()) == Some(&&current_group_id)
                    && properties
                        .get(&"_tb_type".to_string())
                        .is_some_and(|value| value.as_str() == "_tb_group")
            });
        group_id = group_entity.and_then(|entity_id| {
            entity_properties(geomap, entity_id)
                .get(&"_tb_group".to_string())
                .map(|value| (*value).clone())
        });
    }

    let markers = group_markers.into_iter().rev().flatten().collect();
    order_nif_markers(markers)
}

fn nif_markers_for_entity(
    group_id: Option<&String>,
    entity: &HashMap<&String, &String>,
    geomap: &morrobroom::slipgate::GeoMap,
) -> Result<Vec<NifStructuralMarker>, String> {
    let mut markers = nif_markers_for_group(group_id, geomap)?;
    let global_markers = all_nif_markers(geomap);
    let target_scope = if group_id.is_some() {
        markers.as_slice()
    } else {
        global_markers.as_slice()
    };
    if let Some(target) = nonempty_property(entity, "Nif_Target") {
        let target_chain = nif_markers_for_target(Some(target), target_scope)?;
        let mut present_names: HashSet<String> = markers
            .iter()
            .filter_map(|marker| marker.link_name.clone())
            .collect();
        for marker in target_chain {
            if marker
                .link_name
                .as_ref()
                .is_none_or(|name| present_names.insert(name.clone()))
            {
                markers.push(marker);
            }
        }
    }
    validate_unique_link_names(&markers)?;

    // In a flat map with exactly one collision-root marker there is no competing
    // scope to resolve, so it can supply the generated RootCollisionNode for
    // the ungrouped geometry. Otherwise collision roots remain group-scoped.
    if group_id.is_none()
        && !markers
            .iter()
            .any(|marker| marker.kind == NifStructuralKind::CollisionRoot)
    {
        let collision_roots: Vec<_> = global_markers
            .iter()
            .filter(|marker| marker.kind == NifStructuralKind::CollisionRoot)
            .collect();
        if let [collision_root] = collision_roots.as_slice() {
            markers.push((*collision_root).clone());
        }
    }

    order_nif_markers(markers)
}

fn nif_markers_for_target(
    initial_target: Option<&String>,
    candidates: &[NifStructuralMarker],
) -> Result<Vec<NifStructuralMarker>, String> {
    let mut target = initial_target.cloned();
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    while let Some(link_name) = target.take() {
        if !visited.insert(link_name.clone()) {
            return Err(format!("NIF target-link cycle detected at {link_name:?}"));
        }
        let marker = find_nif_link(&link_name, candidates)?;
        target.clone_from(&marker.target);
        chain.push(marker.clone());
    }
    chain.reverse();
    Ok(chain)
}

fn all_nif_markers(geomap: &morrobroom::slipgate::GeoMap) -> Vec<NifStructuralMarker> {
    geomap
        .point_entities
        .iter()
        .filter_map(|entity_id| {
            let properties = entity_properties(geomap, *entity_id);
            structural_marker(&properties)
        })
        .collect()
}

fn find_nif_link<'a>(
    name: &str,
    candidates: &'a [NifStructuralMarker],
) -> Result<&'a NifStructuralMarker, String> {
    let mut matches = candidates
        .iter()
        .filter(|marker| marker.link_name.as_deref() == Some(name));
    let Some(marker) = matches.next() else {
        return Err(format!(
            "Nif_Target {name:?} does not match a Nif_LinkName in its permitted scope"
        ));
    };
    if matches.next().is_some() {
        return Err(format!(
            "Nif_Target {name:?} is ambiguous: multiple Nif_LinkName markers match in its permitted scope"
        ));
    }
    Ok(marker)
}

fn validate_unique_link_names(markers: &[NifStructuralMarker]) -> Result<(), String> {
    let mut names = HashSet::new();
    for name in markers
        .iter()
        .filter_map(|marker| marker.link_name.as_deref())
    {
        if !names.insert(name) {
            return Err(format!(
                "duplicate Nif_LinkName {name:?} in one NIF authoring scope"
            ));
        }
    }
    Ok(())
}

/// `TrenchBroom` writes unset FGD properties as empty strings; treat those as absent.
fn nonempty_property<'a>(
    properties: &HashMap<&String, &'a String>,
    key: &str,
) -> Option<&'a String> {
    properties
        .get(&key.to_string())
        .copied()
        .filter(|value| !value.is_empty())
}

fn structural_marker(properties: &HashMap<&String, &String>) -> Option<NifStructuralMarker> {
    let classname = properties.get(&"classname".to_string()).copied()?;
    let kind = match classname.as_str() {
        "nif_node_billboard" => NifStructuralKind::Billboard {
            mode: properties
                .get(&"Nif_Billboard_Mode".to_string())
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        },
        "nif_node_sort_adjust" => NifStructuralKind::SortAdjust {
            mode: properties
                .get(&"Nif_Sort_Mode".to_string())
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        },
        "nif_node_collision_root" => NifStructuralKind::CollisionRoot,
        _ => return None,
    };
    Some(NifStructuralMarker {
        kind,
        link_name: nonempty_property(properties, "Nif_LinkName").cloned(),
        target: nonempty_property(properties, "Nif_Target").cloned(),
        origin: point_entity_position(1.0, properties),
    })
}

fn order_nif_markers(
    markers: Vec<NifStructuralMarker>,
) -> Result<Vec<NifStructuralMarker>, String> {
    validate_unique_link_names(&markers)?;
    for target in markers.iter().filter_map(|marker| marker.target.as_deref()) {
        find_nif_link(target, &markers)?;
    }
    let mut pending = markers;
    let mut ordered = Vec::with_capacity(pending.len());
    let mut available_names = HashSet::new();

    while !pending.is_empty() {
        let next_index = pending
            .iter()
            .position(|candidate| match candidate.target.as_deref() {
                None => true,
                Some(target) if available_names.contains(target) => true,
                Some(target) => !pending
                    .iter()
                    .any(|marker| marker.link_name.as_deref() == Some(target)),
            })
            .ok_or_else(|| {
                "NIF target-link cycle detected while ordering authoring nodes".to_owned()
            })?;
        let marker = pending.remove(next_index);
        if let Some(name) = &marker.link_name {
            available_names.insert(name.clone());
        }
        ordered.push(marker);
    }

    Ok(ordered)
}

fn entity_properties(
    geomap: &morrobroom::slipgate::GeoMap,
    entity_id: EntityId,
) -> HashMap<&String, &String> {
    geomap
        .entity_properties
        .get(entity_id)
        .expect("every entity ID has a property set")
        .iter()
        .map(|property| (&property.key, &property.value))
        .collect()
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
                flags: game_object::object_flags(props),
            }
            .into();
        }
        "world_Detail" | "nif_geometry" | "func_group" => {
            state.processed_base_objects.insert(ref_id.to_string());
            mesh.game_object = Static {
                id: ref_id.to_string(),
                mesh: mesh_name.to_string(),
                flags: game_object::object_flags(props),
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
        if class == "func_group" || class.starts_with("nif_node_") {
            continue;
        }
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
        .get(&"ESM3_RefId".to_string())
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
            || panic!("All point entities must have an origin!"),
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
                rotation,
                ..Default::default()
            },
        );
        used_indices.insert(lowest_available_index);
    }
}

/// Convert `TrenchBroom`'s Y-up, ZYX Euler `mangle` into TES3 reference angles.
///
/// `TrenchBroom` describes rotations as `Rz * Ry * Rx`. `OpenMW` reconstructs a
/// reference attitude as `Rx(-x) * Ry(-y) * Rz(-z)` around its negative axes.
/// Convert the coordinate basis first, then decompose in `OpenMW`'s order.
fn get_rotation(input: &str) -> [f32; 3] {
    let mut angles = [0.0f32; 3];
    for (index, token) in input.split_whitespace().take(3).enumerate() {
        if let Ok(value) = token.parse::<f32>() {
            angles[index] = value.to_radians();
        }
    }

    let trenchbroom_rotation = Rotation3::from_axis_angle(&Vector3::z_axis(), angles[2])
        * Rotation3::from_axis_angle(&Vector3::y_axis(), angles[1])
        * Rotation3::from_axis_angle(&Vector3::x_axis(), angles[0]);
    let y_up_to_z_up = Rotation3::from_axis_angle(&Vector3::x_axis(), std::f32::consts::FRAC_PI_2);
    let openmw_rotation = y_up_to_z_up * trenchbroom_rotation * y_up_to_z_up.inverse();
    let matrix = openmw_rotation.matrix();
    let sin_y = matrix[(0, 2)].clamp(-1.0, 1.0);
    let y = sin_y.asin();
    let cos_y = y.cos();
    let (x, z) = if cos_y.abs() > 1e-6 {
        (
            (-matrix[(1, 2)]).atan2(matrix[(2, 2)]),
            (-matrix[(0, 1)]).atan2(matrix[(0, 0)]),
        )
    } else if sin_y > 0.0 {
        (matrix[(1, 0)].atan2(matrix[(1, 1)]), 0.0)
    } else {
        ((-matrix[(1, 0)]).atan2(matrix[(1, 1)]), 0.0)
    };

    // OpenMW stores rotations around negative coordinate axes, so the ESP
    // rotation components have the opposite sign of the reconstructed angles.
    [-x, -y, -z]
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

#[cfg(test)]
mod nif_semantic_compile_tests {
    use super::*;
    use morrobroom::slipgate::{map_geometry::MapGeometry, repr::Map};

    fn assert_rotation_degrees(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual.to_degrees() - expected).abs() < 1e-4);
        }
    }

    #[test]
    fn mangle_converts_trenchbroom_y_up_zyx_to_tes3_rotation_order() {
        let rotation = get_rotation("0 90 90");
        assert_rotation_degrees(rotation, [90.0, 90.0, 0.0]);
        assert_rotation_degrees(get_rotation("0 90 0"), [0.0, 0.0, -90.0]);
        assert_rotation_degrees(get_rotation("0 0 90"), [0.0, 90.0, 0.0]);

        let mut cell = Some(Cell::default());
        let mut used_indices = BTreeSet::new();
        append_cell_reference(
            &mut used_indices,
            &mut cell,
            "rotation-fixture",
            SV3::default(),
            rotation,
        );
        let reference = cell
            .as_ref()
            .unwrap()
            .references
            .values()
            .next()
            .expect("expected the compiled reference");
        assert_rotation_degrees(reference.rotation, [90.0, 90.0, 0.0]);
    }

    #[test]
    fn semantic_markers_follow_nested_tb_group_scope_only() {
        let map = r#"// Game: Morrowind
// Format: Valve
{
"classname" "worldspawn"
}
{
"classname" "func_group"
"_tb_type" "_tb_group"
"_tb_id" "outer"
}
{
"classname" "nif_node_billboard"
"origin" "10 20 30"
"_tb_group" "outer"
"Nif_LinkName" "billboard-root"
}
{
"classname" "func_group"
"_tb_type" "_tb_group"
"_tb_id" "inner"
"_tb_group" "outer"
}
{
"classname" "nif_node_sort_adjust"
"origin" "12 20 30"
"_tb_group" "inner"
"Nif_LinkName" "sort-node"
"Nif_Target" "billboard-root"
"Nif_Sort_Mode" "2"
}
{
"classname" "nif_geometry"
"_tb_group" "inner"
}
"#
        .parse::<Map>()
        .expect("minimal TB marker map should parse");
        let geomap = MapGeometry::from_map_without_occlusion(map).geomap;
        let group_id = "inner".to_owned();
        let markers = nif_markers_for_group(Some(&group_id), &geomap).unwrap();

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].kind, NifStructuralKind::Billboard { mode: 0 });
        assert_eq!(markers[0].link_name.as_deref(), Some("billboard-root"));
        assert_eq!(markers[1].kind, NifStructuralKind::SortAdjust { mode: 2 });
        assert_eq!(markers[1].target.as_deref(), Some("billboard-root"));
        assert_eq!(markers[1].origin, SV3::new(12.0, 20.0, 30.0));
    }

    #[test]
    fn nif_target_links_order_the_named_parent_before_its_child() {
        let child = NifStructuralMarker {
            kind: NifStructuralKind::SortAdjust { mode: 2 },
            link_name: Some("child".into()),
            target: Some("parent".into()),
            origin: SV3::default(),
        };
        let parent = NifStructuralMarker {
            kind: NifStructuralKind::Billboard { mode: 0 },
            link_name: Some("parent".into()),
            target: None,
            origin: SV3::default(),
        };

        let ordered = order_nif_markers(vec![child, parent]).unwrap();
        assert_eq!(ordered[0].link_name.as_deref(), Some("parent"));
        assert_eq!(ordered[1].link_name.as_deref(), Some("child"));
    }

    #[test]
    fn nif_torture_map_resolves_linked_sort_node_and_unambiguous_collision_root() {
        let map = include_str!("../tests/fixtures/maps/nif-semantics-torture.map")
            .parse::<Map>()
            .expect("NIF semantics torture map should parse");
        let geomap = MapGeometry::from_map_without_occlusion(map).geomap;
        let geometry_entity = (0..geomap.entity_properties.len())
            .map(EntityId)
            .find(|entity_id| {
                entity_properties(&geomap, *entity_id)
                    .get(&"classname".to_string())
                    .is_some_and(|classname| classname.as_str() == "nif_geometry")
            })
            .expect("torture map should have one nif_geometry entity");
        let geometry_properties = entity_properties(&geomap, geometry_entity);
        let markers = nif_markers_for_entity(None, &geometry_properties, &geomap).unwrap();
        let mangle = geometry_properties
            .get(&"mangle".to_string())
            .expect("torture brush should include its regression mangle");
        assert_rotation_degrees(get_rotation(mangle), [90.0, 90.0, 0.0]);

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].kind, NifStructuralKind::SortAdjust { mode: 2 });
        assert!(
            markers
                .iter()
                .any(|marker| marker.kind == NifStructuralKind::CollisionRoot)
        );
    }

    #[test]
    fn grouped_targets_are_scoped_and_ungrouped_duplicates_are_ambiguous() {
        let map = r#"// Game: Morrowind
// Format: Valve
{
"classname" "worldspawn"
}
{
"classname" "func_group"
"_tb_type" "_tb_group"
"_tb_id" "akulakhan"
}
{
"classname" "nif_node_billboard"
"origin" "0 0 0"
"_tb_group" "akulakhan"
"Nif_LinkName" "head"
}
{
"classname" "nif_geometry"
"_tb_group" "akulakhan"
"Nif_Target" "head"
}
{
"classname" "func_group"
"_tb_type" "_tb_group"
"_tb_id" "dwemer-statue"
}
{
"classname" "nif_node_sort_adjust"
"origin" "0 0 0"
"_tb_group" "dwemer-statue"
"Nif_LinkName" "head"
}
{
"classname" "nif_geometry"
"Nif_Target" "head"
}
"#
        .parse::<Map>()
        .expect("minimal target-scope map should parse");
        let geomap = MapGeometry::from_map_without_occlusion(map).geomap;
        let entity_with = |group: Option<&str>| {
            (0..geomap.entity_properties.len())
                .map(EntityId)
                .find(|entity_id| {
                    let properties = entity_properties(&geomap, *entity_id);
                    properties
                        .get(&"classname".to_string())
                        .is_some_and(|name| {
                            name.as_str() == "nif_geometry"
                                && properties
                                    .get(&"_tb_group".to_string())
                                    .map(|value| value.as_str())
                                    == group
                        })
                })
                .expect("expected geometry entity")
        };

        let grouped_id = entity_with(Some("akulakhan"));
        let grouped_props = entity_properties(&geomap, grouped_id);
        let grouped =
            nif_markers_for_entity(Some(&"akulakhan".to_owned()), &grouped_props, &geomap)
                .expect("grouped target should resolve only within its group");
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].kind, NifStructuralKind::Billboard { mode: 0 });

        let ungrouped_id = entity_with(None);
        let ungrouped_props = entity_properties(&geomap, ungrouped_id);
        let error = nif_markers_for_entity(None, &ungrouped_props, &geomap)
            .expect_err("the duplicate global target must be reported as ambiguous");
        assert!(error.contains("ambiguous"));
    }

    #[test]
    fn duplicate_link_names_in_one_group_are_rejected() {
        let map = r#"// Game: Morrowind
// Format: Valve
{
"classname" "worldspawn"
}
{
"classname" "func_group"
"_tb_type" "_tb_group"
"_tb_id" "asset"
}
{
"classname" "nif_node_billboard"
"origin" "0 0 0"
"_tb_group" "asset"
"Nif_LinkName" "head"
}
{
"classname" "nif_node_sort_adjust"
"origin" "0 0 0"
"_tb_group" "asset"
"Nif_LinkName" "head"
}
"#
        .parse::<Map>()
        .expect("minimal duplicate-name map should parse");
        let geomap = MapGeometry::from_map_without_occlusion(map).geomap;
        let error = nif_markers_for_group(Some(&"asset".to_owned()), &geomap)
            .expect_err("duplicate names in a group must fail");
        assert!(error.contains("duplicate Nif_LinkName"));
    }

    #[test]
    fn apostrophes_in_property_values_keep_the_rest_of_the_map() {
        let map = r#"// Game: Morrowind
// Format: Valve
{
"classname" "worldspawn"
"ESM3_Name" "Balmora, Caius Cosades' House"
{
( -16 -16 -16 ) ( -16 -15 -16 ) ( -16 -16 -15 ) skip [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -16 -16 -15 ) ( -15 -16 -16 ) skip [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -15 -16 -16 ) ( -16 -15 -16 ) skip [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 16 17 16 ) ( 17 16 16 ) skip [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 17 16 16 ) ( 16 16 17 ) skip [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 16 16 16 ) ( 16 16 17 ) ( 16 17 16 ) skip [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
}
"#
        .parse::<Map>()
        .expect("an apostrophe inside a double-quoted value should parse");
        let geomap = MapGeometry::from_map_without_occlusion(map).geomap;
        let worldspawn = entity_properties(&geomap, EntityId(0));
        assert_eq!(
            worldspawn.get(&"ESM3_Name".to_string()).map(|name| name.as_str()),
            Some("Balmora, Caius Cosades' House")
        );
        assert!(!geomap.entity_brushes.is_empty());
    }

    #[test]
    fn nif_target_without_a_scoped_link_is_an_error() {
        let error = nif_markers_for_target(Some(&"missing".to_owned()), &[])
            .expect_err("a target with no in-scope link must fail");
        assert!(error.contains("does not match a Nif_LinkName"));
    }
}
