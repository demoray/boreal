use std::num::{ParseFloatError, ParseIntError};

use nom::error::{ErrorKind as NomErrorKind, ParseError};

use super::types::{Input, Span};

#[derive(Debug)]
pub struct Error {
    errors: Vec<SingleError>,
}

impl Error {
    pub fn new(span: Span, kind: ErrorKind) -> Self {
        Self {
            errors: vec![SingleError { span, kind }],
        }
    }
}

impl ParseError<Input<'_>> for Error {
    fn from_error_kind(input: Input, kind: NomErrorKind) -> Self {
        Self {
            errors: vec![SingleError::from_nom_error_kind(input.get_position(), kind)],
        }
    }

    fn append(input: Input, kind: NomErrorKind, mut other: Self) -> Self {
        other
            .errors
            .push(SingleError::from_nom_error_kind(input.get_position(), kind));
        other
    }
}

#[derive(Debug)]
struct SingleError {
    span: Span,

    kind: ErrorKind,
}

impl SingleError {
    fn from_nom_error_kind(position: usize, kind: NomErrorKind) -> Self {
        Self {
            span: Span {
                start: position,
                end: position + 1,
            },
            kind: ErrorKind::NomError(kind),
        }
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    /// A base64 modifier alphabet has an invalid length.
    ///
    /// The length must be 64.
    Base64AlphabetInvalidLength { length: usize },

    /// Empty regex declaration, forbidden
    EmptyRegex,

    /// Expression with an invalid type
    ExpressionInvalidType {
        /// Type of the expression
        ty: String,
        /// Expected type
        expected_type: String,
    },

    /// Operands of an expression have incompatible types.
    ///
    /// The incompatibility is either between the two operands (e.g. integer
    /// and string) or with the operator (e.g. division between regexes).
    ExpressionIncompatibleTypes {
        /// Type of the left operand
        left_type: String,
        /// Span of the left operand
        left_span: Span,
        /// Type of the right operand
        right_type: String,
        /// Span of the right operand
        right_span: Span,
    },

    /// There are trailing data that could not be parsed.
    HasTrailingData,

    /// Jump of an empty size (i.e. `[0]`).
    JumpEmpty,

    /// Jump with a invalid range, ie `from` > `to`:
    JumpRangeInvalid { from: u32, to: u32 },

    /// Jump over a certain size used inside an alternation (`|`).
    JumpTooBigInAlternation {
        /// Maximum size of jumps (included).
        limit: u32,
    },

    /// Unbounded jump (`[-]`) used inside an alternation (`|`) in an hex string.
    JumpUnboundedInAlternation,

    /// Duplicated string modifiers
    ModifiersDuplicated {
        /// First modifier name
        modifier_name: String,
    },

    /// Incompatible string modifiers.
    ModifiersIncompatible {
        /// First modifier name
        first_modifier_name: String,
        /// Second modifier name
        second_modifier_name: String,
    },

    /// Overflow on a multiplication
    MulOverflow { left: i64, right: i64 },

    /// Generic error on nom parsing utilities
    NomError(NomErrorKind),

    /// Error converting a string to an float
    StrToFloatError(ParseFloatError),

    /// Error converting a string to an integer
    StrToIntError(ParseIntError),

    /// Error converting a string to an integer in base 16
    StrToHexIntError(ParseIntError),

    /// Error converting a string to an integer in base 16
    StrToOctIntError(ParseIntError),

    /// Multiple string declarations with the same name
    StringDeclarationDuplicated { name: String },

    /// A value used in a xor modifier range is outside the [0-255] range.
    XorRangeInvalidValue { value: i64 },

    /// Xor modifier with a invalid range, ie `from` > `to`:
    XorRangeInvalid { from: u8, to: u8 },
}
