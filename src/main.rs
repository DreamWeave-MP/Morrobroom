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
            object_scale,
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
            scale: f64::from(object_scale),
        }),
        BroomCommand::Fgd {
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
    compile_entities(&mut state)?;
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

/// Compile every entity into the map's cell.
///
/// The cell comes from worldspawn's properties before anything is placed in
/// it, so it exists whether or not worldspawn has brushes of its own: nif2map's
/// maps, and maps built only from brush entities and placed records, have none.
fn compile_entities(state: &mut CompileState<'_>) -> io::Result<()> {
    state.cell = Some(worldspawn_cell(state.map_data, state.map_dir)?);
    process_brush_entities(state)?;
    process_point_entities(state);
    Ok(())
}

fn worldspawn_cell(map_data: &MapData, map_dir: &str) -> io::Result<Cell> {
    let worldspawn = map_data
        .geomap
        .entities
        .iter()
        .map(|entity_id| map_data.get_entity_properties(*entity_id))
        .find(|properties| {
            properties
                .get(&"classname".to_string())
                .is_some_and(|classname| classname.as_str() == "worldspawn")
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "The map has no worldspawn entity, so there is no cell to compile into. \
                 TrenchBroom writes one at the top of every map; is the file complete?",
            )
        })?;
    let mut cell = game_object::cell(&worldspawn);
    if cell.name.is_empty() {
        map_dir.clone_into(&mut cell.name);
    }
    Ok(cell)
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
    mesh.mangle = entity_rotation(classname, &prop_map);
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
        "item_Clothing" => mesh.game_object = game_object::clothing(props, ref_id, mesh_name),
        "item_Ingredient" => mesh.game_object = game_object::ingredient(props, ref_id, mesh_name),
        "item_Light" => {
            mesh.game_object = game_object::light(props, state.object_scale, ref_id, mesh_name);
        }
        "item_Misc" => mesh.game_object = game_object::misc(props, ref_id, mesh_name),
        "worldspawn" | "world_Detail" | "nif_geometry" | "func_group" => {
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

/// The brush entity classes the bundled FGDs declare and the compiler builds.
const BRUSH_ENTITY_CLASSES: [&str; 14] = [
    "worldspawn",
    "func_group",
    "world_Detail",
    "world_Activator",
    "world_Container",
    "nif_geometry",
    "item_Alchemy",
    "item_Apparatus",
    "item_Armor",
    "item_Book",
    "item_Clothing",
    "item_Ingredient",
    "item_Light",
    "item_Misc",
];

/// What the compiler makes of a point entity.
#[derive(Debug, PartialEq, Eq)]
enum PointEntity {
    /// Editor-only entities, worldspawn, whose cell is made before anything is
    /// placed, and the NIF markers the brush pass has read.
    Skip,
    /// A brush entity left without brushes: there is nothing to build.
    EmptyBrushEntity,
    PointLight,
    CreatureList,
    ItemList,
    /// A reference to a record that already exists in the load order, such as
    /// one placed from the generated catalog.
    ExistingRecord(String),
    /// Declared in the bundled FGDs as an editor preview; not compiled yet.
    Preview,
    Unidentified,
}

fn classify_point_entity(class: &str, properties: &HashMap<&String, &String>) -> PointEntity {
    if matches!(
        class,
        "worldspawn" | "func_group" | "info_player_start" | "tool_Dictionary"
    ) || class.starts_with("nif_node_")
    {
        PointEntity::Skip
    } else if BRUSH_ENTITY_CLASSES.contains(&class) {
        PointEntity::EmptyBrushEntity
    } else if class.contains("Light_Point") {
        PointEntity::PointLight
    } else if class == "world_CreatureList" {
        PointEntity::CreatureList
    } else if class == "world_ItemList" {
        PointEntity::ItemList
    } else if class == "nif_fx_fire" || class.starts_with("vfx_") {
        PointEntity::Preview
    } else if let Some(record_id) = nonempty_property(properties, "ESM3_RefId") {
        PointEntity::ExistingRecord(record_id.clone())
    } else {
        PointEntity::Unidentified
    }
}

fn process_point_entities(state: &mut CompileState<'_>) {
    for entity_id in state.map_data.geomap.point_entities.iter() {
        let prop_map = state.map_data.get_entity_properties(*entity_id);
        let class = prop_map
            .get(&"classname".to_string())
            .expect("All point entities have class names")
            .as_str();
        match classify_point_entity(class, &prop_map) {
            PointEntity::Skip => {}
            PointEntity::PointLight => {
                let ref_id = format!("{}-PL-{}", state.map_dir, state.used_indices.find_lowest());
                let ref_id = ref_id[..min(ref_id.len(), 32)].to_string();
                let radius = class
                    .chars()
                    .skip_while(|character| !character.is_ascii_digit())
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .expect(
                        "All point light types should have a radius encoded in their classnames!",
                    );
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
            }
            PointEntity::CreatureList => {
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
            }
            PointEntity::ItemList => {
                let ref_id = required_ref_id(&prop_map, *entity_id, "item list");
                if state.processed_base_objects.insert(ref_id.clone()) {
                    state
                        .created_objects
                        .push(game_object::item_list(&prop_map, &ref_id));
                }
            }
            PointEntity::ExistingRecord(record_id) => append_cell_reference(
                &mut state.used_indices,
                &mut state.cell,
                &record_id,
                point_entity_position(state.object_scale, &prop_map),
                entity_rotation(class, &prop_map),
            ),
            PointEntity::EmptyBrushEntity => {
                println!("{class} has no brushes, so there is nothing to build; skipped");
            }
            PointEntity::Preview => {
                println!("{class} is an editor preview and is not compiled yet; nothing placed");
            }
            PointEntity::Unidentified => println!(
                "Unidentified point entity class: {class}. Give it an ESM3_RefId to place that record."
            ),
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

/// The attitude `TrenchBroom` shows for an entity, as TES3 reference angles.
///
/// Rotating an entity in `TrenchBroom` writes its rotation into the first of
/// `angles`, `mangle` and `angle` it has or its class declares, for brush
/// entities as for point entities (`EntityRotation.cpp`). The first two hold
/// pitch, yaw and roll in degrees, positive pitch down, applied as
/// `Rz(yaw) * Ry(pitch) * Rx(roll)` in the map's own Z-up axes, which are
/// Morrowind's; `angle` is a yaw, or -1 and -2 for straight up and down.
/// Classes named `light…` follow Quake's lights instead: `mangle` first, as
/// yaw, pitch and roll with positive pitch up.
///
/// The reference gets exactly that attitude, and a brush entity's mesh is
/// counter-rotated by it, so its own axes in `OpenMW` are the ones the editor
/// shows.
fn entity_rotation(class: &str, properties: &HashMap<&String, &String>) -> [f32; 3] {
    let angles = |key: &str| {
        nonempty_property(properties, key).map(|value| {
            let mut angles = [0.0f32; 3];
            for (index, token) in value.split_whitespace().take(3).enumerate() {
                if let Ok(value) = token.parse::<f32>() {
                    angles[index] = value;
                }
            }
            angles
        })
    };
    let angle = nonempty_property(properties, "angle")
        .and_then(|value| value.trim().parse::<f32>().ok())
        .unwrap_or(0.0);

    let [yaw, pitch, roll] = if class.starts_with("light") {
        if let Some([yaw, pitch, roll]) = angles("mangle") {
            [yaw, -pitch, roll]
        } else if let Some([pitch, yaw, roll]) = angles("angles") {
            [yaw, pitch, roll]
        } else {
            [angle, 0.0, 0.0]
        }
    } else if let Some([pitch, yaw, roll]) = angles("angles").or_else(|| angles("mangle")) {
        [yaw, pitch, roll]
    } else if (angle + 1.0).abs() < f32::EPSILON {
        [0.0, -90.0, 0.0]
    } else if (angle + 2.0).abs() < f32::EPSILON {
        [0.0, 90.0, 0.0]
    } else {
        [angle, 0.0, 0.0]
    }
    .map(f32::to_radians);

    tes3_reference_rotation(
        &(Rotation3::from_axis_angle(&Vector3::z_axis(), yaw)
            * Rotation3::from_axis_angle(&Vector3::y_axis(), pitch)
            * Rotation3::from_axis_angle(&Vector3::x_axis(), roll)),
    )
}

/// Decompose an attitude into the angles `OpenMW` rebuilds it from:
/// `Rx(-x) * Ry(-y) * Rz(-z)`.
fn tes3_reference_rotation(rotation: &Rotation3<f32>) -> [f32; 3] {
    let matrix = rotation.matrix();
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

    fn brush_rotation(mangle: &str) -> [f32; 3] {
        let (key, value) = ("mangle".to_owned(), mangle.to_owned());
        entity_rotation("world_Detail", &HashMap::from([(&key, &value)]))
    }

    #[test]
    fn mangle_converts_trenchbroom_pitch_yaw_roll_to_tes3_reference_angles() {
        // "pitch yaw roll": yaw 90 and roll 90 give Rz(90) * Rx(90), which OpenMW
        // rebuilds as Rx(90) * Ry(90) from the angles -90, -90, 0.
        let rotation = brush_rotation("0 90 90");
        assert_rotation_degrees(rotation, [-90.0, -90.0, 0.0]);
        assert_rotation_degrees(brush_rotation("0 90 0"), [0.0, 0.0, -90.0]);
        assert_rotation_degrees(brush_rotation("0 0 90"), [-90.0, 0.0, 0.0]);
        assert_rotation_degrees(brush_rotation("90 0 0"), [0.0, -90.0, 0.0]);

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
        assert_rotation_degrees(reference.rotation, [-90.0, -90.0, 0.0]);
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
        assert_eq!(mangle.as_str(), "0 90 90");
        assert_rotation_degrees(
            entity_rotation("nif_geometry", &geometry_properties),
            [-90.0, -90.0, 0.0],
        );

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
            worldspawn
                .get(&"ESM3_Name".to_string())
                .map(|name| name.as_str()),
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

#[cfg(test)]
mod point_entity_tests {
    use super::*;
    use morrobroom::slipgate::repr::Map;

    const CATALOG_MAP: &str = r#"// Game: Morrowind
// Format: Valve
{
"classname" "worldspawn"
{
( -16 -16 -16 ) ( -16 -15 -16 ) ( -16 -16 -15 ) skip [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -16 -16 -15 ) ( -15 -16 -16 ) skip [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -15 -16 -16 ) ( -16 -15 -16 ) skip [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 16 17 16 ) ( 17 16 16 ) skip [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 17 16 16 ) ( 16 16 17 ) skip [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 16 16 16 ) ( 16 16 17 ) ( 16 17 16 ) skip [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
}
{
"classname" "static_ex_common_house_01"
"origin" "32 -16 8"
"ESM3_RefId" "ex_common_house_01"
"ESM3_Plugin" "morrowind.esm"
"mangle" "0 90 0"
}
{
"classname" "misc_chargen_boat"
"origin" "0 0 0"
"ESM3_RefId" "chargen boat"
}
{
"classname" "info_player_start"
"origin" "0 0 0"
}
{
"classname" "vfx_kurp_0001"
"origin" "0 0 0"
}
"#;

    fn assert_rotation_degrees(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual.to_degrees() - expected).abs() < 1e-3,
                "{actual:?} != {expected:?}"
            );
        }
    }

    fn owned_properties(properties: &[(&str, &str)]) -> Vec<(String, String)> {
        properties
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn rotation_of(class: &str, properties: &[(&str, &str)]) -> [f32; 3] {
        let owned = owned_properties(properties);
        let properties: HashMap<_, _> = owned.iter().map(|(key, value)| (key, value)).collect();
        entity_rotation(class, &properties)
    }

    /// The attitude `OpenMW` rebuilds from reference angles.
    fn openmw_attitude(rotation: [f32; 3]) -> Rotation3<f32> {
        Rotation3::from_axis_angle(&Vector3::x_axis(), -rotation[0])
            * Rotation3::from_axis_angle(&Vector3::y_axis(), -rotation[1])
            * Rotation3::from_axis_angle(&Vector3::z_axis(), -rotation[2])
    }

    #[test]
    fn records_placed_from_the_catalog_become_references() {
        let map = CATALOG_MAP
            .parse::<Map>()
            .expect("catalog placement map should parse");
        let openmw_config = openmw_config::OpenMWConfiguration::new_empty("catalog-test-config")
            .expect("construct in-memory empty OpenMW configuration");
        let map_data = MapData::from_map(map, "catalog.map", false, &openmw_config);
        let mut state = CompileState {
            map_data: &map_data,
            work_dir: Path::new("unused"),
            map_dir: "catalog",
            object_scale: 2.0,
            cell: Some(Cell::default()),
            created_objects: Vec::new(),
            processed_base_objects: HashSet::new(),
            used_indices: BTreeSet::new(),
        };

        process_point_entities(&mut state);

        assert!(
            state.created_objects.is_empty(),
            "placing an existing record must not create one"
        );
        let references: Vec<_> = state.cell.unwrap().references.into_values().collect();
        assert_eq!(references.len(), 2);
        let house = references
            .iter()
            .find(|reference| reference.id == "ex_common_house_01")
            .expect("the catalog static should be placed");
        for (actual, expected) in house.translation.into_iter().zip([64.0, -32.0, 16.0]) {
            assert!((actual - expected).abs() < 1e-4, "{:?}", house.translation);
        }
        assert_rotation_degrees(house.rotation, [0.0, 0.0, -90.0]);
        assert!(
            references
                .iter()
                .any(|reference| reference.id == "chargen boat")
        );
    }

    #[test]
    fn point_entity_kinds() {
        let owned = owned_properties(&[("ESM3_RefId", "ex_common_house_01")]);
        let with_id: HashMap<_, _> = owned.iter().map(|(key, value)| (key, value)).collect();
        let without_id = HashMap::new();

        assert_eq!(
            classify_point_entity("static_ex_common_house_01", &with_id),
            PointEntity::ExistingRecord("ex_common_house_01".into())
        );
        assert_eq!(
            classify_point_entity("static_ex_common_house_01", &without_id),
            PointEntity::Unidentified
        );
        for editor_only in ["info_player_start", "tool_Dictionary", "nif_node_billboard"] {
            assert_eq!(
                classify_point_entity(editor_only, &with_id),
                PointEntity::Skip
            );
        }
        for preview in ["nif_fx_fire", "vfx_kurp_0001"] {
            assert_eq!(
                classify_point_entity(preview, &with_id),
                PointEntity::Preview
            );
        }
        assert_eq!(
            classify_point_entity("Light_Point128", &without_id),
            PointEntity::PointLight
        );
    }

    #[test]
    fn point_entity_rotation_follows_trenchbroom() {
        assert_rotation_degrees(rotation_of("static_x", &[]), [0.0, 0.0, 0.0]);
        assert_rotation_degrees(
            rotation_of("static_x", &[("mangle", "0 90 0")]),
            [0.0, 0.0, -90.0],
        );
        assert_rotation_degrees(
            rotation_of("static_x", &[("angle", "90")]),
            [0.0, 0.0, -90.0],
        );
        assert_rotation_degrees(
            rotation_of("static_x", &[("angles", "0 45 0"), ("mangle", "0 90 0")]),
            [0.0, 0.0, -45.0],
        );

        let [pitch, yaw, roll] = [20.0_f32, 30.0, 40.0].map(f32::to_radians);
        let expected = Rotation3::from_axis_angle(&Vector3::z_axis(), yaw)
            * Rotation3::from_axis_angle(&Vector3::y_axis(), pitch)
            * Rotation3::from_axis_angle(&Vector3::x_axis(), roll);
        let actual = openmw_attitude(rotation_of("static_x", &[("mangle", "20 30 40")]));
        assert!((actual.matrix() - expected.matrix()).norm() < 1e-4);

        let light = openmw_attitude(rotation_of("light_de_lantern_03", &[("mangle", "90 30 0")]));
        let expected = Rotation3::from_axis_angle(&Vector3::z_axis(), 90.0_f32.to_radians())
            * Rotation3::from_axis_angle(&Vector3::y_axis(), (-30.0_f32).to_radians());
        assert!((light.matrix() - expected.matrix()).norm() < 1e-4);

        let up = openmw_attitude(rotation_of("static_x", &[("angle", "-1")]));
        assert!((up * Vector3::x() - Vector3::z()).norm() < 1e-4);
    }
}

#[cfg(test)]
mod brush_rotation_tests {
    use super::*;
    use morrobroom::slipgate::repr::Map;
    use tes3::nif::{NiStream, NiTriShapeData};

    /// A box's faces as three points each, wound the way `TrenchBroom` writes them.
    const BOX_FACES: [[[f32; 3]; 3]; 6] = [
        [[-1.0, -1.0, -1.0], [-1.0, 0.0, -1.0], [-1.0, -1.0, 0.0]],
        [[-1.0, -1.0, -1.0], [-1.0, -1.0, 0.0], [0.0, -1.0, -1.0]],
        [[-1.0, -1.0, -1.0], [0.0, -1.0, -1.0], [-1.0, 0.0, -1.0]],
        [[1.0, 1.0, 1.0], [1.0, 2.0, 1.0], [2.0, 1.0, 1.0]],
        [[1.0, 1.0, 1.0], [2.0, 1.0, 1.0], [1.0, 1.0, 2.0]],
        [[1.0, 1.0, 1.0], [1.0, 1.0, 2.0], [1.0, 2.0, 1.0]],
    ];
    const HALF_EXTENTS: [f32; 3] = [32.0, 8.0, 4.0];
    const CENTER: [f32; 3] = [96.0, 48.0, 24.0];

    fn attitude(pitch: f32, yaw: f32, roll: f32) -> Rotation3<f32> {
        Rotation3::from_axis_angle(&Vector3::z_axis(), yaw.to_radians())
            * Rotation3::from_axis_angle(&Vector3::y_axis(), pitch.to_radians())
            * Rotation3::from_axis_angle(&Vector3::x_axis(), roll.to_radians())
    }

    /// The attitude `OpenMW` rebuilds from reference angles.
    fn openmw_attitude(rotation: [f32; 3]) -> Rotation3<f32> {
        Rotation3::from_axis_angle(&Vector3::x_axis(), -rotation[0])
            * Rotation3::from_axis_angle(&Vector3::y_axis(), -rotation[1])
            * Rotation3::from_axis_angle(&Vector3::z_axis(), -rotation[2])
    }

    /// A `world_Detail` box, 64 by 16 by 8 along its own axes, as `TrenchBroom`
    /// saves it after rotating it to `mangle`: brushes turned in the map, and
    /// the rotation in `mangle`.
    fn rotated_box_map(mangle: [f32; 3]) -> String {
        let rotation = attitude(mangle[0], mangle[1], mangle[2]);
        let place = |point: [f32; 3]| {
            let local = Vector3::from_fn(|axis, _| point[axis] * HALF_EXTENTS[axis]);
            let world = rotation * local + Vector3::from(CENTER);
            format!("( {:.6} {:.6} {:.6} )", world.x, world.y, world.z)
        };
        let axis = |local: [f32; 3]| {
            let world = rotation * Vector3::from(local);
            format!("{:.6} {:.6} {:.6}", world.x, world.y, world.z)
        };
        let faces: Vec<String> = BOX_FACES
            .iter()
            .map(|points| {
                format!(
                    "{} {} {} mb/canonical [ {} 0 ] [ {} 0 ] 0 1 1",
                    place(points[0]),
                    place(points[1]),
                    place(points[2]),
                    axis([1.0, 0.0, 0.0]),
                    axis([0.0, 0.0, -1.0]),
                )
            })
            .collect();
        format!(
            "// Game: Morrowind\n// Format: Valve\n{{\n\"classname\" \"worldspawn\"\n}}\n{{\n\"classname\" \"world_Detail\"\n\"ESM3_RefId\" \"tilted\"\n\"mangle\" \"{} {} {}\"\n{{\n{}\n}}\n}}\n",
            mangle[0],
            mangle[1],
            mangle[2],
            faces.join("\n")
        )
    }

    #[test]
    fn a_rotated_brush_entity_keeps_its_own_axes_in_openmw() {
        let mangle = [20.0, 30.0, 40.0];
        let map = rotated_box_map(mangle)
            .parse::<Map>()
            .expect("rotated box map should parse");
        let openmw_config = openmw_config::OpenMWConfiguration::new_empty("rotation-test-config")
            .expect("construct in-memory empty OpenMW configuration");
        let map_data = MapData::from_map(map, "tilt.map", false, &openmw_config);
        let output_dir =
            std::env::temp_dir().join(format!("morrobroom-brush-rotation-{}", std::process::id()));
        let (work_dir, map_dir) = morrobroom::create_workdir_at(Path::new("tilt.map"), &output_dir)
            .expect("create the generated-asset tree");
        let mut state = CompileState {
            map_data: &map_data,
            work_dir: &work_dir,
            map_dir: &map_dir,
            object_scale: 2.0,
            cell: Some(Cell::default()),
            created_objects: Vec::new(),
            processed_base_objects: HashSet::new(),
            used_indices: BTreeSet::new(),
        };
        let (entity_id, brushes) = map_data
            .geomap
            .entity_brushes
            .iter()
            .find(|(entity_id, _)| {
                map_data
                    .get_entity_properties(**entity_id)
                    .get(&"classname".to_string())
                    .is_some_and(|classname| classname.as_str() == "world_Detail")
            })
            .expect("the map has one world_Detail");

        process_brush_entity(&mut state, *entity_id, brushes).expect("compile the rotated box");

        let nif = NiStream::from_path(work_dir.join("Meshes/tilt/tilted.nif"));
        std::fs::remove_dir_all(&output_dir).expect("remove the generated-asset tree");
        let reference = state
            .cell
            .expect("the test cell stays")
            .references
            .into_values()
            .find(|reference| reference.id == "tilted")
            .expect("the box should be placed");
        let expected = attitude(mangle[0], mangle[1], mangle[2]);
        assert!(
            (openmw_attitude(reference.rotation).matrix() - expected.matrix()).norm() < 1e-4,
            "OpenMW would turn the box by {:?}, TrenchBroom shows {expected:?}",
            openmw_attitude(reference.rotation)
        );

        // Counter-rotated by the same attitude, the mesh is the box along its own
        // axes: 64 by 16 by 8 around its center.
        let nif = nif.expect("the compiled mesh should load");
        let vertices: Vec<_> = nif
            .objects_of_type::<NiTriShapeData>()
            .flat_map(|data| data.vertices.iter().copied())
            .collect();
        assert!(!vertices.is_empty());
        for vertex in vertices {
            for (axis, coordinate) in vertex.to_array().into_iter().enumerate() {
                assert!(
                    (coordinate.abs() - HALF_EXTENTS[axis]).abs() < 0.01,
                    "mesh vertex {vertex:?} is off the box's own axes"
                );
            }
        }
    }
}

#[cfg(test)]
mod worldspawn_cell_tests {
    use super::*;
    use morrobroom::slipgate::repr::Map;

    /// Everything in brush entities and placed records; worldspawn has no brushes,
    /// as in every map nif2map writes.
    const BRUSHLESS_WORLDSPAWN_MAP: &str = r#"// Game: Morrowind
// Format: Valve
{
"classname" "worldspawn"
"ESM3_Name" "Brushless Hall"
}
{
"classname" "world_Detail"
"ESM3_RefId" "hall_pillar"
{
( -16 -16 -16 ) ( -16 -15 -16 ) ( -16 -16 -15 ) mb/canonical [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -16 -16 -15 ) ( -15 -16 -16 ) mb/canonical [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -15 -16 -16 ) ( -16 -15 -16 ) mb/canonical [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 16 17 16 ) ( 17 16 16 ) mb/canonical [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 17 16 16 ) ( 16 16 17 ) mb/canonical [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 16 16 16 ) ( 16 16 17 ) ( 16 17 16 ) mb/canonical [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
}
{
"classname" "static_ex_common_house_01"
"origin" "64 0 0"
"ESM3_RefId" "ex_common_house_01"
}
"#;

    #[test]
    fn a_worldspawn_without_brushes_still_makes_the_cell() {
        let map = BRUSHLESS_WORLDSPAWN_MAP
            .parse::<Map>()
            .expect("brushless worldspawn map should parse");
        let openmw_config = openmw_config::OpenMWConfiguration::new_empty("worldspawn-test-config")
            .expect("construct in-memory empty OpenMW configuration");
        let map_data = MapData::from_map(map, "hall.map", false, &openmw_config);
        let output_dir = std::env::temp_dir().join(format!(
            "morrobroom-brushless-worldspawn-{}",
            std::process::id()
        ));
        let (work_dir, map_dir) = morrobroom::create_workdir_at(Path::new("hall.map"), &output_dir)
            .expect("create the generated-asset tree");
        let mut state = CompileState {
            map_data: &map_data,
            work_dir: &work_dir,
            map_dir: &map_dir,
            object_scale: 2.0,
            cell: None,
            created_objects: Vec::new(),
            processed_base_objects: HashSet::new(),
            used_indices: BTreeSet::new(),
        };

        let compiled = compile_entities(&mut state);
        std::fs::remove_dir_all(&output_dir).expect("remove the generated-asset tree");
        compiled.expect("a brushless worldspawn map should compile");

        let cell = state.cell.expect("the map should compile into a cell");
        assert_eq!(cell.name, "Brushless Hall");
        let mut placed: Vec<_> = cell
            .references
            .values()
            .map(|reference| reference.id.as_str())
            .collect();
        placed.sort_unstable();
        assert_eq!(placed, ["ex_common_house_01", "hall_pillar"]);
    }

    #[test]
    fn brush_entity_classes_are_the_ones_the_fgds_declare() {
        let mut declared: Vec<&str> = [
            include_str!("../resources/Morrowind.fgd"),
            include_str!("../resources/Nif.fgd"),
        ]
        .into_iter()
        .flat_map(str::lines)
        .filter(|line| line.starts_with("@SolidClass"))
        .filter_map(|line| line.split_once("= "))
        .filter_map(|(_, declaration)| declaration.split([' ', ':']).next())
        .collect();
        declared.push("func_group");
        declared.sort_unstable();
        let mut compiled = BRUSH_ENTITY_CLASSES.to_vec();
        compiled.sort_unstable();
        assert_eq!(compiled, declared);
    }

    #[test]
    fn worldspawn_is_never_an_unidentified_point_entity() {
        let properties = HashMap::new();
        assert_eq!(
            classify_point_entity("worldspawn", &properties),
            PointEntity::Skip
        );
        assert_eq!(
            classify_point_entity("world_Detail", &properties),
            PointEntity::EmptyBrushEntity
        );
    }
}

#[cfg(test)]
mod nif2map_round_trip_tests {
    use super::*;
    use morrobroom::slipgate::repr::Map;
    use tes3::nif::{NiNode, NiStream, NiTriShape, NiTriShapeData, glam};

    const MIN_CORNER: [f32; 3] = [60.0, 36.0, 20.0];
    const MAX_CORNER: [f32; 3] = [140.0, 84.0, 52.0];

    /// The box's corners, numbered by bits: x is bit 0, y bit 1, z bit 2.
    fn corners() -> Vec<glam::Vec3> {
        (0..8_u8)
            .map(|index| {
                let pick = |axis: usize| {
                    if index >> axis & 1 == 0 {
                        MIN_CORNER[axis]
                    } else {
                        MAX_CORNER[axis]
                    }
                };
                glam::vec3(pick(0), pick(1), pick(2))
            })
            .collect()
    }

    /// A closed box in Morrowind units, each face wound counter-clockwise from
    /// outside.
    #[allow(
        clippy::field_reassign_with_default,
        reason = "The tes3 NIF facade exposes flattened accessors but nested constructors."
    )]
    fn box_nif() -> NiStream {
        let faces: [[u16; 4]; 6] = [
            [0, 4, 6, 2],
            [1, 3, 7, 5],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 2, 3, 1],
            [4, 5, 7, 6],
        ];
        let mut stream = NiStream::default();
        let mut data = NiTriShapeData::default();
        data.vertices = corners();
        data.triangles = faces
            .iter()
            .flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]])
            .collect();
        let data_link = stream.insert(data);
        let mut shape = NiTriShape::default();
        shape.geometry_data = data_link.cast();
        shape.name = "crate".into();
        let shape_link = stream.insert(shape);
        let mut root = NiNode::default();
        root.children.push(shape_link.cast());
        let root_link = stream.insert(root);
        stream.roots.push(root_link.cast());
        stream
    }

    #[test]
    fn nif2map_then_compile_at_the_default_scale_keeps_the_nif_size() {
        let scale = broom_args::default_scale()
            .parse::<f32>()
            .expect("the default scale parses");
        let root = std::env::temp_dir().join(format!(
            "morrobroom-nif2map-round-trip-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create the round-trip directory");
        let nif_path = root.join("crate.nif");
        box_nif()
            .save_path(&nif_path)
            .expect("write the source NIF");
        morrobroom::nif2map::run(&morrobroom::nif2map::Options {
            inputs: vec![nif_path],
            recursive: false,
            output_dir: root.join("maps"),
            shell_thickness: 8.0,
            fallback_thickness: 1.0,
            fallback: "planar-prisms".into(),
            skip_material: "skip".into(),
            texture_roots: vec![root.clone()],
            include_collision: false,
            overwrite: true,
            dry_run: false,
            verbose: false,
            validate: true,
            max_brushes: 20_000,
            scale: f64::from(scale),
        })
        .expect("nif2map should reverse the box");
        let map = std::fs::read_to_string(root.join("maps/crate.map"))
            .expect("nif2map should write crate.map")
            .parse::<Map>()
            .expect("nif2map's map should parse");

        let openmw_config = openmw_config::OpenMWConfiguration::new_empty("round-trip-config")
            .expect("construct in-memory empty OpenMW configuration");
        let map_data = MapData::from_map(map, "crate.map", false, &openmw_config);
        let (work_dir, map_dir) =
            morrobroom::create_workdir_at(Path::new("crate.map"), &root.join("build"))
                .expect("create the generated-asset tree");
        let mut state = CompileState {
            map_data: &map_data,
            work_dir: &work_dir,
            map_dir: &map_dir,
            object_scale: scale,
            cell: None,
            created_objects: Vec::new(),
            processed_base_objects: HashSet::new(),
            used_indices: BTreeSet::new(),
        };
        compile_entities(&mut state).expect("compile nif2map's map");

        let cell = state.cell.take().expect("the map compiles into a cell");
        let mut world_vertices = Vec::new();
        for object in &state.created_objects {
            let TES3Object::Static(record) = object else {
                continue;
            };
            let Some(nif) = NiStream::from_path(work_dir.join("Meshes").join(&record.mesh)).ok()
            else {
                continue;
            };
            for reference in cell
                .references
                .values()
                .filter(|reference| reference.id == record.id)
            {
                let origin = glam::Vec3::from(reference.translation);
                for data in nif.objects_of_type::<NiTriShapeData>() {
                    world_vertices
                        .extend(data.vertices.iter().map(|vertex| origin + *vertex * scale));
                }
            }
        }
        std::fs::remove_dir_all(&root).expect("remove the round-trip directory");

        assert!(
            !world_vertices.is_empty(),
            "the compiled box has no vertices"
        );
        let corners = corners();
        for vertex in &world_vertices {
            assert!(
                corners.iter().any(|corner| vertex.distance(*corner) < 0.01),
                "compiled vertex {vertex} is not a corner of the source box {corners:?}"
            );
        }
        for corner in &corners {
            assert!(
                world_vertices
                    .iter()
                    .any(|vertex| vertex.distance(*corner) < 0.01),
                "the source corner {corner} is missing from the compiled box"
            );
        }
    }
}
