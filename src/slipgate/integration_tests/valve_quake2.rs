use crate::slipgate::{parser::repr::parse_map, repr::Extension, repr::TextureOffset};

#[test]
fn valve_quake2_face_preserves_projection_and_surface_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let data = include_str!("../../../tests/fixtures/maps/parser/quake2-valve-220.map");
    let (_, map) = parse_map(data)?;

    assert_eq!(map.to_string().parse::<crate::slipgate::repr::Map>()?, map);

    let face = &map[0].brushes[0][0];
    assert_eq!(face.texture, "mb/canonical");
    assert!((face.angle - 15.0).abs() < f32::EPSILON);
    assert!((face.scale_x - 0.5).abs() < f32::EPSILON);
    assert!((face.scale_y - 0.25).abs() < f32::EPSILON);

    match face.texture_offset {
        TextureOffset::Valve { u, v } => {
            assert_eq!((u.x, u.y, u.z, u.d), (1.0, 0.25, 0.0, 12.0));
            assert_eq!((v.x, v.y, v.z, v.d), (0.0, -1.0, 0.0, -8.0));
        }
        TextureOffset::Standard { .. } => panic!("canonical fixture must use Valve 220 projection"),
    }

    assert_eq!(
        face.extension,
        Extension::Quake2 {
            content_flags: 64,
            surface_flags: 8,
            value: 3.5,
        }
    );

    Ok(())
}
