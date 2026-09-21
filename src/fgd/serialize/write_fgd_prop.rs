// Potions, books, containers, doors, enchantments, lockpicks , probes, repair items

use std::io::{self, Write};
use tes3::esp::{
    Activator, Alchemy, Apparatus, Armor, Book, Clothing, Container, Door, EditorId, EffectId,
    EffectId2, EffectRange, Ingredient, LeveledCreature, LeveledCreatureFlags, LeveledItem,
    LeveledItemFlags, Light, LightFlags, Lockpick, MiscItem, Probe, RepairItem, SkillId, Static,
    Weapon,
};

use super::{
    encode_fgd_token, generate_rgb_from_id, std_write_fgd::STDWriteFGD, write_light_point_class,
    write_object_flags, write_point_class, write_unplaceable_point_class,
};

pub static LEVC_WIDTH: i32 = 16;
pub static LEVC_HEIGHT: i32 = 64;
pub static LEVI_WIDTH: i32 = 48;
pub static LEVI_HEIGHT: i32 = 32;

pub static LEVI_BOUNDS: [i32; 6] = [
    -LEVI_WIDTH,
    -LEVI_WIDTH,
    -LEVI_HEIGHT,
    LEVI_WIDTH,
    LEVI_WIDTH,
    LEVI_HEIGHT,
];

pub static LEVC_BOUNDS: [i32; 6] = [
    -LEVC_WIDTH,
    -LEVC_WIDTH,
    -LEVI_HEIGHT,
    LEVC_WIDTH,
    LEVC_WIDTH,
    LEVC_HEIGHT,
];

pub static BOOL_CHOICES: &str = r#"    [
        "true" : "true"
        "false" : "false"
    ]
"#;

pub trait WriteFGDProp {
    fn write_fgd<W: Write>(
        &self,
        target: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error>;
}

impl WriteFGDProp for Static {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "static_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        write_object_flags(fgd_string, &self.flags)?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_static_fgd {
    use super::*;

