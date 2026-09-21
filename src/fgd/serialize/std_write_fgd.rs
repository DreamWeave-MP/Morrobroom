use std::{
    collections::HashMap,
    io::{self, Write},
};
pub trait STDWriteFGD {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error>;
}

impl STDWriteFGD for &str {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(string): \"{}\": \"{}\"",
            self,
            description.as_ref(),
            default.as_ref()
        )
    }
}

impl STDWriteFGD for &String {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(string): \"{}\": \"{}\"",
            self,
            description.as_ref(),
            default.as_ref()
        )
    }
}

impl STDWriteFGD for String {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(string): \"{}\": \"{}\"",
            self,
            description.as_ref(),
            default.as_ref()
        )
    }
}

#[cfg(test)]
mod test_str_fgd {
    use super::*;

    /// Helper to serialize a string key with description/default, and get result as UTF-8 string.
    fn serialize_to_string(key: &str, description: &str, default: &str) -> String {
        let mut buf = Vec::<u8>::new();
        key.write_fgd(&mut buf, description, default).unwrap();
        String::from_utf8(buf).expect("Output should be valid UTF-8")
    }

    #[test]
    fn test_str_fgd_basic() {
        let out = serialize_to_string("Name", "An entity name", "default_name");
        assert!(out.contains("    Name(string): \"An entity name\": \"default_name\""));
    }

    #[test]
    fn test_str_fgd_empty_default() {
        let out = serialize_to_string("Tag", "Optional label", "");
        assert!(out.contains("    Tag(string): \"Optional label\": \"\""));
    }

    #[test]
    fn test_str_fgd_special_characters() {
        let out = serialize_to_string("Label", "This \"desc\" has quotes", "new\nline");
        assert!(out.contains("    Label(string): \"This \"desc\" has quotes\": \"new\nline\""));
        // We're not escaping quotes/newlines here; just verifying they pass through
    }
}

impl STDWriteFGD for f32 {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        key: S,
        description: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(float): \"{}\": \"{}\"",
            key.as_ref(),
            description.as_ref(),
            self,
        )
    }
}

impl STDWriteFGD for i32 {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        key: S,
        description: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(integer): \"{}\": {}",
            key.as_ref(),
            description.as_ref(),
            self,
        )
    }
}

impl STDWriteFGD for u32 {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        key: S,
        description: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(integer): \"{}\": {}",
            key.as_ref(),
            description.as_ref(),
            self,
        )
    }
}

impl STDWriteFGD for u16 {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        key: S,
        description: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(integer): \"{}\": {}",
            key.as_ref(),
            description.as_ref(),
            self,
        )
    }
}

impl STDWriteFGD for u8 {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        key: S,
        description: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(integer): \"{}\": {}",
            key.as_ref(),
            description.as_ref(),
            self,
        )
    }
}

#[cfg(test)]
mod test_numeric_fgd {
    use super::*; // bring STDWriteFGD implementations into scope

    /// Helper to run a single serialization and return the resulting string.
    fn serialize_to_string<T: STDWriteFGD>(value: &T, key: &str, desc: &str) -> String {
        let mut buf = Vec::<u8>::new();
        value.write_fgd(&mut buf, key, desc).unwrap();
        String::from_utf8(buf).expect("FGD output should be valid UTF‑8")
    }

    #[test]
    #[allow(
        clippy::approx_constant,
        reason = "This test intentionally serializes the exact value 3.14."
    )]
    fn test_f32_write_fgd() {
        let val: f32 = 3.14;
        let out = serialize_to_string(&val, "FloatVal", "Pi-ish");
        assert!(out.contains("FloatVal(float): \"Pi-ish\": \"3.14\""));
    }

    #[test]
    fn test_i32_write_fgd() {
        let val: i32 = -42;
        let out = serialize_to_string(&val, "IntVal", "Negative life");
        assert!(out.contains("IntVal(integer): \"Negative life\": -42"));
    }

    #[test]
    fn test_u32_write_fgd() {
        let val: u32 = 123_456;
        let out = serialize_to_string(&val, "U32Val", "Big number");
        assert!(out.contains("U32Val(integer): \"Big number\": 123456"));
    }

    #[test]
    fn test_u16_write_fgd() {
        let val: u16 = 65535;
        let out = serialize_to_string(&val, "U16Val", "Max u16");
        assert!(out.contains("U16Val(integer): \"Max u16\": 65535"));
    }

    #[test]
    fn test_u8_write_fgd() {
        let val: u8 = 255;
        let out = serialize_to_string(&val, "U8Val", "Max byte");
        assert!(out.contains("U8Val(integer): \"Max byte\": 255"));
    }
}

