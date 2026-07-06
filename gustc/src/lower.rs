use std::collections::HashMap;

use crate::ast::{
    BasicType, BinaryOp, Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, Item,
    MatchBranchBody, Pattern, Program, Stmt, StmtKind, StructDecl, StructInitField, StructMember,
    TypeRef, UnaryOp, number_literal_is_float,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub structs: Vec<LoweredStruct>,
    pub enums: Vec<LoweredEnum>,
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
pub struct LoweredEnum {
    pub name: String,
    pub variants: Vec<LoweredVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredVariant {
    pub name: String,
    pub payload: Option<LoweredType>,
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
    Local {
        name: String,
        value: LoweredExpr,
    },
    Assignment {
        target: LoweredExpr,
        value: LoweredExpr,
    },
    Println(LoweredExpr),
    Expr(LoweredExpr),
    Return(Option<LoweredExpr>),
    If {
        condition: LoweredExpr,
        then_branch: Vec<LoweredStatement>,
        else_branch: Option<Vec<LoweredStatement>>,
    },
    While {
        condition: LoweredExpr,
        body: Vec<LoweredStatement>,
    },
    Break,
    Continue,
    Match {
        value: LoweredExpr,
        temp_name: String,
        branches: Vec<LoweredMatchStatementBranch>,
    },
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
    Enum(String),
    Void,
}

impl LoweredType {
    fn name(&self) -> String {
        match self {
            LoweredType::Basic(type_) => type_.name().to_string(),
            LoweredType::Struct(name) => name.clone(),
            LoweredType::Enum(name) => name.clone(),
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
    PostfixIncrement(Box<LoweredExpr>),
    StringConcat(Box<LoweredExpr>, Box<LoweredExpr>),
    Not(Box<LoweredExpr>),
    Negate(Box<LoweredExpr>),
    Arithmetic {
        left: Box<LoweredExpr>,
        op: BinaryOp,
        right: Box<LoweredExpr>,
    },
    Logical {
        left: Box<LoweredExpr>,
        op: BinaryOp,
        right: Box<LoweredExpr>,
    },
    Comparison {
        left: Box<LoweredExpr>,
        op: BinaryOp,
        right: Box<LoweredExpr>,
    },
    StructLiteral {
        name: String,
        fields: Vec<LoweredStructFieldValue>,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
        payload: Option<Box<LoweredExpr>>,
    },
    EnumPayload {
        object: Box<LoweredExpr>,
        variant: String,
    },
    MatchValue(String),
    Match {
        value: Box<LoweredExpr>,
        temp_name: String,
        branches: Vec<LoweredMatchBranch>,
    },
    FieldAccess {
        object: Box<LoweredExpr>,
        field: String,
    },
    Clone(Box<LoweredExpr>),
    NumberToString(Box<LoweredExpr>),
    Call {
        name: String,
        args: Vec<LoweredExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredMatchBranch {
    pub pattern: LoweredPattern,
    pub statements: Vec<LoweredStatement>,
    pub value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredMatchStatementBranch {
    pub pattern: LoweredPattern,
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredPattern {
    Variant { enum_name: String, variant: String },
    String(String),
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStructFieldValue {
    pub name: String,
    pub value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
    params: Vec<LoweredParamSignature>,
    return_type: LoweredType,
    return_type_known: bool,
    mutable_self: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoweredParamSignature {
    type_: LoweredType,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct LoweringLocal {
    type_: LoweredType,
    mutable: bool,
    replacement: Option<LoweredExpr>,
}

fn is_self_param(param: &crate::ast::Param) -> bool {
    param.name == "self"
}

fn has_mutable_receiver(function: &FunctionDecl) -> bool {
    function
        .params
        .iter()
        .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none())
}

fn method_name(struct_name: &str, method_name: &str) -> String {
    format!("{struct_name}.{method_name}")
}

fn extension_name(type_name: &str, function_name: &str) -> String {
    format!("extension {type_name}.{function_name}")
}

fn source_callable_name(name: &str) -> &str {
    name.rsplit_once("::").map_or(name, |(_, name)| name)
}

fn static_method_name(type_name: &str, function_name: &str) -> String {
    format!("static {type_name}.{function_name}")
}

fn static_extension_name(type_name: &str, function_name: &str) -> String {
    format!("static extension {type_name}.{function_name}")
}

fn callable_method_name(
    type_: &LoweredType,
    name: &str,
    signatures: &HashMap<String, FunctionSignature>,
) -> Option<String> {
    if let LoweredType::Struct(struct_name) = type_ {
        let name = method_name(struct_name, source_callable_name(name));
        if signatures.contains_key(&name) {
            return Some(name);
        }
    }

    let name = extension_name(&type_.name(), name);
    signatures.contains_key(&name).then_some(name)
}

fn callable_static_name(
    type_: &LoweredType,
    name: &str,
    signatures: &HashMap<String, FunctionSignature>,
) -> Option<String> {
    let method_name = static_method_name(&type_.name(), source_callable_name(name));
    if signatures.contains_key(&method_name) {
        return Some(method_name);
    }

    let name = static_extension_name(&type_.name(), name);
    signatures.contains_key(&name).then_some(name)
}

pub fn lower_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    let program = crate::monomorphize::monomorphize(program)?;
    lower_monomorphized_program(&program)
}

fn lower_monomorphized_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let mut main = None;
    let mut structs = HashMap::new();
    let mut enums = HashMap::new();
    let mut signatures = HashMap::new();

    let mut has_return_type_conflict = false;
    let mut has_unresolved_return_type = false;

    for item in &program.items {
        match item {
            Item::Enum(item) => {
                enums.insert(
                    item.name.clone(),
                    LoweredEnum {
                        name: item.name.clone(),
                        variants: Vec::new(),
                    },
                );
            }
            Item::Struct(item) => {
                structs.insert(
                    item.name.clone(),
                    LoweredStruct {
                        name: item.name.clone(),
                        fields: Vec::new(),
                    },
                );
            }
            Item::Import(_) | Item::Extension(_) | Item::Function(_) => {}
        }
    }

    for item in &program.items {
        match item {
            Item::Struct(item) => {
                if let Some(struct_) =
                    lower_struct_definition(item, &structs, &enums, &mut diagnostics)
                {
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
            Item::Extension(_) => {}
            Item::Import(item) => diagnostics.push(Diagnostic::error(
                item.span,
                "imports are not supported in executable builds",
            )),
            Item::Enum(_) => {}
        }
    }

    for item in &program.items {
        if let Item::Enum(item) = item
            && let Some(enum_) = lower_enum_definition(item, &structs, &enums, &mut diagnostics)
        {
            enums.insert(item.name.clone(), enum_);
        }
    }

    let mut functions_to_lower = Vec::new();

    for item in &program.items {
        match item {
            Item::Struct(struct_) => {
                for member in &struct_.members {
                    let (function, lowered_name, has_self) = match member {
                        StructMember::Method(function) => (
                            function,
                            method_name(
                                &struct_.name,
                                function.name.as_deref().unwrap_or("<missing>"),
                            ),
                            true,
                        ),
                        StructMember::StaticMethod(function) => (
                            function,
                            static_method_name(
                                &struct_.name,
                                function.name.as_deref().unwrap_or("<missing>"),
                            ),
                            false,
                        ),
                        StructMember::Field(_) => continue,
                    };
                    if function.name.is_none() {
                        continue;
                    }
                    let self_type = LoweredType::Struct(struct_.name.clone());
                    let Some(mut signature) = lower_function_signature(
                        function,
                        Some(&self_type),
                        has_self,
                        &structs,
                        &enums,
                        &mut diagnostics,
                    ) else {
                        continue;
                    };
                    if has_self {
                        signature.params.insert(
                            0,
                            LoweredParamSignature {
                                type_: self_type.clone(),
                                mutable: false,
                            },
                        );
                    }
                    signatures.insert(lowered_name.clone(), signature);
                    functions_to_lower.push((lowered_name, function, Some(self_type), has_self));
                }
            }
            Item::Extension(extension) => {
                let Some(name) = &extension.function.name else {
                    continue;
                };
                let Some(self_type) = lower_value_type_ref(
                    &extension.type_ref,
                    &structs,
                    &enums,
                    &mut diagnostics,
                    "extension functions require a supported receiver type in executable builds",
                ) else {
                    continue;
                };
                let lowered_name = if extension.static_ {
                    static_extension_name(&self_type.name(), name)
                } else {
                    extension_name(&self_type.name(), name)
                };
                let Some(mut signature) = lower_function_signature(
                    &extension.function,
                    Some(&self_type),
                    !extension.static_,
                    &structs,
                    &enums,
                    &mut diagnostics,
                ) else {
                    continue;
                };
                if !extension.static_ {
                    signature.params.insert(
                        0,
                        LoweredParamSignature {
                            type_: self_type.clone(),
                            mutable: false,
                        },
                    );
                }
                signatures.insert(lowered_name.clone(), signature);
                functions_to_lower.push((
                    lowered_name,
                    &extension.function,
                    Some(self_type),
                    !extension.static_,
                ));
            }
            Item::Function(function) => {
                let Some(name) = &function.name else {
                    continue;
                };

                if name == "main" {
                    continue;
                }

                if let Some(signature) = lower_function_signature(
                    function,
                    None,
                    false,
                    &structs,
                    &enums,
                    &mut diagnostics,
                ) {
                    signatures.insert(name.clone(), signature);
                    functions_to_lower.push((name.clone(), function, None, false));
                }
            }
            Item::Import(_) | Item::Enum(_) => {}
        }
    }

    for _ in 0..signatures.len() {
        let mut changed = false;

        for (name, function, self_type, has_self) in &functions_to_lower {
            if function.return_type.is_some() {
                continue;
            }

            let Ok(Some(return_type)) = infer_function_return_type(
                function,
                self_type.as_ref(),
                *has_self,
                &signatures,
                &structs,
                &enums,
            ) else {
                continue;
            };
            let Some(signature) = signatures.get_mut(name) else {
                continue;
            };

            if !signature.return_type_known || signature.return_type != return_type {
                signature.return_type = return_type;
                signature.return_type_known = true;
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

    for (name, function, self_type, has_self) in &functions_to_lower {
        if function.return_type.is_some() {
            continue;
        }

        if let Err(conflict) = infer_function_return_type(
            function,
            self_type.as_ref(),
            *has_self,
            &signatures,
            &structs,
            &enums,
        ) {
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

    for (name, function, _, _) in &functions_to_lower {
        if function.return_type.is_some() {
            continue;
        }

        if signatures
            .get(name)
            .is_some_and(|signature| !signature.return_type_known)
        {
            has_unresolved_return_type = true;
            diagnostics.push(Diagnostic::error(
                function.span,
                format!(
                    "could not infer return type of function `{name}`; add an explicit return type"
                ),
            ));
        }
    }

    if has_unresolved_return_type {
        return Err(diagnostics);
    }

    let mut functions = Vec::new();

    for (name, function, self_type, has_self) in &functions_to_lower {
        if let Some(function) = lower_function(
            function,
            name,
            self_type.as_ref(),
            *has_self,
            &signatures,
            &structs,
            &enums,
            &mut diagnostics,
        ) {
            functions.push(function);
        }
    }

    let statements = lower_main(main, &signatures, &structs, &enums, &mut diagnostics);

    if diagnostics.is_empty() {
        let mut structs = structs.into_values().collect::<Vec<_>>();
        structs.sort_by(|left, right| left.name.cmp(&right.name));
        let mut enums = enums.into_values().collect::<Vec<_>>();
        enums.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(LoweredProgram {
            structs,
            enums,
            functions,
            statements,
        })
    } else {
        Err(diagnostics)
    }
}

fn lower_enum_definition(
    item: &crate::ast::EnumDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredEnum> {
    let mut variants = Vec::new();
    let mut variant_names = HashMap::new();
    let mut can_lower = true;

    if item.variants.is_empty() {
        diagnostics.push(Diagnostic::error(
            item.span,
            format!("enum `{}` must define at least one variant", item.name),
        ));
        can_lower = false;
    }

    for variant in &item.variants {
        if variant_names
            .insert(variant.name.clone(), variant.span)
            .is_some()
        {
            diagnostics.push(Diagnostic::error(
                variant.span,
                format!(
                    "duplicate variant `{}` in enum `{}`",
                    variant.name, item.name
                ),
            ));
            can_lower = false;
        }

        let payload = variant.payload.as_ref().and_then(|type_ref| {
            lower_value_type_ref(
                type_ref,
                structs,
                enums,
                diagnostics,
                "enum payloads only support basic and known struct or enum types in executable builds",
            )
        });

        if variant.payload.is_some() && payload.is_none() {
            can_lower = false;
        }

        variants.push(LoweredVariant {
            name: variant.name.clone(),
            payload,
        });
    }

    can_lower.then(|| LoweredEnum {
        name: item.name.clone(),
        variants,
    })
}

fn lower_struct_definition(
    item: &StructDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
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

                let Some(type_) = lower_value_type_ref(
                    &field.type_ref,
                    structs,
                    enums,
                    diagnostics,
                    "struct fields only support basic and known struct or enum types in executable builds",
                ) else {
                    can_lower = false;
                    continue;
                };

                fields.push(LoweredField {
                    name: field.name.clone(),
                    type_,
                });
            }
            StructMember::Method(_) | StructMember::StaticMethod(_) => {}
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
    self_type: Option<&LoweredType>,
    has_self: bool,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FunctionSignature> {
    let mut params = Vec::new();
    let mut can_lower = true;

    for param in function
        .params
        .iter()
        .filter(|param| !has_self || !is_self_param(param))
    {
        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "function parameters must include type annotations in executable builds",
            ));
            can_lower = false;
            continue;
        };

        let Some(type_) = lower_value_type_ref_in_context(
            type_ref,
            self_type,
            structs,
            enums,
            diagnostics,
            "only basic, known struct, and known enum parameter types are supported in executable builds",
        ) else {
            can_lower = false;
            continue;
        };

        params.push(LoweredParamSignature {
            type_,
            mutable: param.mutable,
        });
    }

    let (return_type, return_type_known) = if let Some(return_type) = &function.return_type {
        let Some(return_type) = lower_value_type_ref_in_context(
            return_type,
            self_type,
            structs,
            enums,
            diagnostics,
            "only basic, known struct, and known enum return types are supported in executable builds",
        ) else {
            return None;
        };
        (return_type, true)
    } else {
        (LoweredType::Void, false)
    };

    if can_lower {
        Some(FunctionSignature {
            params,
            return_type,
            return_type_known,
            mutable_self: has_self && has_mutable_receiver(function),
        })
    } else {
        None
    }
}

fn lower_value_type_ref_in_context(
    type_ref: &TypeRef,
    self_type: Option<&LoweredType>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if type_ref.name == "Self" && type_ref.args.is_empty() {
        return self_type.cloned().or_else(|| {
            diagnostics.push(Diagnostic::error(
                type_ref.span,
                "`Self` is only available in methods and extension functions",
            ));
            None
        });
    }

    lower_value_type_ref(type_ref, structs, enums, diagnostics, message)
}

fn lower_value_type_ref(
    type_ref: &TypeRef,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
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

    if enums.contains_key(&type_ref.name) {
        return Some(LoweredType::Enum(type_ref.name.clone()));
    }

    diagnostics.push(Diagnostic::error(type_ref.span, message));
    None
}

fn infer_function_return_type(
    function: &FunctionDecl,
    self_type: Option<&LoweredType>,
    has_self: bool,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
) -> Result<Option<LoweredType>, ReturnTypeConflict> {
    let mut locals = HashMap::new();

    if let Some(self_type) = self_type {
        locals.insert("Self".to_string(), self_type.clone());
        if has_self {
            locals.insert("self".to_string(), self_type.clone());
        }
    }

    for param in function
        .params
        .iter()
        .filter(|param| !has_self || !is_self_param(param))
    {
        let Some(type_ref) = param.type_ref.as_ref() else {
            return Ok(None);
        };
        let type_ = if let Some(type_) = BasicType::from_name(&type_ref.name) {
            LoweredType::Basic(type_)
        } else if let Some(struct_) = structs.get(&type_ref.name) {
            LoweredType::Struct(struct_.name.clone())
        } else if let Some(enum_) = enums.get(&type_ref.name) {
            LoweredType::Enum(enum_.name.clone())
        } else {
            return Ok(None);
        };
        locals.insert(param.name.clone(), type_);
    }

    match &function.body {
        FunctionBody::Expr(expr) => Ok(infer_expr_type(expr, &locals, signatures, structs, enums)),
        FunctionBody::Block(block) => {
            let mut return_type = None;
            let mut has_unresolved_value_return = false;
            infer_block_return_types(
                block,
                &mut locals,
                signatures,
                structs,
                enums,
                &mut return_type,
                &mut has_unresolved_value_return,
            )?;

            if return_type.is_none() && has_unresolved_value_return {
                Ok(None)
            } else {
                Ok(Some(return_type.unwrap_or(LoweredType::Void)))
            }
        }
    }
}

fn infer_block_return_types(
    block: &Block,
    locals: &mut HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    return_type: &mut Option<LoweredType>,
    has_unresolved_value_return: &mut bool,
) -> Result<(), ReturnTypeConflict> {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value
                    && let Some(type_) = infer_expr_type(value, locals, signatures, structs, enums)
                {
                    locals.insert(name.clone(), type_);
                }
            }
            StmtKind::Return { value: Some(value) } => {
                if let Some(type_) = infer_expr_type(value, locals, signatures, structs, enums) {
                    merge_inferred_return_type(return_type, type_, value.span)?;
                } else {
                    *has_unresolved_value_return = true;
                }
            }
            StmtKind::Return { value: None } => {
                merge_inferred_return_type(return_type, LoweredType::Void, statement.span)?;
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                let mut branch_locals = locals.clone();
                infer_block_return_types(
                    then_branch,
                    &mut branch_locals,
                    signatures,
                    structs,
                    enums,
                    return_type,
                    has_unresolved_value_return,
                )?;

                if let Some(else_branch) = else_branch {
                    let mut branch_locals = locals.clone();

                    match else_branch {
                        ElseBranch::Block(block) => infer_block_return_types(
                            block,
                            &mut branch_locals,
                            signatures,
                            structs,
                            enums,
                            return_type,
                            has_unresolved_value_return,
                        )?,
                        ElseBranch::If(statement) => {
                            let block = Block {
                                statements: vec![(**statement).clone()],
                                span: statement.span,
                            };
                            infer_block_return_types(
                                &block,
                                &mut branch_locals,
                                signatures,
                                structs,
                                enums,
                                return_type,
                                has_unresolved_value_return,
                            )?;
                        }
                    }
                }
            }
            StmtKind::While { body, .. } => {
                infer_block_return_types(
                    body,
                    locals,
                    signatures,
                    structs,
                    enums,
                    return_type,
                    has_unresolved_value_return,
                )?;
            }
            StmtKind::Assign { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::For { .. }
            | StmtKind::Expr(_) => {}
        }
    }

    Ok(())
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
    enums: &HashMap<String, LoweredEnum>,
) -> Option<LoweredType> {
    match &expr.kind {
        ExprKind::String(_) => Some(LoweredType::Basic(BasicType::String)),
        ExprKind::Bool(_)
        | ExprKind::Unary {
            op: UnaryOp::Not, ..
        }
        | ExprKind::Binary {
            op:
                BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual,
            ..
        } => Some(LoweredType::Basic(BasicType::Bool)),
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } => infer_expr_type(operand, locals, signatures, structs, enums),
        ExprKind::Binary {
            left,
            op:
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight,
            right,
        } => {
            if number_pair_contains_float(left, right) {
                Some(LoweredType::Basic(BasicType::F64))
            } else if matches!(left.kind, ExprKind::Number(_))
                && !matches!(right.kind, ExprKind::Number(_))
            {
                infer_expr_type(right, locals, signatures, structs, enums)
            } else {
                infer_expr_type(left, locals, signatures, structs, enums)
            }
        }
        ExprKind::Number(value) => Some(LoweredType::Basic(if number_literal_is_float(value) {
            BasicType::F64
        } else {
            BasicType::I32
        })),
        ExprKind::Identifier(name) => locals.get(name).cloned(),
        ExprKind::StructInit { name, .. } => {
            let name = if name == "Self" {
                let LoweredType::Struct(name) = locals.get("Self")? else {
                    return None;
                };
                name
            } else {
                name
            };
            structs
                .contains_key(name)
                .then(|| LoweredType::Struct(name.clone()))
        }
        ExprKind::Member { object, name } => {
            if let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
                && variant.payload.is_none()
            {
                return Some(LoweredType::Enum(enum_name.clone()));
            }

            let LoweredType::Struct(struct_name) =
                infer_expr_type(object, locals, signatures, structs, enums)?
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
            if let ExprKind::Member { object, name } = &callee.kind
                && name == "clone"
            {
                return infer_expr_type(object, locals, signatures, structs, enums);
            }

            if let ExprKind::Member { object, name } = &callee.kind
                && let ExprKind::Identifier(enum_name) = &object.kind
                && find_qualified_variant(enums, enum_name, name).is_some()
            {
                return Some(LoweredType::Enum(enum_name.clone()));
            }

            if let ExprKind::Member { object, name } = &callee.kind
                && let Some(type_) = infer_type_expression(object, locals, structs, enums)
            {
                return signatures
                    .get(&callable_static_name(&type_, name, signatures)?)
                    .filter(|signature| signature.return_type_known)
                    .map(|signature| signature.return_type.clone());
            }

            if let ExprKind::Member { object, name } = &callee.kind {
                let object_type = infer_expr_type(object, locals, signatures, structs, enums)?;
                return signatures
                    .get(&callable_method_name(&object_type, name, signatures)?)
                    .filter(|signature| signature.return_type_known)
                    .map(|signature| signature.return_type.clone());
            }

            let ExprKind::Identifier(name) = &callee.kind else {
                return None;
            };

            signatures
                .get(name)
                .filter(|signature| signature.return_type_known)
                .map(|signature| signature.return_type.clone())
        }
        ExprKind::Array(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Lambda(_)
        | ExprKind::Missing => None,
        ExprKind::Match { branches, .. } => branches.iter().find_map(|branch| {
            let MatchBranchBody::Expr(expr) = &branch.body else {
                return None;
            };
            infer_expr_type(expr, locals, signatures, structs, enums)
        }),
        ExprKind::PostfixIncrement(target) => {
            infer_expr_type(target, locals, signatures, structs, enums)
        }
    }
}

fn infer_type_expression(
    expr: &Expr,
    locals: &HashMap<String, LoweredType>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
) -> Option<LoweredType> {
    let ExprKind::Identifier(name) = &expr.kind else {
        return None;
    };

    if name == "Self" {
        return locals.get(name).cloned();
    }
    if locals.contains_key(name) {
        return None;
    }
    if let Some(type_) = BasicType::from_name(name) {
        Some(LoweredType::Basic(type_))
    } else if structs.contains_key(name) {
        Some(LoweredType::Struct(name.clone()))
    } else if enums.contains_key(name) {
        Some(LoweredType::Enum(name.clone()))
    } else {
        None
    }
}

fn lower_type_expression(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
) -> Option<LoweredType> {
    let ExprKind::Identifier(name) = &expr.kind else {
        return None;
    };

    if name == "Self" {
        return locals.get(name).map(|local| local.type_.clone());
    }
    if locals.contains_key(name) {
        return None;
    }
    if let Some(type_) = BasicType::from_name(name) {
        Some(LoweredType::Basic(type_))
    } else if structs.contains_key(name) {
        Some(LoweredType::Struct(name.clone()))
    } else if enums.contains_key(name) {
        Some(LoweredType::Enum(name.clone()))
    } else {
        None
    }
}

fn expression_has_mutable_capability(expr: &Expr, locals: &HashMap<String, LoweringLocal>) -> bool {
    match &expr.kind {
        ExprKind::Identifier(name) => locals.get(name).is_some_and(|local| local.mutable),
        ExprKind::Member { object, .. } => expression_has_mutable_capability(object, locals),
        ExprKind::StructInit { .. }
        | ExprKind::String(_)
        | ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Binary { .. }
        | ExprKind::Unary { .. } => true,
        ExprKind::Call { callee, .. } => {
            matches!(&callee.kind, ExprKind::Member { name, .. } if name == "clone")
        }
        ExprKind::Array(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Lambda(_)
        | ExprKind::Match { .. }
        | ExprKind::PostfixIncrement(_)
        | ExprKind::Missing => false,
    }
}

fn lowered_expression_has_mutable_capability(
    expr: &LoweredExpr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
) -> bool {
    match &expr.kind {
        LoweredExprKind::Local(name) => locals.get(name).is_some_and(|local| local.mutable),
        LoweredExprKind::FieldAccess { object, .. } => {
            lowered_expression_has_mutable_capability(object, locals, signatures, structs)
        }
        LoweredExprKind::StructLiteral { name, fields } => {
            let Some(struct_) = structs.get(name) else {
                return false;
            };

            fields.iter().all(|field| {
                struct_
                    .fields
                    .iter()
                    .find(|definition| definition.name == field.name)
                    .is_none_or(|definition| {
                        !matches!(definition.type_, LoweredType::Struct(_))
                            || lowered_expression_has_mutable_capability(
                                &field.value,
                                locals,
                                signatures,
                                structs,
                            )
                    })
            })
        }
        LoweredExprKind::Clone(_) => true,
        LoweredExprKind::Call { name, args } => signatures.get(name).is_some_and(|signature| {
            matches!(signature.return_type, LoweredType::Struct(_))
                && args.iter().zip(&signature.params).all(|(arg, param)| {
                    !matches!(param.type_, LoweredType::Struct(_))
                        || lowered_expression_has_mutable_capability(
                            arg, locals, signatures, structs,
                        )
                })
        }),
        LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::StringConcat(_, _)
        | LoweredExprKind::Not(_)
        | LoweredExprKind::Negate(_)
        | LoweredExprKind::Arithmetic { .. }
        | LoweredExprKind::Logical { .. }
        | LoweredExprKind::Comparison { .. }
        | LoweredExprKind::NumberToString(_) => true,
        LoweredExprKind::Void
        | LoweredExprKind::PostfixIncrement(_)
        | LoweredExprKind::EnumLiteral { .. }
        | LoweredExprKind::EnumPayload { .. }
        | LoweredExprKind::MatchValue(_)
        | LoweredExprKind::Match { .. } => false,
    }
}

fn lower_function(
    function: &FunctionDecl,
    name: &str,
    self_type: Option<&LoweredType>,
    has_self: bool,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredFunction> {
    let signature = signatures.get(name)?;

    let mut locals = HashMap::new();
    let mut params = Vec::new();
    let mut statements = Vec::new();
    let mut return_value = None;

    if let Some(self_type) = self_type {
        locals.insert(
            "Self".to_string(),
            LoweringLocal {
                type_: self_type.clone(),
                mutable: false,
                replacement: None,
            },
        );
    }

    let signature_params = if has_self {
        let self_type = self_type?;
        let self_param = signature.params.first()?;
        locals.insert(
            "self".to_string(),
            LoweringLocal {
                type_: self_type.clone(),
                mutable: signature.mutable_self,
                replacement: None,
            },
        );
        params.push(LoweredParam {
            name: "self".to_string(),
            type_: self_param.type_.clone(),
        });
        &signature.params[1..]
    } else {
        &signature.params[..]
    };

    for (param, signature_param) in function
        .params
        .iter()
        .filter(|param| !has_self || !is_self_param(param))
        .zip(signature_params)
    {
        if locals
            .insert(
                param.name.clone(),
                LoweringLocal {
                    type_: signature_param.type_.clone(),
                    mutable: param.mutable,
                    replacement: None,
                },
            )
            .is_some()
        {
            diagnostics.push(Diagnostic::error(
                param.span,
                format!("duplicate local `{}` in executable build", param.name),
            ));
        }

        params.push(LoweredParam {
            name: param.name.clone(),
            type_: signature_param.type_.clone(),
        });
    }

    match &function.body {
        FunctionBody::Expr(expr) => {
            return_value = lower_expr(
                expr,
                &locals,
                signatures,
                structs,
                enums,
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
                            enums,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Assign { .. } => {
                        if let Some(statement) = lower_assignment_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
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
                                enums,
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
                            enums,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::If { .. } => {
                        if let Some(statement) = lower_if_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::While { .. } => {
                        if let Some(statement) = lower_while_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
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
            .any(lowered_statement_always_returns_value)
    {
        diagnostics.push(Diagnostic::error(
            function.span,
            "function must return a value in executable builds",
        ));
    }

    Some(LoweredFunction {
        name: name.to_string(),
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
    enums: &HashMap<String, LoweredEnum>,
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
                            enums,
                            diagnostics,
                            None,
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
                            enums,
                            diagnostics,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Assign { .. } => {
                        if let Some(statement) = lower_assignment_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
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
                    StmtKind::If { .. } => {
                        if let Some(statement) = lower_if_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            diagnostics,
                            None,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::While { .. } => {
                        if let Some(statement) = lower_while_statement(
                            statement,
                            &locals,
                            signatures,
                            structs,
                            enums,
                            diagnostics,
                            None,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
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

fn lower_if_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let StmtKind::If {
        condition,
        then_branch,
        else_branch,
    } = &statement.kind
    else {
        return None;
    };

    let condition = lower_expr(
        condition,
        locals,
        signatures,
        structs,
        enums,
        diagnostics,
        Some(LoweredType::Basic(BasicType::Bool)),
        "expected supported `if` condition in executable builds",
    )?;
    let then_branch = lower_conditional_block(
        then_branch,
        &mut locals.clone(),
        signatures,
        structs,
        enums,
        diagnostics,
        return_type,
    );
    let else_branch = else_branch.as_ref().map(|else_branch| {
        let mut branch_locals = locals.clone();

        match else_branch {
            ElseBranch::Block(block) => lower_conditional_block(
                block,
                &mut branch_locals,
                signatures,
                structs,
                enums,
                diagnostics,
                return_type,
            ),
            ElseBranch::If(statement) => lower_if_statement(
                statement,
                &branch_locals,
                signatures,
                structs,
                enums,
                diagnostics,
                return_type,
            )
            .into_iter()
            .collect(),
        }
    });

    Some(LoweredStatement::If {
        condition,
        then_branch,
        else_branch,
    })
}

fn lower_while_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let StmtKind::While { condition, body } = &statement.kind else {
        return None;
    };

    let condition = lower_expr(
        condition,
        locals,
        signatures,
        structs,
        enums,
        diagnostics,
        Some(LoweredType::Basic(BasicType::Bool)),
        "expected supported `while` condition in executable builds",
    )?;
    let body = lower_conditional_block(
        body,
        &mut locals.clone(),
        signatures,
        structs,
        enums,
        diagnostics,
        return_type,
    );

    Some(LoweredStatement::While { condition, body })
}

fn lower_conditional_block(
    block: &Block,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Vec<LoweredStatement> {
    let mut statements = Vec::new();

    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { .. } => {
                if let Some(statement) = lower_local_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Assign { .. } => {
                if let Some(statement) = lower_assignment_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Return { value } => {
                let Some(return_type) = return_type else {
                    diagnostics.push(Diagnostic::error(
                        statement.span,
                        "return statements are not supported in executable builds",
                    ));
                    continue;
                };
                let value = value.as_ref().and_then(|value| {
                    lower_expr(
                        value,
                        locals,
                        signatures,
                        structs,
                        enums,
                        diagnostics,
                        Some(return_type.clone()),
                        "expected supported return value in executable builds",
                    )
                });
                statements.push(LoweredStatement::Return(value));
            }
            StmtKind::If { .. } => {
                if let Some(statement) = lower_if_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::For { .. } => diagnostics.push(Diagnostic::error(
                statement.span,
                "for loops are not supported in executable builds",
            )),
            StmtKind::While { .. } => {
                if let Some(statement) = lower_while_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Break => statements.push(LoweredStatement::Break),
            StmtKind::Continue => statements.push(LoweredStatement::Continue),
            StmtKind::Expr(expr) => {
                if let Some(statement) = lower_expression_statement(
                    expr,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
        }
    }

    statements
}

fn lowered_statement_always_returns_value(statement: &LoweredStatement) -> bool {
    match statement {
        LoweredStatement::Return(Some(_)) => true,
        LoweredStatement::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            then_branch
                .iter()
                .any(lowered_statement_always_returns_value)
                && else_branch
                    .iter()
                    .any(lowered_statement_always_returns_value)
        }
        LoweredStatement::Local { .. }
        | LoweredStatement::Assignment { .. }
        | LoweredStatement::Println(_)
        | LoweredStatement::Expr(_)
        | LoweredStatement::Return(None)
        | LoweredStatement::While { .. }
        | LoweredStatement::Break
        | LoweredStatement::Continue
        | LoweredStatement::Match { .. }
        | LoweredStatement::If {
            else_branch: None, ..
        } => false,
    }
}

fn lower_expression_statement(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    if let ExprKind::Match { branches, .. } = &expr.kind
        && branches
            .iter()
            .any(|branch| matches!(&branch.body, MatchBranchBody::Block(_)))
    {
        return lower_match_statement(
            expr,
            locals,
            signatures,
            structs,
            enums,
            diagnostics,
            return_type,
        );
    }

    if matches!(expr.kind, ExprKind::PostfixIncrement(_)) {
        return lower_expr(
            expr,
            locals,
            signatures,
            structs,
            enums,
            diagnostics,
            None,
            "expected supported increment expression in executable builds",
        )
        .map(LoweredStatement::Expr);
    }

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
            enums,
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
        enums,
        diagnostics,
        None,
        "expected supported function call in executable builds",
    )
    .map(LoweredStatement::Expr)
}

fn lower_match_statement(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let ExprKind::Match { value, branches } = &expr.kind else {
        return None;
    };
    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        diagnostics,
        None,
        "expected supported match value in executable builds",
    )?;
    if !matches!(
        value.type_,
        LoweredType::Enum(_) | LoweredType::Basic(BasicType::String)
    ) {
        diagnostics.push(Diagnostic::error(
            expr.span,
            "match statements require an enum or `String` value in executable builds",
        ));
        return None;
    }

    let mut lowered_branches = Vec::new();
    let temp_name = match_temp_name(expr.span);
    for branch in branches {
        let mut branch_locals = locals.clone();
        let pattern = lower_match_pattern(
            &branch.pattern,
            &value.type_,
            &mut branch_locals,
            enums,
            diagnostics,
            &temp_name,
        )?;
        let statements = match &branch.body {
            MatchBranchBody::Block(block) => lower_conditional_block(
                block,
                &mut branch_locals,
                signatures,
                structs,
                enums,
                diagnostics,
                return_type,
            ),
            MatchBranchBody::Expr(branch_expr) => lower_expression_statement(
                branch_expr,
                &branch_locals,
                signatures,
                structs,
                enums,
                diagnostics,
                return_type,
            )
            .into_iter()
            .collect(),
        };
        lowered_branches.push(LoweredMatchStatementBranch {
            pattern,
            statements,
        });
    }

    Some(LoweredStatement::Match {
        value,
        temp_name,
        branches: lowered_branches,
    })
}

fn lower_match_expression_branch_block(
    block: &Block,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
) -> Option<(Vec<LoweredStatement>, LoweredExpr)> {
    let Some((last_statement, setup_statements)) = block.statements.split_last() else {
        diagnostics.push(Diagnostic::error(
            block.span,
            "block-bodied match expression branches must return a value",
        ));
        return None;
    };
    let StmtKind::Return { value: Some(value) } = &last_statement.kind else {
        diagnostics.push(Diagnostic::error(
            last_statement.span,
            "block-bodied match expression branches must end with `return value`",
        ));
        return None;
    };

    let mut statements = Vec::new();
    for statement in setup_statements {
        match &statement.kind {
            StmtKind::Let { .. } => {
                if let Some(statement) = lower_local_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Assign { .. } => {
                if let Some(statement) = lower_assignment_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::If { .. } => {
                if let Some(statement) = lower_if_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::While { .. } => {
                if let Some(statement) = lower_while_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Expr(expr) => {
                if let Some(statement) = lower_expression_statement(
                    expr,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::Return { .. } => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "return statements are only supported as the final value of block-bodied match expression branches",
                ));
            }
            StmtKind::Break => statements.push(LoweredStatement::Break),
            StmtKind::Continue => statements.push(LoweredStatement::Continue),
            StmtKind::For { .. } => diagnostics.push(Diagnostic::error(
                statement.span,
                "for loops are not supported in executable builds",
            )),
        }
    }

    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        diagnostics,
        expected_type,
        "expected supported match branch value in executable builds",
    )?;

    Some((statements, value))
}

fn void_expr() -> LoweredExpr {
    LoweredExpr {
        type_: LoweredType::Void,
        kind: LoweredExprKind::Void,
    }
}

fn lower_local_statement(
    statement: &Stmt,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
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

    let annotated_type = if let Some(type_annotation) = type_annotation {
        if !type_annotation.args.is_empty() {
            diagnostics.push(Diagnostic::error(
                type_annotation.span,
                "generic local types are not supported in executable builds",
            ));
            can_lower = false;
            None
        } else if type_annotation.name == "Self" {
            locals
                .get("Self")
                .map(|local| local.type_.clone())
                .or_else(|| {
                    diagnostics.push(Diagnostic::error(
                        type_annotation.span,
                        "`Self` is only available in methods and extension functions",
                    ));
                    can_lower = false;
                    None
                })
        } else if let Some(type_) = BasicType::from_name(&type_annotation.name) {
            Some(LoweredType::Basic(type_))
        } else if structs.contains_key(&type_annotation.name) {
            Some(LoweredType::Struct(type_annotation.name.clone()))
        } else if enums.contains_key(&type_annotation.name) {
            Some(LoweredType::Enum(type_annotation.name.clone()))
        } else {
            diagnostics.push(Diagnostic::error(
                type_annotation.span,
                "only basic, struct, and enum local types are supported in executable builds",
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
            enums,
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
            LoweredType::Struct(_) | LoweredType::Enum(_) => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "struct and enum locals must include an initializer in executable builds",
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

    if *mutable
        && matches!(value.type_, LoweredType::Struct(_))
        && !lowered_expression_has_mutable_capability(&value, locals, signatures, structs)
    {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!(
                "cannot initialize mutable binding `{name}` from an immutable value; use `.clone()` to create an independent mutable object"
            ),
        ));
        return None;
    }

    if locals
        .insert(
            name.clone(),
            LoweringLocal {
                type_: value.type_.clone(),
                mutable: *mutable,
                replacement: None,
            },
        )
        .is_some()
    {
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

fn lower_assignment_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredStatement> {
    let StmtKind::Assign { target, op, value } = &statement.kind else {
        return None;
    };
    let binding_name = match &target.kind {
        ExprKind::Identifier(name) => name,
        ExprKind::Member { object, .. } => {
            let Some(name) = mutable_member_root(object) else {
                diagnostics.push(Diagnostic::error(
                    target.span,
                    "field assignment target must be rooted in a mutable local struct binding in executable builds",
                ));
                return None;
            };
            name
        }
        _ => {
            diagnostics.push(Diagnostic::error(
                target.span,
                "assignment target must be a mutable local binding in executable builds",
            ));
            return None;
        }
    };
    let Some(local) = locals.get(binding_name) else {
        diagnostics.push(Diagnostic::error(
            target.span,
            format!("unknown local `{binding_name}` in executable build"),
        ));
        return None;
    };

    if !local.mutable {
        let message = if matches!(target.kind, ExprKind::Member { .. }) {
            format!("cannot mutate field of immutable binding `{binding_name}` in executable build")
        } else {
            format!("cannot assign to immutable binding `{binding_name}` in executable build")
        };
        diagnostics.push(Diagnostic::error(target.span, message));
        return None;
    }

    let lowered_target = lower_expr(
        target,
        locals,
        signatures,
        structs,
        enums,
        diagnostics,
        None,
        "expected supported assignment target in executable builds",
    )?;

    let compound_value;
    let value = if let Some(op) = op {
        compound_value = Expr {
            kind: ExprKind::Binary {
                left: Box::new(target.clone()),
                op: *op,
                right: Box::new(value.clone()),
            },
            span: statement.span,
        };
        &compound_value
    } else {
        value
    };
    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        diagnostics,
        Some(lowered_target.type_.clone()),
        "expected supported assignment value in executable builds",
    )?;

    Some(LoweredStatement::Assignment {
        target: lowered_target,
        value,
    })
}

fn lower_struct_init(
    expr: &Expr,
    name: &str,
    fields: &[StructInitField],
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let resolved_name = if name == "Self" {
        match locals.get("Self").map(|local| &local.type_) {
            Some(LoweredType::Struct(name)) => name.as_str(),
            _ => {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "`Self` does not name a struct in this executable build",
                ));
                return None;
            }
        }
    } else {
        name
    };
    let name = resolved_name;
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
            enums,
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
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
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
                if type_.is_numeric() && (!number_literal_is_float(value) || type_.is_float()) {
                    *type_
                } else if number_literal_is_float(value) {
                    BasicType::F64
                } else {
                    BasicType::I32
                }
            } else if number_literal_is_float(value) {
                BasicType::F64
            } else {
                BasicType::I32
            };

            LoweredExpr {
                type_: LoweredType::Basic(type_),
                kind: LoweredExprKind::NumberLiteral(value.clone()),
            }
        }
        ExprKind::Identifier(name) if locals.contains_key(name) => locals[name]
            .replacement
            .clone()
            .unwrap_or_else(|| LoweredExpr {
                type_: locals[name].type_.clone(),
                kind: LoweredExprKind::Local(name.clone()),
            }),
        ExprKind::Identifier(name) => {
            diagnostics.push(Diagnostic::error(
                expr.span,
                format!("unknown local `{name}` in executable build"),
            ));
            return None;
        }
        ExprKind::PostfixIncrement(target) => {
            let binding_name = match &target.kind {
                ExprKind::Identifier(name) => name,
                ExprKind::Member { object, .. } => {
                    let Some(name) = mutable_member_root(object) else {
                        diagnostics.push(Diagnostic::error(
                            target.span,
                            "increment target must be rooted in a mutable local struct binding in executable builds",
                        ));
                        return None;
                    };
                    name
                }
                _ => {
                    diagnostics.push(Diagnostic::error(
                        target.span,
                        "increment target must be a mutable local binding in executable builds",
                    ));
                    return None;
                }
            };
            let Some(local) = locals.get(binding_name) else {
                diagnostics.push(Diagnostic::error(
                    target.span,
                    format!("unknown local `{binding_name}` in executable build"),
                ));
                return None;
            };

            if !local.mutable {
                diagnostics.push(Diagnostic::error(
                    target.span,
                    format!("cannot mutate immutable binding `{binding_name}` in executable build"),
                ));
                return None;
            }

            let target = lower_expr(
                target,
                locals,
                signatures,
                structs,
                enums,
                diagnostics,
                None,
                "expected supported increment target in executable builds",
            )?;

            if !matches!(&target.type_, LoweredType::Basic(type_) if type_.is_numeric()) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "operator ++ does not support values of type `{}` in executable builds",
                        target.type_.name()
                    ),
                ));
                return None;
            }

            LoweredExpr {
                type_: target.type_.clone(),
                kind: LoweredExprKind::PostfixIncrement(Box::new(target)),
            }
        }
        ExprKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => {
            let operand = lower_expr(
                operand,
                locals,
                signatures,
                structs,
                enums,
                diagnostics,
                Some(LoweredType::Basic(BasicType::Bool)),
                "expected supported boolean operand in executable builds",
            )?;

            LoweredExpr {
                type_: LoweredType::Basic(BasicType::Bool),
                kind: LoweredExprKind::Not(Box::new(operand)),
            }
        }
        ExprKind::Unary {
            op: UnaryOp::Negate,
            operand,
        } => {
            let operand = lower_expr(
                operand,
                locals,
                signatures,
                structs,
                enums,
                diagnostics,
                expected_type.clone(),
                "expected supported signed numeric operand in executable builds",
            )?;

            if !matches!(
                operand.type_,
                LoweredType::Basic(type_) if type_.is_signed_numeric()
            ) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "operator - requires a signed numeric operand in executable builds",
                ));
                return None;
            }

            LoweredExpr {
                type_: operand.type_.clone(),
                kind: LoweredExprKind::Negate(Box::new(operand)),
            }
        }
        ExprKind::Binary {
            left,
            op: op @ (BinaryOp::LogicalAnd | BinaryOp::LogicalOr),
            right,
        } => {
            let bool_type = LoweredType::Basic(BasicType::Bool);
            let left = lower_expr(
                left,
                locals,
                signatures,
                structs,
                enums,
                diagnostics,
                Some(bool_type.clone()),
                "expected supported boolean operand in executable builds",
            )?;
            let right = lower_expr(
                right,
                locals,
                signatures,
                structs,
                enums,
                diagnostics,
                Some(bool_type.clone()),
                "expected supported boolean operand in executable builds",
            )?;

            LoweredExpr {
                type_: bool_type,
                kind: LoweredExprKind::Logical {
                    left: Box::new(left),
                    op: *op,
                    right: Box::new(right),
                },
            }
        }
        ExprKind::Binary {
            left,
            op:
                op @ (BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight),
            right,
        } => {
            let is_bitwise = matches!(
                op,
                BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
            );
            let contextual_type = expected_type.as_ref().filter(|type_| {
                matches!(
                    type_,
                    LoweredType::Basic(type_) if if is_bitwise {
                        type_.is_integer()
                    } else {
                        type_.is_numeric()
                    }
                ) || *op == BinaryOp::Add && **type_ == LoweredType::Basic(BasicType::String)
            });
            let (left, right) = if let Some(type_) = contextual_type {
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            } else if !is_bitwise && number_pair_contains_float(left, right) {
                let type_ = LoweredType::Basic(BasicType::F64);
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(type_),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            } else if matches!(left.kind, ExprKind::Number(_))
                && !matches!(right.kind, ExprKind::Number(_))
            {
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                    "expected supported arithmetic operand in executable builds",
                )?;
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(right.type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            } else {
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                    "expected supported arithmetic operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(left.type_.clone()),
                    "expected supported arithmetic operand in executable builds",
                )?;
                (left, right)
            };

            if *op == BinaryOp::Add && left.type_ == LoweredType::Basic(BasicType::String) {
                LoweredExpr {
                    type_: LoweredType::Basic(BasicType::String),
                    kind: LoweredExprKind::StringConcat(Box::new(left), Box::new(right)),
                }
            } else if matches!(
                left.type_,
                LoweredType::Basic(type_) if if is_bitwise {
                    type_.is_integer()
                } else {
                    type_.is_numeric()
                }
            ) {
                LoweredExpr {
                    type_: left.type_.clone(),
                    kind: LoweredExprKind::Arithmetic {
                        left: Box::new(left),
                        op: *op,
                        right: Box::new(right),
                    },
                }
            } else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "operator {} does not support values of type `{}` in executable builds",
                        op.symbol(),
                        left.type_.name()
                    ),
                ));
                return None;
            }
        }
        ExprKind::Binary { left, op, right } => {
            let (left, right) = if number_pair_contains_float(left, right) {
                let type_ = LoweredType::Basic(BasicType::F64);
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(type_.clone()),
                    "expected supported comparison operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(type_),
                    "expected supported comparison operand in executable builds",
                )?;
                (left, right)
            } else if matches!(left.kind, ExprKind::Number(_))
                && !matches!(right.kind, ExprKind::Number(_))
            {
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                    "expected supported comparison operand in executable builds",
                )?;
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(right.type_.clone()),
                    "expected supported comparison operand in executable builds",
                )?;
                (left, right)
            } else {
                let left = lower_expr(
                    left,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                    "expected supported comparison operand in executable builds",
                )?;
                let right = lower_expr(
                    right,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    Some(left.type_.clone()),
                    "expected supported comparison operand in executable builds",
                )?;
                (left, right)
            };

            let supported = match op {
                BinaryOp::Equal | BinaryOp::NotEqual => {
                    matches!(
                        &left.type_,
                        LoweredType::Basic(BasicType::String | BasicType::Bool)
                    ) || matches!(&left.type_, LoweredType::Basic(type_) if type_.is_numeric())
                }
                BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual => {
                    matches!(&left.type_, LoweredType::Basic(type_) if type_.is_numeric())
                }
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
                | BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr => {
                    unreachable!("non-comparison operator is lowered separately")
                }
            };

            if !supported {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "operator {} does not support values of type `{}` in executable builds",
                        op.symbol(),
                        left.type_.name()
                    ),
                ));
                return None;
            }

            LoweredExpr {
                type_: LoweredType::Basic(BasicType::Bool),
                kind: LoweredExprKind::Comparison {
                    left: Box::new(left),
                    op: *op,
                    right: Box::new(right),
                },
            }
        }
        ExprKind::StructInit { name, fields, .. } => lower_struct_init(
            expr,
            name,
            fields,
            locals,
            signatures,
            structs,
            enums,
            diagnostics,
        )?,
        ExprKind::Member { object, name } => {
            if let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
            {
                if variant.payload.is_some() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("enum variant `{enum_name}.{name}` requires a payload"),
                    ));
                    return None;
                }

                return Some(LoweredExpr {
                    type_: LoweredType::Enum(enum_name.clone()),
                    kind: LoweredExprKind::EnumLiteral {
                        enum_name: enum_name.clone(),
                        variant: name.clone(),
                        payload: None,
                    },
                });
            }

            let object = lower_expr(
                object,
                locals,
                signatures,
                structs,
                enums,
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
            if let ExprKind::Member { object, name } = &callee.kind
                && name == "clone"
            {
                if !args.is_empty() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("`.clone()` expects no arguments, got {}", args.len()),
                    ));
                    return None;
                }

                let object = lower_expr(
                    object,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                    "expected supported clone source in executable builds",
                )?;

                if matches!(object.type_, LoweredType::Struct(_)) {
                    LoweredExpr {
                        type_: object.type_.clone(),
                        kind: LoweredExprKind::Clone(Box::new(object)),
                    }
                } else if object.type_ == LoweredType::Basic(BasicType::String) {
                    object
                } else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "`.clone()` is not supported for `{}` in executable builds",
                            object.type_.name()
                        ),
                    ));
                    return None;
                }
            } else if let ExprKind::Member { object, name } = &callee.kind
                && let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
            {
                let expected_count = usize::from(variant.payload.is_some());

                if args.len() != expected_count {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "enum variant `{enum_name}.{name}` expects {expected_count} arguments, got {}",
                            args.len()
                        ),
                    ));
                    return None;
                }

                let payload = if let Some(type_) = &variant.payload {
                    Some(Box::new(lower_expr(
                        &args[0],
                        locals,
                        signatures,
                        structs,
                        enums,
                        diagnostics,
                        Some(type_.clone()),
                        "expected supported enum payload in executable builds",
                    )?))
                } else {
                    None
                };

                LoweredExpr {
                    type_: LoweredType::Enum(enum_name.clone()),
                    kind: LoweredExprKind::EnumLiteral {
                        enum_name: enum_name.clone(),
                        variant: name.clone(),
                        payload,
                    },
                }
            } else if let ExprKind::Member { object, name } = &callee.kind
                && let Some(type_) = lower_type_expression(object, locals, structs, enums)
            {
                let Some(lowered_name) = callable_static_name(&type_, name, signatures) else {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "unknown static function `{name}` for type `{}` in executable build",
                            type_.name()
                        ),
                    ));
                    return None;
                };
                let Some(signature) = signatures.get(&lowered_name) else {
                    unreachable!("resolved static function must have a signature")
                };

                if args.len() != signature.params.len() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "static function `{}.{name}` expects {} arguments, got {}",
                            type_.name(),
                            signature.params.len(),
                            args.len()
                        ),
                    ));
                    return None;
                }

                let mut lowered_args = Vec::new();
                for (arg, param) in args.iter().zip(&signature.params) {
                    if let Some(arg) = lower_expr(
                        arg,
                        locals,
                        signatures,
                        structs,
                        enums,
                        diagnostics,
                        Some(param.type_.clone()),
                        "expected supported static function argument in executable builds",
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
                        name: lowered_name,
                        args: lowered_args,
                    },
                }
            } else if let ExprKind::Member { object, name } = &callee.kind {
                let receiver_has_mutable_capability =
                    expression_has_mutable_capability(object, locals);
                let receiver_binding = mutable_member_root(object).map(str::to_string);
                let receiver_span = object.span;
                let object = lower_expr(
                    object,
                    locals,
                    signatures,
                    structs,
                    enums,
                    diagnostics,
                    None,
                    "expected supported method receiver in executable builds",
                )?;
                if name == "toString"
                    && matches!(&object.type_, LoweredType::Basic(type_) if type_.is_numeric())
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `{}.toString` expects 0 arguments, got {}",
                                object.type_.name(),
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::String),
                        kind: LoweredExprKind::NumberToString(Box::new(object)),
                    });
                }

                let Some(lowered_name) = callable_method_name(&object.type_, name, signatures)
                else {
                    diagnostics.push(Diagnostic::error(
                        callee.span,
                        format!(
                            "unknown method `{name}` for type `{}` in executable build",
                            object.type_.name()
                        ),
                    ));
                    return None;
                };
                let Some(signature) = signatures.get(&lowered_name) else {
                    unreachable!("resolved method must have a signature")
                };
                let params = &signature.params[1..];

                if signature.mutable_self && !receiver_has_mutable_capability {
                    let qualified_name = format!("{}.{name}", object.type_.name());
                    let message = if let Some(binding_name) = receiver_binding
                        && locals
                            .get(&binding_name)
                            .is_some_and(|local| !local.mutable)
                    {
                        format!(
                            "cannot call mutable function `{qualified_name}` through immutable binding `{binding_name}`; declare it with `let mut {binding_name}` or call the function on a mutable clone"
                        )
                    } else {
                        format!(
                            "mutable function `{qualified_name}` requires a mutable receiver; bind the value with `let mut` or call the function on a mutable clone"
                        )
                    };
                    diagnostics.push(Diagnostic::error(receiver_span, message));
                    return None;
                }

                if args.len() != params.len() {
                    diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "method `{}.{name}` expects {} arguments, got {}",
                            object.type_.name(),
                            params.len(),
                            args.len()
                        ),
                    ));
                    return None;
                }

                let mut lowered_args = vec![object];

                for (arg, param) in args.iter().zip(params) {
                    if let Some(arg) = lower_expr(
                        arg,
                        locals,
                        signatures,
                        structs,
                        enums,
                        diagnostics,
                        Some(param.type_.clone()),
                        "expected supported method argument in executable builds",
                    ) {
                        lowered_args.push(arg);
                    }
                }

                if lowered_args.len() != args.len() + 1 {
                    return None;
                }

                LoweredExpr {
                    type_: signature.return_type.clone(),
                    kind: LoweredExprKind::Call {
                        name: lowered_name,
                        args: lowered_args,
                    },
                }
            } else {
                let ExprKind::Identifier(name) = &callee.kind else {
                    diagnostics.push(Diagnostic::error(
                        callee.span,
                        "only direct helper function and enum variant calls are supported in executable builds",
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

                for (arg, param) in args.iter().zip(&signature.params) {
                    if let Some(arg) = lower_expr(
                        arg,
                        locals,
                        signatures,
                        structs,
                        enums,
                        diagnostics,
                        Some(param.type_.clone()),
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
        }
        ExprKind::Match { value, branches } => {
            let value = lower_expr(
                value,
                locals,
                signatures,
                structs,
                enums,
                diagnostics,
                None,
                "expected supported match value in executable builds",
            )?;
            if !matches!(
                value.type_,
                LoweredType::Enum(_) | LoweredType::Basic(BasicType::String)
            ) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "match expressions require an enum or `String` value in executable builds",
                ));
                return None;
            }
            let mut lowered_branches = Vec::new();
            let mut result_type = None;
            let temp_name = match_temp_name(expr.span);

            for branch in branches {
                let mut branch_locals = locals.clone();
                let pattern = lower_match_pattern(
                    &branch.pattern,
                    &value.type_,
                    &mut branch_locals,
                    enums,
                    diagnostics,
                    &temp_name,
                )?;

                let expected_branch_type = expected_type.clone().or_else(|| result_type.clone());
                let (statements, branch_value) = match &branch.body {
                    MatchBranchBody::Expr(branch_value) => (
                        Vec::new(),
                        lower_expr(
                            branch_value,
                            &branch_locals,
                            signatures,
                            structs,
                            enums,
                            diagnostics,
                            expected_branch_type,
                            "expected supported match branch value in executable builds",
                        )?,
                    ),
                    MatchBranchBody::Block(block) => lower_match_expression_branch_block(
                        block,
                        &mut branch_locals,
                        signatures,
                        structs,
                        enums,
                        diagnostics,
                        expected_branch_type,
                    )?,
                };
                result_type.get_or_insert_with(|| branch_value.type_.clone());
                lowered_branches.push(LoweredMatchBranch {
                    pattern,
                    statements,
                    value: branch_value,
                });
            }

            let Some(result_type) = result_type else {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "match expressions require at least one branch",
                ));
                return None;
            };

            LoweredExpr {
                type_: result_type,
                kind: LoweredExprKind::Match {
                    value: Box::new(value),
                    temp_name,
                    branches: lowered_branches,
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

fn find_qualified_variant<'a>(
    enums: &'a HashMap<String, LoweredEnum>,
    enum_name: &str,
    variant_name: &str,
) -> Option<&'a LoweredVariant> {
    enums
        .get(enum_name)?
        .variants
        .iter()
        .find(|variant| variant.name == variant_name)
}

fn lower_match_pattern(
    pattern: &Pattern,
    value_type: &LoweredType,
    locals: &mut HashMap<String, LoweringLocal>,
    enums: &HashMap<String, LoweredEnum>,
    diagnostics: &mut Vec<Diagnostic>,
    match_value_name: &str,
) -> Option<LoweredPattern> {
    match (pattern, value_type) {
        (
            Pattern::Variant {
                enum_name,
                variant,
                binding,
                span,
            },
            LoweredType::Enum(value_enum_name),
        ) => {
            if enum_name != value_enum_name {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!(
                        "pattern `{enum_name}.{variant}` does not belong to enum `{value_enum_name}`"
                    ),
                ));
                return None;
            }
            let Some(variant_definition) = enums
                .get(enum_name)
                .and_then(|enum_| enum_.variants.iter().find(|item| item.name == *variant))
            else {
                diagnostics.push(Diagnostic::error(
                    *span,
                    format!("unknown variant `{variant}` for enum `{enum_name}`"),
                ));
                return None;
            };

            if let (Some(binding), Some(payload_type)) = (binding, &variant_definition.payload)
                && binding != "_"
            {
                locals.insert(
                    binding.clone(),
                    LoweringLocal {
                        type_: payload_type.clone(),
                        mutable: false,
                        replacement: Some(LoweredExpr {
                            type_: payload_type.clone(),
                            kind: LoweredExprKind::EnumPayload {
                                object: Box::new(LoweredExpr {
                                    type_: value_type.clone(),
                                    kind: LoweredExprKind::MatchValue(match_value_name.to_string()),
                                }),
                                variant: variant.clone(),
                            },
                        }),
                    },
                );
            }

            Some(LoweredPattern::Variant {
                enum_name: enum_name.clone(),
                variant: variant.clone(),
            })
        }
        (Pattern::String { value, .. }, LoweredType::Basic(BasicType::String)) => {
            Some(LoweredPattern::String(value.clone()))
        }
        (Pattern::Wildcard { .. }, _) => Some(LoweredPattern::Wildcard),
        _ => {
            diagnostics.push(Diagnostic::error(
                pattern.span(),
                "match pattern does not apply to the matched value",
            ));
            None
        }
    }
}

fn match_temp_name(span: Span) -> String {
    format!("gust_internal_match_value_{}", span.start)
}

fn mutable_member_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name),
        ExprKind::Member { object, .. } => mutable_member_root(object),
        _ => None,
    }
}

fn number_pair_contains_float(left: &Expr, right: &Expr) -> bool {
    matches!(&left.kind, ExprKind::Number(_))
        && matches!(&right.kind, ExprKind::Number(_))
        && (matches!(&left.kind, ExprKind::Number(value) if number_literal_is_float(value))
            || matches!(&right.kind, ExprKind::Number(value) if number_literal_is_float(value)))
}
