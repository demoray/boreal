//! Errors related to compilation of rules.
use std::ops::Range;

use codespan_reporting::diagnostic::{Diagnostic, Label};

use super::variable::VariableCompilationError;

/// Type of error while compiling a rule.
#[derive(Debug)]
pub enum CompilationError {
    /// Error compiling a regex
    RegexError {
        /// Error generated by the compilation.
        error: crate::regex::Error,
        /// Span of the regex expression in the input
        span: Range<usize>,
    },

    /// Expression with an invalid type
    ExpressionInvalidType {
        /// Type of the expression
        ty: String,
        /// Expected type
        expected_type: String,
        /// Span of the expression
        span: Range<usize>,
    },

    /// Operands of an expression have incompatible types.
    ///
    /// The incompatibility is either between the two operands (e.g. integer
    /// and string) or with the operator (e.g. division between regexes).
    ExpressionIncompatibleTypes {
        /// Type of the left operand
        left_type: String,
        /// Span of the left operand
        left_span: Range<usize>,
        /// Type of the right operand
        right_type: String,
        /// Span of the right operand
        right_span: Range<usize>,
    },

    /// Duplicated rule name in a namespace.
    DuplicatedRuleName {
        /// Name for the rule
        name: String,

        /// Span covering the rule name
        span: Range<usize>,
    },

    /// Duplicated tag in a rule.
    DuplicatedRuleTag {
        /// Name for the duplicated tag
        tag: String,

        /// Span of the first occurrence for the tag.
        span1: Range<usize>,

        /// Span of the second occurrence for the tag.
        span2: Range<usize>,
    },

    /// Duplicated binding on an identifier.
    ///
    /// This indicates that a for expression attempts to bind an identifier that is already
    /// bounded.
    ///
    /// For example:
    ///
    /// ```no_rust
    /// rule duplicated_binding {
    ///     condition:
    ///         for any i in (1..#a): (
    ///             for any i in (1..#b): (
    ///                 ...
    ///             )
    ///         )
    /// }
    /// ```
    DuplicatedIdentifierBinding {
        /// Name being duplicated.
        identifier: String,

        /// Span containing the duplicated binding.
        span: Range<usize>,
    },

    /// Duplicated variable names in a rule.
    ///
    /// The value is the name of the variable that appears more than once
    /// in the declarations.
    DuplicatedVariable {
        /// Variable name appearing multiple times
        name: String,

        /// Span of the second variable declaration with this name.
        span: Range<usize>,
    },

    /// An expression is too deep.
    ///
    /// Expressions are expressed as an AST tree, whose depth is limited.
    /// This error is raised when this limit is reached, and indicates that
    /// the expression is too complex.
    ///
    /// This should never be raised in user defined rules unless trying to
    /// generate a stack-overflow. However, if this happens on a legitimate
    /// rule, the limit can be raised using
    /// [`crate::compiler::CompilerParams::max_condition_depth`].
    ConditionTooDeep {
        /// Position of the expression that reaches max depth.
        span: Range<usize>,
    },

    /// Invalid binding of an identifier in a for expression.
    ///
    /// This indicates that the iterator items is a different cardinality from the bound identifiers.
    ///
    /// For example, `for any i in module.dictionary`, or `for any k, v in (0..3)`.
    InvalidIdentifierBinding {
        /// Actual number of identifiers to bind.
        actual_number: usize,
        /// Expected number of identifiers, i.e. cardinality of the iterator's items.
        expected_number: usize,
        /// Span of the identifiers
        identifiers_span: Range<usize>,
        /// Span of the iterator
        iterator_span: Range<usize>,
    },

    /// Invalid function call on an identifier
    InvalidIdentifierCall {
        /// Types of the provided arguments
        arguments_types: Vec<String>,
        /// The span of the function call.
        span: Range<usize>,
    },

    /// Invalid type for an expression used as an index in an identifier.
    ///
    /// For example, `pe.section[true]`
    InvalidIdentifierIndexType {
        /// Type of the expression
        ty: String,
        /// Span of the expression
        span: Range<usize>,
        /// Expected type for the index
        expected_type: String,
    },

    /// Invalid type for an identifier
    InvalidIdentifierType {
        /// Type of the identifier
        actual_type: String,
        /// The expected type
        expected_type: String,
        /// The span of the identifier with the wrong type.
        span: Range<usize>,
    },

