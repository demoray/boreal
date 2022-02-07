//! Parsing methods for .yar files.
//!
//! This module mainly intends to match the lexical patterns used in libyara.
//!
//! All of the parsing functions, unless otherwise indicated, depends on the
//! following invariants:
//! - The received input has already been left-trimmed
//! - The returned input is right-trimmed
//! The [`nom_recipes::rtrim`] function is provided to make this easier.
//!
//! Progress:
//! [x] hex strings initial impl is complete, need integration testing.
//! [ ] re strings needs to be investigated.
//! [ ] yar files are in progress.
//!   lexer:
//!     [x] identifiers
//!     [x] strings
//!     [x] regexes
//!     [ ] includes
//!   parser:
//!     [ ] all
//!
//! TODO:
//! [ ] check error reporting
//! [ ] replace `from_external_error` with a custom err: the desc is dropped
//!     by nom...
use nom::Finish;

mod error;
pub use error::Error;
mod expression;
mod hex_string;
mod nom_recipes;
mod number;
mod rule;
mod string;
mod types;

/// Parse a YARA file.
///
/// Returns the list of rules declared in the file.
///
/// # Errors
///
/// Returns an error if the parsing fails, or if there are
/// trailing data in the file that has not been parsed.
pub fn parse_str(input: &str) -> Result<Vec<crate::rule::Rule>, Error> {
    let input = types::Input::new(input);

    let (input, rules) = rule::parse_yara_file(input).finish()?;

    if !input.cursor().is_empty() {
        let pos = input.get_position();

        return Err(error::Error::new(
            types::Span {
                start: pos,
                end: pos + 1,
            },
            error::ErrorKind::HasTrailingData,
        ));
    }

    Ok(rules)
}

#[cfg(test)]
mod tests;
