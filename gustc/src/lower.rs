use std::collections::HashMap;

use crate::ast::{BasicType, ExprKind, FunctionBody, Item, Program, StmtKind};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    Local {
        name: String,
        type_: BasicType,
        value: LoweredValue,
    },
    Println(LoweredValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredValue {
    StringLiteral(String),
    BoolLiteral(bool),
    NumberLiteral(String),
    Local(String),
}

pub fn lower_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut main = None;

    for item in &program.items {
        match item {
            Item::Function(function) if function.name.as_deref() == Some("main") => {
                if main.is_some() {
                    diagnostics.push(Diagnostic::error(
                        function.span,
                        "expected exactly one `main` function in executable builds",
                    ));
                } else {
                    main = Some(function);
                }
            }
            Item::Function(function) => diagnostics.push(Diagnostic::error(
                function.span,
                "only `fn main()` is supported in executable builds",
            )),
            Item::Import(item) => diagnostics.push(Diagnostic::error(
                item.span,
                "imports are not supported in executable builds",
            )),
            Item::Enum(item) => diagnostics.push(Diagnostic::error(
                item.span,
                "enums are not supported in executable builds",
            )),
            Item::Struct(item) => diagnostics.push(Diagnostic::error(
                item.span,
                "structs are not supported in executable builds",
            )),
        }
    }

    let Some(main) = main else {
        let span = program.items.first().map_or(Span::new(0, 0), Item::span);
        diagnostics.push(Diagnostic::error(
            span,
            "missing `main` function in executable build",
        ));
        return Err(diagnostics);
    };

    let mut statements = Vec::new();
    let mut locals = HashMap::new();
    if let Some(param) = main.params.first() {
        diagnostics.push(Diagnostic::error(
            param.span,
            "`main` parameters are not supported in executable builds",
        ));
    }

    if let Some(return_type) = &main.return_type {
        diagnostics.push(Diagnostic::error(
            return_type.span,
            "`main` return types are not supported in executable builds",
        ));
    }

    match &main.body {
        FunctionBody::Block(block) => {
            for statement in &block.statements {
                match &statement.kind {
                    StmtKind::Expr(expr) => {
                        let ExprKind::Call { callee, args } = &expr.kind else {
                            diagnostics.push(Diagnostic::error(
                                expr.span,
                                "only `io.println(...)` expression statements are supported in executable builds",
                            ));
                            continue;
                        };

                        let is_io_println = match &callee.kind {
                            ExprKind::Member { object, name } if name == "println" => {
                                matches!(&object.kind, ExprKind::Identifier(name) if name == "io")
                            }
                            _ => false,
                        };

                        if !is_io_println {
                            diagnostics.push(Diagnostic::error(
                                callee.span,
                                "only `io.println` calls are supported in executable builds",
                            ));
                            continue;
                        }

                        if args.len() != 1 {
                            diagnostics.push(Diagnostic::error(
                                expr.span,
                                "`io.println` expects exactly one string literal or string local in executable builds",
                            ));
                            continue;
                        }

                        let arg = &args[0];
                        match &arg.kind {
                            ExprKind::String(value) => statements.push(LoweredStatement::Println(
                                LoweredValue::StringLiteral(value.clone()),
                            )),
                            ExprKind::Identifier(name)
                                if locals.get(name) == Some(&BasicType::String) =>
                            {
                                statements.push(LoweredStatement::Println(LoweredValue::Local(
                                    name.clone(),
                                )));
                            }
                            ExprKind::Identifier(name) if locals.contains_key(name) => {
                                let type_ = locals[name];
                                diagnostics.push(Diagnostic::error(
                                    arg.span,
                                    format!(
                                        "`io.println` only accepts `String` values in executable builds, got `{}`",
                                        type_.name()
                                    ),
                                ));
                            }
                            ExprKind::Identifier(name) => diagnostics.push(Diagnostic::error(
                                arg.span,
                                format!("unknown local `{name}` in `io.println`"),
                            )),
                            _ => diagnostics.push(Diagnostic::error(
                                arg.span,
                                "`io.println` only accepts `String` values in executable builds",
                            )),
                        }
                    }
                    StmtKind::Let {
                        name,
                        mutable,
                        type_annotation,
                        value,
                    } => {
                        let mut can_lower = true;

                        if *mutable {
                            diagnostics.push(Diagnostic::error(
                                statement.span,
                                "`let mut` bindings are not supported in executable builds",
                            ));
                            can_lower = false;
                        }

                        let annotated_type = if let Some(type_annotation) = type_annotation {
                            if !type_annotation.args.is_empty() {
                                diagnostics.push(Diagnostic::error(
                                    type_annotation.span,
                                    "generic local types are not supported in executable builds",
                                ));
                                can_lower = false;
                                None
                            } else if let Some(type_) = BasicType::from_name(&type_annotation.name)
                            {
                                Some(type_)
                            } else {
                                diagnostics.push(Diagnostic::error(
                                    type_annotation.span,
                                    "only basic local types are supported in executable builds",
                                ));
                                can_lower = false;
                                None
                            }
                        } else {
                            None
                        };

                        let lowered = if let Some(value) = value {
                            match (&value.kind, annotated_type) {
                                (ExprKind::String(value), None | Some(BasicType::String)) => {
                                    Some((
                                        BasicType::String,
                                        LoweredValue::StringLiteral(value.clone()),
                                    ))
                                }
                                (ExprKind::String(_), Some(type_)) => {
                                    diagnostics.push(Diagnostic::error(
                                        value.span,
                                        format!(
                                            "expected value of type `{}`, got `String`",
                                            type_.name()
                                        ),
                                    ));
                                    None
                                }
                                (ExprKind::Bool(value), None | Some(BasicType::Bool)) => {
                                    Some((BasicType::Bool, LoweredValue::BoolLiteral(*value)))
                                }
                                (ExprKind::Bool(_), Some(type_)) => {
                                    diagnostics.push(Diagnostic::error(
                                        value.span,
                                        format!(
                                            "expected value of type `{}`, got `bool`",
                                            type_.name()
                                        ),
                                    ));
                                    None
                                }
                                (ExprKind::Number(value), None) => Some((
                                    BasicType::I32,
                                    LoweredValue::NumberLiteral(value.clone()),
                                )),
                                (ExprKind::Number(value), Some(type_)) if type_.is_numeric() => {
                                    Some((type_, LoweredValue::NumberLiteral(value.clone())))
                                }
                                (ExprKind::Number(_), Some(type_)) => {
                                    diagnostics.push(Diagnostic::error(
                                        value.span,
                                        format!(
                                            "expected value of type `{}`, got `i32`",
                                            type_.name()
                                        ),
                                    ));
                                    None
                                }
                                _ => {
                                    diagnostics.push(Diagnostic::error(
                                        value.span,
                                        "only literal local values are supported in executable builds",
                                    ));
                                    None
                                }
                            }
                        } else if let Some(type_) = annotated_type {
                            let value = match type_ {
                                BasicType::String => LoweredValue::StringLiteral(String::new()),
                                BasicType::Bool => LoweredValue::BoolLiteral(false),
                                type_ if type_.is_numeric() => {
                                    LoweredValue::NumberLiteral("0".to_string())
                                }
                                _ => unreachable!("all basic types have default values"),
                            };

                            Some((type_, value))
                        } else {
                            diagnostics.push(Diagnostic::error(
                                statement.span,
                                "let declarations without values must include a type annotation",
                            ));
                            None
                        };

                        let Some((type_, value)) = lowered else {
                            continue;
                        };

                        if !can_lower {
                            continue;
                        }

                        if locals.insert(name.clone(), type_).is_some() {
                            diagnostics.push(Diagnostic::error(
                                statement.span,
                                format!("duplicate local `{name}` in executable build"),
                            ));
                            continue;
                        }

                        statements.push(LoweredStatement::Local {
                            name: name.clone(),
                            type_,
                            value,
                        });
                    }
                    StmtKind::Return { .. } => {
                        diagnostics.push(Diagnostic::error(
                            statement.span,
                            "return statements are not supported in executable builds",
                        ));
                    }
                    StmtKind::For { .. } => {
                        diagnostics.push(Diagnostic::error(
                            statement.span,
                            "for loops are not supported in executable builds",
                        ));
                    }
                }
            }
        }
        FunctionBody::Expr(expr) => diagnostics.push(Diagnostic::error(
            expr.span,
            "arrow function bodies are not supported in executable builds",
        )),
    }

    if diagnostics.is_empty() {
        Ok(LoweredProgram { statements })
    } else {
        Err(diagnostics)
    }
}
