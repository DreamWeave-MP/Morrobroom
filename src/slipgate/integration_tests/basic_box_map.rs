use crate::{
    slipgate::repr::{
        Brush, BrushPlane, Brushes, Entity, Extension, Map, Point, Properties, Property,
        TextureOffset, TrianglePlane,
    },
    test_map,
};

test_map!(
    test_basic_box_map,
    "../../../tests/fixtures/maps/parser/basic-box.map",
    basic_box_map()
);

fn brush_plane(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3]) -> BrushPlane {
    BrushPlane {
        plane: TrianglePlane {
            v0: point(v0),
            v1: point(v1),
            v2: point(v2),
        },
        texture: "__TB_empty".into(),
        texture_offset: TextureOffset::Standard { u: 0.0, v: 0.0 },
        angle: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        extension: Extension::Standard,
    }
}

fn point([x, y, z]: [f32; 3]) -> Point {
    Point { x, y, z }
}

fn basic_box_map() -> Map {
    let planes = vec![
        brush_plane(
            [-64.0, -64.0, -16.0],
            [-64.0, -63.0, -16.0],
            [-64.0, -64.0, -15.0],
        ),
        brush_plane(
            [-64.0, -64.0, -16.0],
            [-64.0, -64.0, -15.0],
            [-63.0, -64.0, -16.0],
        ),
        brush_plane(
            [-64.0, -64.0, -16.0],
            [-63.0, -64.0, -16.0],
            [-64.0, -63.0, -16.0],
        ),
        brush_plane([64.0, 64.0, 16.0], [64.0, 65.0, 16.0], [65.0, 64.0, 16.0]),
        brush_plane([64.0, 64.0, 16.0], [65.0, 64.0, 16.0], [64.0, 64.0, 17.0]),
        brush_plane([64.0, 64.0, 16.0], [64.0, 64.0, 17.0], [64.0, 65.0, 16.0]),
    ];

    Map::new(vec![Entity {
        properties: Properties::new(vec![Property {
            key: "classname".into(),
            value: "worldspawn".into(),
        }]),
        brushes: Brushes::new(vec![Brush::new(planes)]),
    }])
}
