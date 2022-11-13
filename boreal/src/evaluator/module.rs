//! Provides methods to evaluate module values during scanning.
use std::collections::HashMap;
use std::sync::Arc;

use crate::compiler::expression::Expression;
use crate::compiler::module::{BoundedValueIndex, ModuleExpression, ValueOperation};
use crate::module::{ScanContext, Value as ModuleValue};

use super::{Evaluator, PoisonKind, Value};

pub(super) fn evaluate_expr(
    evaluator: &mut Evaluator,
    expr: &ModuleExpression,
) -> Result<ModuleValue, PoisonKind> {
    match expr {
        ModuleExpression::BoundedModuleValueUse { index, operations } => {
            let value = match index {
                BoundedValueIndex::Module(index) => {
                    &evaluator
                        .scan_data
                        .module_values
                        .get(*index)
                        .ok_or(PoisonKind::Undefined)?
                        .1
                }
                BoundedValueIndex::BoundedStack(index) => evaluator
                    .bounded_identifiers_stack
                    .get(*index)
                    .ok_or(PoisonKind::Undefined)?,
            };
            let value = Arc::clone(value);
            evaluate_ops(evaluator, &value, operations.iter())
        }
        ModuleExpression::Function {
            fun,
            arguments,
            operations,
        } => {
            let value = eval_function_op(evaluator, *fun, arguments)?;
            evaluate_ops(evaluator, &value, operations.iter())
        }
    }
}

pub(super) fn evaluate_ops<'a, I>(
    evaluator: &mut Evaluator,
    mut value: &ModuleValue,
    mut operations: I,
) -> Result<ModuleValue, PoisonKind>
where
    I: Iterator<Item = &'a ValueOperation> + 'a,
{
    while let Some(op) = operations.next() {
        match op {
            ValueOperation::Subfield(subfield) => match value {
                ModuleValue::Object(map) => {
                    value = map.get(&**subfield).ok_or(PoisonKind::Undefined)?;
                }
                _ => return Err(PoisonKind::Undefined),
            },
            ValueOperation::Subscript(subscript) => match value {
                ModuleValue::Array(array) => {
                    value = eval_array_op(evaluator, subscript, array)?;
                }
                ModuleValue::Dictionary(dict) => {
                    value = eval_dict_op(evaluator, subscript, dict)?;
                }
                _ => return Err(PoisonKind::Undefined),
            },
            ValueOperation::FunctionCall(arguments) => match value {
                ModuleValue::Function(fun) => {
                    let arguments: Result<Vec<_>, _> = arguments
                        .iter()
                        .map(|expr| {
                            evaluator
                                .evaluate_expr(expr)
                                .map(expr_value_to_module_value)
                        })
                        .collect();

                    let new_value = fun(&evaluator.scan_data.module_ctx, arguments?)
                        .ok_or(PoisonKind::Undefined)?;
                    return evaluate_ops(evaluator, &new_value, operations);
                }
                _ => return Err(PoisonKind::Undefined),
            },
        }
    }

    Ok(value.clone())
}

pub(super) fn module_value_to_expr_value(value: ModuleValue) -> Result<Value, PoisonKind> {
    match value {
        ModuleValue::Integer(v) => Ok(Value::Integer(v)),
        ModuleValue::Float(v) => {
            if v.is_nan() {
                Err(PoisonKind::Undefined)
            } else {
                Ok(Value::Float(v))
            }
        }
        ModuleValue::Bytes(v) => Ok(Value::Bytes(v)),
        ModuleValue::Regex(v) => Ok(Value::Regex(v)),
        ModuleValue::Boolean(v) => Ok(Value::Boolean(v)),

        _ => Err(PoisonKind::Undefined),
    }
}

fn eval_array_op<'a>(
    evaluator: &mut Evaluator,
    subscript: &Expression,
    array: &'a [ModuleValue],
) -> Result<&'a ModuleValue, PoisonKind> {
    let index = evaluator.evaluate_expr(subscript)?.unwrap_number()?;

    usize::try_from(index)
        .ok()
        .and_then(|i| array.get(i))
        .ok_or(PoisonKind::Undefined)
}

fn eval_dict_op<'a>(
    evaluator: &mut Evaluator,
    subscript: &Expression,
    dict: &'a HashMap<Vec<u8>, ModuleValue>,
) -> Result<&'a ModuleValue, PoisonKind> {
    let val = evaluator.evaluate_expr(subscript)?.unwrap_bytes()?;

    dict.get(&val).ok_or(PoisonKind::Undefined)
}

fn eval_function_op(
    evaluator: &mut Evaluator,
    fun: fn(&ScanContext, Vec<ModuleValue>) -> Option<ModuleValue>,
    arguments: &[Expression],
) -> Result<ModuleValue, PoisonKind> {
    let arguments: Result<Vec<_>, _> = arguments
        .iter()
        .map(|expr| {
            evaluator
                .evaluate_expr(expr)
                .map(expr_value_to_module_value)
        })
        .collect();

    fun(&evaluator.scan_data.module_ctx, arguments?).ok_or(PoisonKind::Undefined)
}

fn expr_value_to_module_value(v: Value) -> ModuleValue {
    match v {
        Value::Integer(v) => ModuleValue::Integer(v),
        Value::Float(v) => ModuleValue::Float(v),
        Value::Bytes(v) => ModuleValue::Bytes(v),
        Value::Regex(v) => ModuleValue::Regex(v),
        Value::Boolean(v) => ModuleValue::Boolean(v),
    }
}
