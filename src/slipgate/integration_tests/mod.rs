mod basic_box_map;
mod valve_quake2;

#[macro_export]
macro_rules! test_map {
    // Basic case - no comparison data
    ($test_name:tt, $map:expr) => {
        #[test]
        fn $test_name() -> Result<(), Box<dyn std::error::Error>> {
            // Get imported map string slice
            let data = include_str!($map);

            // Parse
            let (_, map) = $crate::slipgate::parser::repr::parse_map(data)?;

            // Ensure it's non-empty
            assert!(!map.is_empty());

            // Ensure the map can make a lossless round trip from AST > String > AST
            assert_eq!(map.to_string().parse::<$crate::slipgate::repr::Map>()?, map);

            Ok(())
        }
    };
    // Advanced case - test against comparison expression
    ($test_name:tt, $map:expr, $cmp:expr) => {
        #[test]
        fn $test_name() -> Result<(), Box<dyn std::error::Error>> {
            // Get imported map string slice
            let data = include_str!($map);

            // Parse
            let (_, map) = $crate::slipgate::parser::repr::parse_map(data)?;

            // Ensure it matches the comparison data
            assert_eq!(map, $cmp);

            // Ensure the map can make a lossless round trip from AST > String > AST
            assert_eq!(map.to_string().parse::<$crate::slipgate::repr::Map>()?, map);
            Ok(())
        }
    };
}

test_map!(
    test_trenchbroom_group_hierarchy,
    "../../../tests/fixtures/maps/parser/0-trenchbroom-group-hierarchy.map"
);
test_map!(
    test_abstract_test_beveled,
    "../../../tests/fixtures/maps/parser/abstract_test_beveled.map"
);
test_map!(
    test_abstract_test_edited,
    "../../../tests/fixtures/maps/parser/abstract_test_edited.map"
);
test_map!(
    test_abstract_test_a,
    "../../../tests/fixtures/maps/parser/abstract_test.map"
);
test_map!(
    test_abstract_test_b,
    "../../../tests/fixtures/maps/parser/abstract-test.map"
);
test_map!(test_bevel, "../../../tests/fixtures/maps/parser/Bevel.map");
test_map!(
    test_lighting_test,
    "../../../tests/fixtures/maps/parser/lighting_test.map"
);
test_map!(
    test_mean_block_single,
    "../../../tests/fixtures/maps/parser/mean_block_single.map"
);
test_map!(
    test_mean_block,
    "../../../tests/fixtures/maps/parser/mean_block.map"
);
test_map!(
    test_mean_pillar_single,
    "../../../tests/fixtures/maps/parser/mean_pillar_single.map"
);
test_map!(
    test_mean_pillar,
    "../../../tests/fixtures/maps/parser/mean_pillar.map"
);
test_map!(
    test_test_cylinder,
    "../../../tests/fixtures/maps/parser/test_cylinder.map"
);
test_map!(
    test_daikatana_color,
    "../../../tests/fixtures/maps/parser/daikatana-color-test.map"
);
test_map!(
    test_trenchbroom_test_daikatana,
    "../../../tests/fixtures/maps/parser/trenchbroom-test-daikatana.map"
);
test_map!(
    test_trenchbroom_test_q2,
    "../../../tests/fixtures/maps/parser/trenchbroom-test-q2.map"
);
test_map!(
    test_trenchbroom_test_q3,
    "../../../tests/fixtures/maps/parser/trenchbroom-test-q3.map"
);
test_map!(
    test_trenchbroom_test_q3_legacy,
    "../../../tests/fixtures/maps/parser/trenchbroom-test-q3-legacy.map"
);
test_map!(
    test_trenchbroom_test_valve,
    "../../../tests/fixtures/maps/parser/trenchbroom-test-valve.map"
);
test_map!(
    test_unit_beveled,
    "../../../tests/fixtures/maps/parser/unit_beveled.map"
);
test_map!(test_unit, "../../../tests/fixtures/maps/parser/unit.map");
test_map!(
    test_morrobroom_torture_fixture,
    "../../../tests/fixtures/maps/morrobroom_wish_it_had_never_been_written.map"
);
test_map!(
    test_uv_test,
    "../../../tests/fixtures/maps/parser/uv-test.map"
);
