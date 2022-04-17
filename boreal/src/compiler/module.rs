use std::{ops::Range, sync::Arc};

use boreal_parser as parser;

use super::{compile_expression, CompilationError, Expression, RuleCompiler, Type};
use crate::module::{self, Type as ValueType, Value};

#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub value: Arc<Value>,
}

/// Operations on identifiers.
#[derive(Debug)]
pub enum ValueOperation {
    /// Object subfield, i.e. `value.subfield`.
    Subfield(String),
    /// Array subscript, i.e. `value[subscript]`.
    Subscript(Box<Expression>),
    /// Function call, i.e. `value(arguments)`.
    FunctionCall(Vec<Expression>),
}

pub(crate) fn compile_module<M: module::Module>(module: M) -> Module {
    Module {
        name: module.get_name(),
        value: Arc::new(module.get_value()),
    }
}

pub(super) fn compile_identifier(
    compiler: &RuleCompiler<'_>,
    identifier: parser::Identifier,
    identifier_span: &Range<usize>,
) -> Result<(Expression, Type), CompilationError> {
    let module_value = match compiler.file.symbols.get(&identifier.name) {
        Some(v) => Arc::clone(&v.value),
        None => {
            return Err(CompilationError::UnknownIdentifier {
                name: identifier.name,
                span: identifier.name_span,
            })
        }
    };

    let mut module_use = ModuleUse {
        compiler,
        last_immediate_value: &module_value,
        current_value: ValueOrType::Value(&module_value),
        operations: Vec::with_capacity(identifier.operations.len()),
        current_span: identifier.name_span.clone(),
    };

    for op in identifier.operations.into_iter() {
        module_use.add_operation(op)?;
    }

    module_use
        .into_expression()
        .ok_or_else(|| CompilationError::InvalidIdentifierUse {
            span: identifier_span.clone(),
        })
}

struct ModuleUse<'a> {
    compiler: &'a RuleCompiler<'a>,
    last_immediate_value: &'a Value,
    current_value: ValueOrType<'a>,
    operations: Vec<ValueOperation>,
    current_span: Range<usize>,
}

impl ModuleUse<'_> {
    fn add_operation(&mut self, op: parser::IdentifierOperation) -> Result<(), CompilationError> {
        let res = match op.op {
            parser::IdentifierOperationType::Subfield(subfield) => {
                let res = self.current_value.subfield(&subfield);
                match self.current_value {
                    ValueOrType::Value(v) => self.last_immediate_value = v,
                    ValueOrType::Type(_) => {
                        self.operations
                            .push(ValueOperation::Subfield(subfield.to_string()));
                    }
                };
                res
            }
            parser::IdentifierOperationType::Subscript(subscript) => {
                let subscript = compile_expression(self.compiler, *subscript)?;
                self.operations
                    .push(ValueOperation::Subscript(Box::new(subscript.expr)));
                self.current_value.subscript()
            }
            parser::IdentifierOperationType::FunctionCall(arguments) => {
                let arguments: Result<Vec<_>, _> = arguments
                    .into_iter()
                    .map(|expr| compile_expression(self.compiler, expr).map(|v| v.expr))
                    .collect();
                self.operations
                    .push(ValueOperation::FunctionCall(arguments?));
                self.current_value.function_call()
            }
        };

        match res {
            Err(TypeError::UnknownSubfield(subfield)) => {
                return Err(CompilationError::UnknownIdentifierField {
                    field_name: subfield.to_string(),
                    span: op.span,
                });
            }
            Err(TypeError::WrongType {
                actual_type,
                expected_type,
            }) => {
                return Err(CompilationError::InvalidIdentifierType {
                    actual_type,
                    expected_type,
                    span: self.current_span.clone(),
                });
            }
            Ok(()) => (),
        };
        self.current_span.end = op.span.end;
        Ok(())
    }

    fn into_expression(self) -> Option<(Expression, Type)> {
        let ty = self.current_value.into_expression_type()?;

        let expr = match self.last_immediate_value {
            // Those are all primitive values. This means there are no operations applied, and
            // we can directly generate a primitive expression.
            Value::Integer(v) => Expression::Number(*v),
            Value::Float(v) => Expression::Double(*v),
            Value::String(v) => Expression::String(v.to_owned()),
            Value::Regex(v) => Expression::Regex(v.to_owned()),
            Value::Boolean(v) => Expression::Boolean(*v),

            // There is no legitimate situation where we can end up with a dictionary
            // as the last immediate value.
            Value::Dictionary(_) => return None,

            Value::Array { on_scan, .. } => Expression::ModuleArray {
                fun: *on_scan,
                operations: self.operations,
            },
            Value::Function { fun, .. } => Expression::ModuleFunction {
                fun: *fun,
                operations: self.operations,
            },
        };

        Some((expr, ty))
    }
}

