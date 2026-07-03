use std::collections::HashMap;

use crate::ast::{
    BasicType, BinaryOp, Expr, ExprKind, FunctionBody, FunctionDecl, Item, Program, Stmt, StmtKind,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub functions: Vec<LoweredFunction>,
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredFunction {
    pub name: String,
    pub params: Vec<LoweredParam>,
    pub return_type: BasicType,
    pub statements: Vec<LoweredStatement>,
    pub return_value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredParam {
    pub name: String,
    pub type_: BasicType,
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
    Call {
        name: String,
        args: Vec<LoweredExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
    params: Vec<BasicType>,
    return_type: BasicType,
}

pub fn lower_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut main = None;
    let mut signatures = HashMap::new();

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
            Item::Function(function) => {
                if let Some(name) = &function.name {
                    if let Some(signature) = lower_function_signature(function, &mut diagnostics) {
                        signatures.insert(name.clone(), signature);
                    }
                }
            }
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

    let mut functions = Vec::new();

    for item in &program.items {
        let Item::Function(function) = item else {
            continue;
        };

        let Some(name) = &function.name else {
            continue;
        };

        if name == "main" || !signatures.contains_key(name) {
            continue;
        }

        if let Some(function) = lower_function(function, &signatures, &mut diagnostics) {
            functions.push(function);
        }
    }

    let statements = lower_main(main, &signatures, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(LoweredProgram {
            functions,
            statements,
        })
    } else {
        Err(diagnostics)
    }
}

fn lower_function_signature(
    function: &FunctionDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut params = Vec::new();
    let mut can_lower = true;

    for param in &function.params {
        if param.mutable {
            diagnostics.push(Diagnostic::error(
                param.span,
                "mutable parameters are not supported in executable builds",
            ));
            can_lower = false;
        }

        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "function parameters must include type annotations in executable builds",
            ));
            can_lower = false;
            continue;
        };

        if !type_ref.args.is_empty() {
            diagnostics.push(Diagnostic::error(
                type_ref.span,
                "generic parameter types are not supported in executable builds",
            ));
            can_lower = false;
            continue;
        }

        let Some(type_) = BasicType::from_name(&type_ref.name) else {
            diagnostics.push(Diagnostic::error(
                type_ref.span,
                "only basic parameter types are supported in executable builds",
            ));
            can_lower = false;
            continue;
        };

        params.push(type_);
    }

    let Some(return_type) = &function.return_type else {
        diagnostics.push(Diagnostic::error(
            function.span,
            "helper functions must declare a return type in executable builds",
        ));
        return None;
    };

    if !return_type.args.is_empty() {
        diagnostics.push(Diagnostic::error(
            return_type.span,
            "generic return types are not supported in executable builds",
        ));
        return None;
    }

    let Some(return_type) = BasicType::from_name(&return_type.name) else {
        diagnostics.push(Diagnostic::error(
            return_type.span,
            "only basic return types are supported in executable builds",
        ));
        return None;
    };

    if can_lower {
        Some(FunctionSignature {
            params,
            return_type,
        })
    } else {
        None
    }
}

fn lower_function(
    function: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredFunction> {
    let name = function.name.as_ref()?;
    let signature = signatures.get(name)?;
    let FunctionBody::Block(block) = &function.body else {
        diagnostics.push(Diagnostic::error(
            function.span,
            "arrow function bodies are not supported in executable builds",
        ));
        return None;
    };

    let mut locals = HashMap::new();
    let mut params = Vec::new();
    let mut statements = Vec::new();
    let mut return_value = None;

    for (param, type_) in function.params.iter().zip(&signature.params) {
        if locals.insert(param.name.clone(), *type_).is_some() {
            diagnostics.push(Diagnostic::error(
                param.span,
                format!("duplicate local `{}` in executable build", param.name),
            ));
        }

        params.push(LoweredParam {
            name: param.name.clone(),
            type_: *type_,
        });
    }

    for (index, statement) in block.statements.iter().enumerate() {
        let is_last = index + 1 == block.statements.len();

        if is_last {
            match &statement.kind {
                StmtKind::Return { value: Some(value) } => {
                    return_value = lower_expr(
                        value,
                        &locals,
                        signatures,
                        diagnostics,
                        Some(signature.return_type),
                        "expected supported return value in executable builds",
                    );
                }
                StmtKind::Return { value: None } => {
                    diagnostics.push(Diagnostic::error(
                        statement.span,
                        "helper functions must return a value in executable builds",
                    ));
                }
                _ => diagnostics.push(Diagnostic::error(
                    statement.span,
                    "helper functions must end with `return <expr>` in executable builds",
                )),
            }

            continue;
        }

        match &statement.kind {
            StmtKind::Let { .. } => {
                if let Some(statement) =
                    lower_local_statement(statement, &mut locals, signatures, diagnostics)
                {
                    statements.push(statement);
                }
            }
            StmtKind::Return { .. } => diagnostics.push(Diagnostic::error(
                statement.span,
                "early returns are not supported in executable builds",
            )),
            _ => diagnostics.push(Diagnostic::error(
                statement.span,
                "only local declarations are supported before helper returns in executable builds",
            )),
        }
    }

    let Some(return_value) = return_value else {
        return None;
    };

    Some(LoweredFunction {
        name: name.clone(),
        params,
        return_type: signature.return_type,
        statements,
        return_value,
    })
}

fn lower_main(
    main: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LoweredStatement> {
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

                        if let Some(value) = lower_expr(
                            &args[0],
                            &locals,
                            signatures,
                            diagnostics,
                            None,
                            "`io.println` only accepts `String` values in executable builds",
                        ) {
                            if value.type_ != BasicType::String {
                                diagnostics.push(Diagnostic::error(
                                    args[0].span,
                                    format!(
                                        "`io.println` only accepts `String` values in executable builds, got `{}`",
                                        value.type_.name()
                                    ),
                                ));
                                continue;
                            }

                            statements.push(LoweredStatement::Println(value));
                        }
                    }
                    StmtKind::Let { .. } => {
                        if let Some(statement) =
                            lower_local_statement(statement, &mut locals, signatures, diagnostics)
                        {
                            statements.push(statement);
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

    statements
}

fn lower_local_statement(
    statement: &Stmt,
    locals: &mut HashMap<String, BasicType>,
    signatures: &HashMap<String, FunctionSignature>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStatement> {
    let StmtKind::Let {
        name,
        mutable,
        type_annotation,
        value,
    } = &statement.kind
    else {
        return None;
    };

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
        } else if let Some(type_) = BasicType::from_name(&type_annotation.name) {
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

    let value = if let Some(value) = value {
        lower_expr(
            value,
            locals,
            signatures,
            diagnostics,
            annotated_type,
            "only literal, string concat, and function call local values are supported in executable builds",
        )
    } else if let Some(type_) = annotated_type {
        let kind = match type_ {
            BasicType::String => LoweredExprKind::StringLiteral(String::new()),
            BasicType::Bool => LoweredExprKind::BoolLiteral(false),
            type_ if type_.is_numeric() => LoweredExprKind::NumberLiteral("0".to_string()),
            _ => unreachable!("all basic types have default values"),
        };

        Some(LoweredExpr { type_, kind })
    } else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            "let declarations without values must include a type annotation",
        ));
        None
    };

    let Some(value) = value else {
        return None;
    };

    if !can_lower {
        return None;
    }

    if locals.insert(name.clone(), value.type_).is_some() {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("duplicate local `{name}` in executable build"),
        ));
        return None;
    }

    Some(LoweredStatement::Local {
        name: name.clone(),
        value,
    })
}

