use std::{
    borrow::Cow,
    cmp::min,
    collections::{BTreeSet, HashMap, HashSet},
    io,
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
    let broom_args = MorrobroomArgs::parse();

    let (map_path, object_scale, output_path) = match broom_args.command {
        BroomCommand::Compile {
            map_path,
            object_scale,
            output_path,
        } => (map_path, object_scale, output_path),
        BroomCommand::FGD {
            object_scale,
            object_types,
            output_path,
            openmw_config,
        } => {
            let object_types = object_types.unwrap_or_else(|| default_object_types().into());
            let object_type_tags: Vec<&'static str> = object_types
                .iter()
                .map(|object_type| object_type.as_str())
                .collect();

            morrobroom::fgd::generate_fgd(
                openmw_config.as_deref(),
                &object_type_tags,
                object_scale,
                &output_path,
            )
            .map_err(|error| io::Error::other(error.to_string()))?;

            return Ok(());
        }
    };

    let (work_dir, map_dir) = create_workdir(&map_path)
        .map_err(|error_string| io::Error::new(io::ErrorKind::InvalidInput, error_string))?;

    // Push the cell record to the plugin
    // It can't be done multiple times :/
    let mut cell = None;
    let mut created_objects = Vec::new();
    let mut processed_base_objects: HashSet<Cow<'_, str>> = HashSet::new();
    let map_string = map_path.to_string_lossy().to_string();

    let map_data = MapData::new(&map_string);

    let plugin_path = match &output_path {
        Some(path) => path.to_owned(),
        None => {
            let mut plugin_path = map_path.clone();
            plugin_path.set_extension("omwaddon");
            plugin_path
        }
    };

    let mut plugin = esp::Plugin::from_path(&plugin_path).unwrap_or(esp::Plugin::default());

    let mut used_indices =
        plugin
            .objects_of_type::<Cell>()
            .fold(BTreeSet::new(), |mut acc, cell| {
                acc.extend(
                    cell.references
                        .iter()
                        .filter(|((mast_idx, _), _)| *mast_idx == 0)
                        .map(|((_, ref_idx), _)| *ref_idx),
                );
                acc
            });

    assert!(
        map_data.geomap.entity_brushes.len() > 0,
        "No brushes found in map! You probably used an apostrophe in worldspawn properties."
    );

    for (entity_id, brushes) in map_data.geomap.entity_brushes.iter() {
        let prop_map = map_data.get_entity_properties(entity_id);

        let mut mesh = Mesh::from_map(brushes, &map_data, &object_scale, entity_id);

        match prop_map.get(&"_tb_id".to_string()) {
            Some(group_id) => {
                // This object is a group
                let mut ref_instances = 0;
                let mut nodes = Vec::new();
                let mut processed_group_objects: Vec<String> = Vec::new();

                for (entity_id, brushes) in map_data.geomap.entity_brushes.iter() {
                    let prop_map = map_data.get_entity_properties(entity_id);
                    // let group_id;

                    match prop_map.get(&"_tb_id".to_string()) {
                        Some(_) => continue,
                        None => {}
                    }

                    // We also should account for linked groups in the case below!
                    match prop_map.get(&"_tb_group".to_string()) {
                        Some(obj_group) => {
                            if obj_group != group_id {
                                // println!("Found another group! Bailing on creating this mesh and saving it into the cellref.");
                                continue;
                            };
                        }
                        None => {
                            // println!("This object isn't part of a group, don't do anything with it here.");
                            continue;
                        }
                    }

                    match prop_map.get(&"RefId".to_string()) {
                        Some(ref_id) => {
                            ref_instances += 1;
                            if processed_group_objects.contains(ref_id) {
                                println!(
                                    "We don't have full refId support yet, but this object {ref_id} has appeared in this group {ref_instances} times"
                                ); // In theory by this point, we should have a mesh for this object already.
                                // Alternatively, we have to generate it here, which is probably going to be likely.
                                continue; // If it does exist, though, we need to simply derive its placement
                            }
                            println!(
                                "Adding {ref_id} to unique group set. This should actually not be generated as part of the mesh, but rather create a new one for this unique object. Then it should be placed in the ESP file and referred to later."
                            );
                            processed_group_objects.push(ref_id.to_string());
                        }
                        None => {} // object has no refid, and it's not a group, but it is a member of a group. This maybe shouldn't happen
                    }

                    nodes.extend(BrushNiNode::from_brushes(brushes, &map_data, entity_id));
                }

                for node in nodes {
                    mesh.attach_node(node, map_data.vfs());
                }
            }
            None => {}
        }

        let ref_id = match prop_map.get(&"RefId".to_string()) {
            Some(ref_id) => Cow::Borrowed(&ref_id[..min(ref_id.len(), 32)]),
            None => {
                // The only entity that ever has this happen should be worldspawn
                let ref_id = format!("{map_dir}-scene-{entity_id}");
                Cow::Owned(ref_id[..min(ref_id.len(), 32)].to_string())
            }
        };

        if processed_base_objects.contains(&ref_id) {
            println!("Placing new instance of {ref_id}");
        } else {
            processed_base_objects.insert(ref_id.clone());
        }

        let mesh_name = match prop_map.get(&"Model".to_string()) {
            Some(mesh_name) => mesh_name.to_string(),
            None => format!("{}/{}.nif", map_dir, &ref_id),
        };

        // We create the base record for the objects here.
        match prop_map.get(&"classname".to_string()) {
            Some(classname) => match classname.as_str() {
                "world_Activator" => {
                    mesh.game_object = game_object::activator(&prop_map, &ref_id, &mesh_name);
                }
                "world_Container" => {
                    mesh.game_object = game_object::container(&prop_map, &ref_id, &mesh_name);
                }
                "item_Alchemy" => {
                    mesh.game_object = game_object::potion(&prop_map, &ref_id, &mesh_name);
                }
                "item_Apparatus" => {
                    mesh.game_object = game_object::apparatus(&prop_map, &ref_id, &mesh_name);
                }
                "item_Armor" => {
                    mesh.game_object = game_object::armor(&prop_map, &ref_id, &mesh_name);
                }
                "item_Book" => {
                    mesh.game_object = game_object::book(&prop_map, &ref_id, &mesh_name);
                }
                "item_Ingredient" => {
                    mesh.game_object = game_object::ingredient(&prop_map, &ref_id, &mesh_name);
                }
                "item_Light" => {
                    // Keep in mind this is for lights made from brushes. We also need to support point lights, so that they don't necessarily have to be associated with an object.
                    mesh.game_object =
                        game_object::light(&prop_map, &object_scale, &ref_id, &mesh_name);
                }
                "item_Misc" => {
                    mesh.game_object = game_object::misc(&prop_map, &ref_id, &mesh_name);
                }
                "worldspawn" => {
                    let mut local_cell = game_object::cell(&prop_map);

                    if local_cell.name.is_empty() {
                        local_cell.name = map_dir.clone();
                    }

                    processed_base_objects.insert(Cow::Owned(local_cell.name.to_string()));
                    processed_base_objects.insert(ref_id.clone());

                    // processed_base_objects
                    //     .extend([&Cow::Borrowed(&local_cell.name.as_str()), &ref_id]);

                    cell = Some(local_cell);

                    mesh.game_object = Static {
                        id: ref_id.clone().to_string(),
                        mesh: mesh_name.to_owned(),
                        flags: esp::ObjectFlags::default(),
                    }
                    .into();
                }
                "world_Detail" => {
                    processed_base_objects.insert(ref_id.clone());
                    mesh.game_object = Static {
                        id: ref_id.clone().to_string(),
                        mesh: mesh_name.to_owned(),
                        ..Default::default()
                    }
                    .into()
                }
                _ => {
                    println!(
                        "No matching object type found! {classname} requested for {entity_id}"
                    );
                    continue;
                } // Object has a class, but we don't know what it was.
            },
            None => {}
        }

        // All nodes on the mesh collectively have their own position
        // The center of which, is determined to be the actual position of the asset
        // This is then used in plugin serialization to define the object's local position, and this position is then correspondingly stripped off the NIF
        mesh.worldspace_position = Mesh::centroid(&mesh.node_distances) * (object_scale as f32);

        mesh.mangle = match get_prop("mangle", &prop_map) {
            None => get_rotation(&"0 0 0".to_string()),
            Some(mangle) => get_rotation(&mangle),
        };

        // Also use linked groups to determine if the mesh & base def should be ignored
        // Also we should probably just not check this way *only* and
        // also destroy matching objects once the refId has been determined.
        if !created_objects.contains(&mesh.game_object) {
            let mesh_path = format!("{}/Meshes/{mesh_name}", work_dir.display());
            println!("Saving base object definition & mesh for {ref_id} to plugin as {mesh_path}");
            mesh.save(&mesh_path);
            created_objects.push(mesh.game_object.clone());
        }

        append_cell_reference(
            &mut used_indices,
            &mut cell,
            ref_id.to_string(),
            mesh.worldspace_position,
            mesh.mangle,
        );
    }

    for entity_id in map_data.geomap.point_entities.iter() {
        let prop_map = map_data.get_entity_properties(entity_id);
        let lowest_available_index = &used_indices.find_lowest();

        match prop_map
            .get(&"classname".to_string())
            .expect("All point entities have class names")
            .as_str()
        {
            light if light.contains("Light_Point") => {
                let mut ref_id = format!("{map_dir}-PL-{lowest_available_index}");
                ref_id = ref_id[..min(ref_id.len(), 32)].to_string();

                let radius: u32 = light
                    .chars()
                    .skip_while(|c| !c.is_digit(10))
                    .take_while(|c| c.is_digit(10))
                    .collect::<String>()
                    .parse()
                    .expect(
                        "All point light types should have a radius encoded in their classnames!",
                    );

                created_objects.push(game_object::point_light(
                    &prop_map,
                    &object_scale,
                    radius,
                    ref_id.as_str(),
                ));

                append_cell_reference(
                    &mut used_indices,
                    &mut cell,
                    ref_id,
                    point_entity_position(&object_scale, &prop_map),
                    [0.0, 0.0, 0.0],
                );
            }
            "world_CreatureList" => {
                let ref_id = Cow::Owned(match prop_map.get(&"RefId".to_string()) {
                    Some(ref_id) => ref_id[..min(ref_id.len(), 32)].to_string(),
                    None => panic!(
                        "RefIds are mandatory for all point entities, failed on creature list, entity ID: {}",
                        entity_id
                    ),
                });

                if !processed_base_objects.contains(&ref_id) {
                    created_objects.push(game_object::creature_list(&prop_map, &ref_id));
                    processed_base_objects.insert(ref_id.clone());
                }

                append_cell_reference(
                    &mut used_indices,
                    &mut cell,
                    ref_id.clone().to_string(),
                    point_entity_position(&object_scale, &prop_map),
                    [0.0, 0.0, 0.0],
                );
            }
            "world_ItemList" => {
                let ref_id = Cow::Owned(match prop_map.get(&"RefId".to_string()) {
                    Some(ref_id) => ref_id[..min(ref_id.len(), 32)].to_string(),
                    None => panic!(
                        "RefIds are mandatory for all point entities, failed on item list, entity ID: {}",
                        entity_id
                    ),
                });

                if !processed_base_objects.contains(&ref_id) {
                    created_objects.push(game_object::item_list(&prop_map, &ref_id));
                    processed_base_objects.insert(ref_id);
                }
            }
            class => {
                println!("Unidentified point entity class: {class}")
            }
        }
    }

    if let Some(cell) = cell {
        processed_base_objects.insert(Cow::Owned(cell.editor_id().to_string()));
        created_objects.push(cell.into());
    }

    let point_light_string = format!("{map_dir}-PL");
    plugin.objects.retain(|obj| {
        !processed_base_objects.contains(&obj.editor_id())
            && !obj.editor_id().contains(&point_light_string)
    });

    plugin.objects.extend(created_objects);

    create_header_if_missing(&mut plugin);

    plugin.sort_objects();

    plugin
        .save_path(&plugin_path)
        .expect(&format!("Saving {} failed!", &plugin_path.display()));

    println!("Wrote {} to disk successfully.", &plugin_path.display());

    Ok(())
}