/// Used to type-check use of a module in a rule.
///
/// Tries to keep a proper [`Value`] for as long as possible, so that the compiled expression
/// can be optimized if possible (if the end value is a primitive of a function returning a
/// primitive for example).
enum ValueOrType<'a> {
    /// Currently value, if available.
    Value(&'a Value),
    /// Otherwise, type the expression will have when evaluated.
    Type(&'a ValueType),
}

enum TypeError {
    UnknownSubfield(String),
    WrongType {
        actual_type: String,
        expected_type: String,
    },
}

impl ValueOrType<'_> {
    fn subfield(&mut self, subfield: &str) -> Result<(), TypeError> {
        match self {
            Self::Value(value) => match value {
                Value::Dictionary(map) => match map.get(&*subfield) {
                    Some(v) => {
                        *self = Self::Value(v);
                        return Ok(());
                    }
                    None => return Err(TypeError::UnknownSubfield(subfield.to_string())),
                },
                _ => (),
            },
            Self::Type(ty) => match ty {
                ValueType::Dictionary(map) => match map.get(&*subfield) {
                    Some(v) => {
                        *self = Self::Type(v);
                        return Ok(());
                    }
                    None => return Err(TypeError::UnknownSubfield(subfield.to_string())),
                },
                _ => (),
            },
        };

        Err(TypeError::WrongType {
            actual_type: self.type_to_string(),
            expected_type: "dictionary".to_owned(),
        })
    }

    fn subscript(&mut self) -> Result<(), TypeError> {
        match self {
            Self::Value(value) => match value {
                Value::Array { value_type, .. } => {
                    *self = Self::Type(value_type);
                    return Ok(());
                }
                _ => (),
            },
            Self::Type(ty) => match ty {
                ValueType::Array(value_type) => {
                    *self = Self::Type(value_type);
                    return Ok(());
                }
                _ => (),
            },
        }

        Err(TypeError::WrongType {
            actual_type: self.type_to_string(),
            expected_type: "array".to_owned(),
        })
    }

    fn function_call(&mut self) -> Result<(), TypeError> {
        match self {
            Self::Value(value) => match value {
                Value::Function { return_type, .. } => {
                    *self = Self::Type(return_type);
                    return Ok(());
                }
                _ => (),
            },
            Self::Type(ty) => match ty {
                ValueType::Function { return_type } => {
                    *self = Self::Type(return_type);
                    return Ok(());
                }
                _ => (),
            },
        }

        Err(TypeError::WrongType {
            actual_type: self.type_to_string(),
            expected_type: "function".to_owned(),
        })
    }

    fn type_to_string(&self) -> String {
        match self {
            Self::Value(value) => match value {
                Value::Integer(_) => "integer",
                Value::Float(_) => "float",
                Value::String(_) => "string",
                Value::Regex(_) => "regex",
                Value::Boolean(_) => "boolean",
                Value::Array { .. } => "array",
                Value::Dictionary(_) => "dictionary",
                Value::Function { .. } => "function",
            },
            Self::Type(ty) => match ty {
                ValueType::Integer => "integer",
                ValueType::Float => "float",
                ValueType::String => "string",
                ValueType::Regex => "regex",
                ValueType::Boolean => "boolean",
                ValueType::Array { .. } => "array",
                ValueType::Dictionary(_) => "dictionary",
                ValueType::Function { .. } => "function",
            },
        }
        .to_owned()
    }

    fn into_expression_type(self) -> Option<Type> {
        match self {
            Self::Value(value) => match value {
                Value::Integer(_) => Some(Type::Integer),
                Value::Float(_) => Some(Type::Float),
                Value::String(_) => Some(Type::String),
                Value::Regex(_) => Some(Type::Regex),
                Value::Boolean(_) => Some(Type::Boolean),
                _ => None,
            },
            Self::Type(ty) => match ty {
                ValueType::Integer => Some(Type::Integer),
                ValueType::Float => Some(Type::Float),
                ValueType::String => Some(Type::String),
                ValueType::Regex => Some(Type::Regex),
                ValueType::Boolean => Some(Type::Boolean),
                _ => None,
            },
        }
    }
}
