#![allow(
    clippy::wildcard_imports,
    reason = "The semantic importer shares the parent NIF model and error contract."
)]

use super::*;

pub(super) fn material_name(source_texture: Option<&str>) -> String {
    let Some(texture) = source_texture else {
        return "__TB_empty".into();
    };
    let mut material = texture.replace('\\', "/");
    if let Some(dot) = material.rfind('.') {
        material.truncate(dot);
    }
    if material.to_ascii_lowercase().starts_with("textures/") {
        material.drain(..9);
    }
    if material.is_empty() {
        "__TB_empty".into()
    } else {
        material
    }
}

pub(super) fn reject_legacy_bounds(stream: &NiStream, path: &Path) -> Result<(), Error> {
    for (_, node) in stream.objects_of_type_with_link::<NiNode>() {
        if node.bounding_volume.is_some() {
            return Err(Error::Nif(format!(
                "{}: NiNode uses an unsupported legacy bounding volume",
                path.display()
            )));
        }
    }
    for (_, root) in stream.objects_of_type_with_link::<RootCollisionNode>() {
        if root.bounding_volume.is_some() {
            return Err(Error::Nif(format!(
                "{}: RootCollisionNode uses an unsupported legacy bounding volume",
                path.display()
            )));
        }
    }
    for (_, shape) in stream.objects_of_type_with_link::<NiTriShape>() {
        if shape.bounding_volume.is_some() {
            return Err(Error::Nif(format!(
                "{}: NiTriShape uses an unsupported legacy bounding volume",
                path.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn texture_binding(
    stream: &NiStream,
    properties: &[NiKey],
) -> Result<Option<TextureBinding>, String> {
    for property in properties {
        let Some(texturing) = stream.get_as::<_, NiTexturingProperty>(NiLink::<()>::new(*property))
        else {
            continue;
        };
        let Some(Some(TextureMap::Map(map))) = texturing.texture_maps.first() else {
            continue;
        };
        let source = stream
            .get(map.texture)
            .ok_or_else(|| "base texture link is missing from the NIF".to_owned())?;
        return match &source.source {
            TextureSource::External(source) => Ok(Some(TextureBinding {
                source: source.clone(),
                uv_set: map.texture_index,
            })),
            TextureSource::Internal(_) => Err(
                "base texture is embedded in the NIF; provide a materialized texture root".into(),
            ),
        };
    }
    Ok(None)
}

pub(super) fn texture_sources(
    stream: &NiStream,
    properties: &[NiKey],
) -> Result<Vec<String>, String> {
    let mut sources = Vec::new();
    for property in properties {
        let Some(texturing) = stream.get_as::<_, NiTexturingProperty>(NiLink::<()>::new(*property))
        else {
            continue;
        };
        for map in texturing.texture_maps.iter().flatten() {
            let texture = match map {
                TextureMap::Map(map) => map.texture,
                TextureMap::BumpMap(map) => map.texture,
            };
            let source = stream
                .get(texture)
                .ok_or_else(|| "texture link is missing from the NIF".to_owned())?;
            match &source.source {
                TextureSource::External(source) => sources.push(source.clone()),
                TextureSource::Internal(_) => {
                    return Err(
                        "a texture is embedded in the NIF; provide a materialized texture root"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(sources)
}

pub(super) fn push_nif_property(state: &mut NifState, key: &str, value: impl Into<String>) {
    state.properties.retain(|property| property.key != key);
    state.properties.push(NifProperty {
        key: key.into(),
        value: value.into(),
    });
}

pub(super) fn format_color(color: tes3::nif::glam::Vec3) -> String {
    format!(
        "{} {} {}",
        canonical_float(color.x),
        canonical_float(color.y),
        canonical_float(color.z)
    )
}

pub(super) fn texture_source(stream: &NiStream, map: &TextureMap) -> Option<String> {
    let texture = match map {
        TextureMap::Map(map) => map.texture,
        TextureMap::BumpMap(map) => map.texture,
    };
    match &stream.get(texture)?.source {
        TextureSource::External(path) => Some(path.clone()),
        TextureSource::Internal(_) => None,
    }
}

pub(super) fn float_differs(lhs: f32, rhs: f32) -> bool {
    (lhs - rhs).abs() > STATE_FLOAT_EPSILON
}

pub(super) fn material_nif_state(state: &mut NifState, material: &NiMaterialProperty) {
    // These are the defaults used by BrushNiNode when an authoring property is
    // materialized. NiMaterialProperty::default() is a serialization default,
    // not the public NIF authoring contract (in particular, its alpha is 0).
    let default_color = tes3::nif::glam::Vec3::ZERO;
    if material.emissive_color != default_color {
        push_nif_property(
            state,
            "Material_Emissive_color",
            format_color(material.emissive_color),
        );
    }
    if material.ambient_color != default_color {
        push_nif_property(
            state,
            "Material_Ambient_color",
            format_color(material.ambient_color),
        );
    }
    if material.diffuse_color != default_color {
        push_nif_property(
            state,
            "Material_Diffuse_color",
            format_color(material.diffuse_color),
        );
    }
    if material.specular_color != default_color {
        push_nif_property(
            state,
            "Material_Specular_color",
            format_color(material.specular_color),
        );
    }
    if float_differs(material.shine, 0.0) {
        push_nif_property(
            state,
            "Material_Glossiness",
            canonical_float(material.shine),
        );
    }
    if float_differs(material.alpha, CANONICAL_ALPHA) {
        push_nif_property(state, "Material_Alpha", canonical_float(material.alpha));
    }
}

pub(super) fn alpha_nif_state(state: &mut NifState, alpha: &NiAlphaProperty) {
    let flags = alpha.flags;
    if flags == 0 {
        return;
    }
    for (key, mask) in [
        ("Material_Alpha_UseBlend", 0x0001),
        ("Material_Alpha_BlendSourceMode", 0x001e),
        ("Material_Alpha_BlendDestinationMode", 0x01e0),
        ("Material_Alpha_TestEnable", 0x0200),
        ("Material_Alpha_TestFunction", 0x1c00),
        ("Material_Alpha_NoSort", 0x2000),
    ] {
        let value = flags & mask;
        if value != 0 {
            push_nif_property(state, key, value.to_string());
        }
    }
    if flags & 0x0200 != 0 && alpha.test_ref != CANONICAL_ALPHA_TEST_THRESHOLD {
        push_nif_property(
            state,
            "Material_Alpha_TestThreshold",
            alpha.test_ref.to_string(),
        );
    }
}

pub(super) fn texturing_nif_state(
    state: &mut NifState,
    stream: &NiStream,
    texturing: &NiTexturingProperty,
) {
    if texturing.apply_mode as i32 != 2 {
        push_nif_property(
            state,
            "Nif_Texture_ApplyMode",
            (texturing.apply_mode as i32).to_string(),
        );
    }
    for (index, map) in texturing.texture_maps.iter().enumerate() {
        let Some(map) = map else {
            continue;
        };
        let Some(source) = texture_source(stream, map) else {
            continue;
        };
        let key = match index {
            1 => "Nif_Texture_DarkMap",
            2 => "Nif_Texture_DetailMap",
            3 => "Nif_Texture_GlossMap",
            4 => "Nif_Texture_GlowMap",
            5 => "Nif_Texture_BumpMap",
            _ => continue,
        };
        push_nif_property(state, key, source);
    }
    if let Some(Some(TextureMap::Map(map))) = texturing.texture_maps.first() {
        if map.clamp_mode as i32 != 3 {
            push_nif_property(
                state,
                "Nif_Texture_ClampMode",
                (map.clamp_mode as i32).to_string(),
            );
        }
        if map.filter_mode as i32 != 2 {
            push_nif_property(
                state,
                "Nif_Texture_FilterMode",
                (map.filter_mode as i32).to_string(),
            );
        }
    }
}

pub(super) fn shape_nif_state(stream: &NiStream, properties: &[NiKey]) -> NifState {
    let mut state = NifState::default();
    for property in properties {
        if let Some(material) = stream.get_as::<_, NiMaterialProperty>(NiLink::<()>::new(*property))
        {
            material_nif_state(&mut state, material);
        }
        if let Some(alpha) = stream.get_as::<_, NiAlphaProperty>(NiLink::<()>::new(*property)) {
            alpha_nif_state(&mut state, alpha);
        }
        if let Some(texturing) =
            stream.get_as::<_, NiTexturingProperty>(NiLink::<()>::new(*property))
        {
            texturing_nif_state(&mut state, stream, texturing);
        }
    }
    state.properties.sort();
    state
}

pub(super) fn add_scope(
    scopes: &mut Vec<SemanticScope>,
    parent: ScopeId,
    name: &str,
    kind: ImportScope,
    node_kind: &str,
) -> ScopeId {
    let id = scopes.len();
    scopes.push(SemanticScope {
        id,
        parent: Some(parent),
        name: name.to_owned(),
        kind,
        node_kind: node_kind.to_owned(),
    });
    id
}

fn node_link_properties(name: &str) -> Vec<NifProperty> {
    if name.trim().is_empty() {
        Vec::new()
    } else {
        vec![NifProperty {
            key: "Nif_LinkName".into(),
            value: name.to_owned(),
        }]
    }
}

fn node_is_named(name: &str) -> bool {
    !name.trim().is_empty()
}

pub(super) fn marker_origin<F>(
    transform_for: &F,
    key: NiKey,
    transform: tes3::nif::glam::Affine3A,
) -> [f64; 3]
where
    F: Fn(NiKey, tes3::nif::glam::Affine3A) -> tes3::nif::glam::Affine3A,
{
    let origin = transform_for(key, transform).transform_point3(tes3::nif::glam::Vec3::ZERO);
    [
        f64::from(origin.x),
        f64::from(origin.y),
        f64::from(origin.z),
    ]
}

pub(super) struct SemanticContext {
    scopes: Vec<SemanticScope>,
    shape_scopes: HashMap<NiKey, ScopeId>,
    shape_properties: HashMap<NiKey, Vec<NiKey>>,
    markers: Vec<ImportedMarker>,
    diagnostics: Vec<String>,
}

struct SemanticWalker<'a, F> {
    stream: &'a NiStream,
    transform_for: &'a F,
    context: SemanticContext,
    active: HashSet<NiKey>,
}

impl<F> SemanticWalker<'_, F>
where
    F: Fn(NiKey, tes3::nif::glam::Affine3A) -> tes3::nif::glam::Affine3A,
{
    #[allow(
        clippy::too_many_lines,
        reason = "This walker is the single traversal boundary where NIF node kinds, inherited state, and editor scopes meet."
    )]
    fn visit(&mut self, key: NiKey, parent_scope: ScopeId, inherited_properties: &[NiKey]) {
        if !self.active.insert(key) {
            return;
        }

        let mut scope = parent_scope;
        let mut properties = inherited_properties.to_vec();
        let mut children = Vec::new();
        let inherited_kind = self.context.scopes[parent_scope].kind;

        if let Some(root) = self
            .stream
            .get_as::<_, RootCollisionNode>(NiLink::<()>::new(key))
        {
            properties.extend(root.properties.iter().map(|link| link.key));
            children.extend(root.children.iter().map(|link| link.key));
            scope = add_scope(
                &mut self.context.scopes,
                parent_scope,
                &root.name,
                ImportScope::Collision,
                "RootCollisionNode",
            );
            self.context.markers.push(ImportedMarker {
                classname: "nif_node_collision_root".into(),
                name: root.name.clone(),
                origin: marker_origin(self.transform_for, key, root.transform()),
                scope,
                properties: node_link_properties(&root.name),
            });
        } else if let Some(node) = self
            .stream
            .get_as::<_, NiBillboardNode>(NiLink::<()>::new(key))
        {
            properties.extend(node.properties.iter().map(|link| link.key));
            children.extend(node.children.iter().map(|link| link.key));
            scope = add_scope(
                &mut self.context.scopes,
                parent_scope,
                &node.name,
                if inherited_kind == ImportScope::Collision {
                    ImportScope::Collision
                } else {
                    ImportScope::Visual
                },
                "NiBillboardNode",
            );
            self.context.markers.push(ImportedMarker {
                classname: "nif_node_billboard".into(),
                name: node.name.clone(),
                origin: marker_origin(self.transform_for, key, node.transform()),
                scope,
                properties: node_link_properties(&node.name),
            });
            self.context.diagnostics.push(format!(
                "node {:?}: billboard mode is not encoded by the supported NIF version",
                node.name
            ));
        } else if let Some(node) = self
            .stream
            .get_as::<_, NiSortAdjustNode>(NiLink::<()>::new(key))
        {
            properties.extend(node.properties.iter().map(|link| link.key));
            children.extend(node.children.iter().map(|link| link.key));
            scope = add_scope(
                &mut self.context.scopes,
                parent_scope,
                &node.name,
                if inherited_kind == ImportScope::Collision {
                    ImportScope::Collision
                } else {
                    ImportScope::Visual
                },
                "NiSortAdjustNode",
            );
            self.context.markers.push(ImportedMarker {
                classname: "nif_node_sort_adjust".into(),
                name: node.name.clone(),
                origin: marker_origin(self.transform_for, key, node.transform()),
                scope,
                properties: {
                    let mut properties = node_link_properties(&node.name);
                    let mode = node.sorting_mode as i32;
                    if mode == 64 {
                        self.context.diagnostics.push(format!(
                            "node {:?}: Grouped sort-adjust mode (64) is unsupported by the current Morrobroom authoring schema and will import as Inherit",
                            node.name
                        ));
                    } else {
                        properties.push(NifProperty {
                            key: "Nif_Sort_Mode".into(),
                            value: mode.to_string(),
                        });
                    }
                    properties
                },
            });
        } else if let Some(node) = self.stream.get_as::<_, NiNode>(NiLink::<()>::new(key)) {
            properties.extend(node.properties.iter().map(|link| link.key));
            children.extend(node.children.iter().map(|link| link.key));
            if node_is_named(&node.name) {
                scope = add_scope(
                    &mut self.context.scopes,
                    parent_scope,
                    &node.name,
                    if inherited_kind == ImportScope::Collision {
                        ImportScope::Collision
                    } else {
                        ImportScope::Visual
                    },
                    "NiNode",
                );
            }
        } else if let Some(shape) = self.stream.get_as::<_, NiTriShape>(NiLink::<()>::new(key)) {
            properties.extend(shape.properties.iter().map(|link| link.key));
            self.context.shape_scopes.insert(key, scope);
            self.context
                .shape_properties
                .insert(key, properties.clone());
        }

        for child in children {
            self.visit(child, scope, &properties);
        }
        self.active.remove(&key);
    }
}

pub(super) fn semantic_context<F>(stream: &NiStream, transform_for: &F) -> SemanticContext
where
    F: Fn(NiKey, tes3::nif::glam::Affine3A) -> tes3::nif::glam::Affine3A,
{
    let context = SemanticContext {
        scopes: vec![SemanticScope {
            id: 0,
            parent: None,
            name: "Asset".into(),
            kind: ImportScope::Visual,
            node_kind: "asset".into(),
        }],
        shape_scopes: HashMap::new(),
        shape_properties: HashMap::new(),
        markers: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut walker = SemanticWalker {
        stream,
        transform_for,
        context,
        active: HashSet::new(),
    };
    for root in &stream.roots {
        walker.visit(root.key, 0, &[]);
    }
    walker.context
}

pub(super) fn import_nodes(stream: &NiStream) -> (Vec<ImportedNode>, Vec<String>) {
    let mut nodes = Vec::new();
    let mut diagnostics = Vec::new();
    for (_, node) in stream.objects_of_type_with_link::<NiNode>() {
        let has_controller = stream.get(node.controller).is_some();
        let has_extra_data = stream.get(node.extra_data).is_some();
        if has_controller {
            diagnostics.push(format!(
                "node {:?}: NiTimeController chain is not represented",
                node.name
            ));
        }
        if has_extra_data {
            diagnostics.push(format!(
                "node {:?}: NiExtraData chain is not represented",
                node.name
            ));
        }
        if !node.effects.is_empty() {
            diagnostics.push(format!(
                "node {:?}: {} effect(s) are not represented",
                node.name,
                node.effects.len()
            ));
        }
        nodes.push(ImportedNode {
            name: node.name.clone(),
            flags: node.flags,
            effects: node.effects.len(),
            has_controller,
            has_extra_data,
        });
    }
    (nodes, diagnostics)
}

pub(super) fn linear_uv_rate(data: &NiFloatData) -> Option<f32> {
    let NiFloatKey::LinKey(keys) = &data.keys else {
        return None;
    };
    if keys.len() != 2
        || keys
            .iter()
            .any(|key| !key.time.is_finite() || !key.value.is_finite())
    {
        return None;
    }
    let first = keys.first()?;
    let last = keys.last()?;
    let duration = last.time - first.time;
    if duration <= STATE_FLOAT_EPSILON || first.time.abs() > STATE_FLOAT_EPSILON {
        return None;
    }
    Some((last.value - first.value) / duration)
}

fn linear_uv_period(data: &NiFloatData) -> Option<f32> {
    let NiFloatKey::LinKey(keys) = &data.keys else {
        return None;
    };
    if keys.len() != 2
        || keys
            .iter()
            .any(|key| !key.time.is_finite() || !key.value.is_finite())
    {
        return None;
    }
    let first = keys.first()?;
    let last = keys.last()?;
    if first.time.abs() > STATE_FLOAT_EPSILON {
        return None;
    }
    let duration = last.time - first.time;
    (duration > STATE_FLOAT_EPSILON).then_some(duration)
}

pub(super) fn uv_data_has_keys(data: &NiFloatData) -> bool {
    match &data.keys {
        NiFloatKey::LinKey(keys) => !keys.is_empty(),
        NiFloatKey::BezKey(keys) => !keys.is_empty(),
        NiFloatKey::TCBKey(keys) => !keys.is_empty(),
    }
}

pub(super) fn approximately(lhs: f32, rhs: f32) -> bool {
    (lhs - rhs).abs() <= STATE_FLOAT_EPSILON
}

pub(super) fn import_uv_state(
    stream: &NiStream,
    shape_key: NiKey,
    base_uv_set: Option<usize>,
    controllers: &HashMap<NiKey, Vec<&NiUVController>>,
    state: &mut NifState,
    diagnostics: &mut Vec<String>,
) {
    for controller in controllers.get(&shape_key).into_iter().flatten() {
        if base_uv_set != Some(usize::from(controller.texture_set)) {
            diagnostics.push(
                "NiUVController targets a non-base texture set; animation was not represented"
                    .into(),
            );
            continue;
        }
        if !controller.active()
            || controller.cycle_type() != tes3::nif::CycleType::Cycle
            || !approximately(controller.frequency, 1.0)
            || !approximately(controller.phase, 0.0)
            || !approximately(controller.start_time, 0.0)
            || (!approximately(controller.stop_time, 0.0)
                && (!controller.stop_time.is_finite()
                    || controller.stop_time <= STATE_FLOAT_EPSILON))
        {
            diagnostics.push(
                "NiUVController timing/cycle settings do not prove indefinite scrolling; animation was not represented"
                    .into(),
            );
            continue;
        }
        let Some(data) = stream.get_as::<_, NiUVData>(controller.data) else {
            diagnostics
                .push("NiUVController has no NiUVData; animation was not represented".into());
            continue;
        };
        let u_rate = linear_uv_rate(&data.u_offset_data);
        let v_rate = linear_uv_rate(&data.v_offset_data);
        if controller.stop_time > STATE_FLOAT_EPSILON
            && [
                linear_uv_period(&data.u_offset_data),
                linear_uv_period(&data.v_offset_data),
            ]
            .into_iter()
            .flatten()
            .any(|period| !approximately(period, controller.stop_time))
        {
            diagnostics.push(
                "NiUVController key data does not span its cycle interval; animation was not represented"
                    .into(),
            );
            continue;
        }
        if uv_data_has_keys(&data.u_tiling_data) || uv_data_has_keys(&data.v_tiling_data) {
            diagnostics
                .push("NiUVController changes UV tiling; animation was not represented".into());
            continue;
        }
        if u_rate.is_none() && v_rate.is_none() {
            diagnostics.push(
                "NiUVController uses unsupported key data; animation was not represented".into(),
            );
            continue;
        }
        push_nif_property(state, "Nif_UV_Mode", "1");
        if let Some(rate) = u_rate {
            push_nif_property(state, "Nif_UV_U", canonical_float(rate));
        }
        if let Some(rate) = v_rate {
            push_nif_property(state, "Nif_UV_V", canonical_float(rate));
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ShapeImportContext<'a, F> {
    stream: &'a NiStream,
    resolver: &'a TextureResolver,
    ordinals: &'a HashMap<NiKey, usize>,
    transform_for: &'a F,
    scope: ScopeId,
    properties: &'a [NiKey],
    uv_controllers: &'a HashMap<NiKey, Vec<&'a NiUVController>>,
}

pub(super) fn shape_provenance(stream: &NiStream, shape: &NiTriShape) -> NifProvenance {
    NifProvenance {
        av_flags: shape.flags,
        has_controller: stream.get(shape.controller).is_some(),
        has_extra_data: stream.get(shape.extra_data).is_some(),
        has_skin_instance: stream.get(shape.skin_instance).is_some(),
    }
}

pub(super) fn shape_diagnostics(
    data: &NiTriShapeData,
    texture: Option<&TextureBinding>,
    provenance: &NifProvenance,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if provenance.has_skin_instance {
        diagnostics
            .push("NiSkinInstance is not represented; geometry was imported statically".into());
    }
    if provenance.has_controller {
        diagnostics.push("NiTimeController chain is not represented".into());
    }
    if provenance.has_extra_data {
        diagnostics.push("NiExtraData chain is not represented".into());
    }
    if data.num_uv_sets() > 1 {
        diagnostics.push(format!(
            "{} additional UV set(s) are not representable in Valve 220",
            data.num_uv_sets() - 1
        ));
    }
    if let Some(texture) = texture
        && texture.uv_set >= data.num_uv_sets()
    {
        diagnostics.push(format!(
            "base texture selects UV set {}, but geometry provides only {} set(s)",
            texture.uv_set,
            data.num_uv_sets()
        ));
    }
    diagnostics
}

pub(super) fn import_shape<F>(
    context: &ShapeImportContext<'_, F>,
    link: NiKey,
    shape: &NiTriShape,
) -> Result<VisualMesh, String>
where
    F: Fn(NiKey, tes3::nif::glam::Affine3A) -> tes3::nif::glam::Affine3A,
{
    let data = context
        .stream
        .get_as::<_, NiTriShapeData>(shape.geometry_data)
        .ok_or_else(|| format!("shape {link:?}: missing NiTriShapeData"))?;
    let block = *context
        .ordinals
        .get(&link)
        .ok_or_else(|| format!("shape {link:?}: missing diagnostic ordinal"))?;
    let transform = (context.transform_for)(link, shape.transform());
    let vertices: Vec<_> = data
        .vertices
        .iter()
        .map(|vertex| {
            let point = transform.transform_point3(*vertex);
            P3 {
                x: f64::from(point.x),
                y: f64::from(point.y),
                z: f64::from(point.z),
            }
        })
        .collect();
    let texture = texture_binding(context.stream, context.properties)
        .map_err(|error| format!("shape {link:?}: {error}"))?;
    for source in texture_sources(context.stream, context.properties)
        .map_err(|error| format!("shape {link:?}: {error}"))?
    {
        context
            .resolver
            .resolve(&source)
            .map_err(|error| format!("shape {link:?}: {error}"))?;
    }
    let uvs = texture
        .as_ref()
        .and_then(|texture| data.uv_set(texture.uv_set))
        .map(|uvs| {
            uvs.iter()
                .map(|uv| [f64::from(uv.x), f64::from(uv.y)])
                .collect()
        });
    let triangles = data
        .triangles
        .iter()
        .map(|triangle| {
            [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ]
        })
        .collect();
    let name = shape.name.clone();
    let provenance = shape_provenance(context.stream, shape);
    let mut diagnostics = shape_diagnostics(data, texture.as_ref(), &provenance);
    let mut nif_state = shape_nif_state(context.stream, context.properties);
    if !shape.name.trim().is_empty() {
        push_nif_property(&mut nif_state, "Nif_LinkName", shape.name.clone());
    }
    import_uv_state(
        context.stream,
        link,
        texture.as_ref().map(|texture| texture.uv_set),
        context.uv_controllers,
        &mut nif_state,
        &mut diagnostics,
    );
    nif_state.properties.sort();
    let texture_size = texture
        .as_ref()
        .map(|texture| context.resolver.resolve(&texture.source))
        .transpose()
        .map_err(|error| format!("shape {link:?}: {error}"))?;
    let mesh = VisualMesh {
        block,
        name,
        vertices,
        uvs,
        triangles,
        material: material_name(texture.as_ref().map(|texture| texture.source.as_str())),
        texture,
        texture_size,
        scope: context.scope,
        nif_state,
        provenance,
        diagnostics,
    };
    Ok(mesh)
}

pub(super) fn import_scene(
    path: &Path,
    resolver: &TextureResolver,
) -> Result<ImportedAsset, Error> {
    let parse_started = Instant::now();
    let stream = NiStream::from_path(path)
        .map_err(|error| Error::Nif(format!("{}: {error}", path.display())))?;
    reject_legacy_bounds(&stream, path)?;
    let ordinals: HashMap<NiKey, usize> = stream
        .objects
        .iter()
        .enumerate()
        .map(|(index, (key, _))| (key, index))
        .collect();
    let transforms = stream.world_transforms();
    let transform_for = |key, fallback| transforms.get(&key).copied().unwrap_or(fallback);
    let semantic = semantic_context(&stream, &transform_for);
    let uv_controllers: HashMap<NiKey, Vec<&NiUVController>> = stream
        .objects_of_type_with_link::<NiUVController>()
        .fold(HashMap::new(), |mut index, (_, controller)| {
            index
                .entry(controller.target.key)
                .or_default()
                .push(controller);
            index
        });
    let SemanticContext {
        scopes,
        shape_scopes,
        shape_properties,
        markers,
        diagnostics,
    } = semantic;
    let parse_semantic = parse_started.elapsed();
    let texture_started = Instant::now();
    let mut asset = ImportedAsset {
        scopes,
        markers,
        diagnostics,
        timings: StageTimings {
            parse_semantic,
            ..Default::default()
        },
        ..Default::default()
    };
    let (nodes, node_diagnostics) = import_nodes(&stream);
    asset.nodes = nodes;
    asset.diagnostics.extend(node_diagnostics);
    let import_context = ShapeImportContext {
        stream: &stream,
        resolver,
        ordinals: &ordinals,
        transform_for: &transform_for,
        scope: 0,
        properties: &[],
        uv_controllers: &uv_controllers,
    };
    for (link, shape) in stream.objects_of_type_with_link::<NiTriShape>() {
        let import_context = ShapeImportContext {
            scope: shape_scopes.get(&link.key).copied().unwrap_or_default(),
            properties: shape_properties
                .get(&link.key)
                .map_or(&[][..], Vec::as_slice),
            ..import_context
        };
        match import_shape(&import_context, link.key, shape) {
            Ok(mesh) => asset.meshes.push(mesh),
            Err(diagnostic) if diagnostic.contains("texture") => {
                return Err(Error::Nif(format!("{}: {diagnostic}", path.display())));
            }
            Err(diagnostic) => asset.diagnostics.push(diagnostic),
        }
    }
    asset.meshes.sort_by_key(|mesh| (mesh.scope, mesh.block));
    asset.nodes.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    asset.markers.sort_by(|lhs, rhs| {
        lhs.scope
            .cmp(&rhs.scope)
            .then(lhs.classname.cmp(&rhs.classname))
            .then(lhs.name.cmp(&rhs.name))
    });
    asset.timings.texture_resolution = texture_started.elapsed();
    Ok(asset)
}

#[cfg(test)]
pub(super) fn nif_meshes(
    path: &Path,
    resolver: &TextureResolver,
    include_collision: bool,
) -> Result<Vec<VisualMesh>, Error> {
    let asset = import_scene(path, resolver)?;
    let scopes = asset.scopes;
    Ok(asset
        .meshes
        .into_iter()
        .filter(|mesh| include_collision || scope_kind(&scopes, mesh.scope) == ImportScope::Visual)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{NifProperty, NifState, import_uv_state, semantic_context};
    use tes3::nif::{
        NiAVObject, NiBSAnimationNode, NiFloatData, NiFloatKey, NiLinFloatKey, NiNode, NiObjectNET,
        NiSortAdjustNode, NiStream, NiTimeController, NiTriShape, NiUVController, NiUVData,
        SortingMode,
    };

    fn node(name: &str) -> NiNode {
        NiNode {
            base: NiAVObject {
                base: NiObjectNET {
                    name: name.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn named_and_unnamed_nodes_follow_the_source_hierarchy() {
        let mut stream = NiStream::default();
        let mut body = node("body");
        let mut head = node("head");
        let head_shape = stream.insert(NiTriShape::default());
        head.children.push(head_shape.cast());
        let head_key = stream.insert(head);

        let shoulder_key = stream.insert(node("lshoulder"));
        let mut unnamed = node("");
        let unnamed_shape = stream.insert(NiTriShape::default());
        unnamed.children.push(unnamed_shape.cast());
        let unnamed_key = stream.insert(unnamed);
        let synthetic_key = stream.insert(node("__NDL_MultiMtl_Node"));
        let emitter_key = stream.insert(node("SuperSpray01 Emitter"));

        body.children.extend([
            head_key.cast(),
            shoulder_key.cast(),
            unnamed_key.cast(),
            synthetic_key.cast(),
            emitter_key.cast(),
        ]);
        let body_key = stream.insert(body);
        stream.roots.push(body_key.cast());

        let context = semantic_context(&stream, &|_, transform| transform);
        let scope_named = |name: &str| {
            context
                .scopes
                .iter()
                .find(|scope| scope.name == name)
                .unwrap_or_else(|| panic!("expected scope for {name}"))
        };
        let body_scope = scope_named("body");
        let head_scope = scope_named("head");
        let shoulder_scope = scope_named("lshoulder");
        let synthetic_scope = scope_named("__NDL_MultiMtl_Node");
        let emitter_scope = scope_named("SuperSpray01 Emitter");

        assert_eq!(body_scope.parent, Some(0));
        assert_eq!(head_scope.parent, Some(body_scope.id));
        assert_eq!(shoulder_scope.parent, Some(body_scope.id));
        assert_eq!(synthetic_scope.parent, Some(body_scope.id));
        assert_eq!(emitter_scope.parent, Some(body_scope.id));
        assert_eq!(
            context.shape_scopes.get(&head_shape.key),
            Some(&head_scope.id)
        );
        assert_eq!(
            context.shape_scopes.get(&unnamed_shape.key),
            Some(&body_scope.id)
        );
    }

    #[test]
    fn grouped_sort_mode_is_diagnosed_and_not_emitted_as_authoring_state() {
        let mut stream = NiStream::default();
        let sort_adjust = stream.insert(NiSortAdjustNode {
            sorting_mode: SortingMode::Grouped,
            ..Default::default()
        });
        stream.roots.push(sort_adjust.cast());

        let context = semantic_context(&stream, &|_, transform| transform);
        let marker = context
            .markers
            .iter()
            .find(|marker| marker.classname == "nif_node_sort_adjust")
            .expect("sort-adjust node should import");
        assert!(
            marker
                .properties
                .iter()
                .all(|property| property.key != "Nif_Sort_Mode")
        );
        assert!(context.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("Grouped sort-adjust mode (64) is unsupported")
        }));
    }

    #[test]
    fn cyclic_uv_keys_import_as_scroll_rates() {
        let mut stream = NiStream::default();
        let shape_link = stream.insert(NiTriShape::default());
        let uv_data = stream.insert(NiUVData {
            u_offset_data: NiFloatData {
                keys: NiFloatKey::LinKey(vec![
                    NiLinFloatKey {
                        time: 0.0,
                        value: 0.0,
                    },
                    NiLinFloatKey {
                        time: 8.0,
                        value: 1.0,
                    },
                ]),
                ..Default::default()
            },
            v_offset_data: NiFloatData {
                keys: NiFloatKey::LinKey(vec![
                    NiLinFloatKey {
                        time: 0.0,
                        value: 0.0,
                    },
                    NiLinFloatKey {
                        time: 8.0,
                        value: -2.0,
                    },
                ]),
                ..Default::default()
            },
            ..Default::default()
        });
        let controller_link = stream.insert(NiUVController {
            base: NiTimeController {
                flags: 0x0008,
                stop_time: 8.0,
                target: shape_link.cast(),
                ..Default::default()
            },
            texture_set: 0,
            data: uv_data,
        });
        stream.get_mut(shape_link).unwrap().controller = controller_link.cast();

        let controllers: HashMap<_, Vec<_>> = stream
            .objects_of_type_with_link::<NiUVController>()
            .fold(HashMap::new(), |mut controllers, (_, controller)| {
                controllers
                    .entry(controller.target.key)
                    .or_default()
                    .push(controller);
                controllers
            });
        let mut state = NifState::default();
        let mut diagnostics = Vec::new();
        import_uv_state(
            &stream,
            shape_link.key,
            Some(0),
            &controllers,
            &mut state,
            &mut diagnostics,
        );

        assert!(diagnostics.is_empty());
        assert!(state.properties.contains(&NifProperty {
            key: "Nif_UV_Mode".into(),
            value: "1".into(),
        }));
        assert!(state.properties.contains(&NifProperty {
            key: "Nif_UV_U".into(),
            value: "0.125".into(),
        }));
        assert!(state.properties.contains(&NifProperty {
            key: "Nif_UV_V".into(),
            value: "-0.25".into(),
        }));
    }

    #[test]
    fn auto_play_root_keeps_descendant_shapes_in_the_imported_graph() {
        let mut stream = NiStream::default();
        let mut root = node("");
        root.flags |= 0x0020;
        let shape = stream.insert(NiTriShape::default());
        root.children.push(shape.cast());
        let animation_root = stream.insert(NiBSAnimationNode { base: root });
        stream.roots.push(animation_root.cast());

        let context = semantic_context(&stream, &|_, transform| transform);
        assert!(context.shape_scopes.contains_key(&shape.key));
    }
}
