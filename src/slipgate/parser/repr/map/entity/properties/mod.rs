mod property;

pub use property::*;

use std::str::FromStr;

use nom::{
    Finish, IResult, Parser, character::complete::line_ending, error::Error, multi::separated_list0,
};

use crate::slipgate::repr::Properties;

impl FromStr for Properties {
    type Err = Error<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match parse_properties(s).finish() {
            Ok((_, o)) => Ok(o),
            Err(Error { input, code }) => Err(Error {
                input: input.to_string(),
                code,
            }),
        }
    }
}

/// Parse [`Properties`] from `&str`.
pub fn parse_properties(input: &str) -> IResult<&str, Properties> {
    let (i, o) = separated_list0(line_ending, parse_property).parse(input)?;

    Ok((i, Properties::new(o)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::unit_test_data::{test_properties_in, test_properties_out};

    #[test]
    fn test_properties() {
        assert_eq!(parse_properties(""), Ok(("", Properties::new(vec![]))));

        assert_eq!(
            parse_properties(test_properties_in()),
            Ok(("", test_properties_out()))
        );
    }
}
