mod brush;

pub use brush::*;

use std::str::FromStr;

use nom::{
    Finish, IResult, Parser, branch::alt, character::complete::line_ending, combinator::map_res,
    error::Error, multi::separated_list0,
};

use crate::slipgate::{
    parser::parse_eol_comment,
    repr::{Brush, Brushes},
};

impl FromStr for Brushes {
    type Err = Error<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parse_brushes(s).finish() {
            Ok((_, o)) => Ok(o),
            Err(Error { input, code }) => Err(Error {
                input: input.to_string(),
                code,
            }),
        }
    }
}

/// Parse [`Brushes`] from `&str`.
pub fn parse_brushes(input: &str) -> IResult<&str, Brushes> {
    let some_brush = map_res(parse_brush, |res| {
        Ok(Some(res)) as Result<Option<Brush>, ()>
    });
    let none_comment = map_res(parse_eol_comment, |_| Ok(None) as Result<Option<Brush>, ()>);
    let (i, o) = separated_list0(line_ending, alt((some_brush, none_comment))).parse(input)?;

    Ok((i, Brushes::new(o.into_iter().flatten().collect())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::unit_test_data::{test_brushes_in, test_brushes_out};

    #[test]
    fn test_brushes() {
        assert_eq!(parse_brushes(""), Ok(("", Brushes::new(vec![]))));

        assert_eq!(
            parse_brushes(&test_brushes_in()),
            Ok(("", test_brushes_out()))
        );
    }
}
