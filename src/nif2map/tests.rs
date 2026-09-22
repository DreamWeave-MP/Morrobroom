use super::geometry::{generic_projection, projection_error, triangle_projection, validate_brush};
use super::semantic::{material_name, nif_meshes, texture_binding};
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};
use tes3::nif::{NiNode, NiStream};

fn mesh(vertices: Vec<P3>, triangles: Vec<[usize; 3]>) -> VisualMesh {
    VisualMesh {
        block: 7,
        name: "fixture".into(),
        vertices,
        uvs: None,
        triangles,
        material: "fixture/mat".into(),
        texture: Some(TextureBinding {
            source: "fixture.tga".into(),
            uv_set: 0,
        }),
        texture_size: Some(TextureDimensions {
            size: (256, 256),
            source: TextureSizeSource::Resolved,
        }),
        scope: 0,
        nif_state: NifState::default(),
        provenance: NifProvenance {
            av_flags: 0,
            has_controller: false,
            has_extra_data: false,
            has_skin_instance: false,
        },
        diagnostics: Vec::new(),
    }
}

fn mesh_with_uvs(vertices: Vec<P3>, uvs: Vec<[f64; 2]>, triangles: Vec<[usize; 3]>) -> VisualMesh {
    VisualMesh {
        uvs: Some(uvs),
        ..mesh(vertices, triangles)
    }
}

fn emitted_projection(face: &str) -> Projection {
    let mut mappings = face.split('[').skip(1);
    let parse_mapping = |mapping: &str| {
        mapping
            .split(']')
            .next()
            .expect("mapping should have a closing bracket")
            .split_whitespace()
            .map(|value| {
                value
                    .parse::<f64>()
                    .expect("mapping value should be numeric")
            })
            .collect::<Vec<_>>()
    };
    let u = parse_mapping(mappings.next().expect("face should have a U mapping"));
    let v = parse_mapping(mappings.next().expect("face should have a V mapping"));
    let scales = face
        .split("] 0 ")
        .nth(1)
        .expect("face should have texture scales")
        .split_whitespace()
        .map(|value| value.parse::<f64>().expect("scale should be numeric"))
        .collect::<Vec<_>>();
    assert_eq!(u.len(), 4);
    assert_eq!(v.len(), 4);
    assert_eq!(scales.len(), 2);
    Projection {
        u: P3 {
            x: u[0],
            y: u[1],
            z: u[2],
        },
        u_shift: u[3],
        u_scale: scales[0],
        v: P3 {
            x: v[0],
            y: v[1],
            z: v[2],
        },
        v_shift: v[3],
        v_scale: scales[1],
        max_error: 0.0,
    }
}

fn options(fallback: &str) -> Options {
    Options {
        inputs: Vec::new(),
        recursive: false,
        output_dir: PathBuf::new(),
        shell_thickness: 16.0,
        fallback_thickness: 2.0,
        fallback: fallback.into(),
        skip_material: "skip".into(),
        texture_roots: Vec::new(),
        include_collision: false,
        overwrite: false,
        dry_run: false,
        verbose: false,
        validate: true,
        max_brushes: 20_000,
    }
}

#[test]
fn empty_material_uses_trenchbroom_empty_texture() {
    assert_eq!(material_name(None), "__TB_empty");
}

