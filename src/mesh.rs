use std::collections::HashMap;

use morrobroom::slipgate::{Vector3 as SV3, brush::BrushId, entity::EntityId};
use nalgebra::{Rotation3, Vector3};
use tes3::{
    esp,
    nif::{
        self, BumpMap, LightingMode, Map, NiAlphaAccumulator, NiAlphaProperty, NiBSAnimationNode,
        NiBillboardNode, NiFloatData, NiFloatKey, NiLinFloatKey, NiLink, NiMaterialProperty,
        NiNode, NiSortAdjustNode, NiStream, NiTexturingProperty, NiTimeController, NiTriShape,
        NiTriShapeData, NiUVController, NiUVData, NiVertexColorProperty, RootCollisionNode,
        SortingMode, SourceVertexMode, TextureMap, TextureSource,
    },
};
use vfstool_lib::VFS;

use crate::{
    MapData,
    brush_ni_node::{BrushNiAlphaProps, BrushNiColorProps, BrushNiMatProps, BrushNiNode},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NifStructuralKind {
    Billboard { mode: u32 },
    SortAdjust { mode: u32 },
    CollisionRoot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NifStructuralMarker {
    pub kind: NifStructuralKind,
    pub link_name: Option<String>,
    pub target: Option<String>,
    pub origin: SV3,
}

#[derive(Clone, Copy)]
enum RenderParent {
    Root(NiLink<NiNode>),
    Billboard(NiLink<NiBillboardNode>),
    SortAdjust(NiLink<NiSortAdjustNode>),
    CollisionRoot(NiLink<RootCollisionNode>),
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Mesh cardinalities are normalized by the NIF f32 geometry layer."
)]
fn vertex_count_as_f32(count: usize) -> f32 {
    count as f32
}

pub struct Mesh {
    pub game_object: esp::TES3Object,
    pub node_distances: Vec<SV3>,
    pub stream: NiStream,
    pub base_index: NiLink<NiNode>,
    pub worldspace_position: SV3,
    /// ESP reference rotations in radians, after TB-to-TES3 basis conversion.
    pub mangle: [f32; 3],
    collision_index: NiLink<RootCollisionNode>,
}

impl Mesh {
    #[allow(
        clippy::field_reassign_with_default,
        reason = "The current tes3 NIF structs nest transform data behind generated base records."
    )]
    fn new(scale_mode: f32) -> Self {
        let mut stream = NiStream::default();
        let mut root_node = NiNode::default();

        let mut base_node = NiNode::default();
        base_node.scale = scale_mode;
        let base_index = stream.insert(base_node);
        root_node.children.push(base_index.cast());

        let mut collision_base = NiNode::default();
        collision_base.scale = scale_mode;
        let collision_node = RootCollisionNode {
            base: collision_base,
        };
        let collision_index = stream.insert(collision_node);
        root_node.children.push(collision_index.cast());

        let root_index = stream.insert(root_node);

        stream.roots = vec![root_index.cast()];

        Mesh {
            stream,
            base_index,
            collision_index,
            game_object: esp::TES3Object::Static(esp::Static::default()),
            node_distances: Vec::new(),
            worldspace_position: SV3::default(),
            mangle: [0.0, 0.0, 0.0],
        }
    }

    pub fn from_map(
        brushes: &[BrushId],
        map_data: &MapData,
        scale_mode: f32,
        entity_id: EntityId,
    ) -> Mesh {
        let mut mesh = Mesh::new(scale_mode);

        for brush_id in brushes {
            let brush_nodes = BrushNiNode::from_brush(*brush_id, entity_id, map_data);

            for node in brush_nodes {
                mesh.attach_node(node, map_data.vfs(), map_data.lightmap_texture_name());
            }
        }
        mesh
    }

    pub fn align_to_center(&mut self) {
        let center = Mesh::centroid(&self.node_distances);
        let rotation = self.mesh_local_rotation();
        for tri_shape in self.stream.objects_of_type_mut::<NiTriShapeData>() {
            for vert in &mut tri_shape.vertices {
                vert.x -= center.x;
                vert.y -= center.y;
                vert.z -= center.z;
                let rotated_vert = rotation.transform_vector(&Vector3::new(vert.x, vert.y, vert.z));
                vert.x = rotated_vert[0];
                vert.y = rotated_vert[1];
                vert.z = rotated_vert[2];
            }
        }

        for vert in &mut self.node_distances {
            vert.x -= center.x;
            vert.y -= center.y;
            vert.z -= center.z;
            let rotated_vert = rotation.transform_vector(&Vector3::new(vert.x, vert.y, vert.z));
            vert.x = rotated_vert[0];
            vert.y = rotated_vert[1];
            vert.z = rotated_vert[2];
        }
    }

    pub fn attach_nif_semantics(
        &mut self,
        markers: &[NifStructuralMarker],
        geometry_target: Option<&str>,
    ) {
        if markers.is_empty() && geometry_target.is_none() {
            return;
        }

        let center = Mesh::centroid(&self.node_distances);
        let geometry_children = std::mem::take(
            &mut self
                .stream
                .get_mut(self.base_index)
                .expect("base NIF node should remain valid")
                .children,
        );
        let rotation = self.mesh_local_rotation();
        let absolute_origin = |origin: SV3| -> [f32; 3] {
            let relative = rotation.transform_vector(&Vector3::new(
                origin.x - center.x,
                origin.y - center.y,
                origin.z - center.z,
            ));
            [relative.x, relative.y, relative.z]
        };

        let mut current_parent = RenderParent::Root(self.base_index);
        let mut current_origin = [0.0; 3];
        let mut named_nodes: HashMap<String, (RenderParent, [f32; 3])> = HashMap::new();
        let mut collision_markers = 0;

        for marker in markers {
            let world_origin = absolute_origin(marker.origin);
            if marker.kind == NifStructuralKind::CollisionRoot {
                collision_markers += 1;
                if collision_markers > 1 {
                    eprintln!(
                        "More than one nif_node_collision_root applies to this compiled mesh; keeping the first"
                    );
                    continue;
                }
                self.configure_collision_root(world_origin, marker.link_name.as_deref());
                Self::record_link_name(
                    &mut named_nodes,
                    marker.link_name.as_deref(),
                    RenderParent::CollisionRoot(self.collision_index),
                    world_origin,
                );
                continue;
            }

            let (parent, parent_origin) = resolve_nif_target(
                marker.target.as_deref(),
                &named_nodes,
                (current_parent, current_origin),
            );
            let new_parent = self.insert_render_marker(
                marker,
                parent,
                subtract_points(world_origin, parent_origin),
            );
            current_parent = new_parent;
            current_origin = world_origin;
            Self::record_link_name(
                &mut named_nodes,
                marker.link_name.as_deref(),
                new_parent,
                world_origin,
            );
        }

        let (geometry_parent, geometry_origin) = resolve_nif_target(
            geometry_target,
            &named_nodes,
            (current_parent, current_origin),
        );
        for child in geometry_children {
            if let Some(shape) = self.stream.get_mut(child.cast::<NiTriShape>()) {
                shape.translation = negate_point(geometry_origin).into();
            }
            self.push_render_child(geometry_parent, child);
        }
    }

    fn insert_render_marker(
        &mut self,
        marker: &NifStructuralMarker,
        parent: RenderParent,
        translation: [f32; 3],
    ) -> RenderParent {
        match marker.kind {
            NifStructuralKind::Billboard { mode } => {
                let mut node = NiBillboardNode::default();
                node.base.flags |= billboard_mode_flags(mode).unwrap_or_else(|| {
                    eprintln!(
                        "Nif_Billboard_Mode {mode} cannot be stored in a Morrowind NIF, which has modes 0 to 3; using Always Face Camera"
                    );
                    0
                });
                node.base.name = marker.link_name.clone().unwrap_or_default();
                node.base.translation = translation.into();
                let link = self.stream.insert(node);
                self.push_render_child(parent, link.cast());
                RenderParent::Billboard(link)
            }
            NifStructuralKind::SortAdjust { mode } => {
                let sorting_mode = sorting_mode(mode).unwrap_or_else(|| {
                    eprintln!("Invalid Nif_Sort_Mode {mode}; using Inherit");
                    SortingMode::Inherit
                });
                let mut node = NiSortAdjustNode::default();
                node.base.name = marker.link_name.clone().unwrap_or_default();
                node.base.translation = translation.into();
                if sorting_mode == SortingMode::Subsort {
                    let accumulator = self.stream.insert(NiAlphaAccumulator::default());
                    node.sub_sorter = accumulator.cast();
                }
                node.sorting_mode = sorting_mode;
                let link = self.stream.insert(node);
                self.push_render_child(parent, link.cast());
                RenderParent::SortAdjust(link)
            }
            NifStructuralKind::CollisionRoot => unreachable!(),
        }
    }

    fn record_link_name(
        named_nodes: &mut HashMap<String, (RenderParent, [f32; 3])>,
        link_name: Option<&str>,
        parent: RenderParent,
        origin: [f32; 3],
    ) {
        if let Some(name) = link_name
            && named_nodes
                .insert(name.to_owned(), (parent, origin))
                .is_some()
        {
            eprintln!("Duplicate Nif_LinkName {name:?}; the most recently created node wins");
        }
    }

    fn push_render_child(&mut self, parent: RenderParent, child: NiLink<nif::NiAVObject>) {
        match parent {
            RenderParent::Root(link) => self
                .stream
                .get_mut(link)
                .expect("render root should remain valid")
                .children
                .push(child),
            RenderParent::Billboard(link) => self
                .stream
                .get_mut(link)
                .expect("billboard node should remain valid")
                .children
                .push(child),
            RenderParent::SortAdjust(link) => self
                .stream
                .get_mut(link)
                .expect("sort-adjust node should remain valid")
                .children
                .push(child),
            RenderParent::CollisionRoot(link) => self
                .stream
                .get_mut(link)
                .expect("collision root should remain valid")
                .children
                .push(child),
        }
    }

    fn configure_collision_root(&mut self, origin: [f32; 3], name: Option<&str>) {
        let (children, root_scale) = {
            let root = self
                .stream
                .get_mut(self.collision_index)
                .expect("collision root should remain valid");
            root.base.translation = origin.into();
            if let Some(name) = name {
                name.clone_into(&mut root.base.name);
            }
            (root.children.clone(), root.base.scale)
        };
        let child_translation = negate_point(origin).map(|coordinate| coordinate / root_scale);
        for child in children {
            if let Some(collision) = self.stream.get_mut(child) {
                collision.translation = child_translation.into();
            }
        }
    }

    fn mesh_local_rotation(&self) -> Rotation3<f32> {
        // The ESP reference attitude is Rx(-x) * Ry(-y) * Rz(-z) in OpenMW.
        // Bake its inverse into local mesh coordinates so placement restores
        // the brush's authored orientation without treating Euler angles as an
        // axis-angle vector.
        Rotation3::from_axis_angle(&Vector3::z_axis(), self.mangle[2])
            * Rotation3::from_axis_angle(&Vector3::y_axis(), self.mangle[1])
            * Rotation3::from_axis_angle(&Vector3::x_axis(), self.mangle[0])
    }

    pub fn save(&mut self, name: &String) {
        self.align_to_center();
        self.prepare_autoplay_root();
        let _ = self.stream.save_path(name);
    }

    fn prepare_autoplay_root(&mut self) {
        if self
            .stream
            .objects_of_type::<NiUVController>()
            .next()
            .is_none()
        {
            return;
        }
        let Some(root) = self.stream.roots.first().copied() else {
            return;
        };
        if self
            .stream
            .get_as::<_, NiBSAnimationNode>(NiLink::<()>::new(root.key))
            .is_some()
        {
            return;
        }

        // Static assets keep a plain NiNode root. Only animated assets pay for
        // the OpenMW-specific AutoPlay wrapper required by NiUVController.
        let mut animation_root = NiBSAnimationNode::default();
        animation_root.base.flags |= 0x0020;
        animation_root.children.push(root.cast());
        let animation_root = self.stream.insert(animation_root);
        self.stream.roots = vec![animation_root.cast()];
    }

    /// Calculate the sum of all dimensions using fold.
    /// This should return the absolute center of the given point cloud
    pub fn centroid(vertices: &[SV3]) -> SV3 {
        vertices
            .iter()
            .fold(SV3::default(), |acc, v| acc + *v)
            .scale(1.0 / vertex_count_as_f32(vertices.len()))
    }

    pub fn attach_node(
        &mut self,
        node: BrushNiNode,
        vfs: &VFS,
        lightmap_texture_name: Option<&str>,
    ) {
        // HACK: This only gets used if the vis data and collision data are equal, so is always initialized when used
        let mut vis_data_index = NiLink::default();

        if !node.vis_verts.is_empty() {
            self.node_distances.push(node.distance_from_origin);

            let vis_index = self.stream.insert(node.vis_shape);
            if let Some(name) = &node.mat_props.link_name {
                self.stream
                    .get_mut(vis_index)
                    .expect("new shape should remain valid")
                    .name
                    .clone_from(name);
            }

            let bake_lightmap = lightmap_texture_name.filter(|_| {
                if node.mat_props.texturing.dark_map.is_some() {
                    eprintln!(
                        "Authored Nif_Texture_DarkMap takes precedence over the generated lightmap for {}",
                        node.texture
                    );
                    false
                } else {
                    true
                }
            });
            self.assign_base_texture(
                vis_index,
                &node.texture,
                vfs,
                bake_lightmap,
                &node.mat_props,
            );

            self.assign_material(&node.mat_props, vis_index);
            if bake_lightmap.is_some() {
                self.assign_lightmap_material(vis_index);
            }
            self.assign_uv_animation(&node.mat_props, vis_index);

            vis_data_index = self.stream.insert(node.vis_data);

            if let Some(shape) = self.stream.get_mut(vis_index) {
                shape.geometry_data = vis_data_index.cast();
            }

            if let Some(root) = self.stream.get_mut(self.base_index) {
                root.children.push(vis_index.cast());
            }
        }

        if !node.col_verts.is_empty() {
            let col_index = self.stream.insert(node.col_shape);

            //HACK: We should probably implement equality traits instead of checking the length of the vertices, but it works
            let col_data_index = if node.col_verts.len() == node.vis_verts.len() {
                vis_data_index
            } else {
                self.stream.insert(node.col_data)
            };

            if let Some(collision) = self.stream.get_mut(col_index) {
                collision.geometry_data = col_data_index.cast();
            }

            if let Some(collision_root) = self.stream.get_mut(self.collision_index) {
                collision_root.children.push(col_index.cast());
            }
        }
    }

    fn assign_base_texture(
        &mut self,
        object: NiLink<NiTriShape>,
        file_path: &str,
        vfs: &VFS,
        lightmap_texture_name: Option<&str>,
        props: &BrushNiMatProps,
    ) {
        let mut extension = String::default();

        for extension_candidate in ["png", "dds", "tga"] {
            let candidate_path = format!("Textures/{file_path}.{extension_candidate}");
            if vfs.get_file(candidate_path.as_str()).is_some() {
                extension = extension_candidate.to_string();
                break;
            }
        }

        let base_texture = self.insert_texture_source(&format!("{file_path}.{extension}"));
        let mut texturing = NiTexturingProperty {
            apply_mode: props.texturing.apply_mode.unwrap_or_default(),
            texture_maps: vec![None; 7],
            ..Default::default()
        };
        texturing.texture_maps[0] = Some(TextureMap::Map(Map {
            texture: base_texture.cast(),
            clamp_mode: props.texturing.clamp_mode.unwrap_or_default(),
            filter_mode: props.texturing.filter_mode.unwrap_or_default(),
            ..Default::default()
        }));

        for (slot, source) in [
            (1, props.texturing.dark_map.as_deref()),
            (2, props.texturing.detail_map.as_deref()),
            (3, props.texturing.gloss_map.as_deref()),
            (4, props.texturing.glow_map.as_deref()),
            (5, props.texturing.bump_map.as_deref()),
        ] {
            let Some(source) = source else {
                continue;
            };
            let texture = self.insert_texture_source(source);
            let map = Map {
                texture: texture.cast(),
                ..Default::default()
            };
            texturing.texture_maps[slot] = Some(if slot == 5 {
                TextureMap::BumpMap(BumpMap {
                    base: map,
                    ..Default::default()
                })
            } else {
                TextureMap::Map(map)
            });
        }

        if let Some(lightmap_texture_name) = lightmap_texture_name {
            let lightmap_texture = self.insert_texture_source(lightmap_texture_name);
            // Morrowind's runtime lightmap convention uses the second texture map.
            texturing.texture_maps[1] = Some(TextureMap::Map(Map {
                texture: lightmap_texture.cast(),
                texture_index: 1,
                ..Default::default()
            }));
        }

        let texturing_link = self.stream.insert(texturing);
        self.stream
            .get_mut(object)
            .expect("texture target shape should remain valid")
            .properties
            .push(texturing_link.cast());
    }

    fn insert_texture_source(&mut self, source: &str) -> NiLink<nif::NiSourceTexture> {
        self.stream.insert(nif::NiSourceTexture {
            source: TextureSource::External(source.to_owned()),
            ..Default::default()
        })
    }

    fn assign_lightmap_material(&mut self, object: NiLink<NiTriShape>) {
        let material = NiMaterialProperty {
            diffuse_color: nif::glam::Vec3::ONE,
            emissive_color: nif::glam::Vec3::ONE,
            alpha: 1.0,
            ..Default::default()
        };
        let material_link = self.stream.insert(material);
        let vertex_colors = self.stream.insert(NiVertexColorProperty {
            source_vertex_mode: SourceVertexMode::AmbientDiffuse,
            lighting_mode: LightingMode::Emissive,
            ..Default::default()
        });
        let object = self
            .stream
            .get_mut(object)
            .expect("lightmapped object should remain valid");
        object.properties.push(material_link.cast());
        object.properties.push(vertex_colors.cast());
    }

    #[allow(
        clippy::field_reassign_with_default,
        reason = "The tes3 NIF property flags are nested in generated base records."
    )]
    pub fn assign_material(&mut self, props: &BrushNiMatProps, object: NiLink<NiTriShape>) {
        if props.color == BrushNiColorProps::default()
            && props.alpha == BrushNiAlphaProps::default()
            && props.glossiness.is_none()
        {
            return;
        }

        let mut mat = NiMaterialProperty::default();
        mat.base.flags = 1;
        mat.alpha = props.alpha.opacity.unwrap_or(1.0);

        if let Some(color) = props.color.emissive {
            mat.emissive_color = color.into();
        }
        if let Some(color) = props.color.ambient {
            mat.ambient_color = color.into();
        }
        if let Some(color) = props.color.diffuse {
            mat.diffuse_color = color.into();
        }
        if let Some(color) = props.color.specular {
            mat.specular_color = color.into();
        }
        if let Some(glossiness) = props.glossiness {
            mat.shine = glossiness;
        }
        if props.alpha.to_flags() != 0 {
            let mut alpha_prop = NiAlphaProperty::default();
            alpha_prop.base.flags = props.alpha.to_flags();

            if props.alpha.use_test.is_some() {
                alpha_prop.test_ref = props.alpha.test_threshold.unwrap_or(128);
            }

            let alpha_link = self.stream.insert(alpha_prop);

            self.stream
                .get_mut(object)
                .expect("Self retreival should never fail")
                .properties
                .push(alpha_link.cast());
        }

        let mat_link = self.stream.insert(mat);
        self.stream
            .get_mut(object)
            .expect("Self retreival should never fail")
            .properties
            .push(mat_link.cast());
    }

    fn assign_uv_animation(&mut self, props: &BrushNiMatProps, object: NiLink<NiTriShape>) {
        match props.uv.mode {
            Some(0) | None => return,
            Some(1) => {}
            Some(mode) => {
                eprintln!("Unsupported Nif_UV_Mode {mode}; only Scroll is materialized");
                return;
            }
        }
        if props.uv.period.is_some() {
            eprintln!(
                "Nif_UV_Period applies to oscillation; Scroll uses a shared seamless period derived from U/V"
            );
        }

        let u_rate = props.uv.u_rate.unwrap_or(0.0);
        let v_rate = props.uv.v_rate.unwrap_or(0.0);
        if !u_rate.is_finite() || !v_rate.is_finite() || (u_rate == 0.0 && v_rate == 0.0) {
            if !u_rate.is_finite() || !v_rate.is_finite() {
                eprintln!("Invalid Nif_UV_U or Nif_UV_V rate; skipping UV scrolling");
            }
            return;
        }
        let Some(period) = common_scroll_period(u_rate, v_rate) else {
            eprintln!(
                "Nif_UV_U and Nif_UV_V have no practical seamless common cycle; skipping UV scrolling"
            );
            return;
        };

        let data_link = self.stream.insert(NiUVData {
            u_offset_data: scrolling_float_data(props.uv.u_rate, period),
            v_offset_data: scrolling_float_data(props.uv.v_rate, period),
            ..Default::default()
        });
        let previous_controller = self
            .stream
            .get(object)
            .expect("UV controller target shape should remain valid")
            .controller;
        let controller_link = self.stream.insert(NiUVController {
            base: NiTimeController {
                flags: 0x0008,
                frequency: 1.0,
                phase: 0.0,
                start_time: 0.0,
                stop_time: period,
                next: previous_controller,
                target: object.cast(),
                ..Default::default()
            },
            texture_set: 0,
            data: data_link,
        });
        self.stream
            .get_mut(object)
            .expect("UV controller target shape should remain valid")
            .controller = controller_link.cast();
    }
}