    fn serialize_static(record: &Static) -> String {
        let mut buf = Vec::<u8>::new();
        record.write_fgd(&mut buf, "static_test.esp", None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn static_fgd_contains_expected_lines() {
        let record = Static {
            id: "Rock01".into(),
            mesh: "meshes\\m\\rock_01.nif".into(),
            ..Default::default()
        };

        let out = serialize_static(&record);
        assert!(out.contains("static_rock01"));
        assert!(out.contains("Plugin(string)"));
        assert!(out.contains("Model(string)"));
        assert!(out.contains("ObjectFlags(string)"));
        assert!(!out.contains("Script(string)"));
        assert!(out.trim_end().ends_with(']'));

        let expected = r#"@PointClass base(world_Base) color(255 255 255) = static_rock01
[
    Plugin(string): "": "static_test.esp"
    Model(string): "": "meshes\m\rock_01.nif"
    ObjectFlags(string): "": ""
]

"#;

        assert_eq!(out, expected);
        println!("{out}");
    }
}

impl WriteFGDProp for Activator {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "activator_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        write_object_flags(fgd_string, &self.flags)?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_activator_fgd {
    use super::*; // Activator, WriteFGDProp, etc.

    fn serialize_activator(act: &Activator) -> String {
        let mut buf = Vec::<u8>::new();
        act.write_fgd(&mut buf, "activ_test.esp", None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn activator_fgd_contains_expected_lines() {
        let act = Activator {
            id: "DoorWood01".into(),
            mesh: "meshes\\d\\door_wood01.nif".into(),
            script: "door_open_script".into(),
            name: "Test Door".into(),
            ..Default::default()
        };

        let out = serialize_activator(&act);
        assert!(out.contains("activator_doorwood01"));
        assert!(out.contains("Plugin(string)"));
        assert!(out.contains("Model(string)"));
        assert!(out.contains("Name(string)"));
        assert!(out.contains("Script(string)"));
        assert!(out.contains("ObjectFlags(string)"));
        assert!(!out.contains("static_doorwood01"));
        assert!(out.trim_end().ends_with(']'));

        // Explicit equality checks catch instances where invalid strings should be accounted for
        let expected = r#"@PointClass base(world_Base) color(255 255 255) = activator_doorwood01
[
    Plugin(string): "": "activ_test.esp"
    Model(string): "": "meshes\d\door_wood01.nif"
    Name(string): "": "Test_x20_Door"
    Script(string): "": "door_open_script"
    ObjectFlags(string): "": ""
]

"#;

        assert_eq!(out, expected);
        println!("{out}");
    }
}

impl WriteFGDProp for Ingredient {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "ingredient_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        // String parms first
        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        write_object_flags(fgd_string, &self.flags)?;

        // Then numeric
        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        self.data
            .effects
            .iter()
            .enumerate()
            .try_for_each(|(idx, effect)| {
                if effect != &tes3::esp::EffectId::None {
                    writeln!(
                        fgd_string,
                        "    EffectId_{idx}(string) : \"\" : \"{}\"",
                        effect.display()
                    )?;

                    match effect {
                        EffectId::AbsorbAttribute
                        | EffectId::DamageAttribute
                        | EffectId::FortifyAttribute
                        | EffectId::RestoreAttribute
                        | EffectId::DrainAttribute => {
                            writeln!(
                                fgd_string,
                                "    AttributeId_{idx}(string) : \"\" :  \"{}\"",
                                self.data.attributes[idx].display()
                            )?;
                        }
                        EffectId::AbsorbSkill
                        | EffectId::DamageSkill
                        | EffectId::FortifySkill
                        | EffectId::RestoreSkill
                        | EffectId::DrainSkill => {
                            writeln!(
                                fgd_string,
                                "    SkillId_{idx}(string) : \"\" :  \"{}\"",
                                self.data.skills[idx].display()
                            )?;
                        }
                        _ => {}
                    }
                }

                Ok(())
            })
            .map_err(|err: std::io::Error| {
                io::Error::new(io::ErrorKind::InvalidData, err.to_string())
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_ingredient_effect_expansion {
    use super::*;
    use tes3::esp::{AttributeId, EffectId, Ingredient, SkillId};

    /// Serialize an ingredient to a UTF‑8 `String`.
    fn serialize(ing: &Ingredient) -> String {
        let mut buf = Vec::<u8>::new();
        ing.write_fgd(&mut buf, "unit_test.esp", None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// Convenience to check presence/absence of substrings.
    fn assert_contains(haystack: &str, needle: &str) {
        assert!(
            haystack.contains(needle),
            "expected to find `{needle}` in:\n{haystack}"
        );
    }
    fn assert_not_contains(haystack: &str, needle: &str) {
        assert!(
            !haystack.contains(needle),
            "did NOT expect to find `{needle}` in:\n{haystack}"
        );
    }

    #[test]
    fn effect_drives_attribute_or_skill_lines() {
        // Build a synthetic ingredient that hits all three branches:
        //  idx 0 → FortifyAttribute  -> AttributeId_0 expected
        //  idx 1 → DamageSkill       -> SkillId_1 expected
        //  idx 2 → None              -> no extra line
        let ing = Ingredient {
            id: "ingred_unit_test".into(),
            mesh: "meshes\\dummy.nif".into(),
            script: String::new(),
            name: "UnitTest Ingredient".into(),
            data: tes3::esp::IngredientData {
                weight: 0.1,
                value: 1,
                effects: [
                    EffectId::FortifyAttribute,
                    EffectId::DamageSkill,
                    EffectId::None,
                    EffectId::AbsorbHealth,
                ],
                attributes: [
                    tes3::esp::AttributeId::Strength,
                    tes3::esp::AttributeId::Endurance,
                    tes3::esp::AttributeId::Intelligence,
                    tes3::esp::AttributeId::None,
                ],
                skills: [
                    tes3::esp::SkillId::Alchemy,
                    tes3::esp::SkillId::BluntWeapon,
                    tes3::esp::SkillId::Mysticism,
                    tes3::esp::SkillId::None,
                ],
            },
            ..Default::default()
        };

        let out = serialize(&ing);

        // -- Effect lines always present for non‑None effects
        assert_contains(&out, "EffectId_0(string)");
        assert_contains(&out, "EffectId_1(string)");
        assert_not_contains(&out, "EffectId_2(string)");
        assert_contains(&out, "EffectId_3(string)");

        // -- Attribute branch: only idx 0 should have AttributeId_0, not SkillId_0
        assert_contains(&out, "AttributeId_0(string)");
        assert_not_contains(&out, "SkillId_0(string)");

        // -- Skill branch: only idx 1 should have SkillId_1, not AttributeId_1
        assert_contains(&out, "SkillId_1(string)");
        assert_not_contains(&out, "AttributeId_1(string)");

        // -- None branch: idx 2 should have neither
        assert_not_contains(&out, "AttributeId_2(string)");
        assert_not_contains(&out, "SkillId_2(string)");

        // -- Skill only branch
        assert_not_contains(&out, "SkillId_3(string)");
        assert_not_contains(&out, "AttributeId_3(string)");
    }

    #[test]
    fn print_sample_ingredient_fgd() {
        let ingredient = Ingredient {
            id: "ingred_guar_hide".into(),
            mesh: "meshes\\m\\ingred_hide_guar.nif".into(),
            script: "guar_hide_script".into(),
            name: "Guar Hide".into(),
            data: tes3::esp::IngredientData {
                weight: 0.5,
                value: 15,
                effects: [
                    EffectId::FortifyAttribute,
                    EffectId::RestoreHealth,
                    EffectId::AbsorbSkill,
                    EffectId::None,
                ],
                attributes: [
                    AttributeId::Strength,
                    AttributeId::Endurance,
                    AttributeId::Luck,
                    AttributeId::Personality,
                ],
                skills: [
                    SkillId::Alchemy,
                    SkillId::BluntWeapon,
                    SkillId::Destruction,
                    SkillId::Security,
                ],
            },
            ..Default::default()
        };

        let mut buf = Vec::new();
        ingredient
            .write_fgd(&mut buf, "example_plugin.esp", None)
            .unwrap();
        let output = String::from_utf8(buf).unwrap();

        println!("{output}");
    }
}

impl WriteFGDProp for Weapon {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "weapon_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        "Enchantment".write_fgd(fgd_string, "", &self.enchanting)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        self.data.chop_min.write_fgd(fgd_string, "ChopMin", "")?;
        self.data.chop_max.write_fgd(fgd_string, "ChopMax", "")?;
        self.data
            .thrust_min
            .write_fgd(fgd_string, "ThrustMin", "")?;
        self.data
            .thrust_max
            .write_fgd(fgd_string, "ThrustMax", "")?;
        self.data.slash_min.write_fgd(fgd_string, "SlashMin", "")?;
        self.data.slash_max.write_fgd(fgd_string, "SlashMax", "")?;

        self.data.health.write_fgd(fgd_string, "Durability", "")?;
        self.data.reach.write_fgd(fgd_string, "Reach", "")?;
        self.data.speed.write_fgd(fgd_string, "Speed", "")?;
        self.data
            .enchantment
            .write_fgd(fgd_string, "EnchantCap", "")?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod weapon_tests {
    use super::*;

    #[test]
    fn test_weapon_fgd_serialization_basic() {
        let weapon = Weapon {
            id: "weapon_daed_dagger".to_string(),
            name: "Daedric Dagger".into(),
            script: "some_script".into(),
            mesh: "w\\daedric_dagger.nif".into(),
            icon: "w\\tx_dagger.dds".into(),
            enchanting: "enchant_fire".into(),
            data: tes3::esp::WeaponData {
                weight: 10.0,
                value: 1200,
                chop_min: 5,
                chop_max: 20,
                thrust_min: 3,
                thrust_max: 12,
                slash_min: 4,
                slash_max: 18,
                health: 500,
                reach: 1.2,
                speed: 1.0,
                enchantment: 50,
                ..Default::default()
            },
            // fill in whatever else is needed
            ..Default::default()
        };

        let mut buffer = Vec::new();
        weapon.write_fgd(&mut buffer, "mymod.esp", None).unwrap();

        let result = String::from_utf8(buffer).unwrap();
        eprintln!("{result}");

        assert!(result.contains("weapon_daed_dagger"));
        assert!(result.contains("Name(string)"));
        assert!(result.contains("Value(integer)"));
        assert!(result.contains("ObjectFlags(string)"));
        assert!(result.contains("Plugin(string): \"\": \"mymod.esp\""));
        assert!(result.contains("Enchantment(string): \"\": \"enchant_fire\""));
        assert!(result.contains("ChopMax(integer): \"\": 20"));
        assert!(result.contains('['));
        assert!(result.contains(']'));
        eprintln!("{result}");

        let expected = r#"@PointClass base(world_Base) color(255 255 255) = weapon_weapon_daed_dagger
[
    Plugin(string): "": "mymod.esp"
    Model(string): "": "w\daedric_dagger.nif"
    Script(string): "": "some_script"
    Name(string): "": "Daedric_x20_Dagger"
    Icon(string): "": "w\tx_dagger.dds"
    Enchantment(string): "": "enchant_fire"
    ObjectFlags(string): "": ""
    Weight(float): "": "10"
    Value(integer): "": 1200
    ChopMin(integer): "": 5
    ChopMax(integer): "": 20
    ThrustMin(integer): "": 3
    ThrustMax(integer): "": 12
    SlashMin(integer): "": 4
    SlashMax(integer): "": 18
    Durability(integer): "": 500
    Reach(float): "": "1.2"
    Speed(float): "": "1"
    EnchantCap(integer): "": 50
]

"#;

        assert_eq!(result, expected);
    }

    #[test]
    fn test_weapon_fgd_serialization_with_bounds() {
        let weapon = Weapon {
            name: "Iron Saber".into(),
            id: "iron_saber".into(),
            ..Default::default()
        };

        let mut buffer = Vec::new();
        let bounds = [0, 0, 0, 100, 100, 100];
        weapon
            .write_fgd(&mut buffer, "testplugin.esp", Some(&bounds))
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("weapon_iron_saber"));
        assert!(output.contains("Plugin(string): \"\": \"testplugin.esp\""));
        assert!(output.contains("size(0 0 0, 100 100 100)"));
        // Optionally test bounds-related output if it's included
    }
}

impl WriteFGDProp for Light {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        let half_size = i32::try_from(self.data.radius / 2)
            .expect("light radius must fit in the FGD integer bounds");

        if self.data.flags.contains(LightFlags::CAN_CARRY) {
            write_point_class(
                fgd_string,
                &["world_Base"],
                bounds,
                self.editor_id_ascii_lowercase().replace(' ', "_"),
            )?;
        } else {
            write_light_point_class(
                fgd_string,
                &["world_Base"],
                &[
                    -half_size, -half_size, -half_size, half_size, half_size, half_size,
                ],
                &self.data.color,
                format!(
                    "light_{}",
                    self.editor_id_ascii_lowercase().replace(' ', "_")
                ),
            )?;
        }

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        self.data.color.write_fgd(fgd_string, "light_color", "")?;

        let light_flags = self
            .data
            .flags
            .iter_names()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
            .join(" | ");

        "LightFlags".write_fgd(fgd_string, "", &light_flags)?;

        self.data.radius.write_fgd(fgd_string, "Radius", "")?;
        self.data.time.write_fgd(fgd_string, "Duration", "")?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

/// Later write a test that ensures the lower bounds are actually negative values of the upper ones
#[cfg(test)]
mod test_light_fgd {
    use super::*;
    use tes3::esp::{LightData, LightFlags};

    fn serialize_light(light: &Light, parent_mod: &str) -> String {
        let mut buf = Vec::<u8>::new();
        light
            .write_fgd(&mut buf, parent_mod, None /* no bounds */)
            .expect("Light::write_fgd should succeed");
        String::from_utf8(buf).expect("FGD output must be UTF‑8")
    }

    macro_rules! assert_contains {
        ($haystack:expr, $needle:expr) => {
            assert!(
                $haystack.contains($needle),
                "\nexpected to find ➜ {needle}\n\nin output:\n{haystack}",
                needle = $needle,
                haystack = $haystack
            );
        };
    }

    #[test]
    fn test_light_fgd_basic() {
        let light = Light {
            id: "Test_Torch".to_string(),
            name: "Dungeon Torch".into(),
            mesh: "d\\torch.nif".into(),
            script: "torch_script".into(),
            icon: "tx_torch.dds".into(),
            data: LightData {
                weight: 2.5,
                value: 15,
                color: [255, 128, 0, 0],
                radius: 64,
                time: 3600,
                flags: {
                    let mut f = LightFlags::empty();
                    f.insert(LightFlags::DYNAMIC);
                    f.insert(LightFlags::FIRE);
                    f
                },
            },
            ..Default::default()
        };

        let out = serialize_light(&light, "my_lights.esp");

        // -- Class line
        assert_contains!(out, "light_test_torch");
        // -- Core string props
        assert_contains!(out, "Plugin(string): \"\": \"my_lights.esp\"");
        assert_contains!(out, "Model(string): \"\": \"d\\torch.nif\"");
        assert_contains!(out, "Script(string): \"\": \"torch_script\"");
        assert_contains!(out, "Name(string): \"\": \"Dungeon_x20_Torch\"");
        assert_contains!(out, "Icon(string): \"\": \"tx_torch.dds\"");
        // -- Numeric props
        assert_contains!(out, "Weight(float): \"\": \"2.5\"");
        assert_contains!(out, "Value(integer): \"\": 15");
        assert_contains!(out, "Radius(integer): \"\": 64");
        assert_contains!(out, "Duration(integer): \"\": 3600");
        // -- Color
        assert_contains!(out, "light_color(color): \"\": \"1.000 0.502 0.000\"");
        // -- Flags
        assert_contains!(out, "Flags(string): \"\": \"DYNAMIC | FIRE\"");
        // -- Closing bracket
        assert!(out.trim_end().ends_with(']'));
    }

    #[test]
    #[allow(
        clippy::field_reassign_with_default,
        reason = "This fixture mutates only the fields relevant to the empty-flags case."
    )]
    fn test_light_fgd_empty_flags() {
        let mut light = Light::default();
        light.id = "EmptyFlagLight".to_string();
        light.data.flags = LightFlags::empty();

        let out = serialize_light(&light, "empty.esp");
        eprintln!("{out}");

        assert!(!out.contains("LightFlags(Flags): \"\": \"\""));
        assert!(!out.contains("LightFlags(string): \"\": 0"));
        assert!(out.contains("ObjectFlags(string): \"\": \"\""));
        assert!(out.contains("LightFlags(string): \"\": \"\""));
    }
}

impl WriteFGDProp for LeveledCreature {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        _: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_unplaceable_point_class(
            fgd_string,
            &["world_Base"],
            Some(&LEVC_BOUNDS),
            &generate_rgb_from_id(&self.id),
            String::from("leveledcreature_") + &self.editor_id_ascii_lowercase().replace(' ', "_"),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        write_object_flags(fgd_string, &self.flags)?;

        writeln!(
            fgd_string,
            "    CalculateFromAllLevels(choices): \"Whether to ignore the specified level for each possible option, and simply spawn all possible options at all levels.\": \"{}\" =\n{}",
            self.leveled_creature_flags
                .contains(LeveledCreatureFlags::CALCULATE_FROM_ALL_LEVELS),
            BOOL_CHOICES,
        )?;

        self.chance_none
            .write_fgd(fgd_string, "ChanceNone", "Chance to spawn nothing")?;

        self.creatures
            .iter()
            .enumerate()
            .try_for_each(|(idx, (creature, level))| {
                format!("LeveledCreatureId_{idx}").write_fgd(
                    fgd_string,
                    "Leveled Creature RecordId",
                    creature,
                )?;

                format!("LeveledCreatureLevel_{idx}").write_fgd(
                    fgd_string,
                    "Leveled Creature Required Level",
                    &level.to_string(),
                )
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_leveled_creature_fgd {
    use super::*; // assumes your `LeveledCreature` and traits are in scope
    use tes3::esp::LeveledCreature;

    #[test]
    fn print_leveled_creature_fgd() {
        let leveled_creature = LeveledCreature {
            id: "TestLeveledCreature".to_string(),
            chance_none: 15,
            creatures: vec![
                ("rat".to_string(), 1),
                ("skeleton".to_string(), 2),
                ("zombie".to_string(), 3),
                ("cliff_racer".to_string(), 5),
                ("mudcrab".to_string(), 1),
                ("atronach_flame".to_string(), 6),
                ("atronach_frost".to_string(), 7),
                ("atronach_storm".to_string(), 9),
                ("dreugh".to_string(), 8),
                ("daedroth".to_string(), 10),
            ],
            ..Default::default()
        };

        let mut output = Vec::new();
        leveled_creature
            .write_fgd(&mut output, "some_plugin.esp", None)
            .unwrap();

        let out_str = String::from_utf8(output).unwrap();
        println!("{out_str}");
    }
}

impl WriteFGDProp for LeveledItem {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        _: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_unplaceable_point_class(
            fgd_string,
            &["world_Base"],
            Some(&LEVI_BOUNDS),
            &generate_rgb_from_id(&self.id),
            String::from("leveleditem_") + &self.editor_id_ascii_lowercase().replace(' ', "_"),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        write_object_flags(fgd_string, &self.flags)?;

        writeln!(
            fgd_string,
            "    CalculateFromAllLevels(choices): \"Whether to ignore the specified level for each possible option, and simply spawn all possible options at all levels.\": \"{}\" =\n{}",
            self.leveled_item_flags
                .contains(LeveledItemFlags::CALCULATE_FROM_ALL_LEVELS),
            BOOL_CHOICES,
        )?;

        writeln!(
            fgd_string,
            "    CalculateForEachItem(choices): \"Idunno, actually.\": \"{}\" =\n{}",
            self.leveled_item_flags
                .contains(LeveledItemFlags::CALCULATE_FOR_EACH_ITEM),
            BOOL_CHOICES,
        )?;

        self.chance_none
            .write_fgd(fgd_string, "ChanceNone", "Chance to spawn nothing")?;

        self.items
            .iter()
            .enumerate()
            .try_for_each(|(idx, (creature, level))| {
                format!("LeveledItemId_{idx}").write_fgd(
                    fgd_string,
                    "Leveled Item RecordId",
                    creature,
                )?;

                format!("LeveledItemLevel_{idx}").write_fgd(
                    fgd_string,
                    "Leveled Item Required Level",
                    &level.to_string(),
                )
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_leveled_item_fgd {
    use super::*;

    fn serialize(item: &LeveledItem) -> String {
        let mut buf = Vec::<u8>::new();
        item.write_fgd(&mut buf, "unit_test.esp", None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    #[allow(
        clippy::field_reassign_with_default,
        reason = "This fixture is intentionally assembled field by field for readability."
    )]
    fn print_leveled_item_fgd() {
        let mut li = LeveledItem::default();
        li.id = "LItem_TestChest01".into();
        li.leveled_item_flags =
            LeveledItemFlags::CALCULATE_FROM_ALL_LEVELS | LeveledItemFlags::CALCULATE_FOR_EACH_ITEM;
        li.chance_none = 20;
        li.items = vec![
            ("gold_001".into(), 1),
            ("potion_restore_health".into(), 3),
            ("steel_sword".into(), 5),
        ];

        println!("{}", serialize(&li));
    }

    #[test]
    #[allow(
        clippy::field_reassign_with_default,
        reason = "This fixture is intentionally assembled field by field for readability."
    )]
    fn flags_and_items_are_serialized_correctly() {
        let mut li = LeveledItem::default();
        li.id = "LItem_FlagsTest".into();
        li.leveled_item_flags = LeveledItemFlags::CALCULATE_FROM_ALL_LEVELS;
        li.items = vec![("iron_dagger".into(), 2), ("silver_dagger".into(), 6)];
        li.chance_none = 5;

        let out = serialize(&li);

        assert!(out.contains("CalculateFromAllLevels"));
        assert!(out.contains("true"));
        assert!(out.contains("CalculateForEachItem"));
        assert!(out.contains("false"));

        assert!(out.contains("LeveledItemId_0(string)"));
        assert!(out.contains("iron_dagger"));
        assert!(out.contains("LeveledItemLevel_0(string)"));
        assert!(out.contains('2'));

        assert!(out.contains("LeveledItemId_1(string)"));
        assert!(out.contains("silver_dagger"));
        assert!(out.contains("LeveledItemLevel_1(string)"));
        assert!(out.contains('6'));

        assert!(out.trim_end().ends_with(']'));
    }
}

impl WriteFGDProp for Clothing {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "clothing_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        "Enchantment".write_fgd(fgd_string, "", &self.enchanting)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        self.data
            .enchantment
            .write_fgd(fgd_string, "EnchantCap", "")?;

        "ClothingType".write_fgd(fgd_string, "", self.data.clothing_type.display())?;

        self.biped_objects
            .iter()
            .enumerate()
            .try_for_each(|(idx, biped_object)| {
                if biped_object.male_bodypart != String::default() {
                    format!(
                        "PartSlot_{}_{idx}_male",
                        biped_object.biped_object_type.display()
                    )
                    .write_fgd(fgd_string, "", &biped_object.male_bodypart)?;
                }

                if biped_object.female_bodypart == String::default() {
                    Ok(())
                } else {
                    format!(
                        "PartSlot_{}_{idx}_female",
                        biped_object.biped_object_type.display()
                    )
                    .write_fgd(fgd_string, "", &biped_object.female_bodypart)
                }
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_clothing_fgd {
    use super::*;

    /// Serialize helper
    fn serialize(clothing: &Clothing) -> String {
        let mut buf = Vec::<u8>::new();
        clothing
            .write_fgd(&mut buf, "test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_clothing_fgd() {
        let clothing = Clothing {
            id: "TestClothing_Robe".into(),
            mesh: "Meshes\\Clothes\\robe.nif".into(),
            script: "script_clothing_test".into(),
            name: "Robes of Testing".into(),
            icon: "Icons\\robe.dds".into(),
            enchanting: "enchant_robe_test".into(),
            data: tes3::esp::ClothingData {
                weight: 1.5,
                value: 25,
                enchantment: 60,
                clothing_type: tes3::esp::ClothingType::Robe,
            },
            biped_objects: vec![tes3::esp::BipedObject {
                biped_object_type: tes3::esp::BipedObjectType::Chest,
                male_bodypart: "BM_RobeUpper".into(),
                female_bodypart: "BF_RobeUpper".into(),
            }],
            ..Default::default()
        };

        println!("{}", serialize(&clothing)); // inspect with --nocapture
    }

    #[test]
    fn clothing_serialization_includes_new_partslot_fields() {
        // Build deterministic record
        let clothing = Clothing {
            id: "ExactTest".into(),
            mesh: "Meshes\\C\\c.nif".into(),
            script: "script_c".into(),
            name: "C Robe".into(),
            icon: "Icons\\c.tga".into(),
            enchanting: "en_c".into(),
            data: tes3::esp::ClothingData {
                weight: 2.0,
                value: 42,
                enchantment: 120,
                clothing_type: tes3::esp::ClothingType::Robe,
            },
            biped_objects: vec![tes3::esp::BipedObject {
                biped_object_type: tes3::esp::BipedObjectType::Chest,
                male_bodypart: "C_Male_Part".into(),
                female_bodypart: "C_Female_Part".into(),
            }],
            ..Default::default()
        };

        let out = serialize(&clothing);

        // ── core fields
        assert!(out.contains("Plugin(string)"));
        assert!(out.contains("Model(string)"));
        assert!(out.contains("Name(string)"));
        assert!(out.contains("Icon(string)"));
        assert!(out.contains("Weight(float)"));
        assert!(out.contains("Value(integer)"));

        // ── new PartSlot lines
        let part_name = tes3::esp::BipedObjectType::Chest.display();
        let male_slot = format!("PartSlot_{part_name}_0_male(string)");
        let female_slot = format!("PartSlot_{part_name}_0_female(string)");

        assert!(out.contains(&male_slot));
        assert!(out.contains("C_Male_Part"));
        assert!(out.contains(&female_slot));
        assert!(out.contains("C_Female_Part"));

        // ensure trailing double newline
        assert!(out.ends_with("]\n\n"));
    }
}

impl WriteFGDProp for Armor {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "armor_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        "Enchantment".write_fgd(fgd_string, "", &self.enchanting)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        self.data
            .enchantment
            .write_fgd(fgd_string, "EnchantCap", "")?;

        "ArmorType".write_fgd(fgd_string, "", self.data.armor_type.display())?;

        self.data
            .armor_rating
            .write_fgd(fgd_string, "ArmorRating", "")?;

        self.data.health.write_fgd(fgd_string, "Durability", "")?;

        self.biped_objects
            .iter()
            .enumerate()
            .try_for_each(|(idx, biped_object)| {
                if biped_object.male_bodypart != String::default() {
                    format!(
                        "PartSlot_{}_{idx}_male",
                        biped_object.biped_object_type.display()
                    )
                    .write_fgd(fgd_string, "", &biped_object.male_bodypart)?;
                }

                if biped_object.female_bodypart == String::default() {
                    Ok(())
                } else {
                    format!(
                        "PartSlot_{}_{idx}_female",
                        biped_object.biped_object_type.display()
                    )
                    .write_fgd(fgd_string, "", &biped_object.female_bodypart)
                }
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_armor_fgd {
    use super::*;

    fn serialize(armor: &Armor) -> String {
        let mut buf = Vec::<u8>::new();
        armor.write_fgd(&mut buf, "test_plugin.esp", None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_armor_fgd() {
        let armor = Armor {
            id: "TestArmor_Cuirass".into(),
            mesh: "Meshes\\A\\cuirass.nif".into(),
            script: "script_armor_test".into(),
            name: "Cuirass of Testing".into(),
            icon: "Icons\\cuirass.dds".into(),
            enchanting: "enchant_cuirass_test".into(),
            data: tes3::esp::ArmorData {
                weight: 12.5,
                value: 300,
                armor_type: tes3::esp::ArmorType::Cuirass,
                enchantment: 90,
                armor_rating: 45,
                health: 150,
            },
            biped_objects: vec![tes3::esp::BipedObject {
                biped_object_type: tes3::esp::BipedObjectType::Chest,
                male_bodypart: "AM_CuirassUpper".into(),
                female_bodypart: "AF_CuirassUpper".into(),
            }],
            ..Default::default()
        };

        println!("{}", serialize(&armor));
    }

    #[test]
    fn armor_serialization_core_and_partslots() {
        let armor = Armor {
            id: "ExactArmor".into(),
            mesh: "Meshes\\A\\a.nif".into(),
            script: "script_a".into(),
            name: "A Armor".into(),
            icon: "Icons\\a.tga".into(),
            enchanting: "en_a".into(),
            data: tes3::esp::ArmorData {
                weight: 6.0,
                value: 120,
                armor_type: tes3::esp::ArmorType::Cuirass,
                enchantment: 50,
                armor_rating: 33,
                health: 99,
            },
            biped_objects: vec![tes3::esp::BipedObject {
                biped_object_type: tes3::esp::BipedObjectType::Chest,
                male_bodypart: "A_Male_Part".into(),
                female_bodypart: "A_Female_Part".into(),
            }],
            ..Default::default()
        };

        let out = serialize(&armor);

        for needle in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "Icon(string)",
            "Enchantment(string)",
            "Weight(float)",
            "Value(integer)",
            "EnchantCap(integer)",
            "ArmorType(string)",
            "ArmorRating(integer)",
            "Durability(integer)", // NEW
        ] {
            assert!(out.contains(needle), "expected `{needle}` line in output");
        }

        let part_type = tes3::esp::BipedObjectType::Chest.display();
        assert!(out.contains(&format!("PartSlot_{part_type}_0_male(string)")));
        assert!(out.contains("A_Male_Part"));
        assert!(out.contains(&format!("PartSlot_{part_type}_0_female(string)")));
        assert!(out.contains("A_Female_Part"));

        assert!(
            out.ends_with("]\n\n"),
            "output must terminate with `]\\n\\n`"
        );
    }
}

impl WriteFGDProp for MiscItem {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "misc_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        let misc_flags = self
            .data
            .flags
            .iter_names()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
            .join(" | ");

        "MiscFlags".write_fgd(fgd_string, "", &misc_flags)?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_misc_item_fgd {
    use super::*;
    use tes3::esp::{MiscItemData, MiscItemFlags, ObjectFlags};

    fn serialize(misc: &MiscItem) -> String {
        let mut buf = Vec::<u8>::new();
        misc.write_fgd(&mut buf, "test_plugin.esp", None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_misc_item_fgd() {
        let misc = MiscItem {
            id: "TestMiscItem".into(),
            mesh: "Meshes\\Misc\\foo.nif".into(),
            script: "foo_script".into(),
            name: "Foo Item".into(),
            icon: "Icons\\foo.dds".into(),
            flags: ObjectFlags::empty(),
            data: MiscItemData {
                weight: 1.25,
                value: 42,
                flags: MiscItemFlags::empty(),
            },
        };

        println!("{}", serialize(&misc)); // Manual visual inspection
    }

    #[test]
    fn misc_item_serialization_core_fields() {
        use tes3::esp::{MiscItem, MiscItemData, MiscItemFlags};

        let misc = MiscItem {
            id: "Test_Item".into(),
            mesh: "Meshes\\Test\\t.nif".into(),
            script: "test_script".into(),
            name: "Test Item".into(),
            icon: "Icons\\test.dds".into(),
            flags: ObjectFlags::all(),
            data: MiscItemData {
                weight: 2.5,
                value: 100,
                flags: MiscItemFlags::KEY,
            },
        };

        let out = serialize(&misc);

        eprintln!("{out}");

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "Icon(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "MiscFlags(string)",
        ] {
            assert!(
                out.contains(field),
                "expected field `{field}` in FGD output"
            );
        }

        assert!(
            out.contains("DELETED"),
            "expected `DELETED` in ObjectFlags string"
        );

        assert!(
            out.contains("IGNORED"),
            "expected `IGNORED` in ObjectFlags string"
        );

        assert!(
            out.contains("MODIFIED"),
            "expected `MODIFIED` in ObjectFlags string"
        );

        assert!(
            out.contains("PERSISTENT"),
            "expected `PERSISTENT` in ObjectFlags string"
        );

        assert!(
            out.contains("BLOCKED"),
            "expected `BLOCKED` in ObjectFlags string"
        );

        assert!(out.contains("KEY"), "expected `KEY` in MiscFlags string");

        assert!(
            out.ends_with("]\n\n"),
            "expected output to end with closing `]` followed by two newlines"
        );
    }
}

impl WriteFGDProp for Lockpick {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "lockpick_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;
        self.data.quality.write_fgd(fgd_string, "Quality", "")?;
        self.data.uses.write_fgd(fgd_string, "Uses", "")?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_lockpick_fgd {
    use super::*;
    use tes3::esp::{Lockpick, LockpickData, ObjectFlags};

    fn serialize(lockpick: &Lockpick) -> String {
        let mut buf = Vec::<u8>::new();
        lockpick
            .write_fgd(&mut buf, "lockpick_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_lockpick_fgd() {
        let lockpick = Lockpick {
            id: "TestLockpick".into(),
            mesh: "Meshes\\Lockpicks\\test_lock.nif".into(),
            script: "LockScript".into(),
            name: "Test Lockpick".into(),
            icon: "Icons\\Lockpicks\\test_icon.dds".into(),
            flags: ObjectFlags::empty(),
            data: LockpickData {
                weight: 0.2,
                value: 12,
                quality: 1.75,
                uses: 20,
            },
        };

        println!("{}", serialize(&lockpick));
    }

    #[test]
    fn lockpick_fgd_contains_expected_fields() {
        let lockpick = Lockpick {
            id: "Lockpick_X".into(),
            mesh: "Meshes\\L\\l.nif".into(),
            script: "L_Script".into(),
            name: "Pick of Locks".into(),
            icon: "Icons\\L\\i.dds".into(),
            flags: ObjectFlags::empty(),
            data: LockpickData {
                weight: 0.9,
                value: 50,
                quality: 2.5,
                uses: 40,
            },
        };

        let out = serialize(&lockpick);

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "Icon(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "Quality(float)",
            "Uses(integer)",
        ] {
            assert!(
                out.contains(field),
                "expected field `{field}` in FGD output"
            );
        }

        assert!(
            out.ends_with("]\n\n"),
            "expected FGD output to end with a closing bracket followed by two newlines"
        );
    }
}

impl WriteFGDProp for Probe {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "probe_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;
        self.data.quality.write_fgd(fgd_string, "Quality", "")?;
        self.data.uses.write_fgd(fgd_string, "Uses", "")?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_probe_fgd {
    use super::*;
    use tes3::esp::{ObjectFlags, Probe, ProbeData};

    fn serialize(probe: &Probe) -> String {
        let mut buf = Vec::new();
        probe
            .write_fgd(&mut buf, "probe_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_probe_fgd() {
        let probe = Probe {
            id: "ProbeMaster3000".into(),
            mesh: "Meshes\\Probes\\probe.nif".into(),
            script: "ProbeScript".into(),
            name: "Probe of Truth".into(),
            icon: "Icons\\Probes\\probe_icon.dds".into(),
            flags: ObjectFlags::empty(),
            data: ProbeData {
                weight: 0.3,
                value: 60,
                quality: 2.2,
                uses: 15,
            },
        };

        println!("{}", serialize(&probe));
    }

    #[test]
    fn probe_fgd_contains_expected_fields() {
        let probe = Probe {
            id: "Probe_X".into(),
            mesh: "Meshes\\P\\probe.nif".into(),
            script: "P_Script".into(),
            name: "PickProbe".into(),
            icon: "Icons\\P\\icon.dds".into(),
            flags: ObjectFlags::empty(),
            data: ProbeData {
                weight: 0.5,
                value: 42,
                quality: 1.7,
                uses: 10,
            },
        };

        let out = serialize(&probe);

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "Icon(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "Quality(float)",
            "Uses(integer)",
        ] {
            assert!(
                out.contains(field),
                "expected field `{field}` in FGD output"
            );
        }

        assert!(
            out.ends_with("]\n\n"),
            "expected FGD output to end with a closing bracket followed by two newlines"
        );
    }
}

impl WriteFGDProp for RepairItem {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "repairitem_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;
        self.data.quality.write_fgd(fgd_string, "Quality", "")?;
        self.data.uses.write_fgd(fgd_string, "Uses", "")?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}
#[cfg(test)]
mod test_repair_item_fgd {
    use super::*;
    use tes3::esp::{ObjectFlags, RepairItem, RepairItemData};

    fn serialize(repair_item: &RepairItem) -> String {
        let mut buf = Vec::new();
        repair_item
            .write_fgd(&mut buf, "repair_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_repair_item_fgd() {
        let repair = RepairItem {
            id: "FixIt9000".into(),
            mesh: "Meshes\\Repair\\fixer.nif".into(),
            script: "FixScript".into(),
            name: "Fixer Tool".into(),
            icon: "Icons\\Repair\\icon.dds".into(),
            flags: ObjectFlags::empty(),
            data: RepairItemData {
                weight: 1.2,
                value: 75,
                quality: 3.5,
                uses: 20,
            },
        };

        println!("{}", serialize(&repair));
    }

    #[test]
    fn repair_item_fgd_contains_expected_fields() {
        let repair = RepairItem {
            id: "Repair_X".into(),
            mesh: "Meshes\\Fix\\fix.nif".into(),
            script: "Repair_Script".into(),
            name: "RepairTool".into(),
            icon: "Icons\\Fix\\icon.dds".into(),
            flags: ObjectFlags::empty(),
            data: RepairItemData {
                weight: 0.9,
                value: 33,
                quality: 1.1,
                uses: 8,
            },
        };

        let out = serialize(&repair);

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "Icon(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "Quality(float)",
            "Uses(integer)",
        ] {
            assert!(
                out.contains(field),
                "expected field `{field}` in FGD output"
            );
        }

        assert!(
            out.ends_with("]\n\n"),
            "expected FGD output to end with a closing bracket followed by two newlines"
        );
    }
}

impl WriteFGDProp for Door {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "door_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "SoundClose".write_fgd(fgd_string, "", &self.close_sound)?;
        "SoundOpen".write_fgd(fgd_string, "", &self.open_sound)?;
        write_object_flags(fgd_string, &self.flags)?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_door_fgd {
    use super::*;
    use tes3::esp::{Door, ObjectFlags};

    fn serialize(door: &Door) -> String {
        let mut buf = Vec::new();
        door.write_fgd(&mut buf, "door_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_door_fgd() {
        let door = Door {
            id: "SecretDoor01".into(),
            mesh: "Meshes\\Doors\\secret.nif".into(),
            script: "OpenSecretScript".into(),
            name: "Secret Door".into(),
            flags: ObjectFlags::empty(),
            open_sound: "OpenSFX".into(),
            close_sound: "CloseSFX".into(),
        };

        println!("{}", serialize(&door));
    }

    #[test]
    fn door_fgd_contains_expected_fields() {
        let door = Door {
            id: "MyDoor".into(),
            mesh: "Meshes\\Doors\\mydoor.nif".into(),
            script: "DoorScript".into(),
            name: "Fancy Door".into(),
            flags: ObjectFlags::empty(),
            open_sound: "OpenDoorSound".into(),
            close_sound: "CloseDoorSound".into(),
        };

        let out = serialize(&door);

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "ObjectFlags(string)",
            "SoundOpen(string)",
            "SoundClose(string)",
        ] {
            assert!(
                out.contains(field),
                "expected field `{field}` in FGD output"
            );
        }

        assert!(
            out.ends_with("]\n\n"),
            "expected FGD output to end with a closing bracket followed by two newlines"
        );
    }
}

impl WriteFGDProp for Container {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "container_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        write_object_flags(fgd_string, &self.flags)?;

        self.encumbrance.write_fgd(fgd_string, "Capacity", "")?;
        let container_flags = self
            .flags
            .iter_names()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
            .join(" | ");

        "ContainerFlags".write_fgd(fgd_string, "", &container_flags)?;

        self.inventory
            .iter()
            .enumerate()
            .try_for_each(|(idx, (count, id))| {
                format!("InventoryItemID_{idx}").write_fgd(fgd_string, "", id)?;
                count.write_fgd(
                    fgd_string,
                    &format!("InventoryItemCount_{idx}"),
                    &String::default(),
                )
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tes3::esp::{Container, ContainerFlags, ObjectFlags};

    fn mock_container() -> Container {
        Container {
            id: "Crate_Misc01".into(),
            name: "Wooden Crate".into(),
            mesh: "Meshes\\Crate01.nif".into(),
            script: "OpenCrateScript".into(),
            flags: ObjectFlags::DELETED | ObjectFlags::PERSISTENT,
            container_flags: ContainerFlags::all(),
            inventory: vec![
                (5, "misc_com_bottle_01".to_string().into()),
                (1, "ingred_comberry_01".to_string().into()),
            ],
            encumbrance: 150.0,
        }
    }

    #[test]
    fn test_container_fgd_output() {
        let container = mock_container();
        let mut buffer = Cursor::new(Vec::new());

        container
            .write_fgd(&mut buffer, "TestPlugin.esp", Some(&[0; 6]))
            .unwrap();

        let output = String::from_utf8(buffer.into_inner()).unwrap();

        eprintln!("{output}");

        assert!(output.contains(
            "@PointClass base(world_Base) size(0 0 0, 0 0 0) color(0 0 0) = container_crate_misc01"
        ));
        assert!(output.contains("Plugin(string): \"\": \"TestPlugin.esp\""));
        assert!(output.contains("Model(string): \"\": \"Meshes\\Crate01.nif\""));
        assert!(output.contains("Script(string): \"\": \"OpenCrateScript\""));
        assert!(output.contains("Name(string): \"\": \"Wooden_x20_Crate\""));
        assert!(output.contains("ObjectFlags(string): \"\": \"DELETED | PERSISTENT\""));
        assert!(output.contains("Capacity(float): \"\": \"150\""));
        assert!(output.contains("InventoryItemID_0(string): \"\": \"misc_com_bottle_01\""));
        assert!(output.contains("InventoryItemCount_0(integer): \"\": 5"));
        assert!(output.contains("InventoryItemID_1(string): \"\": \"ingred_comberry_01\""));
        assert!(output.contains("InventoryItemCount_1(integer): \"\": 1"));
    }
}

impl WriteFGDProp for Book {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "book_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        write_object_flags(fgd_string, &self.flags)?;
        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Enchantment".write_fgd(fgd_string, "", &self.enchanting)?;
        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;
        "BookType".write_fgd(fgd_string, "", self.data.book_type.display())?;

        if self.data.skill != SkillId::None {
            "SkillId".write_fgd(fgd_string, "", self.data.skill.display())?;
        }

        self.data
            .enchantment
            .write_fgd(fgd_string, "EnchantCap", "")?;

        "BookText".write_fgd(fgd_string, "", self.data.book_type.display())?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_book_fgd {
    use super::*;
    use tes3::esp::{Book, BookData, ObjectFlags, SkillId};

    /// Helper: run the serializer and return UTF‑8 string
    fn serialize(book: &Book) -> String {
        let mut buf = Vec::<u8>::new();
        book.write_fgd(&mut buf, "book_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_book_fgd() {
        let book = Book {
            id: "bk_mysticism_guide".into(),
            mesh: "Meshes\\Books\\bk_myst.nif".into(),
            script: "BookOpenScript".into(),
            name: "Mysticism Guide".into(),
            icon: "Icons\\Books\\myst.dds".into(),
            enchanting: "MysticismEnch".into(),
            text: "The arcane art of Mysticism...".into(),
            flags: ObjectFlags::all(),
            data: BookData {
                weight: 3.0,
                value: 125,
                book_type: tes3::esp::BookType::Scroll,
                skill: SkillId::Mysticism,
                enchantment: 40,
            },
        };

        println!("{}", serialize(&book));
    }

    #[test]
    fn book_fgd_contains_expected_fields() {
        let book = Book {
            id: "bk_blank".into(),
            mesh: "Meshes\\Books\\bk_blank.nif".into(),
            script: String::new(),
            name: "Blank Book".into(),
            icon: "Icons\\Books\\blank.dds".into(),
            enchanting: String::new(),
            text: "Nothing is written here.".into(),
            flags: ObjectFlags::empty(),
            data: BookData {
                weight: 0.5,
                value: 5,
                book_type: tes3::esp::BookType::Book,
                skill: SkillId::None,
                enchantment: 0,
            },
        };

        let out = serialize(&book);

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Icon(string)",
            "Script(string)",
            "Name(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "BookType(string)",
            "EnchantCap(integer)",
        ] {
            assert!(out.contains(field), "expected `{field}` in FGD output");
        }

        assert!(
            !out.contains("SkillId(string)"),
            "SkillId line should be omitted when skill is None"
        );

        assert!(
            out.ends_with("]\n\n"),
            "output must end with closing bracket followed by two newlines"
        );
    }

    #[test]
    fn book_fgd_includes_skill_id_when_set() {
        let book = Book {
            id: "bk_skill_mysticism".into(),
            mesh: String::new(),
            script: String::new(),
            name: "Skill Book: Mysticism".into(),
            icon: String::new(),
            enchanting: String::new(),
            text: String::new(),
            flags: ObjectFlags::empty(),
            data: BookData {
                weight: 1.0,
                value: 50,
                book_type: tes3::esp::BookType::Book,
                skill: SkillId::Mysticism, // Important: non-None
                enchantment: 0,
            },
        };

        let out = serialize(&book);

        assert!(
            out.contains("SkillId(string)"),
            "expected `SkillId(string)` in FGD output when skill is set"
        );
    }
}

impl WriteFGDProp for Alchemy {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "potion_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;

        "AlchemyFlags".write_fgd(
            fgd_string,
            &String::default(),
            &self
                .data
                .flags
                .iter_names()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
                .join(" | "),
        )?;

        self.effects
            .iter()
            .enumerate()
            .try_for_each(|(idx, effect)| {
                if effect.magic_effect != tes3::esp::EffectId2::None {
                    writeln!(
                        fgd_string,
                        "    EffectId_{idx}(string) : \"\" : \"{}\"",
                        effect.magic_effect.display()
                    )?;

                    format!("EffectRange_{idx}").write_fgd(
                        fgd_string,
                        "",
                        effect.range.display(),
                    )?;

                    if effect.range != EffectRange::OnSelf {
                        effect.area.write_fgd(
                            fgd_string,
                            format!("EffectArea_{idx}"),
                            String::default(),
                        )?;
                    }

                    effect.duration.write_fgd(
                        fgd_string,
                        format!("EffectDuration_{idx}"),
                        String::default(),
                    )?;

                    match effect.magic_effect {
                        EffectId2::AbsorbAttribute
                        | EffectId2::DamageAttribute
                        | EffectId2::FortifyAttribute
                        | EffectId2::RestoreAttribute
                        | EffectId2::DrainAttribute => {
                            writeln!(
                                fgd_string,
                                "    AttributeId_{idx}(string) : \"\" :  \"{}\"",
                                effect.attribute.display()
                            )?;
                        }
                        EffectId2::AbsorbSkill
                        | EffectId2::DamageSkill
                        | EffectId2::FortifySkill
                        | EffectId2::RestoreSkill
                        | EffectId2::DrainSkill => {
                            writeln!(
                                fgd_string,
                                "    SkillId_{idx}(string) : \"\" :  \"{}\"",
                                effect.skill.display()
                            )?;
                        }
                        _ => {}
                    }
                }

                Ok(())
            })
            .map_err(|err: std::io::Error| {
                io::Error::new(io::ErrorKind::InvalidData, err.to_string())
            })?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_potion_fgd {
    use super::*;
    use tes3::esp::{
        Alchemy, AlchemyData, AlchemyFlags, Effect, EffectId2, EffectRange, ObjectFlags,
    };

    fn serialize(potion: &Alchemy) -> String {
        let mut buf = Vec::<u8>::new();
        potion
            .write_fgd(&mut buf, "potion_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_potion_fgd() {
        let potion = Alchemy {
            id: "PotionHealth".into(),
            mesh: "meshes\\potions\\health.nif".into(),
            icon: "icons\\potions\\health.dds".into(),
            script: String::new(),
            name: "Potion of Healing".into(),
            flags: ObjectFlags::empty(),
            data: AlchemyData {
                weight: 0.5,
                value: 25,
                flags: AlchemyFlags::all(),
            },
            effects: vec![Effect {
                magic_effect: EffectId2::RestoreHealth,
                skill: tes3::esp::SkillId2::default(),
                attribute: tes3::esp::AttributeId2::default(),
                range: EffectRange::OnSelf,
                area: 0,
                duration: 5,
                min_magnitude: 5,
                max_magnitude: 10,
            }],
        };

        println!("{}", serialize(&potion));
    }

    #[test]
    fn potion_fgd_contains_basic_fields() {
        let potion = Alchemy {
            id: "Potion001".into(),
            mesh: "m\\p001.nif".into(),
            script: "PotionScript".into(),
            name: "Test Potion".into(),
            icon: "icons\\potions\\health.dds".into(),
            flags: ObjectFlags::all(),
            data: AlchemyData {
                weight: 0.1,
                value: 10,
                flags: AlchemyFlags::all(),
            },
            effects: vec![],
        };

        let out = serialize(&potion);
        eprintln!("{out}");

        for field in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "AlchemyFlags(string)",
        ] {
            assert!(
                out.contains(field),
                "expected field `{field}` in FGD output"
            );
        }

        assert!(
            out.contains("AUTO_CALCULATE"),
            "expected `AUTO_CALCULATE` in AlchemyFlags"
        );
    }

    #[test]
    fn potion_fgd_writes_restore_health_effect() {
        let potion = Alchemy {
            id: "HealthRestore".into(),
            mesh: String::new(),
            script: String::new(),
            name: String::new(),
            flags: ObjectFlags::empty(),
            icon: "icons\\potions\\health.dds".into(),
            data: AlchemyData {
                weight: 0.1,
                value: 10,
                flags: AlchemyFlags::AUTO_CALCULATE,
            },
            effects: vec![Effect {
                magic_effect: EffectId2::RestoreHealth,
                skill: tes3::esp::SkillId2::default(),
                attribute: tes3::esp::AttributeId2::default(),
                range: EffectRange::OnSelf,
                area: 0,
                duration: 2,
                min_magnitude: 5,
                max_magnitude: 10,
            }],
        };

        let out = serialize(&potion);

        assert!(
            out.contains("EffectId_0(string)"),
            "expected effect string for first effect"
        );

        assert!(
            out.contains("RestoreHealth"),
            "expected human-readable effect name"
        );

        assert!(
            out.contains("EffectDuration_0(integer)"),
            "expected duration field for effect"
        );

        assert!(
            !out.contains("EffectArea_0(float)"),
            "did not expect area for OnSelf"
        );
    }

    #[test]
    fn potion_fgd_serializes_attribute_effect() {
        let potion = Alchemy {
            id: "PotionStr".into(),
            icon: "icons\\potions\\health.dds".into(),
            mesh: String::new(),
            script: String::new(),
            name: String::new(),
            flags: ObjectFlags::empty(),
            data: AlchemyData {
                weight: 0.1,
                value: 10,
                flags: AlchemyFlags::empty(),
            },
            effects: vec![Effect {
                magic_effect: EffectId2::FortifyAttribute,
                skill: tes3::esp::SkillId2::default(),
                attribute: tes3::esp::AttributeId2::Strength,
                range: EffectRange::OnTouch,
                area: 5,
                duration: 3,
                min_magnitude: 2,
                max_magnitude: 4,
            }],
        };

        let out = serialize(&potion);

        assert!(
            out.contains("AttributeId_0(string)"),
            "expected attribute field for applicable effect"
        );

        assert!(out.contains("Strength"), "expected readable attribute name");

        assert!(
            out.contains("EffectArea_0(integer)"),
            "expected area field for non-OnSelf range"
        );
    }

    #[test]
    fn potion_fgd_serializes_skill_effect() {
        let potion = Alchemy {
            id: "PotionAcrobatics".into(),
            mesh: String::new(),
            icon: "icons\\potions\\health.dds".into(),
            script: String::new(),
            name: String::new(),
            flags: ObjectFlags::empty(),
            data: AlchemyData {
                weight: 0.1,
                value: 10,
                flags: AlchemyFlags::empty(),
            },
            effects: vec![Effect {
                magic_effect: EffectId2::DamageSkill,
                skill: tes3::esp::SkillId2::Acrobatics,
                attribute: tes3::esp::AttributeId2::default(),
                range: EffectRange::OnTarget,
                area: 10,
                duration: 2,
                min_magnitude: 3,
                max_magnitude: 5,
            }],
        };

        let out = serialize(&potion);

        assert!(
            out.contains("SkillId_0(string)"),
            "expected skill field for applicable effect"
        );

        assert!(out.contains("Acrobatics"), "expected readable skill name");
    }

    #[test]
    fn potion_fgd_ignores_none_effects() {
        let potion = Alchemy {
            id: "NoneEffectPotion".into(),
            icon: "icons\\potions\\health.dds".into(),
            mesh: String::new(),
            script: String::new(),
            name: String::new(),
            flags: ObjectFlags::empty(),
            data: AlchemyData {
                weight: 0.1,
                value: 10,
                flags: AlchemyFlags::empty(),
            },
            effects: vec![Effect {
                magic_effect: EffectId2::None,
                skill: tes3::esp::SkillId2::default(),
                attribute: tes3::esp::AttributeId2::default(),
                range: EffectRange::OnSelf,
                area: 0,
                duration: 0,
                min_magnitude: 0,
                max_magnitude: 0,
            }],
        };

        let out = serialize(&potion);

        assert!(
            !out.contains("EffectId_0"),
            "should not write effect data for None effect"
        );
    }
}

impl WriteFGDProp for Apparatus {
    fn write_fgd<W: Write>(
        &self,
        fgd_string: &mut W,
        parent_plugin: &str,
        bounds: Option<&[i32; 6]>,
    ) -> Result<(), io::Error> {
        write_point_class(
            fgd_string,
            &["world_Base"],
            bounds,
            format!(
                "apparatus_{}",
                self.editor_id_ascii_lowercase().replace(' ', "_")
            ),
        )?;

        writeln!(fgd_string, "[")?;

        "Plugin".write_fgd(fgd_string, "", parent_plugin)?;
        "Model".write_fgd(fgd_string, "", &self.mesh)?;
        "Script".write_fgd(fgd_string, "", &self.script)?;
        "Name".write_fgd(fgd_string, "", &encode_fgd_token(&self.name))?;
        "Icon".write_fgd(fgd_string, "", &self.icon)?;
        write_object_flags(fgd_string, &self.flags)?;

        self.data.weight.write_fgd(fgd_string, "Weight", "")?;
        self.data.value.write_fgd(fgd_string, "Value", "")?;
        self.data.quality.write_fgd(fgd_string, "Quality", "")?;
        "ApparatusType".write_fgd(fgd_string, "", self.data.apparatus_type.display())?;

        writeln!(fgd_string, "]\n")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_apparatus_fgd {
    use super::*;
    use tes3::esp::{Apparatus, ApparatusData, ApparatusType, ObjectFlags};

    fn serialize(app: &Apparatus) -> String {
        let mut buf = Vec::<u8>::new();
        app.write_fgd(&mut buf, "apparatus_test_plugin.esp", None)
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn print_apparatus_fgd() {
        let apparatus = Apparatus {
            id: "App_Retort_Dwemer".into(),
            mesh: "Meshes\\Apparatus\\retort_dwm.nif".into(),
            script: "RetortScript".into(),
            name: "Dwemer Retort".into(),
            icon: "Icons\\App\\retort.dds".into(),
            flags: ObjectFlags::all(),
            data: ApparatusData {
                weight: 2.0,
                value: 200,
                quality: 1.5,
                apparatus_type: ApparatusType::Retort,
            },
        };

        println!("{}", serialize(&apparatus));
    }

    #[test]
    fn apparatus_fgd_contains_expected_fields() {
        let app = Apparatus {
            id: "Mortar01".into(),
            mesh: "Meshes\\Apparatus\\mortar01.nif".into(),
            script: String::new(),
            name: "Mortar & Pestle".into(),
            icon: "Icons\\App\\mortar.dds".into(),
            flags: ObjectFlags::all(),
            data: ApparatusData {
                weight: 3.2,
                value: 120,
                quality: 0.8,
                apparatus_type: ApparatusType::MortarAndPestle,
            },
        };

        let out = serialize(&app);

        for label in [
            "Plugin(string)",
            "Model(string)",
            "Script(string)",
            "Name(string)",
            "Icon(string)",
            "ObjectFlags(string)",
            "Weight(float)",
            "Value(integer)",
            "Quality(float)",
            "ApparatusType(string)",
        ] {
            assert!(out.contains(label), "expected `{label}` in FGD output");
        }

        assert!(out.contains("MortarAndPestle"));

        assert!(out.ends_with("]\n\n"), "FGD output must end with `]\\n\\n`");
    }
}