#[test]
fn map_faces_quote_material_names_with_spaces() {
    let face = super::geometry::emit_face(
        &[
            P3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        P3::Z,
        "TX_B_Nigh Elf_M_H05",
        None,
    )
    .expect("valid face should serialize");
    assert!(face.contains("\"TX_B_Nigh Elf_M_H05\" ["));
    let parsed = face
        .parse::<crate::slipgate::repr::BrushPlane>()
        .expect("quoted material should remain parseable");
    assert_eq!(parsed.texture, "TX_B_Nigh Elf_M_H05");
}

#[test]
fn recursive_jobs_preserve_source_subdirectories() {
    let root = std::env::temp_dir().join(format!(
        "morrobroom-nif2map-jobs-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/first.nif"), []).unwrap();
    fs::write(root.join("b/second.nif"), []).unwrap();

    let mut options = options("skip");
    options.inputs = vec![root.clone()];
    options.recursive = true;
    options.output_dir = root.join("out");
    let jobs = build_jobs(&options, gather_inputs(&options.inputs, true));
    let mut outputs: Vec<_> = jobs
        .iter()
        .map(|(_, output, _)| output.strip_prefix(&options.output_dir).unwrap().to_owned())
        .collect();
    outputs.sort();

    assert_eq!(
        outputs,
        vec![PathBuf::from("a/first.map"), PathBuf::from("b/second.map")]
    );
    assert_eq!(
        jobs[0].2.strip_prefix(&options.output_dir).unwrap(),
        Path::new("a/first.nif2map.json")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recursive_jobs_canonicalize_relative_sources() {
    let root = PathBuf::from(format!(
        "morrobroom-nif2map-relative-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/foo.nif"), []).unwrap();

    let mut options = options("skip");
    options.inputs = vec![root.clone()];
    options.recursive = true;
    options.output_dir = root.join("out");
    let source = root.join("a/foo.nif");
    let jobs = build_jobs(&options, vec![source]);

    assert_eq!(
        jobs[0].1.strip_prefix(&options.output_dir).unwrap(),
        Path::new("a/foo.map")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn zero_uv_gradients_use_the_face_normal_axis() {
    let mesh = mesh_with_uvs(
        vec![
            P3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 4.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 3.0,
                z: 0.0,
            },
        ],
        vec![[0.25, 0.75], [0.25, 0.75], [0.25, 0.75]],
        vec![[0, 1, 2]],
    );
    let projection = triangle_projection(&mesh, 0).expect("constant UVs should fit");

    assert_eq!(projection.u, P3::Z);
    assert_eq!(projection.v, P3::Z);
    assert!(projection_error(&mesh, 0, &projection) <= UV_MERGE_TOLERANCE_TEXELS);
}

#[test]
fn projection_preserves_the_nif_v_coordinate() {
    let mesh = mesh_with_uvs(
        vec![
            P3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 4.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 3.0,
                z: 0.0,
            },
        ],
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        vec![[0, 1, 2]],
    );
    let projection = triangle_projection(&mesh, 0).expect("UVs should fit");
    let t = |point: P3| projection.v.dot(point) / projection.v_scale + projection.v_shift;

    assert!(t(mesh.vertices[0]).abs() <= 1e-6);
    assert!((t(mesh.vertices[2]) - 256.0).abs() <= 1e-6);
}

#[test]
fn projection_usability_rejects_high_residuals() {
    let mut projection = generic_projection(P3::Z);

    assert!(projection_is_valid(&projection));
    assert!(projection_is_usable(&projection));
    projection.max_error = UV_MERGE_TOLERANCE_TEXELS + f64::EPSILON;
    assert!(projection_is_valid(&projection));
    assert!(!projection_is_usable(&projection));
}

#[test]
fn exact_prism_uses_structural_recognizer() {
    let vertices = vec![
        P3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 4.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 4.0,
            z: 3.0,
        },
        P3 {
            x: 0.0,
            y: 0.0,
            z: 3.0,
        },
        P3 {
            x: 10.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 10.0,
            y: 4.0,
            z: 0.0,
        },
        P3 {
            x: 10.0,
            y: 4.0,
            z: 3.0,
        },
        P3 {
            x: 10.0,
            y: 0.0,
            z: 3.0,
        },
    ];
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    let result = reconstruct(&[mesh(vertices, triangles)], &options("skip"))
        .expect("prism should reconstruct");
    assert_eq!(result.brushes.len(), 1);
    assert_eq!(result.recognizers[0].kind, "exact-extrusion");
    assert!(result.warnings.is_empty());
}

#[test]
fn planar_fallback_emits_a_convex_prism() {
    let vertices = vec![
        P3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 3.0,
            z: 0.0,
        },
    ];
    let result = reconstruct(
        &[mesh(vertices, vec![[0, 1, 2]])],
        &options("planar-prisms"),
    )
    .expect("triangle should fallback");
    assert_eq!(result.brushes.len(), 1);
    assert_eq!(result.recognizers[0].kind, "planar-prism-fallback");
    assert_eq!(validate_brush(&result.brushes[0]).unwrap(), 6);
}

#[test]
fn planar_fallback_merges_triangles_with_one_uv_chart() {
    let vertices = vec![
        P3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 3.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 3.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 3.0,
            z: 0.0,
        },
    ];
    let mesh = mesh_with_uvs(
        vertices,
        vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    );
    let result = reconstruct(std::slice::from_ref(&mesh), &options("planar-prisms"))
        .expect("compatible triangles should reconstruct");

    assert_eq!(result.recognizers[0].kind, "planar-prism-fallback");
    assert_eq!(result.brushes.len(), 1);
    assert!(validate_brush(&result.brushes[0]).is_ok());
    let projection = emitted_projection(&result.brushes[0].faces[0]);
    for triangle_index in 0..mesh.triangles.len() {
        assert!(projection_error(&mesh, triangle_index, &projection) <= UV_MERGE_TOLERANCE_TEXELS);
    }
}