fn point_entity_position(scale_mode: &f32, prop_map: &HashMap<&String, &String>) -> SV3 {
    let coords: Vec<f32> = match prop_map.iter().find(|(k, _)| k.as_str() == "origin") {
        None => {
            eprintln!("All point entities must have an origin!");
            std::process::exit(256);
        }
        Some((_, v)) => v
            .split_whitespace()
            .map(|s| s.parse::<f32>().expect("Invalid coordinate"))
            .collect(),
    };

    assert_eq!(coords.len(), 3, "Origin must have exactly 3 coordinates");

    SV3::new(coords[0], coords[1], coords[2]) * (*scale_mode)
}

fn append_cell_reference(
    used_indices: &mut BTreeSet<u32>,
    cell: &mut Option<Cell>,
    ref_id: String,
    translation: SV3,
    rotation: [f32; 3],
) {
    let lowest_available_index = used_indices.find_lowest();

    if let Some(local_cell) = cell {
        local_cell.references.insert(
            (0 as u32, lowest_available_index),
            esp::Reference {
                id: ref_id.to_owned(),
                mast_index: 0 as u32,
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

    for (i, token) in input.split_whitespace().take(3).enumerate() {
        if let Ok(val) = token.parse::<f32>() {
            angles[i] = val.to_radians();
        }
    }

    [angles[2], angles[0], angles[1]]
}

/// Should probably make some specific struct for handling ESP objects
fn create_header_if_missing(plugin: &mut Plugin) {
    match plugin.objects_of_type::<Header>().count() {
        0 => {
            // Later during serialization, we should make sure to include author and header info.
            plugin.objects.push(TES3Object::Header(Header {
                version: 1.3,
                ..Default::default()
            }));
        }
        _ => {
            println!(
                "Plugin was found to already have {} header records",
                plugin.objects_of_type::<Header>().count()
            )
        }
    }
}
