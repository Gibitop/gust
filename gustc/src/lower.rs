use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::ast::{
    BasicType, BinaryOp, Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, Item,
    MatchBranchBody, Pattern, Program, Stmt, StmtKind, StructDecl, StructInitField, StructMember,
    TraitDecl, TraitMethodDecl, TypeRef, UnaryOp, number_literal_is_float,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub structs: Vec<LoweredStruct>,
    pub enums: Vec<LoweredEnum>,
    pub traits: Vec<LoweredTrait>,
    pub functions: Vec<LoweredFunction>,
    pub closure_functions: Vec<LoweredClosureFunction>,
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStruct {
    pub name: String,
    pub fields: Vec<LoweredField>,
    pub raw_buffer_element: Option<LoweredType>,
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
pub struct LoweredTrait {
    pub name: String,
    pub methods: Vec<LoweredTraitMethod>,
    pub impls: Vec<LoweredTraitImpl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTraitMethod {
    pub name: String,
    pub params: Vec<LoweredParamSignature>,
    pub return_type: LoweredType,
    pub mutable_self: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTraitImpl {
    pub self_type: LoweredType,
    pub methods: Vec<LoweredTraitImplMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTraitImplMethod {
    pub name: String,
    pub function_name: String,
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
pub struct LoweredClosureFunction {
    pub name: String,
    pub captures: Vec<LoweredClosureCapture>,
    pub params: Vec<LoweredParam>,
    pub return_type: LoweredType,
    pub statements: Vec<LoweredStatement>,
    pub return_value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredClosureCapture {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    Local {
        name: String,
        value: LoweredExpr,
    },
    LocalCell {
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
    Trait(String),
    Function {
        params: Vec<LoweredFunctionTypeParam>,
        return_type: Box<LoweredType>,
    },
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredFunctionTypeParam {
    pub type_: LoweredType,
    pub mutable: bool,
}

impl LoweredType {
    pub fn name(&self) -> String {
        match self {
            LoweredType::Basic(type_) => type_.name().to_string(),
            LoweredType::Struct(name) => name.clone(),
            LoweredType::Enum(name) => name.clone(),
            LoweredType::Trait(name) => name.clone(),
            LoweredType::Function {
                params,
                return_type,
            } => {
                let params = params
                    .iter()
                    .map(|param| {
                        if param.mutable {
                            format!("mut {}", param.type_.name())
                        } else {
                            param.type_.name()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({params}): {}", return_type.name())
            }
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
    LocalCell(String),
    CapturedLocal {
        env_name: String,
        name: String,
    },
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
    CollectionLiteral {
        constructor: String,
        add: String,
        items: Vec<LoweredExpr>,
    },
    TraitObject {
        trait_name: String,
        self_type: LoweredType,
        value: Box<LoweredExpr>,
    },
    DynamicCall {
        object: Box<LoweredExpr>,
        method: String,
        args: Vec<LoweredExpr>,
    },
    Closure {
        name: String,
        captures: Vec<LoweredClosureCapture>,
    },
    IndirectCall {
        callee: Box<LoweredExpr>,
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
    Variant {
        enum_name: String,
        variant: String,
    },
    String(String),
    Number(String),
    Range {
        start: String,
        end: String,
        inclusive: bool,
    },
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
pub struct LoweredParamSignature {
    pub type_: LoweredType,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
struct LoweringLocal {
    type_: LoweredType,
    mutable: bool,
    replacement: Option<LoweredExpr>,
    captured: bool,
}

#[derive(Debug, Default)]
struct LoweringContext {
    diagnostics: Vec<Diagnostic>,
    closure_functions: Vec<LoweredClosureFunction>,
    next_closure_id: usize,
}

thread_local! {
    static CLOSURE_LOWERING: RefCell<LoweringContext> = RefCell::new(LoweringContext::default());
    static CAPTURED_NAMES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
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

fn is_raw_buffer_name(name: &str) -> bool {
    name == "RawBuffer"
        || name.ends_with("::RawBuffer")
        || name.contains("::RawBuffer<")
        || name.starts_with("RawBuffer<")
}

fn raw_buffer_element_name(name: &str) -> Option<&str> {
    let start = name.rfind("RawBuffer<")? + "RawBuffer<".len();
    let element = name.get(start..name.len().checked_sub(1)?)?;
    (!element.is_empty() && name.ends_with('>')).then_some(element)
}

fn lowered_type_from_concrete_name(
    name: &str,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    if let Some(type_) = BasicType::from_name(name) {
        return Some(LoweredType::Basic(type_));
    }

    if name == "void" {
        return Some(LoweredType::Void);
    }

    if structs.contains_key(name) {
        return Some(LoweredType::Struct(name.to_string()));
    }

    if enums.contains_key(name) {
        return Some(LoweredType::Enum(name.to_string()));
    }

    if traits.contains_key(name) {
        return Some(LoweredType::Trait(name.to_string()));
    }

    None
}

fn is_string_builder_name(name: &str) -> bool {
    name == "StringBuilder" || name.ends_with("::StringBuilder")
}

fn trait_method_name(type_name: &str, function_name: &str) -> String {
    format!("trait {type_name}.{function_name}")
}

fn qualified_trait_method_name(trait_name: &str, type_name: &str, function_name: &str) -> String {
    format!("trait {trait_name} for {type_name}.{function_name}")
}

fn qualified_static_trait_method_name(
    trait_name: &str,
    type_name: &str,
    function_name: &str,
) -> String {
    format!("static trait {trait_name} for {type_name}.{function_name}")
}

fn static_trait_method_name(type_name: &str, function_name: &str) -> String {
    format!("static trait {type_name}.{function_name}")
}

fn source_callable_name(name: &str) -> &str {
    name.rsplit_once("::").map_or(name, |(_, name)| name)
}

fn requested_trait_name(name: &str) -> Option<&str> {
    name.rsplit_once("::").map(|(trait_name, _)| trait_name)
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
    match type_ {
        LoweredType::Struct(type_name) | LoweredType::Enum(type_name) => {
            let name = method_name(type_name, source_callable_name(name));
            if signatures.contains_key(&name) {
                return Some(name);
            }
        }
        _ => {}
    }

    let extension = extension_name(&type_.name(), name);
    if signatures.contains_key(&extension) {
        return Some(extension);
    }

    if let Some(trait_name) = requested_trait_name(name) {
        let qualified_name =
            qualified_trait_method_name(trait_name, &type_.name(), source_callable_name(name));
        if signatures.contains_key(&qualified_name) {
            return Some(qualified_name);
        }
    }

    let name = trait_method_name(&type_.name(), source_callable_name(name));
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

    let extension = static_extension_name(&type_.name(), name);
    if signatures.contains_key(&extension) {
        return Some(extension);
    }

    if let Some(trait_name) = requested_trait_name(name) {
        let qualified_name = qualified_static_trait_method_name(
            trait_name,
            &type_.name(),
            source_callable_name(name),
        );
        if signatures.contains_key(&qualified_name) {
            return Some(qualified_name);
        }
    }

    let name = static_trait_method_name(&type_.name(), source_callable_name(name));
    signatures.contains_key(&name).then_some(name)
}

pub fn lower_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    let program = crate::monomorphize::monomorphize(program)?;
    lower_monomorphized_program(&program)
}

fn lower_monomorphized_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    CLOSURE_LOWERING.with(|state| *state.borrow_mut() = LoweringContext::default());

    let mut diagnostics = Vec::new();
    let mut main = None;
    let mut structs = HashMap::new();
    let mut enums = HashMap::new();
    let mut traits = HashMap::new();
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
                        raw_buffer_element: None,
                    },
                );
            }
            Item::Trait(item) => {
                traits.insert(
                    item.name.clone(),
                    LoweredTrait {
                        name: item.name.clone(),
                        methods: Vec::new(),
                        impls: Vec::new(),
                    },
                );
            }
            Item::Import(_) | Item::Impl(_) | Item::Extension(_) | Item::Function(_) => {}
        }
    }

    for item in &program.items {
        match item {
            Item::Struct(item) => {
                if let Some(struct_) =
                    lower_struct_definition(item, &structs, &enums, &traits, &mut diagnostics)
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
            Item::Trait(_) => {}
            Item::Impl(_) => {}
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
            && let Some(enum_) =
                lower_enum_definition(item, &structs, &enums, &traits, &mut diagnostics)
        {
            enums.insert(item.name.clone(), enum_);
        }
    }

    let raw_buffer_elements = structs
        .iter()
        .filter(|(name, _)| is_raw_buffer_name(name))
        .filter_map(|(name, _)| {
            let element = raw_buffer_element_name(name).and_then(|name| {
                lowered_type_from_concrete_name(name, &structs, &enums, &traits)
            })?;
            Some((name.clone(), element))
        })
        .collect::<Vec<_>>();
    for (name, element) in raw_buffer_elements {
        if let Some(struct_) = structs.get_mut(&name) {
            struct_.raw_buffer_element = Some(element);
        }
    }

    for item in &program.items {
        if let Item::Trait(item) = item
            && let Some(trait_) =
                lower_trait_definition(item, &structs, &enums, &traits, &mut diagnostics)
        {
            traits.insert(item.name.clone(), trait_);
        }
    }

    let mut functions_to_lower = Vec::new();

    for item in &program.items {
        match item {
            Item::Enum(enum_) => {
                for member in &enum_.members {
                    let (function, lowered_name, has_self) = match member {
                        StructMember::Method(function) => (
                            function,
                            method_name(
                                &enum_.name,
                                function.name.as_deref().unwrap_or("<missing>"),
                            ),
                            true,
                        ),
                        StructMember::StaticMethod(function) => (
                            function,
                            static_method_name(
                                &enum_.name,
                                function.name.as_deref().unwrap_or("<missing>"),
                            ),
                            false,
                        ),
                        StructMember::Field(_) => continue,
                    };
                    if function.name.is_none() {
                        continue;
                    }
                    let self_type = LoweredType::Enum(enum_.name.clone());
                    let Some(mut signature) = lower_function_signature(
                        function,
                        Some(&self_type),
                        has_self,
                        &structs,
                        &enums,
                        &traits,
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
                        &traits,
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
                    &traits,
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
                    &traits,
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
            Item::Impl(item) => {
                let Some(self_type) = lower_value_type_ref(
                    &item.type_ref,
                    &structs,
                    &enums,
                    &traits,
                    &mut diagnostics,
                    "trait impls require a supported receiver type in executable builds",
                ) else {
                    continue;
                };
                let mut trait_impl_methods = Vec::new();

                for member in &item.methods {
                    let function = &member.function;
                    let Some(name) = &function.name else {
                        continue;
                    };
                    let lowered_name = if item.trait_ref.name.contains('<') {
                        if member.static_ {
                            qualified_static_trait_method_name(
                                &item.trait_ref.name,
                                &self_type.name(),
                                name,
                            )
                        } else {
                            qualified_trait_method_name(
                                &item.trait_ref.name,
                                &self_type.name(),
                                name,
                            )
                        }
                    } else if member.static_ {
                        static_trait_method_name(&self_type.name(), name)
                    } else {
                        trait_method_name(&self_type.name(), name)
                    };
                    let Some(mut signature) = lower_function_signature(
                        function,
                        Some(&self_type),
                        !member.static_,
                        &structs,
                        &enums,
                        &traits,
                        &mut diagnostics,
                    ) else {
                        continue;
                    };
                    if !member.static_ {
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
                        lowered_name.clone(),
                        function,
                        Some(self_type.clone()),
                        !member.static_,
                    ));

                    if !member.static_ {
                        trait_impl_methods.push(LoweredTraitImplMethod {
                            name: name.clone(),
                            function_name: lowered_name.clone(),
                        });
                    }
                }

                if matches!(self_type, LoweredType::Struct(_) | LoweredType::Enum(_))
                    && let Some(trait_) = traits.get_mut(&item.trait_ref.name)
                {
                    trait_.impls.push(LoweredTraitImpl {
                        self_type,
                        methods: trait_impl_methods,
                    });
                }
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
                    &traits,
                    &mut diagnostics,
                ) {
                    signatures.insert(name.clone(), signature);
                    functions_to_lower.push((name.clone(), function, None, false));
                }
            }
            Item::Import(_) | Item::Trait(_) => {}
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
                &traits,
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
            &traits,
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
            &traits,
            &mut diagnostics,
        ) {
            functions.push(function);
        }
    }

    let statements = lower_main(
        main,
        &signatures,
        &structs,
        &enums,
        &traits,
        &mut diagnostics,
    );
    let (closure_functions, closure_diagnostics) = CLOSURE_LOWERING.with(|state| {
        let mut state = state.borrow_mut();
        (
            std::mem::take(&mut state.closure_functions),
            std::mem::take(&mut state.diagnostics),
        )
    });
    diagnostics.extend(closure_diagnostics);

    if diagnostics.is_empty() {
        let mut structs = structs.into_values().collect::<Vec<_>>();
        structs.sort_by(|left, right| left.name.cmp(&right.name));
        let mut enums = enums.into_values().collect::<Vec<_>>();
        enums.sort_by(|left, right| left.name.cmp(&right.name));

        let mut traits = traits.into_values().collect::<Vec<_>>();
        traits.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(LoweredProgram {
            structs,
            enums,
            traits,
            functions,
            closure_functions,
            statements,
        })
    } else {
        Err(diagnostics)
    }
}

fn lower_trait_definition(
    item: &TraitDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredTrait> {
    let mut methods = Vec::new();
    let mut can_lower = true;

    for method in &item.methods {
        if method.static_ {
            continue;
        }

        let Some(method) =
            lower_trait_method_definition(method, structs, enums, traits, diagnostics)
        else {
            can_lower = false;
            continue;
        };
        methods.push(method);
    }

    can_lower.then(|| LoweredTrait {
        name: item.name.clone(),
        methods,
        impls: Vec::new(),
    })
}

fn lower_trait_method_definition(
    method: &TraitMethodDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredTraitMethod> {
    let mut params = Vec::new();
    let mut can_lower = true;

    for param in method.params.iter().filter(|param| !is_self_param(param)) {
        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "trait method parameters must include type annotations in executable builds",
            ));
            can_lower = false;
            continue;
        };

        let Some(type_) = lower_trait_object_type_ref(
            type_ref,
            structs,
            enums,
            traits,
            diagnostics,
            "trait object methods only support basic, known struct, enum, trait, and function parameter types in executable builds",
        ) else {
            can_lower = false;
            continue;
        };

        params.push(LoweredParamSignature {
            type_,
            mutable: param.mutable,
        });
    }

    let Some(return_type) = method.return_type.as_ref().and_then(|return_type| {
        lower_trait_object_type_ref(
            return_type,
            structs,
            enums,
            traits,
            diagnostics,
            "trait object methods only support basic, known struct, enum, trait, and function return types in executable builds",
        )
    }) else {
        return None;
    };

    can_lower.then(|| LoweredTraitMethod {
        name: method.name.clone(),
        params,
        return_type,
        mutable_self: has_trait_mutable_receiver(method),
    })
}

fn lower_trait_object_type_ref(
    type_ref: &TypeRef,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if type_ref.name == "Self" && type_ref.args.is_empty() {
        diagnostics.push(Diagnostic::error(
            type_ref.span,
            "`Self` is not supported in dynamically dispatched trait method signatures yet",
        ));
        return None;
    }

    lower_value_type_ref(type_ref, structs, enums, traits, diagnostics, message)
}

fn has_trait_mutable_receiver(method: &TraitMethodDecl) -> bool {
    method
        .params
        .iter()
        .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none())
}

fn lower_enum_definition(
    item: &crate::ast::EnumDecl,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
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
                traits,
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
    traits: &HashMap<String, LoweredTrait>,
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
                    traits,
                    diagnostics,
                    "struct fields only support basic, known struct, enum, trait, and function types in executable builds",
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
            raw_buffer_element: None,
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
    traits: &HashMap<String, LoweredTrait>,
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
            traits,
            diagnostics,
            "only basic, known struct, enum, trait, and function parameter types are supported in executable builds",
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
            traits,
            diagnostics,
            "only basic, known struct, enum, trait, and function return types are supported in executable builds",
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
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if let Some(function) = &type_ref.function {
        let mut params = Vec::new();
        let mut can_lower = true;
        for param in &function.params {
            let Some(type_) = lower_value_type_ref_in_context(
                &param.type_ref,
                self_type,
                structs,
                enums,
                traits,
                diagnostics,
                message,
            ) else {
                can_lower = false;
                continue;
            };
            params.push(LoweredFunctionTypeParam {
                type_,
                mutable: param.mutable,
            });
        }
        let return_type = lower_value_type_ref_in_context(
            &function.return_type,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            message,
        );
        return if can_lower {
            return_type.map(|return_type| LoweredType::Function {
                params,
                return_type: Box::new(return_type),
            })
        } else {
            None
        };
    }

    if type_ref.name == "Self" && type_ref.args.is_empty() {
        return self_type.cloned().or_else(|| {
            diagnostics.push(Diagnostic::error(
                type_ref.span,
                "`Self` is only available in methods and extension functions",
            ));
            None
        });
    }

    lower_value_type_ref(type_ref, structs, enums, traits, diagnostics, message)
}

fn lower_value_type_ref(
    type_ref: &TypeRef,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    message: &str,
) -> Option<LoweredType> {
    if let Some(function) = &type_ref.function {
        let mut params = Vec::new();
        let mut can_lower = true;
        for param in &function.params {
            let Some(type_) = lower_value_type_ref(
                &param.type_ref,
                structs,
                enums,
                traits,
                diagnostics,
                message,
            ) else {
                can_lower = false;
                continue;
            };
            params.push(LoweredFunctionTypeParam {
                type_,
                mutable: param.mutable,
            });
        }
        let return_type = lower_value_type_ref(
            &function.return_type,
            structs,
            enums,
            traits,
            diagnostics,
            message,
        );
        return if can_lower {
            return_type.map(|return_type| LoweredType::Function {
                params,
                return_type: Box::new(return_type),
            })
        } else {
            None
        };
    }

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

    if traits.contains_key(&type_ref.name) {
        return Some(LoweredType::Trait(type_ref.name.clone()));
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
    traits: &HashMap<String, LoweredTrait>,
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
        let Some(type_) = quiet_lower_type_ref(type_ref, &locals, structs, enums, traits) else {
            return Ok(None);
        };
        locals.insert(param.name.clone(), type_);
    }

    match &function.body {
        FunctionBody::Expr(expr) => Ok(infer_expr_type(
            expr, &locals, signatures, structs, enums, traits,
        )),
        FunctionBody::Block(block) => {
            let mut return_type = None;
            let mut has_unresolved_value_return = false;
            infer_block_return_types(
                block,
                &mut locals,
                signatures,
                structs,
                enums,
                traits,
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
    traits: &HashMap<String, LoweredTrait>,
    return_type: &mut Option<LoweredType>,
    has_unresolved_value_return: &mut bool,
) -> Result<(), ReturnTypeConflict> {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value
                    && let Some(type_) =
                        infer_expr_type(value, locals, signatures, structs, enums, traits)
                {
                    locals.insert(name.clone(), type_);
                }
            }
            StmtKind::Return { value: Some(value) } => {
                if let Some(type_) =
                    infer_expr_type(value, locals, signatures, structs, enums, traits)
                {
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
                    traits,
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
                            traits,
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
                                traits,
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
                    traits,
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
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    match &expr.kind {
        ExprKind::String(_) => Some(LoweredType::Basic(BasicType::String)),
        ExprKind::Char(_) => Some(LoweredType::Basic(BasicType::Char)),
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
        } => infer_expr_type(operand, locals, signatures, structs, enums, traits),
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
                infer_expr_type(right, locals, signatures, structs, enums, traits)
            } else {
                infer_expr_type(left, locals, signatures, structs, enums, traits)
            }
        }
        ExprKind::Number(value) => Some(LoweredType::Basic(if number_literal_is_float(value) {
            BasicType::F64
        } else {
            BasicType::I32
        })),
        ExprKind::Identifier(name) => locals.get(name).cloned().or_else(|| {
            let signature = signatures.get(name)?;
            signature
                .return_type_known
                .then(|| function_type_from_signature(signature))
        }),
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
        ExprKind::Range { inclusive, .. } => {
            let source_name = if *inclusive {
                "RangeInclusive"
            } else {
                "Range"
            };
            find_lowered_struct_by_source_name(source_name, structs).map(LoweredType::Struct)
        }
        ExprKind::Member { object, name } => {
            if let ExprKind::Identifier(enum_name) = &object.kind
                && let Some(variant) = find_qualified_variant(enums, enum_name, name)
                && variant.payload.is_none()
            {
                return Some(LoweredType::Enum(enum_name.clone()));
            }

            let LoweredType::Struct(struct_name) =
                infer_expr_type(object, locals, signatures, structs, enums, traits)?
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
        ExprKind::GenericMember { object, .. } => {
            infer_expr_type(object, locals, signatures, structs, enums, traits)
        }
        ExprKind::Call { callee, .. } => {
            if let ExprKind::Member { object, name } = &callee.kind
                && name == "clone"
            {
                return infer_expr_type(object, locals, signatures, structs, enums, traits);
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
                let object_type =
                    infer_expr_type(object, locals, signatures, structs, enums, traits)?;
                if let LoweredType::Trait(trait_name) = &object_type {
                    return traits
                        .get(trait_name)?
                        .methods
                        .iter()
                        .find(|method| method.name == *name)
                        .map(|method| method.return_type.clone());
                }
                return signatures
                    .get(&callable_method_name(&object_type, name, signatures)?)
                    .filter(|signature| signature.return_type_known)
                    .map(|signature| signature.return_type.clone());
            }

            if let Some(LoweredType::Function { return_type, .. }) =
                infer_expr_type(callee, locals, signatures, structs, enums, traits)
            {
                return Some(*return_type);
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
        | ExprKind::CollectionLiteral { .. }
        | ExprKind::GenericType { .. }
        | ExprKind::Missing => None,
        ExprKind::Lambda(function) => {
            infer_lambda_expr_type(function, locals, signatures, structs, enums, traits)
        }
        ExprKind::Match { branches, .. } => branches.iter().find_map(|branch| {
            let MatchBranchBody::Expr(expr) = &branch.body else {
                return None;
            };
            infer_expr_type(expr, locals, signatures, structs, enums, traits)
        }),
        ExprKind::PostfixIncrement(target) => {
            infer_expr_type(target, locals, signatures, structs, enums, traits)
        }
    }
}

fn function_type_from_signature(signature: &FunctionSignature) -> LoweredType {
    LoweredType::Function {
        params: signature
            .params
            .iter()
            .map(|param| LoweredFunctionTypeParam {
                type_: param.type_.clone(),
                mutable: param.mutable,
            })
            .collect(),
        return_type: Box::new(signature.return_type.clone()),
    }
}

fn infer_lambda_expr_type(
    function: &FunctionDecl,
    outer_locals: &HashMap<String, LoweredType>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    let mut locals = outer_locals.clone();
    let mut params = Vec::new();
    for param in &function.params {
        let type_ = quiet_lower_type_ref(
            param.type_ref.as_ref()?,
            outer_locals,
            structs,
            enums,
            traits,
        )?;
        locals.insert(param.name.clone(), type_.clone());
        params.push(LoweredFunctionTypeParam {
            type_,
            mutable: param.mutable,
        });
    }

    let return_type = if let Some(type_ref) = &function.return_type {
        quiet_lower_type_ref(type_ref, outer_locals, structs, enums, traits)?
    } else {
        match &function.body {
            FunctionBody::Expr(expr) => {
                infer_expr_type(expr, &locals, signatures, structs, enums, traits)?
            }
            FunctionBody::Block(block) => {
                let mut return_type = None;
                let mut has_unresolved_value_return = false;
                infer_block_return_types(
                    block,
                    &mut locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    &mut return_type,
                    &mut has_unresolved_value_return,
                )
                .ok()?;
                if has_unresolved_value_return {
                    return None;
                }
                return_type.unwrap_or(LoweredType::Void)
            }
        }
    };

    Some(LoweredType::Function {
        params,
        return_type: Box::new(return_type),
    })
}

fn quiet_lower_type_ref(
    type_ref: &TypeRef,
    locals: &HashMap<String, LoweredType>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
) -> Option<LoweredType> {
    if let Some(function) = &type_ref.function {
        let params = function
            .params
            .iter()
            .map(|param| {
                Some(LoweredFunctionTypeParam {
                    type_: quiet_lower_type_ref(&param.type_ref, locals, structs, enums, traits)?,
                    mutable: param.mutable,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let return_type =
            quiet_lower_type_ref(&function.return_type, locals, structs, enums, traits)?;
        return Some(LoweredType::Function {
            params,
            return_type: Box::new(return_type),
        });
    }

    if !type_ref.args.is_empty() {
        return None;
    }
    if type_ref.name == "Self" {
        return locals.get("Self").cloned();
    }
    if let Some(type_) = BasicType::from_name(&type_ref.name) {
        Some(LoweredType::Basic(type_))
    } else if type_ref.name == "void" {
        Some(LoweredType::Void)
    } else if structs.contains_key(&type_ref.name) {
        Some(LoweredType::Struct(type_ref.name.clone()))
    } else if enums.contains_key(&type_ref.name) {
        Some(LoweredType::Enum(type_ref.name.clone()))
    } else if traits.contains_key(&type_ref.name) {
        Some(LoweredType::Trait(type_ref.name.clone()))
    } else {
        None
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
        ExprKind::GenericMember { object, .. } => expression_has_mutable_capability(object, locals),
        ExprKind::StructInit { .. }
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Range { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Unary { .. } => true,
        ExprKind::Call { callee, .. } => {
            matches!(&callee.kind, ExprKind::Member { name, .. } if name == "clone")
        }
        ExprKind::Array(_)
        | ExprKind::CollectionLiteral { .. }
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
        LoweredExprKind::Local(name) | LoweredExprKind::LocalCell(name) => {
            locals.get(name).is_some_and(|local| local.mutable)
        }
        LoweredExprKind::CapturedLocal { .. } => true,
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
        LoweredExprKind::TraitObject { .. } => true,
        LoweredExprKind::Call { name, args } => signatures.get(name).is_some_and(|signature| {
            matches!(signature.return_type, LoweredType::Struct(_))
                && args.iter().zip(&signature.params).all(|(arg, param)| {
                    !matches!(param.type_, LoweredType::Struct(_))
                        || lowered_expression_has_mutable_capability(
                            arg, locals, signatures, structs,
                        )
                })
        }),
        LoweredExprKind::IndirectCall { .. } | LoweredExprKind::DynamicCall { .. } => {
            matches!(expr.type_, LoweredType::Struct(_) | LoweredType::Trait(_))
        }
        LoweredExprKind::StringLiteral(_)
        | LoweredExprKind::BoolLiteral(_)
        | LoweredExprKind::NumberLiteral(_)
        | LoweredExprKind::StringConcat(_, _)
        | LoweredExprKind::Not(_)
        | LoweredExprKind::Negate(_)
        | LoweredExprKind::Arithmetic { .. }
        | LoweredExprKind::Logical { .. }
        | LoweredExprKind::Comparison { .. }
        | LoweredExprKind::NumberToString(_)
        | LoweredExprKind::CollectionLiteral { .. }
        | LoweredExprKind::Closure { .. } => true,
        LoweredExprKind::Void
        | LoweredExprKind::PostfixIncrement(_)
        | LoweredExprKind::EnumLiteral { .. }
        | LoweredExprKind::EnumPayload { .. }
        | LoweredExprKind::MatchValue(_)
        | LoweredExprKind::Match { .. } => false,
    }
}

fn captured_let_names(function: &FunctionDecl) -> HashSet<String> {
    let mut captured = HashSet::new();
    let mut available = HashSet::new();
    match &function.body {
        FunctionBody::Block(block) => collect_block_captures(block, &mut available, &mut captured),
        FunctionBody::Expr(expr) => collect_expr_captures(expr, &available, &mut captured),
    }
    captured
}

fn collect_block_captures(
    block: &Block,
    available: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value {
                    collect_expr_captures(value, available, captured);
                }
                available.insert(name.clone());
            }
            StmtKind::Assign { target, value, .. } => {
                collect_expr_captures(target, available, captured);
                collect_expr_captures(value, available, captured);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value.as_ref() {
                    collect_expr_captures(value, available, captured);
                }
            }
            StmtKind::Expr(value) => collect_expr_captures(value, available, captured),
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_expr_captures(condition, available, captured);
                let mut branch_available = available.clone();
                collect_block_captures(then_branch, &mut branch_available, captured);
                if let Some(else_branch) = else_branch {
                    let mut branch_available = available.clone();
                    match else_branch {
                        ElseBranch::Block(block) => {
                            collect_block_captures(block, &mut branch_available, captured)
                        }
                        ElseBranch::If(statement) => {
                            collect_statement_captures(statement, &mut branch_available, captured)
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                collect_expr_captures(condition, available, captured);
                let mut body_available = available.clone();
                collect_block_captures(body, &mut body_available, captured);
            }
            StmtKind::For {
                iterable,
                body,
                name,
            } => {
                collect_expr_captures(iterable, available, captured);
                let mut body_available = available.clone();
                body_available.insert(name.clone());
                collect_block_captures(body, &mut body_available, captured);
            }
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

fn collect_statement_captures(
    statement: &Stmt,
    available: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    let block = Block {
        statements: vec![statement.clone()],
        span: statement.span,
    };
    collect_block_captures(&block, available, captured);
}

fn collect_expr_captures(expr: &Expr, available: &HashSet<String>, captured: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Lambda(function) => {
            let mut lambda_locals = HashSet::new();
            for param in &function.params {
                lambda_locals.insert(param.name.clone());
            }
            collect_lambda_body_captures(function, available, &mut lambda_locals, captured);
        }
        ExprKind::Call { callee, args } => {
            collect_expr_captures(callee, available, captured);
            for arg in args {
                collect_expr_captures(arg, available, captured);
            }
        }
        ExprKind::Member { object, .. }
        | ExprKind::GenericMember { object, .. }
        | ExprKind::Unary {
            operand: object, ..
        }
        | ExprKind::PostfixIncrement(object) => collect_expr_captures(object, available, captured),
        ExprKind::Binary { left, right, .. } => {
            collect_expr_captures(left, available, captured);
            collect_expr_captures(right, available, captured);
        }
        ExprKind::Array(items) => {
            for item in items {
                collect_expr_captures(item, available, captured);
            }
        }
        ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_expr_captures(item, available, captured);
            }
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_expr_captures(&field.value, available, captured);
            }
        }
        ExprKind::Range { start, end, .. } => {
            collect_expr_captures(start, available, captured);
            collect_expr_captures(end, available, captured);
        }
        ExprKind::Match { value, branches } => {
            collect_expr_captures(value, available, captured);
            for branch in branches {
                match &branch.body {
                    MatchBranchBody::Expr(expr) => collect_expr_captures(expr, available, captured),
                    MatchBranchBody::Block(block) => {
                        let mut branch_available = available.clone();
                        collect_block_captures(block, &mut branch_available, captured);
                    }
                }
            }
        }
        ExprKind::Identifier(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Missing => {}
    }
}

fn collect_lambda_body_captures(
    function: &FunctionDecl,
    available: &HashSet<String>,
    lambda_locals: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    match &function.body {
        FunctionBody::Expr(expr) => {
            collect_lambda_expr_captures(expr, available, lambda_locals, captured)
        }
        FunctionBody::Block(block) => {
            for statement in &block.statements {
                match &statement.kind {
                    StmtKind::Let { name, value, .. } => {
                        if let Some(value) = value {
                            collect_lambda_expr_captures(value, available, lambda_locals, captured);
                        }
                        lambda_locals.insert(name.clone());
                    }
                    StmtKind::Assign { target, value, .. } => {
                        collect_lambda_expr_captures(target, available, lambda_locals, captured);
                        collect_lambda_expr_captures(value, available, lambda_locals, captured);
                    }
                    StmtKind::Return { value } => {
                        if let Some(value) = value.as_ref() {
                            collect_lambda_expr_captures(value, available, lambda_locals, captured);
                        }
                    }
                    StmtKind::Expr(value) => {
                        collect_lambda_expr_captures(value, available, lambda_locals, captured)
                    }
                    StmtKind::If {
                        condition,
                        then_branch,
                        else_branch,
                    } => {
                        collect_lambda_expr_captures(condition, available, lambda_locals, captured);
                        let mut branch_locals = lambda_locals.clone();
                        collect_lambda_block_captures(
                            then_branch,
                            available,
                            &mut branch_locals,
                            captured,
                        );
                        if let Some(else_branch) = else_branch {
                            let mut branch_locals = lambda_locals.clone();
                            match else_branch {
                                ElseBranch::Block(block) => collect_lambda_block_captures(
                                    block,
                                    available,
                                    &mut branch_locals,
                                    captured,
                                ),
                                ElseBranch::If(statement) => {
                                    let block = Block {
                                        statements: vec![(**statement).clone()],
                                        span: statement.span,
                                    };
                                    collect_lambda_block_captures(
                                        &block,
                                        available,
                                        &mut branch_locals,
                                        captured,
                                    );
                                }
                            }
                        }
                    }
                    StmtKind::While { condition, body } => {
                        collect_lambda_expr_captures(condition, available, lambda_locals, captured);
                        let mut body_locals = lambda_locals.clone();
                        collect_lambda_block_captures(body, available, &mut body_locals, captured);
                    }
                    StmtKind::For {
                        iterable,
                        body,
                        name,
                    } => {
                        collect_lambda_expr_captures(iterable, available, lambda_locals, captured);
                        let mut body_locals = lambda_locals.clone();
                        body_locals.insert(name.clone());
                        collect_lambda_block_captures(body, available, &mut body_locals, captured);
                    }
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
        }
    }
}

fn collect_lambda_block_captures(
    block: &Block,
    available: &HashSet<String>,
    lambda_locals: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    let function = FunctionDecl {
        name: None,
        type_params: Vec::new(),
        type_param_bounds: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body: FunctionBody::Block(block.clone()),
        span: block.span,
    };
    collect_lambda_body_captures(&function, available, lambda_locals, captured);
}

fn collect_lambda_expr_captures(
    expr: &Expr,
    available: &HashSet<String>,
    lambda_locals: &mut HashSet<String>,
    captured: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            if available.contains(name) && !lambda_locals.contains(name) {
                captured.insert(name.clone());
            }
        }
        ExprKind::Lambda(function) => {
            let mut nested_locals = lambda_locals.clone();
            for param in &function.params {
                nested_locals.insert(param.name.clone());
            }
            collect_lambda_body_captures(function, available, &mut nested_locals, captured);
        }
        ExprKind::Call { callee, args } => {
            collect_lambda_expr_captures(callee, available, lambda_locals, captured);
            for arg in args {
                collect_lambda_expr_captures(arg, available, lambda_locals, captured);
            }
        }
        ExprKind::Member { object, .. }
        | ExprKind::GenericMember { object, .. }
        | ExprKind::Unary {
            operand: object, ..
        }
        | ExprKind::PostfixIncrement(object) => {
            collect_lambda_expr_captures(object, available, lambda_locals, captured)
        }
        ExprKind::Binary { left, right, .. } => {
            collect_lambda_expr_captures(left, available, lambda_locals, captured);
            collect_lambda_expr_captures(right, available, lambda_locals, captured);
        }
        ExprKind::Array(items) => {
            for item in items {
                collect_lambda_expr_captures(item, available, lambda_locals, captured);
            }
        }
        ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_lambda_expr_captures(item, available, lambda_locals, captured);
            }
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_lambda_expr_captures(&field.value, available, lambda_locals, captured);
            }
        }
        ExprKind::Range { start, end, .. } => {
            collect_lambda_expr_captures(start, available, lambda_locals, captured);
            collect_lambda_expr_captures(end, available, lambda_locals, captured);
        }
        ExprKind::Match { value, branches } => {
            collect_lambda_expr_captures(value, available, lambda_locals, captured);
            for branch in branches {
                match &branch.body {
                    MatchBranchBody::Expr(expr) => {
                        collect_lambda_expr_captures(expr, available, lambda_locals, captured)
                    }
                    MatchBranchBody::Block(block) => {
                        let mut branch_locals = lambda_locals.clone();
                        collect_lambda_block_captures(
                            block,
                            available,
                            &mut branch_locals,
                            captured,
                        );
                    }
                }
            }
        }
        ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Missing => {}
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
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredFunction> {
    let signature = signatures.get(name)?;
    let captured_names = captured_let_names(function);
    CAPTURED_NAMES.with(|names| *names.borrow_mut() = captured_names);

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
                captured: false,
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
                captured: false,
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
                    captured: false,
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
                traits,
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
                            traits,
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
                            traits,
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
                                traits,
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
                            traits,
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
                            traits,
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
                            traits,
                            diagnostics,
                            Some(&signature.return_type),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
                    StmtKind::For { .. } => statements.extend(lower_for_statement(
                        statement,
                        &locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(&signature.return_type),
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
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LoweredStatement> {
    let mut statements = Vec::new();
    let mut locals = HashMap::new();
    let captured_names = captured_let_names(main);
    CAPTURED_NAMES.with(|names| *names.borrow_mut() = captured_names);

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
                            traits,
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
                            traits,
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
                            traits,
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
                            traits,
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
                            traits,
                            diagnostics,
                            None,
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
                    StmtKind::For { .. } => statements.extend(lower_for_statement(
                        statement,
                        &locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        None,
                    )),
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
    traits: &HashMap<String, LoweredTrait>,
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
        traits,
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
        traits,
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
                traits,
                diagnostics,
                return_type,
            ),
            ElseBranch::If(statement) => lower_if_statement(
                statement,
                &branch_locals,
                signatures,
                structs,
                enums,
                traits,
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
    traits: &HashMap<String, LoweredTrait>,
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
        traits,
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
        traits,
        diagnostics,
        return_type,
    );

    Some(LoweredStatement::While { condition, body })
}

fn iteration_trait_name_matches(trait_name: &str, protocol: &str) -> bool {
    let Some((trait_head, _)) = trait_name.split_once('<') else {
        return false;
    };

    source_callable_name(trait_head) == protocol
}

fn iterator_trait_for<'a>(
    type_: &LoweredType,
    traits: &'a HashMap<String, LoweredTrait>,
) -> Option<&'a LoweredTrait> {
    if let LoweredType::Trait(trait_name) = type_
        && iteration_trait_name_matches(trait_name, "Iterator")
    {
        return traits.get(trait_name);
    }

    traits.values().find(|trait_| {
        iteration_trait_name_matches(&trait_.name, "Iterator")
            && trait_.impls.iter().any(|impl_| impl_.self_type == *type_)
    })
}

fn iterable_trait_for<'a>(
    type_: &LoweredType,
    traits: &'a HashMap<String, LoweredTrait>,
) -> Option<&'a LoweredTrait> {
    if let LoweredType::Trait(trait_name) = type_
        && iteration_trait_name_matches(trait_name, "Iterable")
    {
        return traits.get(trait_name);
    }

    traits.values().find(|trait_| {
        iteration_trait_name_matches(&trait_.name, "Iterable")
            && trait_.impls.iter().any(|impl_| impl_.self_type == *type_)
    })
}

fn trait_impl_method_name(
    trait_: &LoweredTrait,
    self_type: &LoweredType,
    method_name: &str,
) -> Option<String> {
    trait_
        .impls
        .iter()
        .find(|impl_| impl_.self_type == *self_type)?
        .methods
        .iter()
        .find(|method| method.name == method_name)
        .map(|method| method.function_name.clone())
}

fn coerce_to_iterator(
    value: LoweredExpr,
    iterator_trait: &LoweredTrait,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let iterator_type = LoweredType::Trait(iterator_trait.name.clone());
    if value.type_ == iterator_type {
        return Some(value);
    }

    if !matches!(value.type_, LoweredType::Struct(_) | LoweredType::Enum(_))
        || !iterator_trait
            .impls
            .iter()
            .any(|impl_| impl_.self_type == value.type_)
    {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "iteration method must return `{}`, got `{}`",
                iterator_type.name(),
                value.type_.name()
            ),
        ));
        return None;
    }

    Some(LoweredExpr {
        type_: iterator_type,
        kind: LoweredExprKind::TraitObject {
            trait_name: iterator_trait.name.clone(),
            self_type: value.type_.clone(),
            value: Box::new(value),
        },
    })
}

fn for_iterator_local_name(span: Span) -> String {
    format!("internal_for_iterator_{}_{}", span.start, span.end)
}

fn lower_for_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Vec<LoweredStatement> {
    let StmtKind::For {
        name,
        iterable,
        body,
    } = &statement.kind
    else {
        return Vec::new();
    };

    let Some(iterable) = lower_expr(
        iterable,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        None,
        "expected an iterable value in executable builds",
    ) else {
        return Vec::new();
    };

    let iterator = if let Some(iterator_trait) = iterator_trait_for(&iterable.type_, traits) {
        coerce_to_iterator(iterable, iterator_trait, statement.span, diagnostics)
    } else {
        let Some(iterable_trait) = iterable_trait_for(&iterable.type_, traits) else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "`for` requires an `Iterator<T>` or `Iterable<T>`, got `{}`",
                    iterable.type_.name()
                ),
            ));
            return Vec::new();
        };
        let Some(iterator_method) = iterable_trait
            .methods
            .iter()
            .find(|method| method.name == "iterator")
        else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "trait `{}` does not define `iterator()`",
                    iterable_trait.name
                ),
            ));
            return Vec::new();
        };
        let LoweredType::Trait(iterator_trait_name) = &iterator_method.return_type else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "`{}.iterator()` must return an `Iterator<T>`",
                    iterable_trait.name
                ),
            ));
            return Vec::new();
        };
        let Some(iterator_trait) = traits.get(iterator_trait_name) else {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!("unknown iterator trait `{iterator_trait_name}`"),
            ));
            return Vec::new();
        };
        if !iteration_trait_name_matches(&iterator_trait.name, "Iterator") {
            diagnostics.push(Diagnostic::error(
                statement.span,
                format!(
                    "`{}.iterator()` must return an `Iterator<T>`",
                    iterable_trait.name
                ),
            ));
            return Vec::new();
        }

        let iterator_value = if matches!(iterable.type_, LoweredType::Trait(_)) {
            LoweredExpr {
                type_: iterator_method.return_type.clone(),
                kind: LoweredExprKind::DynamicCall {
                    object: Box::new(iterable),
                    method: "iterator".to_string(),
                    args: Vec::new(),
                },
            }
        } else {
            let Some(function_name) =
                trait_impl_method_name(iterable_trait, &iterable.type_, "iterator")
            else {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    format!(
                        "`{}` does not implement `{}::iterator()`",
                        iterable.type_.name(),
                        iterable_trait.name
                    ),
                ));
                return Vec::new();
            };
            let Some(signature) = signatures.get(&function_name) else {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "failed to lower the iterable `iterator()` implementation",
                ));
                return Vec::new();
            };
            LoweredExpr {
                type_: signature.return_type.clone(),
                kind: LoweredExprKind::Call {
                    name: function_name,
                    args: vec![iterable],
                },
            }
        };

        coerce_to_iterator(iterator_value, iterator_trait, statement.span, diagnostics)
    };
    let Some(iterator) = iterator else {
        return Vec::new();
    };

    let LoweredType::Trait(iterator_trait_name) = &iterator.type_ else {
        unreachable!("iterator coercion must produce a trait object");
    };
    let Some(iterator_trait) = traits.get(iterator_trait_name) else {
        return Vec::new();
    };
    let Some(next_method) = iterator_trait
        .methods
        .iter()
        .find(|method| method.name == "next")
    else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("trait `{}` does not define `next()`", iterator_trait.name),
        ));
        return Vec::new();
    };
    let LoweredType::Enum(option_name) = &next_method.return_type else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("`{}.next()` must return `Option<T>`", iterator_trait.name),
        ));
        return Vec::new();
    };
    let Some(option) = enums.get(option_name) else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("unknown iterator result enum `{option_name}`"),
        ));
        return Vec::new();
    };
    let Some(item_type) = option
        .variants
        .iter()
        .find(|variant| variant.name == "Some")
        .and_then(|variant| variant.payload.clone())
    else {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("iterator result enum `{option_name}` must define `Some(T)`"),
        ));
        return Vec::new();
    };
    if !option.variants.iter().any(|variant| variant.name == "None") {
        diagnostics.push(Diagnostic::error(
            statement.span,
            format!("iterator result enum `{option_name}` must define `None`"),
        ));
        return Vec::new();
    }

    let iterator_name = for_iterator_local_name(statement.span);
    let match_value_name = match_temp_name(statement.span);
    let mut loop_locals = locals.clone();
    loop_locals.insert(
        iterator_name.clone(),
        LoweringLocal {
            type_: iterator.type_.clone(),
            mutable: true,
            replacement: None,
            captured: false,
        },
    );
    let mut item_locals = loop_locals.clone();
    item_locals.insert(
        name.clone(),
        LoweringLocal {
            type_: item_type.clone(),
            mutable: false,
            replacement: Some(LoweredExpr {
                type_: item_type.clone(),
                kind: LoweredExprKind::EnumPayload {
                    object: Box::new(LoweredExpr {
                        type_: next_method.return_type.clone(),
                        kind: LoweredExprKind::MatchValue(match_value_name.clone()),
                    }),
                    variant: "Some".to_string(),
                },
            }),
            captured: false,
        },
    );
    let body = lower_conditional_block(
        body,
        &mut item_locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        return_type,
    );
    let next = LoweredExpr {
        type_: next_method.return_type.clone(),
        kind: LoweredExprKind::DynamicCall {
            object: Box::new(LoweredExpr {
                type_: iterator.type_.clone(),
                kind: LoweredExprKind::Local(iterator_name.clone()),
            }),
            method: "next".to_string(),
            args: Vec::new(),
        },
    };

    vec![
        LoweredStatement::Local {
            name: iterator_name,
            value: iterator,
        },
        LoweredStatement::While {
            condition: LoweredExpr {
                type_: LoweredType::Basic(BasicType::Bool),
                kind: LoweredExprKind::BoolLiteral(true),
            },
            body: vec![LoweredStatement::Match {
                value: next,
                temp_name: match_value_name,
                branches: vec![
                    LoweredMatchStatementBranch {
                        pattern: LoweredPattern::Variant {
                            enum_name: option_name.clone(),
                            variant: "Some".to_string(),
                        },
                        statements: body,
                    },
                    LoweredMatchStatementBranch {
                        pattern: LoweredPattern::Variant {
                            enum_name: option_name.clone(),
                            variant: "None".to_string(),
                        },
                        statements: vec![LoweredStatement::Break],
                    },
                ],
            }],
        },
    ]
}