#[test]
fn planar_fallback_preserves_uv_seams_between_coplanar_triangles() {
    let vertices = vec![
        P3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 3.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        P3 {
            x: 4.0,
            y: 3.0,
            z: 0.0,
        },
        P3 {
            x: 0.0,
            y: 3.0,
            z: 0.0,
        },
    ];
    let mesh = mesh_with_uvs(
        vertices,
        vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.5, 0.0],
            [1.5, 1.0],
            [0.5, 1.0],
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    );
    let result = reconstruct(&[mesh], &options("planar-prisms"))
        .expect("seamed triangles should reconstruct");

    assert_eq!(result.recognizers[0].kind, "planar-prism-fallback");
    assert_eq!(result.brushes.len(), 2);
    assert!(
        result
            .brushes
            .iter()
            .all(|brush| validate_brush(brush).is_ok())
    );
}

#[test]
fn planar_fallback_respects_uv_merge_tolerance() {
    for (shift_texels, expected_brushes) in [(0.049, 1), (0.051, 2)] {
        let shift = shift_texels / 256.0;
        let mesh = mesh_with_uvs(
            vec![
                P3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                P3 {
                    x: 4.0,
                    y: 0.0,
                    z: 0.0,
                },
                P3 {
                    x: 4.0,
                    y: 3.0,
                    z: 0.0,
                },
                P3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                P3 {
                    x: 4.0,
                    y: 3.0,
                    z: 0.0,
                },
                P3 {
                    x: 0.0,
                    y: 3.0,
                    z: 0.0,
                },
            ],
            vec![
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [shift, 0.0],
                [1.0 + shift, 1.0],
                [shift, 1.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let result = reconstruct(std::slice::from_ref(&mesh), &options("planar-prisms"))
            .expect("tolerance fixture should reconstruct");

        assert_eq!(result.brushes.len(), expected_brushes);
    }
}

#[test]
fn malformed_uvs_cannot_reach_map_output() {
    let mesh = mesh_with_uvs(
        vec![
            P3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 4.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 3.0,
                z: 0.0,
            },
        ],
        vec![[0.0, 0.0], [f64::NAN, 1.0], [0.0, 1.0]],
        vec![[0, 1, 2]],
    );
    let projection = generic_projection(P3::Z);

    assert!(projection_error(&mesh, 0, &projection).is_infinite());
    assert!(triangle_projection(&mesh, 0).is_none());

    let result = reconstruct(std::slice::from_ref(&mesh), &options("planar-prisms"))
        .expect("malformed UVs should use a finite fallback projection");
    let map_output = map_text(
        Path::new("malformed-uv.nif"),
        &result,
        &[SemanticScope {
            id: 0,
            parent: None,
            name: "Asset".into(),
            kind: ImportScope::Visual,
            node_kind: "asset".into(),
        }],
        false,
    )
    .to_ascii_lowercase();
    assert!(!map_output.contains("nan"));
    assert!(!map_output.contains("inf"));
}

#[test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "The tes3 NIF facade exposes flattened accessors but nested constructors."
)]
fn nif_adapter_preserves_visual_shape_and_transform() {
    let mut stream = NiStream::default();
    let mut data = NiTriShapeData::default();
    data.vertices = vec![
        tes3::nif::glam::vec3(0.0, 0.0, 0.0),
        tes3::nif::glam::vec3(1.0, 0.0, 0.0),
        tes3::nif::glam::vec3(0.0, 1.0, 0.0),
    ];
    data.triangles = vec![[0, 1, 2]];
    let data_link = stream.insert(data);
    let mut shape = NiTriShape::default();
    shape.geometry_data = data_link.cast();
    shape.name = "fixture-shape".into();
    shape.translation = tes3::nif::glam::vec3(10.0, 20.0, 30.0);
    let shape_link = stream.insert(shape);
    let mut root = NiNode::default();
    root.children.push(shape_link.cast());
    let root_link = stream.insert(root);
    stream.roots.push(root_link.cast());
    let bytes = stream.save_bytes().expect("fixture NIF should serialize");
    let path = std::env::temp_dir().join(format!(
        "morrobroom-nif2map-{}-{}.nif",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, bytes).unwrap();
    let resolver = TextureResolver::new(&[]).expect("empty test VFS should build");
    let meshes = nif_meshes(&path, &resolver, false).expect("fixture NIF should parse");
    fs::remove_file(path).unwrap();
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].name, "fixture-shape");
    assert_eq!(
        meshes[0].vertices[0],
        P3 {
            x: 10.0,
            y: 20.0,
            z: 30.0
        }
    );
}

