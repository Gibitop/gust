use crate::ast::{ExprKind, FunctionBody, Item, Program, StmtKind};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    Println(String),
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
                                "only `io.println(\"...\")` expression statements are supported in executable builds",
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
                                "`io.println` expects exactly one string literal in executable builds",
                            ));
                            continue;
                        }

                        let arg = &args[0];
                        let ExprKind::String(value) = &arg.kind else {
                            diagnostics.push(Diagnostic::error(
                                arg.span,
                                "`io.println` only accepts a string literal in executable builds",
                            ));
                            continue;
                        };

                        statements.push(LoweredStatement::Println(value.clone()));
                    }
                    StmtKind::Let { .. } => {
                        diagnostics.push(Diagnostic::error(
                            statement.span,
                            "let statements are not supported in executable builds",
                        ));
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