fn lower_conditional_block(
    block: &Block,
    locals: &mut HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
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
                    traits,
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
                    traits,
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
                        traits,
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
                    traits,
                    diagnostics,
                    return_type,
                ) {
                    statements.push(statement);
                }
            }
            StmtKind::For { .. } => statements.extend(lower_for_statement(
                statement,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                return_type,
            )),
            StmtKind::While { .. } => {
                if let Some(statement) = lower_while_statement(
                    statement,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
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
                    traits,
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
        | LoweredStatement::LocalCell { .. }
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
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    if matches!(expr.kind, ExprKind::Match { .. }) {
        return lower_match_statement(
            expr,
            locals,
            signatures,
            structs,
            enums,
            traits,
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
            traits,
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
                "`io.println` expects exactly one `string` value in executable builds",
            ));
            return None;
        }

        let value = lower_expr(
            &args[0],
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            None,
            "`io.println` only accepts `string` values in executable builds",
        )?;

        if value.type_ != LoweredType::Basic(BasicType::String) {
            diagnostics.push(Diagnostic::error(
                args[0].span,
                format!(
                    "`io.println` only accepts `string` values in executable builds, got `{}`",
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
        traits,
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
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    return_type: Option<&LoweredType>,
) -> Option<LoweredStatement> {
    let ExprKind::Match { value, branches } = &expr.kind else {
        return None;
    };
    let value_mutable = expression_has_mutable_capability(value, locals);
    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        None,
        "expected supported match value in executable builds",
    )?;
    if !matches!(
        value.type_,
        LoweredType::Enum(_)
            | LoweredType::Basic(BasicType::String)
            | LoweredType::Basic(BasicType::I32)
    ) {
        diagnostics.push(Diagnostic::error(
            expr.span,
            "match statements require an enum, `string`, or `i32` value in executable builds",
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
            value_mutable,
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
                traits,
                diagnostics,
                return_type,
            ),
            MatchBranchBody::Expr(branch_expr) => lower_expression_statement(
                branch_expr,
                &branch_locals,
                signatures,
                structs,
                enums,
                traits,
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
    traits: &HashMap<String, LoweredTrait>,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
            StmtKind::For { .. } => statements.extend(lower_for_statement(
                statement,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                None,
            )),
        }
    }

    let value = lower_expr(
        value,
        locals,
        signatures,
        structs,
        enums,
        traits,
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
    traits: &HashMap<String, LoweredTrait>,
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
        let self_type = locals.get("Self").map(|local| &local.type_);
        let lowered = lower_value_type_ref_in_context(
            type_annotation,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "only basic, struct, enum, trait, and function local types are supported in executable builds",
        );
        if lowered.is_none() {
            can_lower = false;
        }
        lowered
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
            traits,
            diagnostics,
            annotated_type.clone(),
            "only literal, string concat, struct literal, field access, and function call local values are supported in executable builds",
        )
    } else if let Some(type_) = annotated_type.clone() {
        let kind = match type_ {
            LoweredType::Basic(BasicType::String) => LoweredExprKind::StringLiteral(String::new()),
            LoweredType::Basic(BasicType::Char) => LoweredExprKind::NumberLiteral("0".to_string()),
            LoweredType::Basic(BasicType::Bool) => LoweredExprKind::BoolLiteral(false),
            LoweredType::Basic(type_) if type_.is_numeric() => {
                LoweredExprKind::NumberLiteral("0".to_string())
            }
            LoweredType::Struct(_)
            | LoweredType::Enum(_)
            | LoweredType::Trait(_)
            | LoweredType::Function { .. } => {
                diagnostics.push(Diagnostic::error(
                    statement.span,
                    "struct, enum, trait, and function locals must include an initializer in executable builds",
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

    let captured = CAPTURED_NAMES.with(|names| names.borrow().contains(name));

    if locals
        .insert(
            name.clone(),
            LoweringLocal {
                type_: value.type_.clone(),
                mutable: *mutable,
                replacement: None,
                captured,
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

    if captured {
        Some(LoweredStatement::LocalCell {
            name: name.clone(),
            value,
        })
    } else {
        Some(LoweredStatement::Local {
            name: name.clone(),
            value,
        })
    }
}

fn lower_assignment_statement(
    statement: &Stmt,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
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
        traits,
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
        traits,
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
    traits: &HashMap<String, LoweredTrait>,
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
            traits,
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

fn find_lowered_struct_by_source_name(
    source_name: &str,
    structs: &HashMap<String, LoweredStruct>,
) -> Option<String> {
    structs
        .keys()
        .find_map(|name| (source_callable_name(name) == source_name).then(|| name.clone()))
}

fn lower_range_literal(
    expr: &Expr,
    start: &Expr,
    end: &Expr,
    inclusive: bool,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let type_name = if inclusive { "RangeInclusive" } else { "Range" };
    let Some(name) = find_lowered_struct_by_source_name(type_name, structs) else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range literals require an imported `{type_name}` in executable builds"),
        ));
        return None;
    };
    let Some(struct_) = structs.get(&name) else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("unknown range struct `{name}` in executable build"),
        ));
        return None;
    };
    if struct_.fields.len() != 2 {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range struct `{name}` must declare only `start` and `end` fields"),
        ));
        return None;
    }
    let Some(start_field) = struct_.fields.iter().find(|field| field.name == "start") else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range struct `{name}` must declare a `start` field"),
        ));
        return None;
    };
    let Some(end_field) = struct_.fields.iter().find(|field| field.name == "end") else {
        diagnostics.push(Diagnostic::error(
            expr.span,
            format!("range struct `{name}` must declare an `end` field"),
        ));
        return None;
    };

    let start = lower_expr(
        start,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(start_field.type_.clone()),
        "expected supported range start in executable builds",
    )?;
    let end = lower_expr(
        end,
        locals,
        signatures,
        structs,
        enums,
        traits,
        diagnostics,
        Some(end_field.type_.clone()),
        "expected supported range end in executable builds",
    )?;

    Some(LoweredExpr {
        type_: LoweredType::Struct(name.clone()),
        kind: LoweredExprKind::StructLiteral {
            name,
            fields: vec![
                LoweredStructFieldValue {
                    name: "start".to_string(),
                    value: start,
                },
                LoweredStructFieldValue {
                    name: "end".to_string(),
                    value: end,
                },
            ],
        },
    })
}