fn subtract_points(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

fn resolve_nif_target(
    target: Option<&str>,
    named_nodes: &HashMap<String, (RenderParent, [f32; 3])>,
    fallback: (RenderParent, [f32; 3]),
) -> (RenderParent, [f32; 3]) {
    if let Some(parent) = target.and_then(|name| named_nodes.get(name).copied()) {
        return parent;
    }
    if let Some(target) = target {
        eprintln!(
            "Nif_Target {target:?} did not resolve to an earlier Nif_LinkName in this asset; using the containing TrenchBroom group"
        );
    }
    fallback
}

fn negate_point(point: [f32; 3]) -> [f32; 3] {
    [-point[0], -point[1], -point[2]]
}

/// A Morrowind NIF (4.0.0.2) keeps a billboard's mode in bits 5 and 6 of the
/// node's flags, so it can hold modes 0 to 3 only.
fn billboard_mode_flags(mode: u32) -> Option<u16> {
    let mode = u16::try_from(mode).ok().filter(|mode| *mode <= 3)?;
    Some(mode << 5)
}

fn sorting_mode(value: u32) -> Option<SortingMode> {
    match value {
        0 => Some(SortingMode::Inherit),
        1 => Some(SortingMode::Off),
        2 => Some(SortingMode::Subsort),
        _ => None,
    }
}

fn scrolling_float_data(rate: Option<f32>, period: f32) -> NiFloatData {
    // Cycle by one whole UV tile so wrapped textures do not jump back by an
    // arbitrary fractional offset at the end of each controller cycle.
    let keys = rate.map_or_else(Vec::new, |rate| {
        vec![
            NiLinFloatKey {
                time: 0.0,
                value: 0.0,
            },
            NiLinFloatKey {
                time: period,
                value: rate * period,
            },
        ]
    });
    NiFloatData {
        keys: NiFloatKey::LinKey(keys),
        ..Default::default()
    }
}

fn common_scroll_period(u_rate: f32, v_rate: f32) -> Option<f32> {
    let fastest_rate = u_rate.abs().max(v_rate.abs());
    let base_period = 1.0 / fastest_rate;
    if !base_period.is_finite() || base_period <= 0.0 {
        return None;
    }

    for multiple in 1..=4096_u16 {
        let period = base_period * f32::from(multiple);
        if !period.is_finite() {
            return None;
        }
        let u_tiles = u_rate * period;
        let v_tiles = v_rate * period;
        if (u_tiles - u_tiles.round()).abs() <= 0.001 && (v_tiles - v_tiles.round()).abs() <= 0.001
        {
            return Some(period);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use tes3::nif::{
        ApplyMode, ClampMode, FilterMode, NiAlphaAccumulator, NiAlphaProperty, NiBSAnimationNode,
        NiBillboardNode, NiLink, NiMaterialProperty, NiNode, NiSortAdjustNode, NiStream,
        NiTexturingProperty, NiTriShape, NiTriShapeData, NiUVController, NiUVData,
        RootCollisionNode, TextureMap, TextureSource, glam,
    };
    use vfstool_lib::VFS;

    use crate::brush_ni_node::{
        BrushAlphaTestFunction, BrushDestinationBlendMode, BrushNiAlphaProps, BrushNiColorProps,
        BrushNiMatProps, BrushNiTextureProps, BrushNiUvProps, BrushNoSort, BrushSourceBlendMode,
        BrushUseAlpha,
    };

    use super::{Mesh, NifStructuralKind, NifStructuralMarker, SV3, SortingMode};

    fn round_trip_render_props() -> (NiStream, BrushNiMatProps) {
        let mut mesh = Mesh::new(1.0);
        let shape_link = mesh.stream.insert(NiTriShape::default());
        mesh.stream
            .get_mut(mesh.base_index)
            .unwrap()
            .children
            .push(shape_link.cast());
        let mut geometry_data = NiTriShapeData::default();
        geometry_data.vertices.extend([
            glam::vec3(0.0, 0.0, 0.0),
            glam::vec3(1.0, 0.0, 0.0),
            glam::vec3(0.0, 1.0, 0.0),
        ]);
        geometry_data.triangles.push([0, 1, 2]);
        let geometry_link = mesh.stream.insert(geometry_data);
        mesh.stream.get_mut(shape_link).unwrap().geometry_data = geometry_link.cast();

        let props = BrushNiMatProps {
            color: BrushNiColorProps {
                emissive: Some([0.1, 0.2, 0.3]),
                ambient: Some([0.2, 0.3, 0.4]),
                diffuse: Some([0.3, 0.4, 0.5]),
                specular: Some([0.4, 0.5, 0.6]),
            },
            alpha: BrushNiAlphaProps {
                opacity: Some(0.75),
                use_blend: Some(BrushUseAlpha::BlendEnable),
                blend_source_mode: Some(BrushSourceBlendMode::SourceAlpha),
                blend_destination_mode: Some(BrushDestinationBlendMode::OneMinusSourceAlpha),
                use_test: Some(BrushUseAlpha::OFF),
                test_function: Some(BrushAlphaTestFunction::GreaterThan),
                test_threshold: Some(128),
                no_sort: Some(BrushNoSort::ON),
            },
            glossiness: Some(24.0),
            texturing: BrushNiTextureProps {
                apply_mode: Some(ApplyMode::Decal),
                clamp_mode: Some(ClampMode::ClampSWrapT),
                filter_mode: Some(FilterMode::NearestMipLerp),
                dark_map: Some("textures/dark.dds".into()),
                detail_map: Some("textures/detail.dds".into()),
                gloss_map: Some("textures/gloss.dds".into()),
                glow_map: Some("textures/glow.dds".into()),
                bump_map: Some("textures/bump.dds".into()),
            },
            uv: BrushNiUvProps {
                mode: Some(1),
                u_rate: Some(0.25),
                v_rate: Some(-0.5),
                period: None,
            },
            link_name: None,
            target: None,
        };

        mesh.assign_base_texture(shape_link, "base/stone", &VFS::new(), None, &props);
        mesh.assign_material(&props, shape_link);
        mesh.assign_uv_animation(&props, shape_link);
        mesh.prepare_autoplay_root();

        let bytes = mesh
            .stream
            .save_bytes()
            .expect("NIF stream should serialize");
        let stream = NiStream::from_bytes(&bytes).expect("NIF stream should deserialize");
        (stream, props)
    }

    fn authored_shape(stream: &NiStream) -> &NiTriShape {
        stream
            .objects_of_type::<NiTriShape>()
            .next()
            .expect("authored shape should survive round-trip")
    }

    fn assert_texturing_round_trip(stream: &NiStream, shape: &NiTriShape) {
        let texturing = shape
            .properties
            .iter()
            .find_map(|link| stream.get_as::<_, NiTexturingProperty>(NiLink::<()>::new(link.key)))
            .expect("texturing property should be assigned to shape");
        assert_eq!(texturing.apply_mode, ApplyMode::Decal);
        let base_map = match texturing.texture_maps[0].as_ref().unwrap() {
            TextureMap::Map(map) => map,
            TextureMap::BumpMap(_) => panic!("base texture map should not be a bump map"),
        };
        assert_eq!(base_map.clamp_mode, ClampMode::ClampSWrapT);
        assert_eq!(base_map.filter_mode, FilterMode::NearestMipLerp);
        let map_source = |slot: usize| {
            let texture_link = match texturing.texture_maps[slot].as_ref().unwrap() {
                TextureMap::Map(map) => map.texture,
                TextureMap::BumpMap(map) => map.base.texture,
            };
            match &stream.get(texture_link).unwrap().source {
                TextureSource::External(path) => path.as_str(),
                TextureSource::Internal(_) => panic!("authored textures are external"),
            }
        };
        assert_eq!(map_source(1), "textures/dark.dds");
        assert_eq!(map_source(2), "textures/detail.dds");
        assert_eq!(map_source(3), "textures/gloss.dds");
        assert_eq!(map_source(4), "textures/glow.dds");
        assert_eq!(map_source(5), "textures/bump.dds");
    }

    fn assert_material_round_trip(stream: &NiStream, shape: &NiTriShape, props: &BrushNiMatProps) {
        let material = shape
            .properties
            .iter()
            .find_map(|link| stream.get_as::<_, NiMaterialProperty>(NiLink::<()>::new(link.key)))
            .expect("material property should be assigned to shape");
        assert_eq!(material.emissive_color, glam::vec3(0.1, 0.2, 0.3));
        assert_eq!(material.ambient_color, glam::vec3(0.2, 0.3, 0.4));
        assert_eq!(material.diffuse_color, glam::vec3(0.3, 0.4, 0.5));
        assert_eq!(material.specular_color, glam::vec3(0.4, 0.5, 0.6));
        assert_float_close(material.shine, 24.0);
        assert_float_close(material.alpha, 0.75);

        let alpha = shape
            .properties
            .iter()
            .find_map(|link| stream.get_as::<_, NiAlphaProperty>(NiLink::<()>::new(link.key)))
            .expect("alpha property should be assigned to shape");
        assert_eq!(alpha.base.flags, props.alpha.to_flags());
        assert_eq!(alpha.test_ref, 128);
    }

    fn assert_uv_scroll_round_trip(stream: &NiStream, shape: &NiTriShape) {
        let controller = stream
            .get_as::<_, NiUVController>(shape.controller)
            .expect("UV scroll controller should target the shape");
        assert_eq!(controller.texture_set, 0);
        let data = stream
            .get_as::<_, NiUVData>(controller.data)
            .expect("UV controller should own NiUVData");
        let tes3::nif::NiFloatKey::LinKey(u_keys) = &data.u_offset_data.keys else {
            panic!("scroll U offset should use linear keys");
        };
        let tes3::nif::NiFloatKey::LinKey(v_keys) = &data.v_offset_data.keys else {
            panic!("scroll V offset should use linear keys");
        };
        let u_start = u_keys.first().unwrap();
        let u_end = u_keys.last().unwrap();
        let v_start = v_keys.first().unwrap();
        let v_end = v_keys.last().unwrap();
        assert_float_close(
            (u_end.value - u_start.value) / (u_end.time - u_start.time),
            0.25,
        );
        assert_float_close(
            (v_end.value - v_start.value) / (v_end.time - v_start.time),
            -0.5,
        );
        assert_float_close(u_end.time, 4.0);
        assert_float_close(v_end.time, 4.0);
        assert_float_close(controller.base.stop_time, 4.0);
        assert_eq!(controller.base.flags & 0x0008, 0x0008);
    }

    fn assert_float_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }

    #[test]
    fn authored_render_state_and_scroll_round_trip_through_nif() {
        let (stream, props) = round_trip_render_props();
        let root = stream
            .objects_of_type::<NiBSAnimationNode>()
            .next()
            .expect("NiUVController needs an auto-play animation root in OpenMW");
        assert_eq!(root.base.flags & 0x0020, 0x0020);
        let shape = authored_shape(&stream);
        assert_texturing_round_trip(&stream, shape);
        assert_material_round_trip(&stream, shape, &props);
        assert_uv_scroll_round_trip(&stream, shape);
    }

    #[test]
    fn static_mesh_keeps_a_plain_node_root_without_autoplay() {
        let mesh = Mesh::new(1.0);
        let root = mesh.stream.roots[0];
        assert!(
            mesh.stream
                .get_as::<_, NiNode>(NiLink::<()>::new(root.key))
                .is_some()
        );
        assert!(
            mesh.stream
                .get_as::<_, NiBSAnimationNode>(NiLink::<()>::new(root.key))
                .is_none()
        );
    }

    #[test]
    fn only_subsort_nodes_receive_an_alpha_accumulator() {
        let mut mesh = Mesh::new(1.0);
        mesh.attach_nif_semantics(
            &[NifStructuralMarker {
                kind: NifStructuralKind::SortAdjust { mode: 0 },
                link_name: Some("inherit".into()),
                target: None,
                origin: SV3::default(),
            }],
            None,
        );
        let node = mesh
            .stream
            .objects_of_type::<NiSortAdjustNode>()
            .next()
            .expect("sort-adjust marker should materialize");
        assert_eq!(node.sorting_mode, SortingMode::Inherit);
        assert!(
            mesh.stream
                .get_as::<_, NiAlphaAccumulator>(node.sub_sorter)
                .is_none()
        );
        assert_eq!(
            mesh.stream.objects_of_type::<NiAlphaAccumulator>().count(),
            0
        );
    }

    fn billboard_flags_after_round_trip(mode: u32) -> u16 {
        let mut mesh = Mesh::new(1.0);
        mesh.node_distances.push(SV3::default());
        mesh.attach_nif_semantics(
            &[NifStructuralMarker {
                kind: NifStructuralKind::Billboard { mode },
                link_name: None,
                target: None,
                origin: SV3::default(),
            }],
            None,
        );
        let bytes = mesh
            .stream
            .save_bytes()
            .expect("billboard should serialize");
        let stream = NiStream::from_bytes(&bytes).expect("billboard should deserialize");
        stream
            .objects_of_type::<NiBillboardNode>()
            .next()
            .expect("billboard marker should materialize")
            .flags
    }

    #[test]
    fn billboard_mode_is_stored_in_the_node_flags() {
        for (mode, flags) in [(0, 0x0000), (1, 0x0020), (2, 0x0040), (3, 0x0060)] {
            assert_eq!(
                billboard_flags_after_round_trip(mode) & 0x0060,
                flags,
                "Nif_Billboard_Mode {mode}"
            );
        }
    }

    #[test]
    fn billboard_modes_a_morrowind_nif_cannot_hold_face_the_camera() {
        for mode in [4, 5, 9] {
            assert_eq!(billboard_flags_after_round_trip(mode) & 0x0060, 0);
        }
    }

    #[test]
    fn semantic_markers_build_nif_nodes_and_resolve_target_links() {
        let mut mesh = Mesh::new(1.0);
        mesh.node_distances.push(SV3::new(5.0, 0.0, 0.0));
        let shape = mesh.stream.insert(NiTriShape::default());
        mesh.stream
            .get_mut(mesh.base_index)
            .unwrap()
            .children
            .push(shape.cast());

        mesh.attach_nif_semantics(
            &[
                NifStructuralMarker {
                    kind: NifStructuralKind::Billboard { mode: 0 },
                    link_name: Some("billboard".into()),
                    target: None,
                    origin: SV3::new(10.0, 0.0, 0.0),
                },
                NifStructuralMarker {
                    kind: NifStructuralKind::SortAdjust { mode: 2 },
                    link_name: Some("sort".into()),
                    target: Some("billboard".into()),
                    origin: SV3::new(12.0, 0.0, 0.0),
                },
            ],
            Some("sort"),
        );

        let bytes = mesh
            .stream
            .save_bytes()
            .expect("marker graph should serialize");
        let stream = NiStream::from_bytes(&bytes).expect("marker graph should deserialize");
        let billboard = stream
            .objects_of_type::<NiBillboardNode>()
            .next()
            .expect("billboard marker should materialize");
        let sort_adjust = stream
            .objects_of_type::<NiSortAdjustNode>()
            .next()
            .expect("sort marker should materialize");
        assert_eq!(billboard.name, "billboard");
        assert_float_close(billboard.translation.x, 5.0);
        assert_eq!(sort_adjust.name, "sort");
        assert_float_close(sort_adjust.translation.x, 2.0);
        assert_eq!(sort_adjust.sorting_mode, SortingMode::Subsort);
        assert!(
            stream
                .get_as::<_, NiAlphaAccumulator>(sort_adjust.sub_sorter)
                .is_some()
        );
        assert_eq!(stream.objects_of_type::<NiAlphaAccumulator>().count(), 1);
        let shape = stream
            .objects_of_type::<NiTriShape>()
            .next()
            .expect("shape should remain under the semantic node chain");
        assert_float_close(shape.translation.x, -7.0);
        assert_eq!(sort_adjust.children.len(), 1);
    }

    #[test]
    fn collision_root_marker_renames_and_positions_the_collision_branch() {
        let mut mesh = Mesh::new(2.0);
        mesh.node_distances.push(SV3::new(3.0, 0.0, 0.0));
        let collision_shape = mesh.stream.insert(NiTriShape::default());
        mesh.stream
            .get_mut(mesh.collision_index)
            .unwrap()
            .children
            .push(collision_shape.cast());

        mesh.attach_nif_semantics(
            &[NifStructuralMarker {
                kind: NifStructuralKind::CollisionRoot,
                link_name: Some("collision-root".into()),
                target: None,
                origin: SV3::new(8.0, 0.0, 0.0),
            }],
            None,
        );

        let bytes = mesh
            .stream
            .save_bytes()
            .expect("collision marker graph should serialize");
        let stream =
            NiStream::from_bytes(&bytes).expect("collision marker graph should deserialize");
        let root = stream
            .objects_of_type::<RootCollisionNode>()
            .next()
            .expect("collision root should remain in the serialized graph");
        assert_eq!(root.name, "collision-root");
        assert_float_close(root.translation.x, 5.0);
        assert_float_close(root.scale, 2.0);
        assert_float_close(stream.get(collision_shape).unwrap().translation.x, -2.5);
        assert_eq!(stream.objects_of_type::<RootCollisionNode>().count(), 1);
    }

    #[test]
    fn nif_round_trip_preserves_a_second_uv_channel() {
        let channel_0 = [
            glam::vec2(0.0, 0.0),
            glam::vec2(1.0, 0.0),
            glam::vec2(0.0, 1.0),
        ];
        let channel_1 = [
            glam::vec2(0.25, 0.25),
            glam::vec2(0.75, 0.25),
            glam::vec2(0.25, 0.75),
        ];

        let mut data = NiTriShapeData::default();
        data.vertices.extend([
            glam::vec3(0.0, 0.0, 0.0),
            glam::vec3(1.0, 0.0, 0.0),
            glam::vec3(0.0, 1.0, 0.0),
        ]);
        data.uv_sets.extend(channel_0.into_iter().chain(channel_1));
        data.triangles.push([0, 1, 2]);

        let mut stream = NiStream::default();
        let data_link = stream.insert(data);
        stream.roots.push(data_link.cast());
        let bytes = stream.save_bytes().expect("NIF data should serialize");
        let loaded = NiStream::from_bytes(&bytes).expect("NIF data should deserialize");
        let loaded_data = loaded
            .objects_of_type::<NiTriShapeData>()
            .next()
            .expect("round-tripped geometry data should be reachable");

        assert_eq!(loaded_data.num_uv_sets(), 2);
        assert_eq!(loaded_data.uv_set(0).unwrap(), channel_0);
        assert_eq!(loaded_data.uv_set(1).unwrap(), channel_1);
    }
}