impl STDWriteFGD for [u8; 4] {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        key: S,
        description: S,
    ) -> Result<(), io::Error> {
        let (r, g, b) = (
            f32::from(self[0]) / 255.,
            f32::from(self[1]) / 255.,
            f32::from(self[2]) / 255.,
        );

        writeln!(
            target,
            "    {}(color): \"{}\": \"{:.3} {:.3} {:.3}\"",
            key.as_ref(),
            description.as_ref(),
            r,
            g,
            b
        )
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    fn extract_color_floats(output: &str) -> Vec<f32> {
        output
            .split('"')
            .filter_map(|part| {
                if part.trim().contains('.') && part.contains(' ') {
                    Some(
                        part.split_whitespace()
                            .filter_map(|n| n.parse::<f32>().ok())
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                }
            })
            .flatten()
            .collect()
    }

    fn approx_eq(a: f32, b: f32, epsilon: f32) -> bool {
        (a - b).abs() < epsilon
    }

    #[test]
    fn test_color_precision_and_formatting() {
        let color = [128, 64, 255, 0]; // (0.502, 0.251, 1.0)
        let mut buffer = Vec::new();

        color.write_fgd(&mut buffer, "LightColor", "desc").unwrap();

        let out = String::from_utf8(buffer).unwrap();
        let floats = extract_color_floats(&out);

        assert_eq!(floats.len(), 3);
        assert!(approx_eq(floats[0], 128.0 / 255.0, 0.001));
        assert!(approx_eq(floats[1], 64.0 / 255.0, 0.001));
        assert!(approx_eq(floats[2], 255.0 / 255.0, 0.001));
    }

    #[test]
    fn test_black_and_white_colors() {
        let tests = vec![
            ([0, 0, 0, 0], vec![0.0, 0.0, 0.0]),
            ([255, 255, 255, 255], vec![1.0, 1.0, 1.0]),
        ];

        for (input, expected) in tests {
            let mut buf = Vec::new();
            input.write_fgd(&mut buf, "ColorTest", "desc").unwrap();
            let out = String::from_utf8(buf).unwrap();
            let floats = extract_color_floats(&out);

            assert_eq!(floats.len(), 3);
            for (f, &e) in floats.iter().zip(expected.iter()) {
                assert!(approx_eq(*f, e, 0.001));
            }
        }
    }
}

/// `HashMap` u32, String corresponds to a `Flags` type in FGD
impl STDWriteFGD for HashMap<u32, String> {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(Flags): \"{}\": \"\" =\n    [",
            description.as_ref(),
            default.as_ref()
        )?;

        for (k, v) in self {
            writeln!(target, "        \"{k}\": \"{v}\"")?;
        }

        writeln!(target, "    ]")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_hashmap_u32_string_fgd {
    use super::*;
    use std::collections::HashMap;

    fn serialize_flags_map(map: &HashMap<u32, String>, description: &str, default: &str) -> String {
        let mut buffer = Vec::new();
        map.write_fgd(&mut buffer, description, default).unwrap();
        String::from_utf8(buffer).expect("Output should be valid UTF-8")
    }

    #[test]
    fn test_flags_map_basic() {
        let mut flags = HashMap::new();
        flags.insert(1, "FlagOne".into());
        flags.insert(2, "FlagTwo".into());

        let output = serialize_flags_map(&flags, "SomeFlags", "none");

        assert!(output.contains("    SomeFlags(Flags): \"none\": \"\" ="));
        assert!(output.contains("        \"1\": \"FlagOne\""));
        assert!(output.contains("        \"2\": \"FlagTwo\""));
        assert!(output.contains("    ]"));
    }

    #[test]
    fn test_flags_map_empty() {
        let flags: HashMap<u32, String> = HashMap::new();
        let output = serialize_flags_map(&flags, "EmptyFlags", "default");

        assert!(output.contains("    EmptyFlags(Flags): \"default\": \"\" ="));
        assert!(output.contains("    ]"));
    }

    #[test]
    fn test_flags_map_with_zero_key() {
        let mut flags = HashMap::new();
        flags.insert(0, "None".into());

        let output = serialize_flags_map(&flags, "ZeroFlag", "unset");

        assert!(output.contains("        \"0\": \"None\""));
    }
}

impl STDWriteFGD for Vec<(String, u16)> {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(choices): \"{}\": \"\" =\n    [",
            description.as_ref(),
            default.as_ref()
        )?;

        for (k, v) in self {
            writeln!(target, "        \"{k}\": \"{v}\"")?;
        }

        writeln!(target, "    ]")?;

        Ok(())
    }
}

impl<D: std::fmt::Display> STDWriteFGD for Vec<(D, D)> {
    fn write_fgd<S: AsRef<str>, W: Write>(
        &self,
        target: &mut W,
        description: S,
        default: S,
    ) -> Result<(), io::Error> {
        writeln!(
            target,
            "    {}(choices): \"{}\": \"\" =\n    [",
            description.as_ref(),
            default.as_ref()
        )?;

        for (k, v) in self {
            writeln!(target, "        \"{k}\": \"{v}\"")?;
        }

        writeln!(target, "    ]")?;

        Ok(())
    }
}

#[cfg(test)]
mod test_vec_choices_fgd {
    use super::*;

    #[test]
    fn test_write_fgd_vec_choices_basic() {
        let vec = vec![("Option1", "First choice"), ("Option2", "Second choice")];

        let mut buffer = Vec::new();
        vec.write_fgd(&mut buffer, "MyVecChoices", "Pick one")
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("MyVecChoices(choices): \"Pick one\": \"\" ="));
        assert!(output.contains("        \"Option1\": \"First choice\""));
        assert!(output.contains("        \"Option2\": \"Second choice\""));
        assert!(output.contains("    ]"));
    }

    #[test]
    fn test_write_fgd_vec_choices_empty() {
        let vec: Vec<(String, String)> = vec![];
        let mut buffer = Vec::new();

        vec.write_fgd(&mut buffer, "EmptyVec", "Nothing to show")
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.contains("EmptyVec(choices): \"Nothing to show\": \"\" ="));
        assert!(output.contains("    ["));
        assert!(output.contains("    ]"));
    }

    #[test]
    fn test_write_fgd_vec_choices_special_chars() {
        let vec = vec![("Key \"1\"", "Value\nLine"), ("Esc\\Key", "Quote\"Inside")];

        let mut buffer = Vec::new();
        vec.write_fgd(&mut buffer, "SpecialChars", "Watch out")
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();

        assert!(output.contains("SpecialChars(choices): \"Watch out\": \"\" ="));
        assert!(output.contains("        \"Key \"1\"\": \"Value"));
        assert!(output.contains("        \"Esc\\Key\": \"Quote\"Inside\""));
    }
}