fn lower_expr(
    expr: &Expr,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
    message: &str,
) -> Option<LoweredExpr> {
    let mut lowered = match &expr.kind {
        ExprKind::String(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::String),
            kind: LoweredExprKind::StringLiteral(value.clone()),
        },
        ExprKind::Char(value) => LoweredExpr {
            type_: LoweredType::Basic(BasicType::Char),
            kind: LoweredExprKind::NumberLiteral(value.to_string()),
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
        ExprKind::CollectionLiteral { items, collection } => lower_collection_literal(
            expr.span,
            items,
            collection,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
        )?,
        ExprKind::Identifier(name) if locals.contains_key(name) => locals[name]
            .replacement
            .clone()
            .unwrap_or_else(|| LoweredExpr {
                type_: locals[name].type_.clone(),
                kind: if locals[name].captured {
                    LoweredExprKind::LocalCell(name.clone())
                } else {
                    LoweredExprKind::Local(name.clone())
                },
            }),
        ExprKind::Identifier(name) if signatures.contains_key(name) => lower_function_value_expr(
            name,
            signatures,
            expected_type.clone(),
            diagnostics,
            expr.span,
        )?,
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
                traits,
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
                traits,
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
                traits,
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
                traits,
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
                traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                    traits,
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
                        LoweredType::Basic(BasicType::String | BasicType::Char | BasicType::Bool)
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
            traits,
            diagnostics,
        )?,
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => lower_range_literal(
            expr,
            start,
            end,
            *inclusive,
            locals,
            signatures,
            structs,
            enums,
            traits,
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

                LoweredExpr {
                    type_: LoweredType::Enum(enum_name.clone()),
                    kind: LoweredExprKind::EnumLiteral {
                        enum_name: enum_name.clone(),
                        variant: name.clone(),
                        payload: None,
                    },
                }
            } else {
                let object = lower_expr(
                    object,
                    locals,
                    signatures,
                    structs,
                    enums,
                    traits,
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
                    traits,
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
                        traits,
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
                        traits,
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
                    traits,
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

                if matches!(object.type_, LoweredType::Basic(BasicType::String))
                    && source_callable_name(name) == "byteLen"
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.byteLen` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Usize),
                        kind: LoweredExprKind::FieldAccess {
                            object: Box::new(object),
                            field: "byte_len".to_string(),
                        },
                    });
                }

                if matches!(&object.type_, LoweredType::Struct(name) if is_string_builder_name(name))
                    && matches!(source_callable_name(name), "append" | "build")
                {
                    let source_name = source_callable_name(name);
                    let expected_count = usize::from(source_name == "append");
                    if args.len() != expected_count {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `StringBuilder.{source_name}` expects {expected_count} arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let mut lowered_args = vec![object];
                    for arg in args {
                        let expected_type = if source_name == "append" {
                            Some(LoweredType::Basic(BasicType::String))
                        } else {
                            None
                        };
                        lowered_args.push(lower_expr(
                            arg,
                            locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            expected_type,
                            "expected StringBuilder argument in executable builds",
                        )?);
                    }

                    return Some(LoweredExpr {
                        type_: if source_name == "append" {
                            LoweredType::Void
                        } else {
                            LoweredType::Basic(BasicType::String)
                        },
                        kind: LoweredExprKind::Call {
                            name: format!("intrinsic StringBuilder.{source_name}"),
                            args: lowered_args,
                        },
                    });
                }

                if matches!(object.type_, LoweredType::Basic(BasicType::String))
                    && source_callable_name(name) == "len"
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.len` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }
                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Usize),
                        kind: LoweredExprKind::Call {
                            name: "intrinsic string.len".to_string(),
                            args: vec![object],
                        },
                    });
                }

                if matches!(object.type_, LoweredType::Basic(BasicType::String))
                    && source_callable_name(name) == "isEmpty"
                {
                    if !args.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `string.isEmpty` expects 0 arguments, got {}",
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let byte_len = LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Usize),
                        kind: LoweredExprKind::FieldAccess {
                            object: Box::new(object),
                            field: "byte_len".to_string(),
                        },
                    };
                    return Some(LoweredExpr {
                        type_: LoweredType::Basic(BasicType::Bool),
                        kind: LoweredExprKind::Comparison {
                            left: Box::new(byte_len),
                            op: BinaryOp::Equal,
                            right: Box::new(LoweredExpr {
                                type_: LoweredType::Basic(BasicType::Usize),
                                kind: LoweredExprKind::NumberLiteral("0".to_string()),
                            }),
                        },
                    });
                }

                if let LoweredType::Trait(trait_name) = &object.type_ {
                    let Some(trait_) = traits.get(trait_name) else {
                        diagnostics.push(Diagnostic::error(
                            callee.span,
                            format!("unknown trait `{trait_name}` in executable build"),
                        ));
                        return None;
                    };
                    let source_name = source_callable_name(name);
                    let Some(method) = trait_
                        .methods
                        .iter()
                        .find(|method| method.name == source_name)
                    else {
                        diagnostics.push(Diagnostic::error(
                            callee.span,
                            format!(
                                "unknown method `{source_name}` for trait `{trait_name}` in executable build"
                            ),
                        ));
                        return None;
                    };

                    if method.mutable_self && !receiver_has_mutable_capability {
                        let qualified_name = format!("{trait_name}.{source_name}");
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

                    if args.len() != method.params.len() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "method `{trait_name}.{source_name}` expects {} arguments, got {}",
                                method.params.len(),
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let mut lowered_args = Vec::new();
                    for (arg, param) in args.iter().zip(&method.params) {
                        if let Some(arg) = lower_expr(
                            arg,
                            locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(param.type_.clone()),
                            "expected supported dynamic method argument in executable builds",
                        ) {
                            lowered_args.push(arg);
                        }
                    }

                    if lowered_args.len() != args.len() {
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: method.return_type.clone(),
                        kind: LoweredExprKind::DynamicCall {
                            object: Box::new(object),
                            method: source_name.to_string(),
                            args: lowered_args,
                        },
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
                        traits,
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
                let try_indirect = !matches!(&callee.kind, ExprKind::Identifier(name) if signatures.contains_key(name));
                if try_indirect
                    && let Some(callee) = lower_expr(
                        callee,
                        locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        None,
                        "expected supported function value in executable builds",
                    )
                    && let LoweredType::Function {
                        params,
                        return_type,
                    } = &callee.type_
                {
                    if args.len() != params.len() {
                        diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "function value expects {} arguments, got {}",
                                params.len(),
                                args.len()
                            ),
                        ));
                        return None;
                    }

                    let mut lowered_args = Vec::new();
                    for (arg, param) in args.iter().zip(params) {
                        if let Some(arg) = lower_expr(
                            arg,
                            locals,
                            signatures,
                            structs,
                            enums,
                            traits,
                            diagnostics,
                            Some(param.type_.clone()),
                            "expected supported function value argument in executable builds",
                        ) {
                            lowered_args.push(arg);
                        }
                    }
                    if lowered_args.len() != args.len() {
                        return None;
                    }

                    return Some(LoweredExpr {
                        type_: *return_type.clone(),
                        kind: LoweredExprKind::IndirectCall {
                            callee: Box::new(callee),
                            args: lowered_args,
                        },
                    });
                }

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
                        traits,
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
        ExprKind::Lambda(function) => lower_lambda_expr(
            expr.span,
            function,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            expected_type.clone(),
        )?,
        ExprKind::Match { value, branches } => {
            let value_mutable = expression_has_mutable_capability(value, locals);
            let value = lower_expr(
                value,
                locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                None,
                "expected supported match value in executable builds",
            )?;
            if !matches!(
                value.type_,
                LoweredType::Enum(_)
                    | LoweredType::Basic(BasicType::String)
                    | LoweredType::Basic(BasicType::I32)
            ) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    "match expressions require an enum, `string`, or `i32` value in executable builds",
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
                    value_mutable,
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
                            traits,
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
                        traits,
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
        if let LoweredType::Trait(trait_name) = &expected_type
            && lowered.type_ != expected_type
            && lowered_type_implements_trait(traits, trait_name, &lowered.type_)
        {
            if !matches!(lowered.type_, LoweredType::Struct(_) | LoweredType::Enum(_)) {
                diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "only struct and enum values can be coerced to trait `{trait_name}` in executable builds"
                    ),
                ));
                return None;
            }

            lowered = LoweredExpr {
                type_: expected_type.clone(),
                kind: LoweredExprKind::TraitObject {
                    trait_name: trait_name.clone(),
                    self_type: lowered.type_.clone(),
                    value: Box::new(lowered),
                },
            };
        }

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