    /// Invalid use of an identifier.
    ///
    /// This indicates either:
    ///
    /// - that an identifier with a compound type was used as a value in an expression.
    ///   For example, `pe.foo > 0`, where `pe.foo` is an array, a dictionary or a function.
    /// - that a rule identifier (so a boolean) was used as a compound type.
    ///   For example, `a.foo`, when `a` is the name of a rule, as `a` is a boolean.
    InvalidIdentifierUse {
        /// The span of the identifier that is not used correctly.
        span: Range<usize>,
    },

    /// A rule matching a previous wildcard rule set cannot be added.
    ///
    /// Rules that match previous wildcard rule set are not allowed in a namespace.
    ///
    /// For example:
    ///
    /// ```no_rust
    /// rule a0 { condition: true }
    /// rule b { condition: all of (a*) }
    /// rule a1 { condition: true } // This rule is not allowed
    /// ```
    MatchOnWildcardRuleSet {
        /// The name of the rule being rejected
        rule_name: String,
        /// The span for the name of the rule being rejected.
        name_span: Range<usize>,
        /// The corresponding wildcard rule set previously used in the namespace.
        rule_set: String,
    },

    /// An identifier used as an iterator is not iterable.
    ///
    /// When iterating on an identifier, only arrays and dictionaries are allowed.
    NonIterableIdentifier {
        /// The span of the identifier used as an iterator.
        span: Range<usize>,
    },

    /// Unknown identifier used in a rule.
    UnknownIdentifier {
        /// The name of the identifier that is not bound.
        name: String,
        /// Span of the identifier name
        span: Range<usize>,
    },

    /// Unknown import used in a file.
    ///
    /// The value is the name of the import that did not match any known module.
    UnknownImport {
        /// The name being imported.
        name: String,
        /// The span covering the import.
        span: Range<usize>,
    },

    /// Unknown field used in a identifier.
    UnknownIdentifierField {
        /// The name of the field that is unknown.
        field_name: String,
        /// Span of the field access
        span: Range<usize>,
    },

    /// Unknown variable used in a rule.
    UnknownVariable {
        /// Name of the variable
        variable_name: String,
        /// Span of the variable use in the condition
        span: Range<usize>,
    },

    /// A variable declared in a rule was not used.
    UnusedVariable {
        /// Name of the variable
        name: String,

        /// Span covering the declaration of the variable
        span: Range<usize>,
    },

    /// Error while compiling a variable, indicating an issue with
    /// its expression.
    VariableCompilation {
        /// Name of the variable
        variable_name: String,

        /// Span covering the declaration of the variable
        span: Range<usize>,

        /// Type of error
        error: VariableCompilationError,
    },

    // Errors classified as warnings
    /// A bytes value is used as a boolean expression.
    ImplicitBytesToBooleanCast {
        /// Span of the expression being casted.
        span: Range<usize>,
    },

    /// A non ascii character is present in a regex.
    RegexContainsNonAsciiChar {
        /// Span of the non the ascii byte in the input
        span: Range<usize>,
    },
}

