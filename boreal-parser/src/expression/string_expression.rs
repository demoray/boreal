//! Parsing related to expressions involving string count/offset/length.
//!
//! This implements the `string_count/offset/length` elements in grammar.y
//! in libyara.
use nom::{
    bytes::complete::tag,
    character::complete::char,
    combinator::{cut, opt},
    sequence::{delimited, preceded},
};

use super::{common::range, primary_expression::primary_expression, Expression, ParsedExpr, Type};
use crate::{
    nom_recipes::rtrim,
    string,
    types::{Input, ParseResult},
};

/// Parse a `string_count ( 'in' range )` expression
pub(super) fn string_count_expression(input: Input) -> ParseResult<ParsedExpr> {
    let start = input;
    let (input, variable_name) = string::count(input)?;
    let (input, range) = opt(preceded(rtrim(tag("in")), cut(range)))(input)?;

    let expr = match range {
        // string_count
        None => Expression::Count(variable_name),
        // string_count 'in' range
        Some((from, to)) => Expression::CountInRange {
            variable_name,
            from: from.unwrap_expr(Type::Integer)?,
            to: to.unwrap_expr(Type::Integer)?,
        },
    };
    Ok((
        input,
        ParsedExpr {
            expr,
            ty: Type::Integer,
            span: input.get_span_from(start),
        },
    ))
}

/// Parse a `string_offset ( '[' primary_expression ']' )` expression
pub(super) fn string_offset_expression(input: Input) -> ParseResult<ParsedExpr> {
    let start = input;
    let (input, variable_name) = string::offset(input)?;
    let (input, expr) = opt(delimited(
        rtrim(char('[')),
        cut(primary_expression),
        cut(rtrim(char(']'))),
    ))(input)?;

    let span = input.get_span_from(start);
    let expr = Expression::Offset {
        variable_name,
        occurence_number: match expr {
            Some(v) => v.unwrap_expr(Type::Integer)?,
            None => Box::new(ParsedExpr {
                expr: Expression::Number(1),
                ty: Type::Integer,
                span: span.clone(),
            }),
        },
    };
    Ok((
        input,
        ParsedExpr {
            expr,
            ty: Type::Integer,
            span,
        },
    ))
}

/// Parse a `string_length ( '[' primary_expression ']' )` expression
pub(super) fn string_length_expression(input: Input) -> ParseResult<ParsedExpr> {
    let start = input;
    let (input, variable_name) = string::length(input)?;
    let (input, expr) = opt(delimited(
        rtrim(char('[')),
        cut(primary_expression),
        cut(rtrim(char(']'))),
    ))(input)?;

    let span = input.get_span_from(start);
    let expr = Expression::Length {
        variable_name,
        occurence_number: match expr {
            Some(v) => v.unwrap_expr(Type::Integer)?,
            None => Box::new(ParsedExpr {
                expr: Expression::Number(1),
                ty: Type::Integer,
                span: span.clone(),
            }),
        },
    };
    Ok((
        input,
        ParsedExpr {
            expr,
            ty: Type::Integer,
            span,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{parse, parse_err};

    #[test]
    fn test_string_count_expression() {
        parse(
            string_count_expression,
            "#foo bar",
            "bar",
            ParsedExpr {
                expr: Expression::Count("foo".to_owned()),
                ty: Type::Integer,
                span: 0..4,
            },
        );
        parse(
            string_count_expression,
            "#foo in (0 ..filesize ) c",
            "c",
            ParsedExpr {
                expr: Expression::CountInRange {
                    variable_name: "foo".to_owned(),
                    from: Box::new(ParsedExpr {
                        expr: Expression::Number(0),
                        ty: Type::Integer,
                        span: 9..10,
                    }),
                    to: Box::new(ParsedExpr {
                        expr: Expression::Filesize,
                        ty: Type::Integer,
                        span: 13..21,
                    }),
                },
                ty: Type::Integer,
                span: 0..23,
            },
        );

        parse_err(string_count_expression, "");
        parse_err(string_count_expression, "foo");
    }

    #[test]
    fn test_string_offset_expression() {
        parse(
            string_offset_expression,
            "@a c",
            "c",
            ParsedExpr {
                expr: Expression::Offset {
                    variable_name: "a".to_owned(),
                    occurence_number: Box::new(ParsedExpr {
                        expr: Expression::Number(1),
                        ty: Type::Integer,
                        span: 0..2,
                    }),
                },
                ty: Type::Integer,
                span: 0..2,
            },
        );
        parse(
            string_offset_expression,
            "@a [ 2] c",
            "c",
            ParsedExpr {
                expr: Expression::Offset {
                    variable_name: "a".to_owned(),
                    occurence_number: Box::new(ParsedExpr {
                        expr: Expression::Number(2),
                        ty: Type::Integer,
                        span: 5..6,
                    }),
                },
                ty: Type::Integer,
                span: 0..7,
            },
        );
    }

    #[test]
    fn test_string_length_expression() {
        parse(
            string_length_expression,
            "!a c",
            "c",
            ParsedExpr {
                expr: Expression::Length {
                    variable_name: "a".to_owned(),
                    occurence_number: Box::new(ParsedExpr {
                        expr: Expression::Number(2),
                        ty: Type::Integer,
                        span: 0..2,
                    }),
                },
                ty: Type::Integer,
                span: 0..2,
            },
        );
        parse(
            string_length_expression,
            "!a [ 2] c",
            "c",
            ParsedExpr {
                expr: Expression::Length {
                    variable_name: "a".to_owned(),
                    occurence_number: Box::new(ParsedExpr {
                        expr: Expression::Number(2),
                        ty: Type::Integer,
                        span: 0..2,
                    }),
                },
                ty: Type::Integer,
                span: 0..7,
            },
        );

        parse_err(string_length_expression, "");
        parse_err(string_length_expression, "foo");
    }
}