fn lower_collection_literal(
    span: Span,
    items: &[Expr],
    collection: &TypeRef,
    locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredExpr> {
    let collection = lower_value_type_ref(
        collection,
        structs,
        enums,
        traits,
        diagnostics,
        "collection literals require a concrete collection type in executable builds",
    )?;
    if !matches!(collection, LoweredType::Struct(_)) {
        diagnostics.push(Diagnostic::error(
            span,
            "collection literals require a struct type that implements `FromElements<T>`",
        ));
        return None;
    }

    let Some((element, constructor, add)) = traits.values().find_map(|trait_| {
        let source_name = trait_.name.rsplit("::").next().unwrap_or(&trait_.name);
        if !source_name.starts_with("FromElements<") {
            return None;
        }
        let element = trait_
            .methods
            .iter()
            .find(|method| method.name == "add" && method.mutable_self)
            .and_then(|method| method.params.first())
            .map(|param| param.type_.clone())?;
        let constructor = qualified_static_trait_method_name(
            &trait_.name,
            &collection.name(),
            "withElementCapacity",
        );
        let add = qualified_trait_method_name(&trait_.name, &collection.name(), "add");
        (signatures.contains_key(&constructor) && signatures.contains_key(&add)).then_some((
            element,
            constructor,
            add,
        ))
    }) else {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "collection type `{}` must implement `FromElements<T>` to use a collection literal",
                collection.name()
            ),
        ));
        return None;
    };

    let mut lowered_items = Vec::new();
    for item in items {
        lowered_items.push(lower_expr(
            item,
            locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
            Some(element.clone()),
            "collection literal elements must match the collection element type",
        )?);
    }

    Some(LoweredExpr {
        type_: collection,
        kind: LoweredExprKind::CollectionLiteral {
            constructor,
            add,
            items: lowered_items,
        },
    })
}