fn lower_expr(
    expr: &Expr,
    locals: &HashMap<String, BasicType>,
    signatures: &HashMap<String, FunctionSignature>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<BasicType>,
    message: &str,
) -> Option<LoweredExpr> {
    let lowered = match &expr.kind {
        ExprKind::String(value) => LoweredExpr {
            type_: BasicType::String,
            kind: LoweredExprKind::StringLiteral(value.clone()),
        },
        ExprKind::Bool(value) => LoweredExpr {
            type_: BasicType::Bool,
            kind: LoweredExprKind::BoolLiteral(*value),
        },
        ExprKind::Number(value) => {
            let type_ = if expected_type.is_some_and(BasicType::is_numeric) {
                expected_type.unwrap()
            } else {
                BasicType::I32
            };

            LoweredExpr {
                type_,
                kind: LoweredExprKind::NumberLiteral(value.clone()),
            }
        }
        ExprKind::Identifier(name) if locals.contains_key(name) => LoweredExpr {
            type_: locals[name],
            kind: LoweredExprKind::Local(name.clone()),
        },
        ExprKind::Identifier(name) => {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("unknown local `{name}` in executable build"),
            ));
            return None;
        }
        ExprKind::Binary {
            left,
            op: BinaryOp::Add,
            right,
        } => {
            let left = lower_expr(
                left,
                locals,
                signatures,
                diagnostics,
                Some(BasicType::String),
                "expected `String` value in executable builds",
            );
            let right = lower_expr(
                right,
                locals,
                signatures,
                diagnostics,
                Some(BasicType::String),
                "expected `String` value in executable builds",
            );

            let (Some(left), Some(right)) = (left, right) else {
                return None;
            };

            LoweredExpr {
                type_: BasicType::String,
                kind: LoweredExprKind::StringConcat(Box::new(left), Box::new(right)),
            }
        }
        ExprKind::Call { callee, args } => {
            let ExprKind::Identifier(name) = &callee.kind else {
                diagnostics.push(Diagnostic::error(
                    callee.span,
                    "only direct helper function calls are supported in executable builds",
                ));
                return None;
            };

            let Some(signature) = signatures.get(name) else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown helper function `{name}` in executable build"),
                ));
                return None;
            };

            if args.len() != signature.params.len() {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "function `{name}` expects {} arguments, got {}",
                        signature.params.len(),
                        args.len()
                    ),
                ));
                return None;
            }

            let mut lowered_args = Vec::new();

            for (arg, type_) in args.iter().zip(&signature.params) {
                if let Some(arg) = lower_expr(
                    arg,
                    locals,
                    signatures,
                    diagnostics,
                    Some(*type_),
                    "expected supported function argument in executable builds",
                ) {
                    lowered_args.push(arg);
                }
            }

            if lowered_args.len() != args.len() {
                return None;
            }

            LoweredExpr {
                type_: signature.return_type,
                kind: LoweredExprKind::Call {
                    name: name.clone(),
                    args: lowered_args,
                },
            }
        }
        _ => {
            diagnostics.push(Diagnostic::error(expr.span, message));
            return None;
        }
    };

    if let Some(expected_type) = expected_type {
        if lowered.type_ != expected_type {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "expected value of type `{}`, got `{}`",
                    expected_type.name(),
                    lowered.type_.name()
                ),
            ));
            return None;
        }
    }

    Some(lowered)
}
