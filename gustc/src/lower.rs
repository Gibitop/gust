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
    Expr(LoweredExpr),
    Return(Option<LoweredExpr>),
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
    Void,
}

impl LoweredType {
    fn name(&self) -> String {
        match self {
            LoweredType::Basic(type_) => type_.name().to_string(),
            LoweredType::Struct(name) => name.clone(),
            LoweredType::Void => "void".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredExprKind {
    Void,
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

    let mut has_return_type_conflict = false;

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
            Item::Function(_) => {}
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

    for item in &program.items {
        let Item::Function(function) = item else {
            continue;
        };

        let Some(name) = &function.name else {
            continue;
        };

        if name == "main" {
            continue;
        }

        if let Some(signature) = lower_function_signature(function, &structs, &mut diagnostics) {
            signatures.insert(name.clone(), signature);
        }
    }

    for _ in 0..signatures.len() {
        let mut changed = false;

        for item in &program.items {
            let Item::Function(function) = item else {
                continue;
            };
            let Some(name) = &function.name else {
                continue;
            };

            if name == "main" || function.return_type.is_some() {
                continue;
            }

            let Ok(Some(return_type)) = infer_function_return_type(function, &signatures, &structs)
            else {
                continue;
            };
            let Some(signature) = signatures.get_mut(name) else {
                continue;
            };

            if signature.return_type != return_type {
                signature.return_type = return_type;
                changed = true;
            }
        }

        if !changed {
            break;
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

    for item in &program.items {
        let Item::Function(function) = item else {
            continue;
        };
        let Some(name) = &function.name else {
            continue;
        };

        if name == "main" || function.return_type.is_some() {
            continue;
        }

        if let Err(conflict) = infer_function_return_type(function, &signatures, &structs) {
            has_return_type_conflict = true;
            diagnostics.push(Diagnostic::error(
                conflict.span,
                format!(
                    "function `{name}` has multiple return types (`{}` and `{}`); inferred return types must be consistent",
                    conflict.first.name(),
                    conflict.second.name()
                ),
            ));
        }
    }

    if has_return_type_conflict {
        return Err(diagnostics);
    }

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
    structs: &HashMap<String, LoweredStruct>,
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

        let Some(type_) = lower_value_type_ref(
            type_ref,
            structs,
            diagnostics,
            "only basic and known struct parameter types are supported in executable builds",
        ) else {
            can_lower = false;
            continue;
        };

        params.push(type_);
    }

    let return_type = if let Some(return_type) = &function.return_type {
        let Some(return_type) = lower_value_type_ref(
            return_type,
            structs,
            diagnostics,
            "only basic and known struct return types are supported in executable builds",
        ) else {
            return None;
        };
        return_type
    } else {
        LoweredType::Void
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

fn lower_value_type_ref(
    type_ref: &TypeRef,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if !type_ref.args.is_empty() {
        diagnostics.push(Diagnostic::error(
            type_ref.span,
            "generic types are not supported in executable builds",
        ));
        return None;
    }

    if let Some(type_) = BasicType::from_name(&type_ref.name) {
        return Some(LoweredType::Basic(type_));
    }

    if type_ref.name == "void" {
        return Some(LoweredType::Void);
    }

    if structs.contains_key(&type_ref.name) {
        return Some(LoweredType::Struct(type_ref.name.clone()));
    }

    diagnostics.push(Diagnostic::error(type_ref.span, message));
    None
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

fn infer_function_return_type(
    function: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
) -> Result<Option<LoweredType>, ReturnTypeConflict> {
    let mut locals = HashMap::new();

    for param in &function.params {
        let Some(type_ref) = param.type_ref.as_ref() else {
            return Ok(None);
        };
        let type_ = if let Some(type_) = BasicType::from_name(&type_ref.name) {
            LoweredType::Basic(type_)
        } else if let Some(struct_) = structs.get(&type_ref.name) {
            LoweredType::Struct(struct_.name.clone())
        } else {
            return Ok(None);
        };
        locals.insert(param.name.clone(), type_);
    }

    match &function.body {
        FunctionBody::Expr(expr) => Ok(infer_expr_type(expr, &locals, signatures, structs)),
        FunctionBody::Block(block) => {
            let mut return_type = None;

            for statement in &block.statements {
                match &statement.kind {
                    StmtKind::Let { name, value, .. } => {
                        if let Some(value) = value
                            && let Some(type_) =
                                infer_expr_type(value, &locals, signatures, structs)
                        {
                            locals.insert(name.clone(), type_);
                        }
                    }
                    StmtKind::Return { value: Some(value) } => {
                        if let Some(type_) = infer_expr_type(value, &locals, signatures, structs) {
                            merge_inferred_return_type(&mut return_type, type_, value.span)?;
                        }
                    }
                    StmtKind::Return { value: None } => {
                        merge_inferred_return_type(
                            &mut return_type,
                            LoweredType::Void,
                            statement.span,
                        )?;
                    }
                    StmtKind::For { .. } | StmtKind::Expr(_) => {}
                }
            }

            Ok(Some(return_type.unwrap_or(LoweredType::Void)))
        }
    }
}

struct ReturnTypeConflict {
    span: Span,
    first: LoweredType,
    second: LoweredType,
}

fn merge_inferred_return_type(
    return_type: &mut Option<LoweredType>,
    next_type: LoweredType,
    span: Span,
) -> Result<(), ReturnTypeConflict> {
    let Some(current_type) = return_type else {
        *return_type = Some(next_type);
        return Ok(());
    };

    if *current_type == next_type {
        return Ok(());
    }

    Err(ReturnTypeConflict {
        span,
        first: current_type.clone(),
        second: next_type,
    })
}

fn infer_expr_type(
    expr: &Expr,
    locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
) -> Option<LoweredType> {
    match &expr.kind {
        ExprKind::String(_)
        | ExprKind::Binary {
            op: BinaryOp::Add, ..
        } => Some(LoweredType::Basic(BasicType::String)),
        ExprKind::Bool(_) => Some(LoweredType::Basic(BasicType::Bool)),
        ExprKind::Number(_) => Some(LoweredType::Basic(BasicType::I32)),
        ExprKind::Identifier(name) => locals.get(name).cloned(),
        ExprKind::StructInit { name, .. } if structs.contains_key(name) => {
            Some(LoweredType::Struct(name.clone()))
        }
        ExprKind::Member { object, name } => {
            let LoweredType::Struct(struct_name) =
                infer_expr_type(object, locals, signatures, structs)?
            else {
                return None;
            };
            structs
                .get(&struct_name)?
                .fields
                .iter()
                .find(|field| field.name == *name)
                .map(|field| field.type_.clone())
        }
        ExprKind::Call { callee, .. } => {
            let ExprKind::Identifier(name) = &callee.kind else {
                return None;
            };
            signatures
                .get(name)
                .map(|signature| signature.return_type.clone())
        }
        ExprKind::Array(_)
        | ExprKind::StructInit { .. }
        | ExprKind::Binary {
            op: BinaryOp::GreaterEqual,
            ..
        }
        | ExprKind::Lambda(_)
        | ExprKind::Match { .. }
        | ExprKind::Missing
        | ExprKind::PostfixIncrement(_) => None,
    }
}

fn lower_function(
    function: &FunctionDecl,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredFunction> {
    let name = function.name.as_ref()?;
    let signature = signatures.get(name)?;

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

    match &function.body {
        FunctionBody::Expr(expr) => {
            return_value = lower_expr(
                expr,
                &locals,
                signatures,
                structs,
                diagnostics,
                Some(signature.return_type.clone()),
                "expected supported arrow function value in executable builds",
            );
        }
        FunctionBody::Block(block) => {
            for (index, statement) in block.statements.iter().enumerate() {
                let is_last = index + 1 == block.statements.len();

                match &statement.kind {
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
                    StmtKind::Return { value } => {
                        let value = value.as_ref().and_then(|value| {
                            lower_expr(
                                value,
                                &locals,
                                signatures,
                                structs,
                                diagnostics,
                                Some(signature.return_type.clone()),
                                "expected supported return value in executable builds",
                            )
                        });

                        if is_last && value.is_some() {
                            return_value = value;
                        } else {
                            statements.push(LoweredStatement::Return(value));
                        }
                    }
                    StmtKind::Expr(expr) => {
                        if let Some(statement) = lower_expression_statement(
                            expr,
                            &locals,
                            signatures,
                            structs,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::For { .. } => diagnostics.push(Diagnostic::error(
                        statement.span,
                        "for loops are not supported in executable builds",
                    )),
                }
            }
        }
    }

    let return_value = return_value.unwrap_or_else(void_expr);

    if signature.return_type != LoweredType::Void
        && return_value.type_ == LoweredType::Void
        && !statements
            .iter()
            .any(|statement| matches!(statement, LoweredStatement::Return(Some(_))))
    {
        diagnostics.push(Diagnostic::error(
            function.span,
            "function must return a value in executable builds",
        ));
    }

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
                        if let Some(statement) = lower_expression_statement(
                            expr,
                            &locals,
                            signatures,
                            structs,
                            diagnostics,
                        ) {
                            statements.push(statement);
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

fn lower_expression_statement(
    expr: &Expr,
    locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStatement> {
    let ExprKind::Call { callee, args } = &expr.kind else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            "only function calls are supported as expression statements in executable builds",
        ));
        return None;
    };

    let is_io_println = match &callee.kind {
        ExprKind::Member { object, name } if name == "println" => {
            matches!(&object.kind, ExprKind::Identifier(name) if name == "io")
        }
        _ => false,
    };

    if is_io_println {
        if args.len() != 1 {
            diagnostics.push(Diagnostic::error(
                expr.span,
                "`io.println` expects exactly one `String` value in executable builds",
            ));
            return None;
        }

        let value = lower_expr(
            &args[0],
            locals,
            signatures,
            structs,
            diagnostics,
            None,
            "`io.println` only accepts `String` values in executable builds",
        )?;

        if value.type_ != LoweredType::Basic(BasicType::String) {
            diagnostics.push(Diagnostic::error(
                args[0].span,
                format!(
                    "`io.println` only accepts `String` values in executable builds, got `{}`",
                    value.type_.name()
                ),
            ));
            return None;
        }

        return Some(LoweredStatement::Println(value));
    }

    lower_expr(
        expr,
        locals,
        signatures,
        structs,
        diagnostics,
        None,
        "expected supported function call in executable builds",
    )
    .map(LoweredStatement::Expr)
}

fn void_expr() -> LoweredExpr {
    LoweredExpr {
        type_: LoweredType::Void,
        kind: LoweredExprKind::Void,
    }
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
            LoweredType::Void => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "`void` locals are not supported in executable builds",
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