fn lowered_type_implements_trait(
    traits: &HashMap<String, LoweredTrait>,
    trait_name: &str,
    type_: &LoweredType,
) -> bool {
    traits
        .get(trait_name)
        .is_some_and(|trait_| trait_.impls.iter().any(|impl_| impl_.self_type == *type_))
}

fn lower_lambda_expr(
    span: Span,
    function: &FunctionDecl,
    outer_locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
    expected_type: Option<LoweredType>,
) -> Option<LoweredExpr> {
    let function_type = expected_type.or_else(|| {
        infer_lambda_function_type(
            span,
            function,
            outer_locals,
            signatures,
            structs,
            enums,
            traits,
            diagnostics,
        )
    })?;
    let LoweredType::Function {
        params: function_params,
        return_type,
    } = &function_type
    else {
        diagnostics.push(Diagnostic::error(
            span,
            "lambda expressions require a function type context in executable builds",
        ));
        return None;
    };
    let function_params = function_params.clone();
    let return_type = return_type.clone();

    if function.params.len() != function_params.len() {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "lambda expects {} parameters from context, got {}",
                function_params.len(),
                function.params.len()
            ),
        ));
        return None;
    }

    let name = CLOSURE_LOWERING.with(|state| {
        let mut state = state.borrow_mut();
        let name = format!("lambda{}", state.next_closure_id);
        state.next_closure_id += 1;
        name
    });

    let captures = outer_locals
        .iter()
        .filter(|(_, local)| local.captured)
        .map(|(name, local)| LoweredClosureCapture {
            name: name.clone(),
            type_: local.type_.clone(),
        })
        .collect::<Vec<_>>();

    let mut locals = HashMap::new();
    for capture in &captures {
        locals.insert(
            capture.name.clone(),
            LoweringLocal {
                type_: capture.type_.clone(),
                mutable: true,
                replacement: Some(LoweredExpr {
                    type_: capture.type_.clone(),
                    kind: LoweredExprKind::CapturedLocal {
                        env_name: "env".to_string(),
                        name: capture.name.clone(),
                    },
                }),
                captured: false,
            },
        );
    }

    let mut params = Vec::new();
    for (param, param_type) in function.params.iter().zip(function_params) {
        locals.insert(
            param.name.clone(),
            LoweringLocal {
                type_: param_type.type_.clone(),
                mutable: param.mutable,
                replacement: None,
                captured: false,
            },
        );
        params.push(LoweredParam {
            name: param.name.clone(),
            type_: param_type.type_.clone(),
        });
    }

    let mut statements = Vec::new();
    let mut return_value = None;
    match &function.body {
        FunctionBody::Expr(expr) => {
            return_value = lower_expr(
                expr,
                &locals,
                signatures,
                structs,
                enums,
                traits,
                diagnostics,
                Some(*return_type.clone()),
                "expected supported lambda return value in executable builds",
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
                            traits,
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
                            traits,
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
                                traits,
                                diagnostics,
                                Some(*return_type.clone()),
                                "expected supported lambda return value in executable builds",
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
                            traits,
                            diagnostics,
                            Some(return_type.as_ref()),
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
                            traits,
                            diagnostics,
                            Some(return_type.as_ref()),
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
                            traits,
                            diagnostics,
                            Some(return_type.as_ref()),
                        ) {
                            statements.push(statement);
                        }
                    }
                    StmtKind::Break => statements.push(LoweredStatement::Break),
                    StmtKind::Continue => statements.push(LoweredStatement::Continue),
                    StmtKind::For { .. } => statements.extend(lower_for_statement(
                        statement,
                        &locals,
                        signatures,
                        structs,
                        enums,
                        traits,
                        diagnostics,
                        Some(return_type.as_ref()),
                    )),
                }
            }
        }
    }

    let return_value = return_value.unwrap_or_else(void_expr);
    CLOSURE_LOWERING.with(|state| {
        state
            .borrow_mut()
            .closure_functions
            .push(LoweredClosureFunction {
                name: name.clone(),
                captures: captures.clone(),
                params,
                return_type: *return_type.clone(),
                statements,
                return_value,
            });
    });

    Some(LoweredExpr {
        type_: function_type,
        kind: LoweredExprKind::Closure { name, captures },
    })
}

