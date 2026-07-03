use std::collections::HashMap;

use crate::ast::{
    BasicType, BinaryOp, Expr, ExprKind, FunctionBody, FunctionDecl, Item, Program, Stmt, StmtKind,
    StructDecl, StructInitField, StructMember, TypeRef,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub structs: Vec<LoweredStruct>,
    pub functions: Vec<LoweredFunction>,
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStruct {
    pub name: String,
    pub fields: Vec<LoweredField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredField {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredFunction {
    pub name: String,
    pub params: Vec<LoweredParam>,
    pub return_type: LoweredType,
    pub statements: Vec<LoweredStatement>,
    pub return_value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredParam {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    Local { name: String, value: LoweredExpr },
    Println(LoweredExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredExpr {
    pub type_: LoweredType,
    pub kind: LoweredExprKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredType {
    Basic(BasicType),
    Struct(String),
}

impl LoweredType {
    fn name(&self) -> String {
        match self {
            LoweredType::Basic(type_) => type_.name().to_string(),
            LoweredType::Struct(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredExprKind {
    StringLiteral(String),
    BoolLiteral(bool),
    NumberLiteral(String),
    Local(String),
    StringConcat(Box<LoweredExpr>, Box<LoweredExpr>),
    StructLiteral {
        name: String,
        fields: Vec<LoweredStructFieldValue>,
    },
    FieldAccess {
        object: Box<LoweredExpr>,
        field: String,
    },
    Call {
        name: String,
        args: Vec<LoweredExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStructFieldValue {
    pub name: String,
    pub value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
    params: Vec<LoweredType>,
    return_type: LoweredType,
}

pub fn lower_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut main = None;
    let mut structs = HashMap::new();
    let mut signatures = HashMap::new();

    for item in &program.items {
        match item {
            Item::Struct(item) => {
                if let Some(struct_) = lower_struct_definition(item, &mut diagnostics) {
                    structs.insert(item.name.clone(), struct_);
                }
            }
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

        if let Some(function) = lower_function(function, &signatures, &structs, &mut diagnostics) {
            functions.push(function);
        }
    }

    let statements = lower_main(main, &signatures, &structs, &mut diagnostics);

    if diagnostics.is_empty() {
        let mut structs = structs.into_values().collect::<Vec<_>>();
        structs.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(LoweredProgram {
            structs,
            functions,
            statements,
        })
    } else {
        Err(diagnostics)
    }
}

fn lower_struct_definition(
    item: &StructDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStruct> {
    let mut fields = Vec::new();
    let mut field_names = HashMap::new();
    let mut can_lower = true;

    for member in &item.members {
        match member {
            StructMember::Field(field) => {
                if field_names.insert(field.name.clone(), field.span).is_some() {
                    diagnostics.push(Diagnostic::error(
                        field.span,
                        format!("duplicate field `{}` in struct `{}`", field.name, item.name),
                    ));
                    can_lower = false;
                }

                let Some(type_) = lower_basic_type_ref(
                    &field.type_ref,
                    diagnostics,
                    "only basic struct field types are supported in executable builds",
                ) else {
                    can_lower = false;
                    continue;
                };

                fields.push(LoweredField {
                    name: field.name.clone(),
                    type_: LoweredType::Basic(type_),
                });
            }
            StructMember::Method(method) => {
                diagnostics.push(Diagnostic::error(
                    method.span,
                    "methods are not supported in executable builds",
                ));
                can_lower = false;
            }
        }
    }

    if can_lower {
        Some(LoweredStruct {
            name: item.name.clone(),
            fields,
        })
    } else {
        None
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

        let Some(type_) = lower_basic_type_ref(
            type_ref,
            diagnostics,
            "only basic parameter types are supported in executable builds",
        ) else {
            can_lower = false;
            continue;
        };

        params.push(LoweredType::Basic(type_));
    }

    let Some(return_type) = &function.return_type else {
        diagnostics.push(Diagnostic::error(
            function.span,
            "helper functions must declare a return type in executable builds",
        ));
        return None;
    };

    let Some(return_type) = lower_basic_type_ref(
        return_type,
        diagnostics,
        "only basic return types are supported in executable builds",
    ) else {
        return None;
    };

    if can_lower {
        Some(FunctionSignature {
            params,
            return_type: LoweredType::Basic(return_type),
        })
    } else {
        None
    }
}

fn lower_basic_type_ref(
    type_ref: &TypeRef,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<BasicType> {
    if !type_ref.args.is_empty() {
        diagnostics.push(Diagnostic::error(
            type_ref.span,
            "generic types are not supported in executable builds",
        ));
        return None;
    }

    let Some(type_) = BasicType::from_name(&type_ref.name) else {
        diagnostics.push(Diagnostic::error(type_ref.span, message));
        return None;
    };

    Some(type_)
}

fn lower_function(
    function: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
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
        if locals.insert(param.name.clone(), type_.clone()).is_some() {
            diagnostics.push(Diagnostic::error(
                param.span,
                format!("duplicate local `{}` in executable build", param.name),
            ));
        }

        params.push(LoweredParam {
            name: param.name.clone(),
            type_: type_.clone(),
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
                        structs,
                        diagnostics,
                        Some(signature.return_type.clone()),
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
                    lower_local_statement(statement, &mut locals, signatures, structs, diagnostics)
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
        return_type: signature.return_type.clone(),
        statements,
        return_value,
    })
}

fn lower_main(
    main: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
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
                            structs,
                            diagnostics,
                            None,
                            "`io.println` only accepts `String` values in executable builds",
                        ) {
                            if value.type_ != LoweredType::Basic(BasicType::String) {
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
                        if let Some(statement) = lower_local_statement(
                            statement,
                            &mut locals,
                            signatures,
                            structs,
                            diagnostics,
                        ) {
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
    locals: &mut HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
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
            Some(LoweredType::Basic(type_))
        } else if structs.contains_key(&type_annotation.name) {
            Some(LoweredType::Struct(type_annotation.name.clone()))
        } else {
            diagnostics.push(Diagnostic::error(
                type_annotation.span,
                "only basic and struct local types are supported in executable builds",
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
            structs,
            diagnostics,
            annotated_type.clone(),
            "only literal, string concat, struct literal, field access, and function call local values are supported in executable builds",
        )
    } else if let Some(type_) = annotated_type.clone() {
        let kind = match type_ {
            LoweredType::Basic(BasicType::String) => LoweredExprKind::StringLiteral(String::new()),
            LoweredType::Basic(BasicType::Bool) => LoweredExprKind::BoolLiteral(false),
            LoweredType::Basic(type_) if type_.is_numeric() => {
                LoweredExprKind::NumberLiteral("0".to_string())
            }
            LoweredType::Struct(_) => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "struct locals must include an initializer in executable builds",
                ));
                return None;
            }
            LoweredType::Basic(_) => unreachable!("all basic types have default values"),
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

    if locals.insert(name.clone(), value.type_.clone()).is_some() {
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

fn lower_struct_init(
    expr: &Expr,
    name: &str,
    fields: &[StructInitField],
    locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let Some(struct_) = structs.get(name) else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("unknown struct `{name}` in executable build"),
        ));
        return None;
    };

    let mut lowered_fields = Vec::new();
    let mut seen_fields = HashMap::new();
    let mut can_lower = true;

    for field in fields {
        if seen_fields.insert(field.name.clone(), field.span).is_some() {
            diagnostics.push(Diagnostic::error(
                field.span,
                format!("duplicate field `{}` in struct literal", field.name),
            ));
            can_lower = false;
        }

        let Some(expected_field) = struct_
            .fields
            .iter()
            .find(|expected_field| expected_field.name == field.name)
        else {
            diagnostics.push(Diagnostic::error(
                field.span,
                format!("unknown field `{}` for struct `{name}`", field.name),
            ));
            can_lower = false;
            continue;
        };

        if let Some(value) = lower_expr(
            &field.value,
            locals,
            signatures,
            structs,
            diagnostics,
            Some(expected_field.type_.clone()),
            "expected supported struct field value in executable builds",
        ) {
            lowered_fields.push(LoweredStructFieldValue {
                name: field.name.clone(),
                value,
            });
        }
    }

    for field in &struct_.fields {
        if !seen_fields.contains_key(&field.name) {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("missing field `{}` in struct literal `{name}`", field.name),
            ));
            can_lower = false;
        }
    }

    if !can_lower || lowered_fields.len() != fields.len() {
        return None;
    }

    Some(LoweredExpr {
        type_: LoweredType::Struct(name.to_string()),
        kind: LoweredExprKind::StructLiteral {
            name: name.to_string(),
            fields: lowered_fields,
        },
    })
}

fn lower_expr(
    expr: &Expr,
    locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
    message: &str,
) -> Option<LoweredExpr> {
    let lowered = match &expr.kind {
        ExprKind::String(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::String),
            kind: LoweredExprKind::StringLiteral(value.clone()),
        },
        ExprKind::Bool(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::Bool),
            kind: LoweredExprKind::BoolLiteral(*value),
        },
        ExprKind::Number(value) => {
            let type_ = if let Some(LoweredType::Basic(type_)) = expected_type.as_ref() {
                if type_.is_numeric() {
                    *type_
                } else {
                    BasicType::I32
                }
            } else {
                BasicType::I32
            };

            LoweredExpr {
                type_: LoweredType::Basic(type_),
                kind: LoweredExprKind::NumberLiteral(value.clone()),
            }
        }
        ExprKind::Identifier(name) if locals.contains_key(name) => LoweredExpr {
            type_: locals[name].clone(),
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
                structs,
                diagnostics,
                Some(LoweredType::Basic(BasicType::String)),
                "expected `String` value in executable builds",
            );
            let right = lower_expr(
                right,
                locals,
                signatures,
                structs,
                diagnostics,
                Some(LoweredType::Basic(BasicType::String)),
                "expected `String` value in executable builds",
            );

            let (Some(left), Some(right)) = (left, right) else {
                return None;
            };

            LoweredExpr {
                type_: LoweredType::Basic(BasicType::String),
                kind: LoweredExprKind::StringConcat(Box::new(left), Box::new(right)),
            }
        }
        ExprKind::StructInit { name, fields } => {
            lower_struct_init(expr, name, fields, locals, signatures, structs, diagnostics)?
        }
        ExprKind::Member { object, name } => {
            let object = lower_expr(
                object,
                locals,
                signatures,
                structs,
                diagnostics,
                None,
                "expected supported field access object in executable builds",
            )?;

            let LoweredType::Struct(struct_name) = &object.type_ else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "field access requires a struct value in executable builds",
                ));
                return None;
            };

            let Some(struct_) = structs.get(struct_name) else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown struct `{struct_name}` in executable build"),
                ));
                return None;
            };

            let Some(field) = struct_.fields.iter().find(|field| field.name == *name) else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown field `{name}` for struct `{struct_name}`"),
                ));
                return None;
            };

            LoweredExpr {
                type_: field.type_.clone(),
                kind: LoweredExprKind::FieldAccess {
                    object: Box::new(object),
                    field: name.clone(),
                },
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
                    structs,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported function argument in executable builds",
                ) {
                    lowered_args.push(arg);
                }
            }

            if lowered_args.len() != args.len() {
                return None;
            }

            LoweredExpr {
                type_: signature.return_type.clone(),
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
