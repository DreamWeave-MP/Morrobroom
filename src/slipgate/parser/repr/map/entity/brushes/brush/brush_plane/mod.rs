mod extension;
mod texture_offset;
mod triangle;

pub use extension::*;
pub use texture_offset::*;
pub use triangle::*;

use std::str::FromStr;

use nom::{
    Finish, IResult, Parser,
    bytes::complete::take_until,
    character::complete::space1,
    combinator::opt,
    error::{Error, ErrorKind},
    sequence::terminated,
};

use crate::slipgate::{parser::primitive::parse_f32, repr::BrushPlane};

impl FromStr for BrushPlane {
    type Err = Error<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parse_brush_plane(s).finish() {
            Ok((_, o)) => Ok(o),
            Err(Error { input, code }) => Err(Error {
                input: input.to_string(),
                code,
            }),
        }
    }
}

/// Parse a [`BrushPlane`] from `&str`
///
/// # Errors
///
/// Returns a parser error when the input does not contain a complete brush plane.
pub fn parse_brush_plane(i: &str) -> IResult<&str, BrushPlane> {
    let (i, plane) = terminated(parse_triangle, space1).parse(i)?;
    let (i, texture) = terminated(parse_texture_name, space1).parse(i)?;
    let (i, texture_offset) = terminated(parse_texture_offset, space1).parse(i)?;
    let (i, angle) = terminated(parse_f32, space1).parse(i)?;
    let (i, scale_x) = terminated(parse_f32, space1).parse(i)?;
    let (i, scale_y) = terminated(parse_f32, opt(space1)).parse(i)?;
    let (i, extension) = parse_extension(i)?;

    Ok((
        i,
        BrushPlane {
            plane,
            texture,
            texture_offset,
            angle,
            scale_x,
            scale_y,
            extension,
        },
    ))
}

fn parse_texture_name(i: &str) -> IResult<&str, String> {
    let Some(input) = i.strip_prefix('"') else {
        let (i, texture) = take_until(" ").parse(i)?;
        return Ok((i, texture.to_owned()));
    };
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character != '"' {
            continue;
        }
        let (raw, remainder) = input.split_at(index);
        let remainder = &remainder[character.len_utf8()..];
        let mut texture = String::with_capacity(raw.len());
        escaped = false;
        for character in raw.chars() {
            if escaped {
                texture.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else {
                texture.push(character);
            }
        }
        if escaped {
            return Err(nom::Err::Error(Error::new(i, ErrorKind::Escaped)));
        }
        return Ok((remainder, texture));
    }
    Err(nom::Err::Error(Error::new(i, ErrorKind::Escaped)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::unit_test_data::{test_brush_plane_in, test_brush_plane_out};

    #[test]
    fn test_brush_plane() {
        assert_eq!(
            parse_brush_plane(test_brush_plane_in()),
            Ok(("", test_brush_plane_out()))
        );
    }

    #[test]
    fn quoted_texture_names_preserve_spaces_and_escapes() {
        let input = concat!(
            "( -16 -16 -16 ) ( -16 -15 -16 ) ( -16 -16 -15 ) ",
            "\"TX_B_Nigh Elf_M_H05\\\"test\" ",
            "[ 1 0 0 1 ] [ 0 -1 0 1 ] 0 1 1"
        );
        let (_, plane) = parse_brush_plane(input).expect("quoted material should parse");
        assert_eq!(plane.texture, "TX_B_Nigh Elf_M_H05\"test");
    }
}
