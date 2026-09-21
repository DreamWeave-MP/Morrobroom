mod texture_plane;

pub use texture_plane::*;

use nom::{
    IResult, Parser, branch::alt, character::complete::space1, combinator::map_res,
    sequence::separated_pair,
};

use crate::slipgate::{parser::primitive::parse_f32, repr::TextureOffset};

/// Parse a [`TextureOffset`] from `&str`
///
/// # Errors
///
/// Returns a parser error when the input is not a supported texture offset.
pub fn parse_texture_offset(input: &str) -> IResult<&str, TextureOffset> {
    alt((parse_texture_offset_standard, parse_texture_offset_valve)).parse(input)
}

/// Parse a [`TextureOffset::Standard`] from `&str`
///
/// # Errors
///
/// Returns a parser error when the input is not a pair of decimal offsets.
pub fn parse_texture_offset_standard(input: &str) -> IResult<&str, TextureOffset> {
    map_res(separated_pair(parse_f32, space1, parse_f32), |(u, v)| {
        Ok(TextureOffset::Standard { u, v }) as Result<TextureOffset, ()>
    })
    .parse(input)
}

/// Parse a [`TextureOffset::Valve`] from `&str`
///
/// # Errors
///
/// Returns a parser error when the input is not a pair of texture planes.
pub fn parse_texture_offset_valve(input: &str) -> IResult<&str, TextureOffset> {
    map_res(
        separated_pair(parse_texture_plane, space1, parse_texture_plane),
        |(u, v)| Ok(TextureOffset::Valve { u, v }) as Result<TextureOffset, ()>,
    )
    .parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::unit_test_data::{test_texture_offset_in, test_texture_offset_out};

    #[test]
    fn test_valve_texture_offset() {
        assert_eq!(
            parse_texture_offset_valve(test_texture_offset_in()),
            Ok(("", test_texture_offset_out()))
        );
    }
}