fn infer_lambda_function_type(
    span: Span,
    function: &FunctionDecl,
    outer_locals: &HashMap<String, LoweringLocal>,
    signatures: &HashMap<String, FunctionSignature>,
    structs: &HashMap<String, LoweredStruct>,
    enums: &HashMap<String, LoweredEnum>,
    traits: &HashMap<String, LoweredTrait>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LoweredType> {
    let self_type = outer_locals.get("Self").map(|local| &local.type_);
    let mut locals = outer_locals
        .iter()
        .map(|(name, local)| (name.clone(), local.type_.clone()))
        .collect::<HashMap<_, _>>();
    let mut params = Vec::new();

    for param in &function.params {
        let Some(type_ref) = &param.type_ref else {
            diagnostics.push(Diagnostic::error(
                param.span,
                "lambda parameters must include type annotations when no function type context is available",
            ));
            return None;
        };
        let type_ = lower_value_type_ref_in_context(
            type_ref,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "lambda parameter types must be supported in executable builds",
        )?;
        locals.insert(param.name.clone(), type_.clone());
        params.push(LoweredFunctionTypeParam {
            type_,
            mutable: param.mutable,
        });
    }

    let return_type = if let Some(type_ref) = &function.return_type {
        lower_value_type_ref_in_context(
            type_ref,
            self_type,
            structs,
            enums,
            traits,
            diagnostics,
            "lambda return types must be supported in executable builds",
        )?
    } else {
        match &function.body {
            FunctionBody::Expr(expr) => {
                let Some(return_type) =
                    infer_expr_type(expr, &locals, signatures, structs, enums, traits)
                else {
                    diagnostics.push(Diagnostic::error(
                        span,
                        "could not infer lambda return type; add a function type annotation",
                    ));
                    return None;
                };
                return_type
            }
            FunctionBody::Block(block) => {
                let mut return_type = None;
                let mut has_unresolved_value_return = false;
                if let Err(conflict) = infer_block_return_types(
                    block,
                    &mut locals,
                    signatures,
                    structs,
                    enums,
                    traits,
                    &mut return_type,
                    &mut has_unresolved_value_return,
                ) {
                    diagnostics.push(Diagnostic::error(
                        conflict.span,
                        format!(
                            "lambda has multiple return types (`{}` and `{}`); inferred return types must be consistent",
                            conflict.first.name(),
                            conflict.second.name()
                        ),
                    ));
                    return None;
                }
                if has_unresolved_value_return {
                    diagnostics.push(Diagnostic::error(
                        span,
                        "could not infer lambda return type; add a function type annotation",
                    ));
                    return None;
                }
                return_type.unwrap_or(LoweredType::Void)
            }
        }
    };

    Some(LoweredType::Function {
        params,
        return_type: Box::new(return_type),
    })
}

fn lower_function_value_expr(
    name: &str,
    signatures: &HashMap<String, FunctionSignature>,
    expected_type: Option<LoweredType>,
    diagnostics: &mut Vec<Diagnostic>,
    span: Span,
) -> Option<LoweredExpr> {
    let Some(signature) = signatures.get(name) else {
        return None;
    };
    let function_type = LoweredType::Function {
        params: signature
            .params
            .iter()
            .map(|param| LoweredFunctionTypeParam {
                type_: param.type_.clone(),
                mutable: param.mutable,
            })
            .collect(),
        return_type: Box::new(signature.return_type.clone()),
    };

    if let Some(expected_type) = expected_type
        && expected_type != function_type
    {
        diagnostics.push(Diagnostic::error(
            span,
            format!(
                "expected value of type `{}`, got `{}`",
                expected_type.name(),
                function_type.name()
            ),
        ));
        return None;
    }

    let wrapper_name = CLOSURE_LOWERING.with(|state| {
        let mut state = state.borrow_mut();
        let wrapper_name = format!("functionValue{}_{}", state.next_closure_id, name);
        state.next_closure_id += 1;

        let params = signature
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| LoweredParam {
                name: format!("arg{index}"),
                type_: param.type_.clone(),
            })
            .collect::<Vec<_>>();
        let args = params
            .iter()
            .map(|param| LoweredExpr {
                type_: param.type_.clone(),
                kind: LoweredExprKind::Local(param.name.clone()),
            })
            .collect::<Vec<_>>();

        state.closure_functions.push(LoweredClosureFunction {
            name: wrapper_name.clone(),
            captures: Vec::new(),
            params,
            return_type: signature.return_type.clone(),
            statements: Vec::new(),
            return_value: LoweredExpr {
                type_: signature.return_type.clone(),
                kind: LoweredExprKind::Call {
                    name: name.to_string(),
                    args,
                },
            },
        });

        wrapper_name
    });

    Some(LoweredExpr {
        type_: function_type,
        kind: LoweredExprKind::Closure {
            name: wrapper_name,
            captures: Vec::new(),
        },
    })
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
    value_mutable: bool,
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
                binding_mutable,
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
                if *binding_mutable && !value_mutable {
                    diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "cannot bind mutable payload `{binding}` from an immutable match value in executable build"
                        ),
                    ));
                    return None;
                }
                locals.insert(
                    binding.clone(),
                    LoweringLocal {
                        type_: payload_type.clone(),
                        mutable: *binding_mutable,
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
                        captured: false,
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
        (Pattern::Number { value, span }, LoweredType::Basic(BasicType::I32)) => {
            if number_literal_is_float(value) {
                diagnostics.push(Diagnostic::error(
                    *span,
                    "numeric match patterns for `i32` require integer literals in executable builds",
                ));
                return None;
            }
            Some(LoweredPattern::Number(value.clone()))
        }
        (
            Pattern::Range {
                start,
                end,
                inclusive,
                span,
            },
            LoweredType::Basic(BasicType::I32),
        ) => {
            if number_literal_is_float(start) || number_literal_is_float(end) {
                diagnostics.push(Diagnostic::error(
                    *span,
                    "numeric range patterns for `i32` require integer literal bounds in executable builds",
                ));
                return None;
            }
            Some(LoweredPattern::Range {
                start: start.clone(),
                end: end.clone(),
                inclusive: *inclusive,
            })
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
