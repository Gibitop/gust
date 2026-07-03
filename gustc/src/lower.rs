use std::collections::HashSet;

use crate::ast::{ExprKind, FunctionBody, Item, Program, StmtKind};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    StringLocal { name: String, value: String },
    Println(LoweredValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredValue {
    StringLiteral(String),
    StringLocal(String),
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
    let mut string_locals = HashSet::new();
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
                            ExprKind::Identifier(name) if string_locals.contains(name) => {
                                statements.push(LoweredStatement::Println(
                                    LoweredValue::StringLocal(name.clone()),
                                ));
                            }
                            ExprKind::Identifier(name) => diagnostics.push(Diagnostic::error(
                                arg.span,
                                format!("unknown string local `{name}` in `io.println`"),
                            )),
                            _ => diagnostics.push(Diagnostic::error(
                                arg.span,
                                "`io.println` only accepts a string literal or string local in executable builds",
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

                        if let Some(type_annotation) = type_annotation {
                            diagnostics.push(Diagnostic::error(
                                type_annotation.span,
                                "typed let bindings are not supported in executable builds",
                            ));
                            can_lower = false;
                        }

                        let ExprKind::String(string_value) = &value.kind else {
                            diagnostics.push(Diagnostic::error(
                                value.span,
                                "only string literal let values are supported in executable builds",
                            ));
                            continue;
                        };

                        if can_lower {
                            if string_locals.insert(name.clone()) {
                                statements.push(LoweredStatement::StringLocal {
                                    name: name.clone(),
                                    value: string_value.clone(),
                                });
                            } else {
                                diagnostics.push(Diagnostic::error(
                                    statement.span,
                                    format!("duplicate string local `{name}` in executable build"),
                                ));
                            }
                        }
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
