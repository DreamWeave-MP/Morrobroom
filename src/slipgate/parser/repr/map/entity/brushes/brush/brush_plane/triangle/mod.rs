mod point;

pub use point::*;

use std::str::FromStr;

use nom::{
    Finish, IResult, Parser,
    character::complete::space1,
    error::Error,
    sequence::{preceded, terminated},
};

use crate::slipgate::repr::TrianglePlane;

impl FromStr for TrianglePlane {
    type Err = Error<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parse_triangle(s).finish() {
            Ok((_, o)) => Ok(o),
            Err(Error { input, code }) => Err(Error {
                input: input.to_string(),
                code,
            }),
        }
    }
}

/// Parse a [`Triangle`] from `&str`.
///
/// # Errors
///
/// Returns a parser error when the input does not contain three points.
pub fn parse_triangle(input: &str) -> IResult<&str, TrianglePlane> {
    let (i, (v0, v1, v2)) = (
        terminated(parse_point, space1),
        parse_point,
        preceded(space1, parse_point),
    )
        .parse(input)?;

    Ok((i, TrianglePlane { v0, v1, v2 }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::unit_test_data::{test_plane_in, test_plane_out};

    #[test]
    fn test_plane() {
        assert_eq!(parse_triangle(test_plane_in()), Ok(("", test_plane_out())));
    }
}