#[test]
fn map_header_matches_prototype_contract() {
    let result = Reconstruction {
        brushes: vec![Brush {
            faces: vec!["face".into()],
            kind: "test".into(),
            shapes: vec![1],
            ..Default::default()
        }],
        ..Default::default()
    };
    let text = map_text(
        Path::new("fixture.nif"),
        &result,
        &[SemanticScope {
            id: 0,
            parent: None,
            name: "Asset".into(),
            kind: ImportScope::Visual,
            node_kind: "asset".into(),
        }],
        false,
    );
    assert_eq!(text.lines().next(), Some("// Game: Morrowind"));
    assert_eq!(text.lines().nth(1), Some("// Format: Quake2 (Valve)"));
    assert!(text.contains("\"mapversion\" \"220\""));
    assert!(text.contains("\"classname\" \"func_group\""));
    assert!(text.contains("\"_tb_type\" \"_tb_group\""));
    assert!(!text.contains("// brush_"));
}

#[test]
fn visual_scope_names_are_plain_editor_groups() {
    let result = Reconstruction {
        brushes: vec![Brush {
            faces: vec!["face".into()],
            kind: "test".into(),
            shapes: vec![1],
            scope: 1,
            ..Default::default()
        }],
        ..Default::default()
    };
    let text = map_text(
        Path::new("fixture.nif"),
        &result,
        &[
            SemanticScope {
                id: 0,
                parent: None,
                name: "Asset".into(),
                kind: ImportScope::Visual,
                node_kind: "asset".into(),
            },
            SemanticScope {
                id: 1,
                parent: Some(0),
                name: "head".into(),
                kind: ImportScope::Visual,
                node_kind: "NiNode".into(),
            },
        ],
        false,
    );
    assert!(text.contains("\"_tb_name\" \"head\""));
    assert!(!text.contains("\"_tb_name\" \"Visual: head\""));
}

