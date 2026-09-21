use nom::{
    IResult, Parser,
    bytes::complete::tag,
    character::complete::space1,
    combinator::map_res,
    sequence::{delimited, preceded},
};

use crate::slipgate::{parser::primitive::parse_f32, repr::TexturePlane};

/// Parse a [`TexturePlane`] from `&str`
///
/// # Errors
///
/// Returns a parser error when the input is not a bracketed texture plane.
pub fn parse_texture_plane(input: &str) -> IResult<&str, TexturePlane> {
    map_res(
        (
            preceded(tag("[ "), parse_f32),
            preceded(space1, parse_f32),
            preceded(space1, parse_f32),
            delimited(space1, parse_f32, tag(" ]")),
        ),
        |(x, y, z, d)| Ok(TexturePlane { x, y, z, d }) as Result<TexturePlane, ()>,
    )
    .parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::unit_test_data::{test_texture_plane_in, test_texture_plane_out};

    #[test]
    fn test_texture_plane() {
        assert_eq!(
            parse_texture_plane(test_texture_plane_in()),
            Ok(("", test_texture_plane_out()))
        );
    }
}
