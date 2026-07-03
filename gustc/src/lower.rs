use std::collections::HashMap;

use crate::ast::{BasicType, BinaryOp, Expr, ExprKind, FunctionBody, Item, Program, StmtKind};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    Local { name: String, value: LoweredExpr },
    Println(LoweredExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredExpr {
    pub type_: BasicType,
    pub kind: LoweredExprKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredExprKind {
    StringLiteral(String),
    BoolLiteral(bool),
    NumberLiteral(String),
    Local(String),
    StringConcat(Box<LoweredExpr>, Box<LoweredExpr>),
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
                                "`io.println` expects exactly one `String` value in executable builds",
                            ));
                            continue;
                        }

                        if let Some(value) = lower_string_expr(
                            &args[0],
                            &locals,
                            &mut diagnostics,
                            "`io.println` only accepts `String` values in executable builds",
                        ) {
                            statements.push(LoweredStatement::Println(value));
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
                                        LoweredExpr {
                                            type_: BasicType::String,
                                            kind: LoweredExprKind::StringLiteral(value.clone()),
                                        },
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
                                (ExprKind::Identifier(name), None | Some(BasicType::String))
                                    if locals.get(name) == Some(&BasicType::String) =>
                                {
                                    Some((
                                        BasicType::String,
                                        LoweredExpr {
                                            type_: BasicType::String,
                                            kind: LoweredExprKind::Local(name.clone()),
                                        },
                                    ))
                                }
                                (ExprKind::Identifier(name), Some(type_))
                                    if locals.get(name) == Some(&BasicType::String) =>
                                {
                                    diagnostics.push(Diagnostic::error(
                                        value.span,
                                        format!(
                                            "expected value of type `{}`, got `String`",
                                            type_.name()
                                        ),
                                    ));
                                    None
                                }
                                (
                                    ExprKind::Binary {
                                        op: BinaryOp::Add, ..
                                    },
                                    None | Some(BasicType::String),
                                ) => lower_string_expr(
                                    value,
                                    &locals,
                                    &mut diagnostics,
                                    "expected `String` value in executable builds",
                                )
                                .map(|value| (value.type_, value)),
                                (
                                    ExprKind::Binary {
                                        op: BinaryOp::Add, ..
                                    },
                                    Some(type_),
                                ) => {
                                    if lower_string_expr(
                                        value,
                                        &locals,
                                        &mut diagnostics,
                                        "expected `String` value in executable builds",
                                    )
                                    .is_some()
                                    {
                                        diagnostics.push(Diagnostic::error(
                                            value.span,
                                            format!(
                                                "expected value of type `{}`, got `String`",
                                                type_.name()
                                            ),
                                        ));
                                    }

                                    None
                                }
                                (ExprKind::Bool(value), None | Some(BasicType::Bool)) => Some((
                                    BasicType::Bool,
                                    LoweredExpr {
                                        type_: BasicType::Bool,
                                        kind: LoweredExprKind::BoolLiteral(*value),
                                    },
                                )),
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
                                    LoweredExpr {
                                        type_: BasicType::I32,
                                        kind: LoweredExprKind::NumberLiteral(value.clone()),
                                    },
                                )),
                                (ExprKind::Number(value), Some(type_)) if type_.is_numeric() => {
                                    Some((
                                        type_,
                                        LoweredExpr {
                                            type_,
                                            kind: LoweredExprKind::NumberLiteral(value.clone()),
                                        },
                                    ))
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
                                        "only literal and string concat local values are supported in executable builds",
                                    ));
                                    None
                                }
                            }
                        } else if let Some(type_) = annotated_type {
                            let value = match type_ {
                                BasicType::String => LoweredExprKind::StringLiteral(String::new()),
                                BasicType::Bool => LoweredExprKind::BoolLiteral(false),
                                type_ if type_.is_numeric() => {
                                    LoweredExprKind::NumberLiteral("0".to_string())
                                }
                                _ => unreachable!("all basic types have default values"),
                            };

                            Some((type_, LoweredExpr { type_, kind: value }))
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

fn lower_string_expr(
    expr: &Expr,
    locals: &HashMap<String, BasicType>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredExpr> {
    match &expr.kind {
        ExprKind::String(value) => Some(LoweredExpr {
            type_: BasicType::String,
            kind: LoweredExprKind::StringLiteral(value.clone()),
        }),
        ExprKind::Identifier(name) if locals.get(name) == Some(&BasicType::String) => {
            Some(LoweredExpr {
                type_: BasicType::String,
                kind: LoweredExprKind::Local(name.clone()),
            })
        }
        ExprKind::Identifier(name) if locals.contains_key(name) => {
            let type_ = locals[name];
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("{message}, got `{}`", type_.name()),
            ));
            None
        }
        ExprKind::Identifier(name) => {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("unknown local `{name}` in string expression"),
            ));
            None
        }
        ExprKind::Binary {
            left,
            op: BinaryOp::Add,
            right,
        } => {
            let left = lower_string_expr(left, locals, diagnostics, message);
            let right = lower_string_expr(right, locals, diagnostics, message);

            match (left, right) {
                (Some(left), Some(right)) => Some(LoweredExpr {
                    type_: BasicType::String,
                    kind: LoweredExprKind::StringConcat(Box::new(left), Box::new(right)),
                }),
                _ => None,
            }
        }
        ExprKind::Binary { .. } => {
            diagnostics.push(Diagnostic::error(expr.span, message));
            None
        }
        _ => {
            diagnostics.push(Diagnostic::error(expr.span, message));
            None
        }
    }
}