impl CompilationError {
    /// Convert to a [`Diagnostic`].
    ///
    /// This can be used to display the error in a user-friendly manner.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic<()> {
        match self {
            Self::RegexError { error, span } => Diagnostic::error()
                .with_message(format!("regex failed to build: {error:?}"))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::ExpressionInvalidType {
                ty,
                expected_type,
                span,
            } => Diagnostic::error()
                .with_message("expression has an invalid type")
                .with_labels(vec![Label::primary((), span.clone())
                    .with_message(format!("expected {expected_type}, found {ty}"))]),

            Self::ExpressionIncompatibleTypes {
                left_type,
                left_span,
                right_type,
                right_span,
            } => Diagnostic::error()
                .with_message("expressions have invalid types")
                .with_labels(vec![
                    Label::secondary((), left_span.clone())
                        .with_message(format!("this has type {left_type}")),
                    Label::secondary((), right_span.clone())
                        .with_message(format!("this has type {right_type}")),
                ]),

            Self::DuplicatedRuleName { name, span } => Diagnostic::error()
                .with_message(format!(
                    "rule `{name}` is already declared in this namespace"
                ))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::DuplicatedRuleTag { tag, span1, span2 } => Diagnostic::error()
                .with_message(format!("tag `{tag}` specified multiple times"))
                .with_labels(vec![
                    Label::secondary((), span1.clone()).with_message("first occurrence"),
                    Label::secondary((), span2.clone()).with_message("second occurrence"),
                ]),

            Self::DuplicatedVariable { name, span } => Diagnostic::error()
                .with_message(format!("variable ${name} is declared more than once"))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::DuplicatedIdentifierBinding { identifier, span } => Diagnostic::error()
                .with_message(format!("duplicated loop identifier {identifier}"))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::ConditionTooDeep { span } => Diagnostic::error()
                .with_message("condition is too complex and reached max depth".to_owned())
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::InvalidIdentifierIndexType {
                ty,
                span,
                expected_type,
            } => Diagnostic::error()
                .with_message(format!("expected an expression of type {expected_type}"))
                .with_labels(vec![
                    Label::primary((), span.clone()).with_message(format!("this has type {ty}"))
                ]),

            Self::InvalidIdentifierType {
                actual_type,
                expected_type,
                span,
            } => Diagnostic::error()
                .with_message("invalid identifier type")
                .with_labels(vec![Label::primary((), span.clone()).with_message(
                    format!("expected {expected_type}, found {actual_type}"),
                )]),

            Self::InvalidIdentifierBinding {
                actual_number,
                expected_number,
                identifiers_span,
                iterator_span,
            } => Diagnostic::error()
                .with_message(format!(
                    "expected {expected_number} identifiers to bind, got {actual_number}"
                ))
                .with_labels(vec![
                    Label::primary(
                        (),
                        Range {
                            start: identifiers_span.start,
                            end: iterator_span.end,
                        },
                    ),
                    Label::secondary((), identifiers_span.clone())
                        .with_message(format!("{actual_number} identifier(s) being bound")),
                    Label::secondary((), iterator_span.clone()).with_message(format!(
                        "this yields {expected_number} elements on every iteration"
                    )),
                ]),

            Self::InvalidIdentifierCall {
                arguments_types,
                span,
            } => Diagnostic::error()
                .with_message(format!(
                    "invalid arguments types: [{}]",
                    arguments_types.join(", ")
                ))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::InvalidIdentifierUse { span } => Diagnostic::error()
                .with_message("wrong use of identifier")
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::MatchOnWildcardRuleSet {
                rule_name,
                name_span,
                rule_set,
            } => Diagnostic::error()
                .with_message(format!(
                    "rule \"{rule_name}\" matches a previous rule set \"{rule_set}\""
                ))
                .with_labels(vec![Label::primary((), name_span.clone())]),

            Self::NonIterableIdentifier { span } => Diagnostic::error()
                .with_message("identifier is not iterable")
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::UnknownIdentifier { name, span } => Diagnostic::error()
                .with_message(format!("unknown identifier \"{name}\""))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::UnknownImport { name, span } => Diagnostic::error()
                .with_message(format!("unknown import {name}"))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::UnknownIdentifierField { field_name, span } => Diagnostic::error()
                .with_message(format!("unknown field \"{field_name}\""))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::UnknownVariable {
                variable_name,
                span,
            } => Diagnostic::error()
                .with_message(format!("unknown variable ${variable_name}"))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::UnusedVariable { name, span } => Diagnostic::error()
                .with_message(format!("variable ${name} is unused"))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::VariableCompilation {
                variable_name,
                span,
                error,
            } => Diagnostic::error()
                .with_message(format!(
                    "variable ${variable_name} cannot be compiled: {error}"
                ))
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::ImplicitBytesToBooleanCast { span } => Diagnostic::warning()
                .with_message("implicit cast from a bytes value to a boolean")
                .with_labels(vec![Label::primary((), span.clone())]),

            Self::RegexContainsNonAsciiChar { span } => Diagnostic::warning()
                .with_message("a non ascii character is present in a regex")
                .with_labels(vec![Label::primary((), span.clone())])
                .with_notes(vec![
                    "This may cause unexpected matching behavior, either due \
                     to different encodings, or because matching is only done \
                     on bytes."
                        .into(),
                    "For example, the regex `/<µ+>/` does not match `<µµ>`.".into(),
                    "You should replace the character with explicit bytes \
                      that do not depend on any specific encoding, for \
                      example `/\\xCE\\xBC/` instead of `/µ/`."
                        .into(),
                ]),
        }
    }
}