#[test]
fn authored_map_separates_collision_and_nif_state() {
    let mut state = NifState::default();
    state.properties.push(NifProperty {
        key: "Material_Alpha".into(),
        value: "0.5".into(),
    });
    let result = Reconstruction {
        brushes: vec![
            Brush {
                faces: vec!["visual".into()],
                kind: "test".into(),
                shapes: vec![1],
                ..Default::default()
            },
            Brush {
                faces: vec!["collision".into()],
                kind: "test".into(),
                shapes: vec![2],
                scope: 1,
                ..Default::default()
            },
            Brush {
                faces: vec!["state".into()],
                kind: "test".into(),
                shapes: vec![3],
                nif_state: state,
                ..Default::default()
            },
        ],
        markers: vec![ImportedMarker {
            classname: "nif_node_collision_root".into(),
            name: "Collision Root".into(),
            origin: [0.0, 0.0, 0.0],
            scope: 1,
            properties: Vec::new(),
        }],
        ..Default::default()
    };
    let text = map_text(
        Path::new("fixture.nif"),
        &result,
        &[
            SemanticScope {
                id: 0,
                parent: None,
                name: "Asset".into(),
                kind: ImportScope::Visual,
                node_kind: "asset".into(),
            },
            SemanticScope {
                id: 1,
                parent: Some(0),
                name: "Collision Root".into(),
                kind: ImportScope::Collision,
                node_kind: "RootCollisionNode".into(),
            },
        ],
        true,
    );
    assert!(text.contains("\"classname\" \"nif_geometry\""));
    assert!(text.contains("\"Material_Alpha\" \"0.5\""));
    assert!(text.contains("\"classname\" \"nif_node_collision_root\""));
    assert!(text.contains("\"_tb_name\" \"Collision: Collision Root\""));
}

#[test]
fn texture_resolver_keeps_width_before_height() {
    let root = std::env::temp_dir().join(format!(
        "morrobroom-nif2map-texture-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("Textures/fixture.dds");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut header = vec![0; 20];
    header[..4].copy_from_slice(b"DDS ");
    header[12..16].copy_from_slice(&32u32.to_le_bytes());
    header[16..20].copy_from_slice(&64u32.to_le_bytes());
    fs::write(&path, header).unwrap();
    let resolver =
        TextureResolver::new(std::slice::from_ref(&root)).expect("test VFS should build");
    assert_eq!(resolver.dimensions("Textures/fixture.dds"), (64, 32));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn texture_resolver_prefers_dds_and_later_roots() {
    let root = std::env::temp_dir().join(format!(
        "morrobroom-nif2map-texture-priority-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let low = root.join("low/Textures");
    let high = root.join("high/Textures");
    fs::create_dir_all(&low).unwrap();
    fs::create_dir_all(&high).unwrap();
    let dds = |path: &Path, width: u32, height: u32| {
        let mut header = vec![0; 20];
        header[..4].copy_from_slice(b"DDS ");
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&width.to_le_bytes());
        fs::write(path, header).unwrap();
    };
    dds(&low.join("fixture.dds"), 64, 32);
    dds(&high.join("fixture.dds"), 128, 96);
    fs::write(low.join("fixture.tga"), b"not used").unwrap();
    fs::write(high.join("fixture.tga"), b"not used").unwrap();

    let resolver = TextureResolver::new(&[root.join("low"), root.join("high")])
        .expect("test VFS should build");
    assert_eq!(
        resolver.resolved_path("Textures/fixture.tga"),
        "textures/fixture.dds"
    );
    assert_eq!(resolver.dimensions("Textures/fixture.tga"), (128, 96));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn base_texture_binding_preserves_the_declared_uv_set() {
    let mut stream = NiStream::default();
    let texture_link = stream.insert(tes3::nif::NiSourceTexture {
        source: TextureSource::External("Textures/base.tga".into()),
        ..Default::default()
    });
    let property_link = stream.insert(NiTexturingProperty {
        texture_maps: vec![Some(TextureMap::Map(tes3::nif::Map {
            texture: texture_link.cast(),
            texture_index: 1,
            ..Default::default()
        }))],
        ..Default::default()
    });

    let binding = texture_binding(&stream, &[property_link.key])
        .expect("base map binding should be valid")
        .expect("base map binding");
    assert_eq!(binding.source, "Textures/base.tga");
    assert_eq!(binding.uv_set, 1);
}
