use std::collections::{HashMap, HashSet};

use crate::ast::{
    BasicType, BinaryOp, Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, ImplDecl,
    Item, MatchBranchBody, Pattern, Program, Stmt, StmtKind, StructDecl, StructInitField,
    StructMember, TraitDecl, TraitMethodDecl, TypeRef, UnaryOp, number_literal_is_float,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let program = match crate::monomorphize::monomorphize(program) {
        Ok(program) => program,
        Err(diagnostics) => return diagnostics,
    };
    let mut analyzer = Analyzer::new();
    analyzer.collect_top_level(&program);
    analyzer.validate_program(&program);
    analyzer.diagnostics
}

fn extension_name(type_name: &str, function_name: &str) -> String {
    format!("extension {type_name}.{function_name}")
}

fn trait_method_name(type_name: &str, function_name: &str) -> String {
    format!("trait {type_name}.{function_name}")
}

fn static_trait_method_name(type_name: &str, function_name: &str) -> String {
    format!("static trait {type_name}.{function_name}")
}

fn source_callable_name(name: &str) -> &str {
    name.rsplit_once("::").map_or(name, |(_, name)| name)
}

struct Analyzer {
    diagnostics: Vec<Diagnostic>,
    values: HashSet<String>,
    types: HashSet<String>,
    structs: HashMap<String, StructDefinition>,
    enums: HashMap<String, EnumDefinition>,
    traits: HashMap<String, TraitDefinition>,
    functions: HashMap<String, FunctionSignature>,
    extensions: HashMap<String, FunctionSignature>,
    static_extensions: HashMap<String, FunctionSignature>,
    trait_methods: HashMap<String, FunctionSignature>,
    static_trait_methods: HashMap<String, FunctionSignature>,
    trait_impls: HashSet<(String, String)>,
    imported_namespaces: HashSet<String>,
    unsupported_features: HashSet<&'static str>,
    scopes: Vec<HashMap<String, Binding>>,
    return_types: Vec<Type>,
    self_types: Vec<Type>,
    loop_depth: usize,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<ParamSignature>,
    return_type: Type,
    mutable_self: bool,
}

#[derive(Debug, Clone)]
struct ParamSignature {
    type_: Type,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct StructDefinition {
    fields: HashMap<String, Type>,
    methods: HashMap<String, FunctionSignature>,
    static_methods: HashMap<String, FunctionSignature>,
}

#[derive(Debug, Clone)]
struct EnumDefinition {
    variants: HashMap<String, Option<Type>>,
}

#[derive(Debug, Clone)]
struct TraitDefinition {
    methods: HashMap<String, FunctionSignature>,
    static_methods: HashMap<String, FunctionSignature>,
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    type_: Type,
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Type {
    Basic(BasicType),
    Struct(String),
    Enum(String),
    Function {
        params: Vec<FunctionTypeParam>,
        return_type: Box<Type>,
    },
    Void,
    Named(String),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionTypeParam {
    type_: Type,
    mutable: bool,
}

impl Type {
    fn name(&self) -> String {
        match self {
            Type::Basic(type_) => type_.name().to_string(),
            Type::Struct(name) => name.clone(),
            Type::Enum(name) => name.clone(),
            Type::Function {
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
            Type::Void => "void".to_string(),
            Type::Named(name) => name.clone(),
            Type::Unknown => "unknown".to_string(),
        }
    }
}

impl Analyzer {
    fn new() -> Self {
        let values = HashSet::from(["io".to_string()]);
        let types = HashSet::from(["void".to_string(), "ArrayList".to_string()]);

        Self {
            diagnostics: Vec::new(),
            values,
            types,
            structs: HashMap::new(),
            enums: HashMap::new(),
            traits: HashMap::new(),
            functions: HashMap::new(),
            extensions: HashMap::new(),
            static_extensions: HashMap::new(),
            trait_methods: HashMap::new(),
            static_trait_methods: HashMap::new(),
            trait_impls: HashSet::new(),
            imported_namespaces: HashSet::new(),
            unsupported_features: HashSet::new(),
            scopes: Vec::new(),
            return_types: Vec::new(),
            self_types: Vec::new(),
            loop_depth: 0,
        }
    }

    fn collect_top_level(&mut self, program: &Program) {
        let mut names: HashMap<String, Span> = HashMap::new();
        let mut main_count = 0;

        for item in &program.items {
            match item {
                Item::Import(item) => {
                    if let Some(namespace) = &item.namespace {
                        self.values.insert(namespace.name.clone());
                        self.imported_namespaces.insert(namespace.name.clone());
                        self.insert_top_level(&mut names, &namespace.name, namespace.span);
                    }
                    for import in &item.names {
                        let name = import.alias.as_ref().unwrap_or(&import.name);
                        self.values.insert(name.clone());
                        self.types.insert(name.clone());
                        self.insert_top_level(&mut names, name, import.span);
                    }
                }
                Item::Enum(item) => {
                    self.types.insert(item.name.clone());
                    self.enums.insert(
                        item.name.clone(),
                        EnumDefinition {
                            variants: HashMap::new(),
                        },
                    );
                    self.insert_top_level(&mut names, &item.name, item.span);
                }
                Item::Struct(item) => {
                    self.values.insert(item.name.clone());
                    self.types.insert(item.name.clone());
                    self.structs.insert(
                        item.name.clone(),
                        StructDefinition {
                            fields: HashMap::new(),
                            methods: HashMap::new(),
                            static_methods: HashMap::new(),
                        },
                    );
                    self.insert_top_level(&mut names, &item.name, item.span);
                }
                Item::Trait(item) => {
                    self.types.insert(item.name.clone());
                    self.traits.insert(
                        item.name.clone(),
                        TraitDefinition {
                            methods: HashMap::new(),
                            static_methods: HashMap::new(),
                        },
                    );
                    self.insert_top_level(&mut names, &item.name, item.span);
                }
                Item::Function(item) => {
                    if let Some(name) = &item.name {
                        if name == "main" {
                            main_count += 1;
                        }

                        self.values.insert(name.clone());
                        self.insert_top_level(&mut names, name, item.span);
                    }
                }
                Item::Impl(_) => {}
                Item::Extension(_) => {}
            }
        }

        for item in &program.items {
            match item {
                Item::Struct(item) => self.collect_struct_definition(item),
                Item::Enum(item) => self.collect_enum_definition(item),
                Item::Trait(item) => self.collect_trait_definition(item),
                Item::Function(item) => {
                    if let Some(name) = &item.name {
                        self.functions.insert(
                            name.clone(),
                            FunctionSignature {
                                params: item
                                    .params
                                    .iter()
                                    .map(|param| ParamSignature {
                                        type_: self
                                            .type_ref_without_diagnostics(param.type_ref.as_ref()),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_without_diagnostics(item.return_type.as_ref()),
                                mutable_self: false,
                            },
                        );
                    }
                }
                Item::Impl(_) => {}
                Item::Extension(item) => self.collect_extension_definition(item),
                Item::Import(_) => {}
            }
        }

        for item in &program.items {
            if let Item::Impl(item) = item {
                self.collect_impl_definition(item);
            }
        }

        if main_count == 0 {
            let span = program
                .items
                .first()
                .map_or_else(|| Span::new(0, 0), Item::span);
            self.diagnostics
                .push(Diagnostic::error(span, "missing `main` function"));
        } else if main_count > 1 {
            self.diagnostics.push(Diagnostic::error(
                Span::new(0, 0),
                "expected exactly one `main` function",
            ));
        }
    }

    fn collect_struct_definition(&mut self, item: &StructDecl) {
        let mut fields = HashMap::new();
        let mut methods = HashMap::new();
        let mut static_methods = HashMap::new();
        let self_type = Type::Struct(item.name.clone());

        for member in &item.members {
            match member {
                StructMember::Field(field) => {
                    if fields
                        .insert(
                            field.name.clone(),
                            self.type_ref_without_diagnostics(Some(&field.type_ref)),
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            field.span,
                            format!("duplicate field `{}` in struct `{}`", field.name, item.name),
                        ));
                    }
                }
                StructMember::Method(method) => {
                    let Some(name) = &method.name else {
                        continue;
                    };

                    if name == "clone" {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            "method name `clone` is reserved for the built-in deep clone operation",
                        ));
                    }

                    if methods
                        .insert(
                            name.clone(),
                            FunctionSignature {
                                params: method
                                    .params
                                    .iter()
                                    .filter(|param| !is_self_param(param))
                                    .map(|param| ParamSignature {
                                        type_: self.type_ref_in_context(
                                            param.type_ref.as_ref(),
                                            &self_type,
                                        ),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_in_context(method.return_type.as_ref(), &self_type),
                                mutable_self: has_mutable_receiver(method),
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            format!("duplicate method `{name}` in struct `{}`", item.name),
                        ));
                    }
                }
                StructMember::StaticMethod(method) => {
                    let Some(name) = &method.name else {
                        continue;
                    };

                    if name == "clone" {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            "static function name `clone` is reserved for the built-in deep clone operation",
                        ));
                    }

                    if static_methods
                        .insert(
                            name.clone(),
                            FunctionSignature {
                                params: method
                                    .params
                                    .iter()
                                    .map(|param| ParamSignature {
                                        type_: self.type_ref_in_context(
                                            param.type_ref.as_ref(),
                                            &self_type,
                                        ),
                                        mutable: param.mutable,
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_in_context(method.return_type.as_ref(), &self_type),
                                mutable_self: false,
                            },
                        )
                        .is_some()
                    {
                        self.diagnostics.push(Diagnostic::error(
                            method.span,
                            format!(
                                "duplicate static function `{name}` in struct `{}`",
                                item.name
                            ),
                        ));
                    }
                }
            }
        }

        self.structs.insert(
            item.name.clone(),
            StructDefinition {
                fields,
                methods,
                static_methods,
            },
        );
    }

    fn collect_enum_definition(&mut self, item: &crate::ast::EnumDecl) {
        let mut variants = HashMap::new();

        for variant in &item.variants {
            let payload = variant
                .payload
                .as_ref()
                .map(|type_ref| self.type_ref_without_diagnostics(Some(type_ref)));

            if variants
                .insert(variant.name.clone(), payload.clone())
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    variant.span,
                    format!(
                        "duplicate variant `{}` in enum `{}`",
                        variant.name, item.name
                    ),
                ));
            }
        }

        self.enums
            .insert(item.name.clone(), EnumDefinition { variants });
    }

    fn collect_trait_definition(&mut self, item: &TraitDecl) {
        let mut methods = HashMap::new();
        let mut static_methods = HashMap::new();

        for method in &item.methods {
            if method.name == "clone" {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    "trait method name `clone` is reserved for the built-in deep clone operation",
                ));
            }

            if method.return_type.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "trait method `{}.{}` must include a return type",
                        item.name, method.name
                    ),
                ));
            }

            let target_methods = if method.static_ {
                &mut static_methods
            } else {
                &mut methods
            };

            if target_methods
                .insert(
                    method.name.clone(),
                    self.trait_method_signature(method, method.static_),
                )
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate {}method `{}` in trait `{}`",
                        if method.static_ { "static " } else { "" },
                        method.name,
                        item.name
                    ),
                ));
            }
        }

        self.traits.insert(
            item.name.clone(),
            TraitDefinition {
                methods,
                static_methods,
            },
        );
    }

    fn collect_impl_definition(&mut self, item: &ImplDecl) {
        let trait_name = item.trait_ref.name.clone();
        let self_type = self.type_ref_without_diagnostics(Some(&item.type_ref));
        let self_type_name = self_type.name();

        let Some(trait_) = self.traits.get(&trait_name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                item.trait_ref.span,
                format!("unknown trait `{trait_name}`"),
            ));
            return;
        };

        if !item.trait_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                item.trait_ref.span,
                "generic traits are not implemented yet",
            ));
        }

        if matches!(
            self_type,
            Type::Unknown | Type::Void | Type::Function { .. } | Type::Named(_)
        ) {
            return;
        }

        if !self
            .trait_impls
            .insert((trait_name.clone(), self_type_name.clone()))
        {
            self.diagnostics.push(Diagnostic::error(
                item.span,
                format!("duplicate impl of trait `{trait_name}` for type `{self_type_name}`"),
            ));
        }

        let mut impl_methods = HashMap::new();
        let mut static_impl_methods = HashMap::new();
        for member in &item.methods {
            let method = &member.function;
            let Some(name) = &method.name else {
                continue;
            };
            if name == "clone" {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    "trait impl method name `clone` is reserved for the built-in deep clone operation",
                ));
            }

            let expected = if member.static_ {
                trait_.static_methods.get(name)
            } else {
                trait_.methods.get(name)
            }
            .map(|signature| signature_with_self_type(signature, &self_type));

            let signature = FunctionSignature {
                params: method
                    .params
                    .iter()
                    .filter(|param| member.static_ || !is_self_param(param))
                    .map(|param| ParamSignature {
                        type_: self.type_ref_in_context(param.type_ref.as_ref(), &self_type),
                        mutable: param.mutable,
                    })
                    .collect(),
                return_type: method.return_type.as_ref().map_or_else(
                    || {
                        expected
                            .as_ref()
                            .map_or(Type::Unknown, |signature| signature.return_type.clone())
                    },
                    |return_type| self.type_ref_in_context(Some(return_type), &self_type),
                ),
                mutable_self: !member.static_ && has_mutable_receiver(method),
            };

            let target_impl_methods = if member.static_ {
                &mut static_impl_methods
            } else {
                &mut impl_methods
            };
            if target_impl_methods
                .insert(name.clone(), signature.clone())
                .is_some()
            {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate {}method `{name}` in impl of trait `{trait_name}` for type `{self_type_name}`",
                        if member.static_ { "static " } else { "" },
                    ),
                ));
            }

            let Some(expected) = expected else {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "{}method `{name}` is not declared in trait `{trait_name}`",
                        if member.static_ { "static " } else { "" },
                    ),
                ));
                continue;
            };

            if !signatures_match(&expected, &signature) {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "method `{name}` does not match trait `{trait_name}` for type `{self_type_name}`"
                    ),
                ));
            }

            let trait_method_name = if member.static_ {
                static_trait_method_name(&self_type_name, name)
            } else {
                trait_method_name(&self_type_name, name)
            };
            let trait_methods = if member.static_ {
                &mut self.static_trait_methods
            } else {
                &mut self.trait_methods
            };
            if trait_methods.insert(trait_method_name, signature).is_some() {
                self.diagnostics.push(Diagnostic::error(
                    method.span,
                    format!(
                        "duplicate {}trait method `{name}` for type `{self_type_name}`",
                        if member.static_ { "static " } else { "" },
                    ),
                ));
            }
        }

        for name in trait_.methods.keys() {
            if !impl_methods.contains_key(name) {
                self.diagnostics.push(Diagnostic::error(
                    item.span,
                    format!(
                        "impl of trait `{trait_name}` for type `{self_type_name}` is missing method `{name}`"
                    ),
                ));
            }
        }
        for name in trait_.static_methods.keys() {
            if !static_impl_methods.contains_key(name) {
                self.diagnostics.push(Diagnostic::error(
                    item.span,
                    format!(
                        "impl of trait `{trait_name}` for type `{self_type_name}` is missing static method `{name}`"
                    ),
                ));
            }
        }
    }

    fn trait_method_signature(&self, method: &TraitMethodDecl, static_: bool) -> FunctionSignature {
        FunctionSignature {
            params: method
                .params
                .iter()
                .filter(|param| static_ || !is_self_param(param))
                .map(|param| ParamSignature {
                    type_: self.trait_type_ref_without_diagnostics(param.type_ref.as_ref()),
                    mutable: param.mutable,
                })
                .collect(),
            return_type: self.trait_type_ref_without_diagnostics(method.return_type.as_ref()),
            mutable_self: !static_
                && method
                    .params
                    .iter()
                    .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none()),
        }
    }

    fn trait_type_ref_without_diagnostics(&self, type_ref: Option<&TypeRef>) -> Type {
        if let Some(type_ref) = type_ref
            && type_ref.name == "Self"
            && type_ref.args.is_empty()
        {
            Type::Named("Self".to_string())
        } else {
            self.type_ref_without_diagnostics(type_ref)
        }
    }

    fn collect_extension_definition(&mut self, item: &crate::ast::ExtensionDecl) {
        let Some(name) = &item.function.name else {
            return;
        };
        let self_type = self.type_ref_without_diagnostics(Some(&item.type_ref));

        if name == "clone" {
            self.diagnostics.push(Diagnostic::error(
                item.function.span,
                "extension function name `clone` is reserved for the built-in deep clone operation",
            ));
        }

        let signature = FunctionSignature {
            params: item
                .function
                .params
                .iter()
                .filter(|param| item.static_ || !is_self_param(param))
                .map(|param| ParamSignature {
                    type_: self.type_ref_in_context(param.type_ref.as_ref(), &self_type),
                    mutable: param.mutable,
                })
                .collect(),
            return_type: self.type_ref_in_context(item.function.return_type.as_ref(), &self_type),
            mutable_self: !item.static_ && has_mutable_receiver(&item.function),
        };
        let extensions = if item.static_ {
            &mut self.static_extensions
        } else {
            &mut self.extensions
        };

        if extensions
            .insert(extension_name(&self_type.name(), name), signature)
            .is_some()
        {
            self.diagnostics.push(Diagnostic::error(
                item.function.span,
                format!(
                    "duplicate {}extension function `{}` for type `{}`",
                    if item.static_ { "static " } else { "" },
                    name,
                    item.type_ref.name
                ),
            ));
        }
    }

    fn insert_top_level(&mut self, names: &mut HashMap<String, Span>, name: &str, span: Span) {
        if let Some(previous_span) = names.insert(name.to_string(), span) {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("duplicate top-level name `{name}`"),
            ));
            self.diagnostics.push(Diagnostic::error(
                previous_span,
                format!("first definition of `{name}` is here"),
            ));
        }
    }

    fn validate_program(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                Item::Import(item) => self.unsupported(
                    item.span,
                    "imports are parsed but module resolution is not implemented yet",
                ),
                Item::Enum(item) => {
                    if item.variants.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            item.span,
                            format!("enum `{}` must define at least one variant", item.name),
                        ));
                    }

                    for variant in &item.variants {
                        if let Some(type_ref) = &variant.payload {
                            self.validate_type(type_ref);
                        }
                    }
                }
                Item::Struct(item) => {
                    for member in &item.members {
                        match member {
                            StructMember::Field(field) => {
                                self.validate_type(&field.type_ref);
                            }
                            StructMember::Method(method) => {
                                self.validate_function(
                                    method,
                                    Some(Type::Struct(item.name.clone())),
                                    true,
                                );
                            }
                            StructMember::StaticMethod(method) => self.validate_function(
                                method,
                                Some(Type::Struct(item.name.clone())),
                                false,
                            ),
                        }
                    }
                }
                Item::Trait(item) => self.validate_trait(item),
                Item::Impl(item) => self.validate_impl(item),
                Item::Function(function) => self.validate_function(function, None, false),
                Item::Extension(item) => {
                    let self_type = self.validate_type(&item.type_ref);
                    self.validate_function(&item.function, Some(self_type), !item.static_);
                }
            }
        }
    }

    fn validate_trait(&mut self, item: &TraitDecl) {
        let mut names = HashSet::new();
        for method in &item.methods {
            if !names.insert((method.static_, method.name.clone())) {
                continue;
            }

            let self_params = method
                .params
                .iter()
                .filter(|param| is_self_param(param))
                .collect::<Vec<_>>();
            if self_params.len() > 1 {
                self.diagnostics.push(Diagnostic::error(
                    self_params[1].span,
                    "a trait method can declare only one `self` receiver",
                ));
            }
            if let Some(param) = self_params.first() {
                if method.static_ {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "`self` receivers are only allowed on instance trait methods",
                    ));
                } else if !param.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "immutable `self` is implicit; remove it from the parameter list",
                    ));
                }
                if param.type_ref.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "mutable receivers must be written `mut self` without a type annotation",
                    ));
                }
            }

            self.self_types.push(Type::Named("Self".to_string()));
            for param in &method.params {
                if is_self_param(param) {
                    continue;
                }
                if let Some(type_ref) = &param.type_ref {
                    self.validate_type(type_ref);
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        format!(
                            "trait method `{}.{}` parameters must include type annotations",
                            item.name, method.name
                        ),
                    ));
                }
            }
            if let Some(return_type) = &method.return_type {
                self.validate_type(return_type);
            }
            self.self_types.pop();
        }
    }

    fn validate_impl(&mut self, item: &ImplDecl) {
        if !self.traits.contains_key(&item.trait_ref.name) {
            self.diagnostics.push(Diagnostic::error(
                item.trait_ref.span,
                format!("unknown trait `{}`", item.trait_ref.name),
            ));
        }
        if !item.trait_ref.args.is_empty() {
            self.unsupported(
                item.trait_ref.span,
                "generic traits are not implemented yet",
            );
        }

        let self_type = self.validate_type(&item.type_ref);
        for member in &item.methods {
            let method = &member.function;
            let expected_return_type = method.name.as_ref().and_then(|name| {
                self.traits
                    .get(&item.trait_ref.name)
                    .and_then(|trait_| {
                        if member.static_ {
                            trait_.static_methods.get(name)
                        } else {
                            trait_.methods.get(name)
                        }
                    })
                    .map(|signature| signature_with_self_type(signature, &self_type).return_type)
            });
            self.validate_function_with_return_type(
                method,
                Some(self_type.clone()),
                !member.static_,
                expected_return_type,
            );
        }
    }

    fn validate_function(
        &mut self,
        function: &FunctionDecl,
        self_type: Option<Type>,
        has_self: bool,
    ) {
        self.validate_function_with_return_type(function, self_type, has_self, None);
    }

    fn validate_function_with_return_type(
        &mut self,
        function: &FunctionDecl,
        self_type: Option<Type>,
        has_self: bool,
        inferred_return_type: Option<Type>,
    ) {
        self.push_scope();

        if let Some(self_type) = self_type.clone() {
            self.self_types.push(self_type.clone());
            if has_self {
                self.define("self", has_mutable_receiver(function), self_type.clone());
            }
        }

        let self_params = function
            .params
            .iter()
            .filter(|param| is_self_param(param))
            .collect::<Vec<_>>();
        if has_self {
            if self_params.len() > 1 {
                self.diagnostics.push(Diagnostic::error(
                    self_params[1].span,
                    "a function can declare only one `self` receiver",
                ));
            }
            if let Some(param) = self_params.first() {
                if !param.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "immutable `self` is implicit; remove it from the parameter list",
                    ));
                }
                if param.type_ref.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "mutable receivers must be written `mut self` without a type annotation",
                    ));
                }
            }
        } else if let Some(param) = self_params.first() {
            self.diagnostics.push(Diagnostic::error(
                param.span,
                "`self` receivers are only allowed on instance methods and extension functions",
            ));
        }

        for param in &function.params {
            if is_self_param(param) {
                continue;
            }
            let type_ = param
                .type_ref
                .as_ref()
                .map_or(Type::Unknown, |type_ref| self.validate_type(type_ref));

            self.define(&param.name, param.mutable, type_);
        }

        let return_type = function.return_type.as_ref().map_or_else(
            || inferred_return_type.unwrap_or(Type::Unknown),
            |type_ref| self.validate_type(type_ref),
        );
        self.return_types.push(return_type.clone());

        match &function.body {
            FunctionBody::Block(block) => self.validate_block(block),
            FunctionBody::Expr(expr) => {
                let value_type = self.validate_expr_with_context(expr, Some(return_type.clone()));
                self.report_type_mismatch(expr.span, return_type.clone(), value_type);
            }
        }

        self.validate_missing_return(function, return_type);
        self.return_types.pop();
        if self_type.is_some() {
            self.self_types.pop();
        }
        self.pop_scope();
    }

    fn validate_block(&mut self, block: &Block) {
        self.push_scope();

        for statement in &block.statements {
            self.validate_statement(statement);
        }

        self.pop_scope();
    }

    fn validate_statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Let {
                name,
                mutable,
                type_annotation,
                value,
            } => {
                let annotated_type = type_annotation
                    .as_ref()
                    .map(|type_ref| self.validate_type(type_ref));
                let value_type = if let Some(value) = value {
                    self.validate_expr_with_context(value, annotated_type.clone())
                } else {
                    if type_annotation.is_none() {
                        self.diagnostics.push(Diagnostic::error(
                            statement.span,
                            "let declarations without values must include a type annotation",
                        ));
                    } else if type_annotation
                        .as_ref()
                        .is_some_and(|type_ref| self.requires_unsupported_default(type_ref))
                    {
                        self.diagnostics.push(Diagnostic::error(
                            statement.span,
                            "default values are only supported for basic types",
                        ));
                    }

                    Type::Unknown
                };

                if let Some(annotated_type) = annotated_type.clone() {
                    self.report_type_mismatch(statement.span, annotated_type, value_type.clone());
                }

                let binding_type = annotated_type.clone().unwrap_or_else(|| value_type.clone());
                if *mutable
                    && value.as_ref().is_some_and(|value| {
                        self.requires_mutable_capability(&binding_type)
                            && !self.expr_has_mutable_capability(value)
                    })
                {
                    self.diagnostics.push(Diagnostic::error(
                        value.as_ref().map_or(statement.span, |value| value.span),
                        format!(
                            "cannot initialize mutable binding `{name}` from an immutable value; use `.clone()` to create an independent mutable object"
                        ),
                    ));
                }

                self.define(name, *mutable, annotated_type.unwrap_or(value_type));
            }
            StmtKind::Assign { target, op, value } => {
                if matches!(target.kind, ExprKind::Member { .. }) {
                    self.validate_member_assignment(statement.span, target, *op, value);
                    return;
                }

                let ExprKind::Identifier(name) = &target.kind else {
                    self.validate_expr(target);
                    self.validate_expr(value);
                    self.diagnostics.push(Diagnostic::error(
                        target.span,
                        "assignment target must be a mutable local binding",
                    ));
                    return;
                };

                let Some(binding) = self.lookup(name) else {
                    self.validate_expr(target);
                    self.validate_expr(value);
                    return;
                };

                if !binding.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        target.span,
                        format!("cannot assign to immutable binding `{name}`"),
                    ));
                }

                if op.is_none()
                    && binding.mutable
                    && self.requires_mutable_capability(&binding.type_)
                    && !self.expr_has_mutable_capability(value)
                {
                    self.diagnostics.push(Diagnostic::error(
                        value.span,
                        format!(
                            "cannot assign an immutable value to mutable binding `{name}`; use `.clone()` to create an independent mutable object"
                        ),
                    ));
                }

                let value_type = self.validate_assignment_value(
                    statement.span,
                    target,
                    *op,
                    value,
                    binding.type_.clone(),
                );
                self.report_type_mismatch(value.span, binding.type_, value_type);
            }
            StmtKind::Return { value } => {
                let expected_type = self.current_return_type();

                if let Some(value) = value {
                    let value_type =
                        self.validate_expr_with_context(value, Some(expected_type.clone()));
                    self.report_type_mismatch(value.span, expected_type, value_type);
                } else if !matches!(expected_type, Type::Unknown | Type::Void) {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "return value required for this function",
                    ));
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition_type =
                    self.validate_expr_with_context(condition, Some(Type::Basic(BasicType::Bool)));
                self.report_type_mismatch(
                    condition.span,
                    Type::Basic(BasicType::Bool),
                    condition_type,
                );
                self.validate_block(then_branch);

                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.validate_block(block),
                        ElseBranch::If(statement) => self.validate_statement(statement),
                    }
                }
            }
            StmtKind::While { condition, body } => {
                let condition_type =
                    self.validate_expr_with_context(condition, Some(Type::Basic(BasicType::Bool)));
                self.report_type_mismatch(
                    condition.span,
                    Type::Basic(BasicType::Bool),
                    condition_type,
                );
                self.loop_depth += 1;
                self.validate_block(body);
                self.loop_depth -= 1;
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "`break` can only be used inside a loop",
                    ));
                }
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "`continue` can only be used inside a loop",
                    ));
                }
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                self.unsupported(
                    statement.span,
                    "for loops are parsed but iteration lowering is not implemented yet",
                );
                self.validate_expr(iterable);
                self.loop_depth += 1;
                self.push_scope();
                self.define(name, false, Type::Unknown);

                for statement in &body.statements {
                    self.validate_statement(statement);
                }

                self.pop_scope();
                self.loop_depth -= 1;
            }
            StmtKind::Expr(expr) => {
                self.validate_expr(expr);
            }
        }
    }

    fn validate_expr(&mut self, expr: &Expr) -> Type {
        self.validate_expr_with_context(expr, None)
    }

    fn validate_expr_with_context(&mut self, expr: &Expr, expected_type: Option<Type>) -> Type {
        match &expr.kind {
            ExprKind::Identifier(name) => {
                if let Some(binding) = self.lookup(name) {
                    binding.type_
                } else if let Some(signature) = self.functions.get(name) {
                    Type::Function {
                        params: signature
                            .params
                            .iter()
                            .map(|param| FunctionTypeParam {
                                type_: param.type_.clone(),
                                mutable: param.mutable,
                            })
                            .collect(),
                        return_type: Box::new(signature.return_type.clone()),
                    }
                } else if self.values.contains(name) {
                    Type::Unknown
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown name `{name}`"),
                    ));
                    Type::Unknown
                }
            }
            ExprKind::Number(value) => match expected_type {
                Some(Type::Basic(type_))
                    if type_.is_numeric()
                        && (!number_literal_is_float(value) || type_.is_float()) =>
                {
                    Type::Basic(type_)
                }
                Some(Type::Unknown) => Type::Unknown,
                Some(Type::Basic(_))
                | Some(Type::Struct(_))
                | Some(Type::Enum(_))
                | Some(Type::Function { .. })
                | Some(Type::Void)
                | Some(Type::Named(_))
                | None => Type::Basic(if number_literal_is_float(value) {
                    BasicType::F64
                } else {
                    BasicType::I32
                }),
            },
            ExprKind::String(_) => Type::Basic(BasicType::String),
            ExprKind::Bool(_) => Type::Basic(BasicType::Bool),
            ExprKind::Missing => Type::Unknown,
            ExprKind::GenericType { .. } => Type::Unknown,
            ExprKind::GenericMember { object, .. } => {
                self.validate_expr(object);
                Type::Unknown
            }
            ExprKind::Array(items) => {
                self.unsupported(
                    expr.span,
                    "array literals are parsed but collection lowering is not implemented yet",
                );

                for item in items {
                    self.validate_expr(item);
                }

                Type::Unknown
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Member { object, name } = &callee.kind
                    && name == "clone"
                {
                    return self.validate_clone(expr.span, object, args);
                }

                if let ExprKind::Identifier(name) = &callee.kind {
                    return self.validate_call(expr, name, args);
                }
                if let ExprKind::Member { object, name } = &callee.kind
                    && let ExprKind::Identifier(enum_name) = &object.kind
                    && self.enums.contains_key(enum_name)
                {
                    return self.validate_variant_call(expr, enum_name, name, args);
                }
                if let ExprKind::Member { object, name } = &callee.kind
                    && let Some(type_) = self.resolve_type_expression(object)
                {
                    return self.validate_static_call(expr, type_, name, args);
                }
                if let ExprKind::Member { object, name } = &callee.kind {
                    return self.validate_method_call(expr, object, name, args);
                }

                let callee_type = self.validate_expr(callee);
                if let Type::Function {
                    params,
                    return_type,
                } = callee_type
                {
                    return self.validate_function_value_call(expr, &params, &return_type, args);
                }

                for arg in args {
                    self.validate_expr(arg);
                }

                Type::Unknown
            }
            ExprKind::Member { object, name } => {
                if let ExprKind::Identifier(enum_name) = &object.kind
                    && self.enums.contains_key(enum_name)
                {
                    self.validate_unit_variant(expr.span, enum_name, name)
                } else {
                    self.validate_member(expr.span, object, name)
                }
            }
            ExprKind::StructInit { name, fields, .. } => {
                self.validate_struct_init(expr.span, name, fields)
            }
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => {
                let operand_type =
                    self.validate_expr_with_context(operand, Some(Type::Basic(BasicType::Bool)));
                self.report_type_mismatch(
                    operand.span,
                    Type::Basic(BasicType::Bool),
                    operand_type.clone(),
                );

                if matches!(operand_type, Type::Unknown) {
                    Type::Unknown
                } else {
                    Type::Basic(BasicType::Bool)
                }
            }
            ExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } => {
                let operand_type = self.validate_expr_with_context(operand, expected_type.clone());

                if matches!(operand_type, Type::Unknown) {
                    Type::Unknown
                } else if matches!(
                    operand_type,
                    Type::Basic(type_) if type_.is_signed_numeric()
                ) {
                    operand_type
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "operator - only supports signed numeric operands, got `{}`",
                            operand_type.name()
                        ),
                    ));
                    Type::Unknown
                }
            }
            ExprKind::Binary {
                left,
                op: BinaryOp::LogicalAnd | BinaryOp::LogicalOr,
                right,
            } => {
                let expected_type = Type::Basic(BasicType::Bool);
                let left_type = self.validate_expr_with_context(left, Some(expected_type.clone()));
                let right_type =
                    self.validate_expr_with_context(right, Some(expected_type.clone()));
                self.report_type_mismatch(left.span, expected_type.clone(), left_type.clone());
                self.report_type_mismatch(right.span, expected_type, right_type.clone());

                if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
                    Type::Unknown
                } else {
                    Type::Basic(BasicType::Bool)
                }
            }
            ExprKind::Binary {
                left,
                op:
                    op @ (BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Remainder),
                right,
            } => self.validate_arithmetic(expr.span, left, *op, right, expected_type.clone()),
            ExprKind::Binary {
                left,
                op:
                    op @ (BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight),
                right,
            } => self.validate_bitwise(expr.span, left, *op, right, expected_type.clone()),
            ExprKind::Binary { left, op, right } => {
                self.validate_comparison(expr.span, left, *op, right)
            }
            ExprKind::Match { value, branches } => {
                let value_type = self.validate_expr(value);
                let mut seen = HashSet::new();
                let mut has_wildcard = false;
                let mut branch_type = None;

                for branch in branches {
                    if has_wildcard {
                        self.diagnostics.push(Diagnostic::error(
                            branch.pattern.span(),
                            "match branches after a wildcard are unreachable",
                        ));
                    }
                    self.push_scope();
                    match (&value_type, &branch.pattern) {
                        (Type::Enum(enum_name), Pattern::Variant { .. }) => {
                            if let Some(variant_name) =
                                self.validate_pattern(&branch.pattern, Some(enum_name.as_str()))
                                && !seen.insert(variant_name.clone())
                            {
                                self.diagnostics.push(Diagnostic::error(
                                    branch.pattern.span(),
                                    format!("duplicate match branch for variant `{variant_name}`"),
                                ));
                            }
                        }
                        (Type::Basic(BasicType::String), Pattern::String { value, span }) => {
                            if !seen.insert(value.clone()) {
                                self.diagnostics.push(Diagnostic::error(
                                    *span,
                                    format!("duplicate match branch for string `{value}`"),
                                ));
                            }
                        }
                        (
                            Type::Enum(_) | Type::Basic(BasicType::String),
                            Pattern::Wildcard { span },
                        ) => {
                            if has_wildcard {
                                self.diagnostics.push(Diagnostic::error(
                                    *span,
                                    "duplicate wildcard match branch",
                                ));
                            }
                            has_wildcard = true;
                        }
                        (Type::Enum(enum_name), Pattern::String { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                format!("string patterns cannot match enum `{enum_name}`"),
                            ));
                        }
                        (Type::Basic(BasicType::String), Pattern::Variant { span, .. }) => {
                            self.diagnostics.push(Diagnostic::error(
                                *span,
                                "enum patterns cannot match a `String` value",
                            ));
                        }
                        (Type::Unknown, _) => {
                            self.validate_pattern(&branch.pattern, None);
                        }
                        (_, _) => {}
                    }
                    let value_type =
                        self.validate_match_branch_body(&branch.body, expected_type.clone());
                    self.pop_scope();

                    if !matches!(value_type, Type::Unknown) {
                        if let Some(first_type) = branch_type.clone() {
                            self.report_type_mismatch(branch.body.span(), first_type, value_type);
                        } else {
                            branch_type = Some(value_type);
                        }
                    }
                }

                if let Type::Enum(enum_name) = &value_type
                    && !has_wildcard
                    && let Some(definition) = self.enums.get(enum_name)
                {
                    let mut missing = definition
                        .variants
                        .keys()
                        .filter(|name| !seen.contains(*name))
                        .cloned()
                        .collect::<Vec<_>>();
                    missing.sort();

                    if !missing.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "non-exhaustive match for enum `{enum_name}`; missing {}",
                                missing
                                    .iter()
                                    .map(|name| format!("`{name}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                } else if value_type == Type::Basic(BasicType::String) && !has_wildcard {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        "non-exhaustive match for `String`; add a wildcard branch",
                    ));
                } else if !matches!(
                    value_type,
                    Type::Enum(_) | Type::Basic(BasicType::String) | Type::Unknown
                ) {
                    self.diagnostics.push(Diagnostic::error(
                        value.span,
                        "match expressions require an enum or `String` value",
                    ));
                }

                branch_type.unwrap_or(Type::Unknown)
            }
            ExprKind::Lambda(function) => self.validate_lambda(expr.span, function, expected_type),
            ExprKind::PostfixIncrement(target) => {
                if matches!(target.kind, ExprKind::Member { .. }) {
                    return self.validate_member_increment(expr.span, target);
                }

                let ExprKind::Identifier(name) = &target.kind else {
                    self.validate_expr(target);
                    self.diagnostics.push(Diagnostic::error(
                        target.span,
                        "increment target must be a mutable local binding",
                    ));
                    return Type::Unknown;
                };

                let Some(binding) = self.lookup(name) else {
                    self.validate_expr(target);
                    return Type::Unknown;
                };

                if !binding.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("cannot mutate immutable binding `{name}`"),
                    ));
                }

                if matches!(&binding.type_, Type::Basic(type_) if type_.is_numeric()) {
                    binding.type_
                } else if matches!(binding.type_, Type::Unknown) {
                    Type::Unknown
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!(
                            "operator ++ only supports numeric operands, got `{}`",
                            binding.type_.name()
                        ),
                    ));
                    Type::Unknown
                }
            }
        }
    }

    fn validate_member_assignment(
        &mut self,
        span: Span,
        target: &Expr,
        op: Option<BinaryOp>,
        value: &Expr,
    ) {
        let ExprKind::Member { object, .. } = &target.kind else {
            return;
        };
        let Some(binding_name) = mutable_member_root(object) else {
            self.validate_expr(target);
            self.validate_expr(value);
            self.diagnostics.push(Diagnostic::error(
                target.span,
                "field assignment target must be rooted in a mutable local struct binding",
            ));
            return;
        };
        let Some(binding) = self.lookup(binding_name) else {
            self.validate_expr(target);
            self.validate_expr(value);
            return;
        };

        if !binding.mutable {
            self.diagnostics.push(Diagnostic::error(
                target.span,
                format!("cannot mutate field of immutable binding `{binding_name}`"),
            ));
        }

        let field_type = self.validate_expr(target);
        if matches!(field_type, Type::Unknown) {
            self.validate_expr(value);
            return;
        }

        if op.is_none()
            && self.requires_mutable_capability(&field_type)
            && !self.expr_has_mutable_capability(value)
        {
            self.diagnostics.push(Diagnostic::error(
                value.span,
                "cannot assign an immutable value to a mutable field; use `.clone()` to create an independent mutable object",
            ));
        }

        let value_type =
            self.validate_assignment_value(span, target, op, value, field_type.clone());
        self.report_type_mismatch(value.span, field_type, value_type);
    }

    fn validate_assignment_value(
        &mut self,
        span: Span,
        target: &Expr,
        op: Option<BinaryOp>,
        value: &Expr,
        expected_type: Type,
    ) -> Type {
        if let Some(op) = op {
            if matches!(
                op,
                BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
            ) {
                self.validate_bitwise(span, target, op, value, Some(expected_type))
            } else {
                self.validate_arithmetic(span, target, op, value, Some(expected_type))
            }
        } else {
            self.validate_expr_with_context(value, Some(expected_type))
        }
    }

    fn validate_member_increment(&mut self, span: Span, target: &Expr) -> Type {
        let ExprKind::Member { object, .. } = &target.kind else {
            return Type::Unknown;
        };
        let Some(binding_name) = mutable_member_root(object) else {
            self.validate_expr(target);
            self.diagnostics.push(Diagnostic::error(
                target.span,
                "increment target must be rooted in a mutable local struct binding",
            ));
            return Type::Unknown;
        };
        let Some(binding) = self.lookup(binding_name) else {
            self.validate_expr(target);
            return Type::Unknown;
        };

        if !binding.mutable {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("cannot mutate field of immutable binding `{binding_name}`"),
            ));
        }

        let field_type = self.validate_expr(target);
        if matches!(&field_type, Type::Basic(type_) if type_.is_numeric()) {
            field_type
        } else if matches!(field_type, Type::Unknown) {
            Type::Unknown
        } else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "operator ++ only supports numeric operands, got `{}`",
                    field_type.name()
                ),
            ));
            Type::Unknown
        }
    }

    fn validate_arithmetic(
        &mut self,
        span: Span,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        expected_type: Option<Type>,
    ) -> Type {
        let contextual_type = expected_type.filter(|type_| {
            matches!(type_, Type::Basic(type_) if type_.is_numeric())
                || op == BinaryOp::Add && *type_ == Type::Basic(BasicType::String)
        });
        let (left_type, right_type) = if let Some(type_) = contextual_type {
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if number_pair_contains_float(left, right) {
            let type_ = Type::Basic(BasicType::F64);
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if matches!(left.kind, ExprKind::Number(_))
            && !matches!(right.kind, ExprKind::Number(_))
        {
            let right_type = self.validate_expr(right);
            let left_type = self.validate_expr_with_context(left, Some(right_type.clone()));
            (left_type, right_type)
        } else {
            let left_type = self.validate_expr(left);
            let right_type = self.validate_expr_with_context(right, Some(left_type.clone()));
            (left_type, right_type)
        };

        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
            return Type::Unknown;
        }

        if left_type != right_type {
            self.report_type_mismatch(right.span, left_type, right_type);
            return Type::Unknown;
        }

        if matches!(&left_type, Type::Basic(type_) if type_.is_numeric())
            || op == BinaryOp::Add && left_type == Type::Basic(BasicType::String)
        {
            return left_type;
        }

        let requirement = if op == BinaryOp::Add {
            "only supports numeric or String operands"
        } else {
            "only supports numeric operands"
        };
        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "operator {} {requirement}, got `{}`",
                op.symbol(),
                left_type.name()
            ),
        ));
        Type::Unknown
    }

    fn validate_bitwise(
        &mut self,
        span: Span,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        expected_type: Option<Type>,
    ) -> Type {
        let contextual_type =
            expected_type.filter(|type_| matches!(type_, Type::Basic(type_) if type_.is_integer()));
        let (left_type, right_type) = if let Some(type_) = contextual_type {
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if matches!(left.kind, ExprKind::Number(_))
            && !matches!(right.kind, ExprKind::Number(_))
        {
            let right_type = self.validate_expr(right);
            let left_type = self.validate_expr_with_context(left, Some(right_type.clone()));
            (left_type, right_type)
        } else {
            let left_type = self.validate_expr(left);
            let right_type = self.validate_expr_with_context(right, Some(left_type.clone()));
            (left_type, right_type)
        };

        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
            return Type::Unknown;
        }

        if left_type != right_type {
            self.report_type_mismatch(right.span, left_type, right_type);
            return Type::Unknown;
        }

        if matches!(&left_type, Type::Basic(type_) if type_.is_integer()) {
            return left_type;
        }

        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "operator {} only supports integer operands, got `{}`",
                op.symbol(),
                left_type.name()
            ),
        ));
        Type::Unknown
    }

    fn validate_comparison(&mut self, span: Span, left: &Expr, op: BinaryOp, right: &Expr) -> Type {
        let (left_type, right_type) = if number_pair_contains_float(left, right) {
            let type_ = Type::Basic(BasicType::F64);
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if matches!(left.kind, ExprKind::Number(_))
            && !matches!(right.kind, ExprKind::Number(_))
        {
            let right_type = self.validate_expr(right);
            let left_type = self.validate_expr_with_context(left, Some(right_type.clone()));
            (left_type, right_type)
        } else {
            let left_type = self.validate_expr(left);
            let right_type = self.validate_expr_with_context(right, Some(left_type.clone()));
            (left_type, right_type)
        };

        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
            return Type::Unknown;
        }

        if left_type != right_type {
            self.report_type_mismatch(right.span, left_type, right_type);
            return Type::Unknown;
        }

        let supported = match op {
            BinaryOp::Equal | BinaryOp::NotEqual => {
                matches!(left_type, Type::Basic(BasicType::String | BasicType::Bool))
                    || matches!(&left_type, Type::Basic(type_) if type_.is_numeric())
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                matches!(&left_type, Type::Basic(type_) if type_.is_numeric())
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
                unreachable!("non-comparison operator is validated separately")
            }
        };

        if !supported {
            let requirement = match op {
                BinaryOp::Equal | BinaryOp::NotEqual => {
                    "only supports numeric, bool, and String operands"
                }
                BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual => "only supports numeric operands",
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
                    unreachable!("non-comparison operator is validated separately")
                }
            };
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "operator {} {requirement}, got `{}`",
                    op.symbol(),
                    left_type.name()
                ),
            ));
            return Type::Unknown;
        }

        Type::Basic(BasicType::Bool)
    }

    fn validate_call(&mut self, expr: &Expr, name: &str, args: &[Expr]) -> Type {
        let Some(signature) = self.functions.get(name).cloned() else {
            if let Some(Binding {
                type_:
                    Type::Function {
                        params,
                        return_type,
                    },
                ..
            }) = self.lookup(name)
            {
                return self.validate_function_value_call(expr, &params, &return_type, args);
            }

            if self.values.contains(name) {
                for arg in args {
                    self.validate_expr(arg);
                }

                return Type::Unknown;
            }

            let binding = self.lookup(name);
            if binding.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown name `{name}`"),
                ));
            } else if binding.is_some_and(|binding| matches!(binding.type_, Type::Unknown)) {
                for arg in args {
                    self.validate_expr(arg);
                }

                return Type::Unknown;
            } else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("`{name}` is not callable"),
                ));
            }

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Unknown;
        };

        if args.len() != signature.params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "function `{name}` expects {} arguments, got {}",
                    signature.params.len(),
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return signature.return_type;
        }

        for (arg, param) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_.clone(), arg_type);

            if param.mutable
                && self.requires_mutable_capability(&param.type_)
                && !self.expr_has_mutable_capability(arg)
            {
                self.diagnostics.push(Diagnostic::error(
                    arg.span,
                    format!(
                        "function `{name}` requires a mutable argument; use `.clone()` to pass an independent mutable object"
                    ),
                ));
            }
        }

        signature.return_type
    }

    fn validate_function_value_call(
        &mut self,
        expr: &Expr,
        params: &[FunctionTypeParam],
        return_type: &Type,
        args: &[Expr],
    ) -> Type {
        if args.len() != params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "function value expects {} arguments, got {}",
                    params.len(),
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return return_type.clone();
        }

        for (arg, param) in args.iter().zip(params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_.clone(), arg_type);

            if param.mutable
                && self.requires_mutable_capability(&param.type_)
                && !self.expr_has_mutable_capability(arg)
            {
                self.diagnostics.push(Diagnostic::error(
                    arg.span,
                    "function value requires a mutable argument; use `.clone()` to pass an independent mutable object",
                ));
            }
        }

        return_type.clone()
    }

    fn resolve_type_expression(&self, expr: &Expr) -> Option<Type> {
        let ExprKind::Identifier(name) = &expr.kind else {
            return None;
        };

        if name == "Self" {
            return self.self_types.last().cloned();
        }
        if self.lookup(name).is_some() {
            return None;
        }
        if let Some(type_) = BasicType::from_name(name) {
            Some(Type::Basic(type_))
        } else if self.structs.contains_key(name) {
            Some(Type::Struct(name.clone()))
        } else if self.enums.contains_key(name) {
            Some(Type::Enum(name.clone()))
        } else if self.types.contains(name) && name != "void" {
            Some(Type::Named(name.clone()))
        } else {
            None
        }
    }

    fn validate_static_call(
        &mut self,
        expr: &Expr,
        type_: Type,
        name: &str,
        args: &[Expr],
    ) -> Type {
        let source_name = source_callable_name(name);
        let intrinsic = if let Type::Struct(struct_name) = &type_ {
            self.structs
                .get(struct_name)
                .and_then(|struct_| struct_.static_methods.get(source_name))
                .cloned()
        } else {
            None
        };
        let signature = intrinsic
            .or_else(|| {
                self.static_extensions
                    .get(&extension_name(&type_.name(), name))
                    .cloned()
            })
            .or_else(|| {
                self.static_trait_methods
                    .get(&static_trait_method_name(&type_.name(), source_name))
                    .cloned()
            });
        let Some(signature) = signature else {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "unknown static function `{source_name}` for type `{}`",
                    type_.name()
                ),
            ));
            for arg in args {
                self.validate_expr(arg);
            }
            return Type::Unknown;
        };
        let qualified_name = format!("{}.{source_name}", type_.name());

        if args.len() != signature.params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "static function `{qualified_name}` expects {} arguments, got {}",
                    signature.params.len(),
                    args.len()
                ),
            ));
            for arg in args {
                self.validate_expr(arg);
            }
            return signature.return_type;
        }

        for (arg, param) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_, arg_type);
        }

        signature.return_type
    }

    fn validate_method_call(
        &mut self,
        expr: &Expr,
        object: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Type {
        let object_type = self.validate_expr(object);
        if matches!(object_type, Type::Unknown) {
            for arg in args {
                self.validate_expr(arg);
            }
            return Type::Unknown;
        }

        let source_name = source_callable_name(name);
        if matches!(&object_type, Type::Basic(type_) if type_.is_numeric())
            && source_name == "toString"
        {
            if !args.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!(
                        "method `{}.toString` expects 0 arguments, got {}",
                        object_type.name(),
                        args.len()
                    ),
                ));
                for arg in args {
                    self.validate_expr(arg);
                }
            }

            return Type::Basic(BasicType::String);
        }

        let intrinsic = if let Type::Struct(struct_name) = &object_type {
            self.structs
                .get(struct_name)
                .and_then(|struct_| struct_.methods.get(source_name))
                .cloned()
        } else {
            None
        };
        let signature = intrinsic
            .or_else(|| {
                self.extensions
                    .get(&extension_name(&object_type.name(), name))
                    .cloned()
            })
            .or_else(|| {
                self.trait_methods
                    .get(&trait_method_name(&object_type.name(), source_name))
                    .cloned()
            });
        let Some(signature) = signature else {
            if matches!(object_type, Type::Basic(_)) {
                self.unsupported(expr.span, "methods on basic values are not implemented yet");
            } else {
                let target = match &object_type {
                    Type::Struct(name) => format!("struct `{name}`"),
                    _ => format!("type `{}`", object_type.name()),
                };
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown method `{source_name}` for {target}"),
                ));
            }

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Unknown;
        };
        let qualified_name = format!("{}.{source_name}", object_type.name());

        if signature.mutable_self && !self.expr_has_mutable_capability(object) {
            let message = if let Some(binding_name) = mutable_member_root(object)
                && self
                    .lookup(binding_name)
                    .is_some_and(|binding| !binding.mutable)
            {
                format!(
                    "cannot call mutable function `{qualified_name}` through immutable binding `{binding_name}`; declare it with `let mut {binding_name}` or call the function on a mutable clone"
                )
            } else {
                format!(
                    "mutable function `{qualified_name}` requires a mutable receiver; bind the value with `let mut` or call the function on a mutable clone"
                )
            };
            self.diagnostics
                .push(Diagnostic::error(object.span, message));
        }

        if args.len() != signature.params.len() {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "method `{qualified_name}` expects {} arguments, got {}",
                    signature.params.len(),
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return signature.return_type;
        }

        for (arg, param) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(param.type_.clone()));
            self.report_type_mismatch(arg.span, param.type_.clone(), arg_type);

            if param.mutable
                && self.requires_mutable_capability(&param.type_)
                && !self.expr_has_mutable_capability(arg)
            {
                self.diagnostics.push(Diagnostic::error(
                    arg.span,
                    format!(
                        "method `{qualified_name}` requires a mutable argument; use `.clone()` to pass an independent mutable object"
                    ),
                ));
            }
        }

        signature.return_type
    }

    fn validate_lambda(
        &mut self,
        span: Span,
        function: &FunctionDecl,
        expected_type: Option<Type>,
    ) -> Type {
        let expected_function = match expected_type {
            Some(Type::Function {
                params,
                return_type,
            }) => Some((params, *return_type)),
            Some(Type::Unknown) | None => None,
            Some(type_) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("expected function type for lambda, got `{}`", type_.name()),
                ));
                None
            }
        };

        let expected_params = expected_function
            .as_ref()
            .map(|(params, _)| params.as_slice())
            .unwrap_or(&[]);

        if !expected_params.is_empty() && function.params.len() != expected_params.len() {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "lambda expects {} parameters from context, got {}",
                    expected_params.len(),
                    function.params.len()
                ),
            ));
        }

        self.push_scope();
        let mut params = Vec::new();
        for (index, param) in function.params.iter().enumerate() {
            let expected_param = expected_params.get(index);
            let type_ = if let Some(type_ref) = &param.type_ref {
                self.validate_type(type_ref)
            } else if let Some(expected_param) = expected_param {
                expected_param.type_.clone()
            } else {
                self.diagnostics.push(Diagnostic::error(
                    param.span,
                    "lambda parameters must include type annotations when no function type context is available",
                ));
                Type::Unknown
            };

            if let Some(expected_param) = expected_param {
                self.report_type_mismatch(param.span, expected_param.type_.clone(), type_.clone());
                if expected_param.mutable != param.mutable {
                    self.diagnostics.push(Diagnostic::error(
                        param.span,
                        "lambda parameter mutability does not match function type context",
                    ));
                }
            }

            self.define(&param.name, param.mutable, type_.clone());
            params.push(FunctionTypeParam {
                type_,
                mutable: param.mutable,
            });
        }

        let expected_return = expected_function
            .as_ref()
            .map(|(_, return_type)| return_type.clone());
        let annotated_return = function
            .return_type
            .as_ref()
            .map(|type_ref| self.validate_type(type_ref));
        let return_context = annotated_return
            .clone()
            .or_else(|| expected_return.clone())
            .unwrap_or(Type::Unknown);
        self.return_types.push(return_context.clone());

        let return_type = match &function.body {
            FunctionBody::Expr(expr) => {
                let value_type =
                    self.validate_expr_with_context(expr, Some(return_context.clone()));
                if !matches!(return_context, Type::Unknown) {
                    self.report_type_mismatch(expr.span, return_context.clone(), value_type);
                    return_context
                } else {
                    value_type
                }
            }
            FunctionBody::Block(block) => {
                self.validate_block(block);
                annotated_return
                    .or(expected_return)
                    .unwrap_or(Type::Unknown)
            }
        };

        self.return_types.pop();
        self.pop_scope();

        Type::Function {
            params,
            return_type: Box::new(return_type),
        }
    }

    fn validate_clone(&mut self, span: Span, object: &Expr, args: &[Expr]) -> Type {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("`.clone()` expects no arguments, got {}", args.len()),
            ));
            for arg in args {
                self.validate_expr(arg);
            }
        }

        let object_type = self.validate_expr(object);
        if matches!(
            object_type,
            Type::Struct(_) | Type::Basic(BasicType::String)
        ) {
            object_type
        } else if matches!(object_type, Type::Unknown) {
            Type::Unknown
        } else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "`.clone()` is only supported for struct and String values, got `{}`",
                    object_type.name()
                ),
            ));
            Type::Unknown
        }
    }

    fn validate_variant_call(
        &mut self,
        expr: &Expr,
        enum_name: &str,
        variant_name: &str,
        args: &[Expr],
    ) -> Type {
        let Some(variant) = self
            .enums
            .get(enum_name)
            .and_then(|enum_| enum_.variants.get(variant_name))
            .cloned()
        else {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!("unknown variant `{enum_name}.{variant_name}`"),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Unknown;
        };
        let expected_count = usize::from(variant.is_some());

        if args.len() != expected_count {
            self.diagnostics.push(Diagnostic::error(
                expr.span,
                format!(
                    "enum variant `{enum_name}.{variant_name}` expects {expected_count} arguments, got {}",
                    args.len()
                ),
            ));

            for arg in args {
                self.validate_expr(arg);
            }

            return Type::Enum(enum_name.to_string());
        }

        if let Some(expected_type) = variant {
            let arg_type = self.validate_expr_with_context(&args[0], Some(expected_type.clone()));
            self.report_type_mismatch(args[0].span, expected_type, arg_type);
        }

        Type::Enum(enum_name.to_string())
    }

    fn validate_unit_variant(&mut self, span: Span, enum_name: &str, variant_name: &str) -> Type {
        let Some(variant) = self
            .enums
            .get(enum_name)
            .and_then(|enum_| enum_.variants.get(variant_name))
        else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown variant `{enum_name}.{variant_name}`"),
            ));
            return Type::Unknown;
        };

        if variant.is_some() {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("enum variant `{enum_name}.{variant_name}` requires a payload"),
            ));
            return Type::Unknown;
        }

        Type::Enum(enum_name.to_string())
    }

    fn validate_struct_init(&mut self, span: Span, name: &str, fields: &[StructInitField]) -> Type {
        let resolved_name = if name == "Self" {
            match self.self_types.last() {
                Some(Type::Struct(name)) => name.clone(),
                Some(type_) => {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!(
                            "`Self` is `{}` and cannot be initialized as a struct",
                            type_.name()
                        ),
                    ));
                    return Type::Unknown;
                }
                None => name.to_string(),
            }
        } else {
            name.to_string()
        };
        let name = resolved_name.as_str();
        let Some(definition) = self.structs.get(name).cloned() else {
            if self.is_imported_namespace_member(name) {
                for field in fields {
                    self.validate_expr(&field.value);
                }
                return Type::Unknown;
            }
            if BasicType::from_name(name).is_none() && !self.types.contains(name) {
                self.diagnostics
                    .push(Diagnostic::error(span, format!("unknown type `{name}`")));
            } else {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("`{name}` is not a struct type"),
                ));
            }

            for field in fields {
                self.validate_expr(&field.value);
            }

            return Type::Unknown;
        };

        let mut seen_fields = HashSet::new();

        for field in fields {
            if !seen_fields.insert(field.name.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    field.span,
                    format!("duplicate field `{}` in struct literal", field.name),
                ));
            }

            let Some(expected_type) = definition.fields.get(&field.name).cloned() else {
                self.diagnostics.push(Diagnostic::error(
                    field.span,
                    format!("unknown field `{}` for struct `{name}`", field.name),
                ));
                self.validate_expr(&field.value);
                continue;
            };

            let value_type =
                self.validate_expr_with_context(&field.value, Some(expected_type.clone()));
            self.report_type_mismatch(field.value.span, expected_type, value_type);
        }

        for field in definition.fields.keys() {
            if !seen_fields.contains(field) {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("missing field `{field}` in struct literal `{name}`"),
                ));
            }
        }

        Type::Struct(name.to_string())
    }

    fn validate_member(&mut self, span: Span, object: &Expr, name: &str) -> Type {
        let object_type = self.validate_expr(object);

        let struct_name = match object_type {
            Type::Struct(struct_name) => struct_name,
            Type::Unknown => return Type::Unknown,
            Type::Enum(_) => {
                self.unsupported(
                    span,
                    "direct enum payload member access is not implemented yet",
                );
                return Type::Unknown;
            }
            Type::Named(_) => return Type::Unknown,
            Type::Basic(_) | Type::Function { .. } | Type::Void => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    "field access requires a struct value",
                ));
                return Type::Unknown;
            }
        };

        let Some(definition) = self.structs.get(&struct_name) else {
            return Type::Unknown;
        };

        let Some(type_) = definition.fields.get(name) else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown field `{name}` for struct `{struct_name}`"),
            ));
            return Type::Unknown;
        };

        type_.clone()
    }

    fn validate_missing_return(&mut self, function: &FunctionDecl, return_type: Type) {
        if matches!(return_type, Type::Unknown | Type::Void) {
            return;
        }

        let FunctionBody::Block(block) = &function.body else {
            return;
        };

        if block_always_returns_value(block) {
            return;
        }

        self.diagnostics.push(Diagnostic::error(
            function.span,
            "missing return value for function with explicit return type",
        ));
    }

    fn report_type_mismatch(&mut self, span: Span, expected_type: Type, value_type: Type) {
        if matches!(expected_type, Type::Unknown) || matches!(value_type, Type::Unknown) {
            return;
        }

        if !types_are_compatible(&expected_type, &value_type) {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "expected value of type `{}`, got `{}`",
                    expected_type.name(),
                    value_type.name()
                ),
            ));
        }
    }

    fn validate_pattern(&mut self, pattern: &Pattern, enum_name: Option<&str>) -> Option<String> {
        match pattern {
            Pattern::Variant {
                enum_name: pattern_enum_name,
                variant,
                binding,
                span,
            } => {
                if enum_name.is_some_and(|enum_name| enum_name != pattern_enum_name) {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "pattern `{pattern_enum_name}.{variant}` does not belong to enum `{}`",
                            enum_name.unwrap_or_default()
                        ),
                    ));
                    return None;
                }

                let Some(payload) = self
                    .enums
                    .get(pattern_enum_name)
                    .and_then(|enum_| enum_.variants.get(variant))
                    .cloned()
                else {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!("unknown pattern `{pattern_enum_name}.{variant}`"),
                    ));
                    return None;
                };

                match (binding, payload) {
                    (Some(binding), Some(payload)) if binding != "_" => {
                        self.define(binding, false, payload)
                    }
                    (Some(_), Some(_)) => {}
                    (Some(_), None) => self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "unit variant `{pattern_enum_name}.{variant}` does not bind a payload"
                        ),
                    )),
                    (None, Some(payload)) => self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!(
                            "`{pattern_enum_name}.{variant}` contains a `{}` value; use `{pattern_enum_name}.{variant}(value)` to bind it or `{pattern_enum_name}.{variant}(_)` to ignore it",
                            payload.name()
                        ),
                    )),
                    (None, None) => {}
                }

                Some(variant.clone())
            }
            Pattern::String { .. } | Pattern::Wildcard { .. } => None,
        }
    }

    fn validate_match_branch_body(
        &mut self,
        body: &MatchBranchBody,
        expected_type: Option<Type>,
    ) -> Type {
        match body {
            MatchBranchBody::Expr(expr) => self.validate_expr_with_context(expr, expected_type),
            MatchBranchBody::Block(block) => {
                self.validate_block(block);
                Type::Unknown
            }
        }
    }

    fn validate_type(&mut self, type_ref: &TypeRef) -> Type {
        if let Some(function) = &type_ref.function {
            let params = function
                .params
                .iter()
                .map(|param| FunctionTypeParam {
                    type_: self.validate_type(&param.type_ref),
                    mutable: param.mutable,
                })
                .collect();
            let return_type = Box::new(self.validate_type(&function.return_type));
            return Type::Function {
                params,
                return_type,
            };
        }

        if type_ref.name == "Self" {
            let self_type = self.self_types.last().cloned();
            if self_type.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    type_ref.span,
                    "`Self` is only available in methods and extension functions",
                ));
            }
            return self_type.unwrap_or(Type::Unknown);
        }

        let basic_type = BasicType::from_name(&type_ref.name);
        let imported_namespace_member = self.is_imported_namespace_member(&type_ref.name);

        if basic_type.is_none()
            && !self.types.contains(&type_ref.name)
            && !imported_namespace_member
        {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("unknown type `{}`", type_ref.name),
            ));
        }

        if !type_ref.args.is_empty() {
            self.unsupported(
                type_ref.span,
                "generic types are parsed but monomorphization is not implemented yet",
            );
        }

        for arg in &type_ref.args {
            self.validate_type(arg);
        }

        if imported_namespace_member || !type_ref.args.is_empty() {
            Type::Unknown
        } else if let Some(basic_type) = basic_type {
            Type::Basic(basic_type)
        } else if self.structs.contains_key(&type_ref.name) {
            Type::Struct(type_ref.name.clone())
        } else if self.enums.contains_key(&type_ref.name) {
            Type::Enum(type_ref.name.clone())
        } else if type_ref.name == "void" {
            Type::Void
        } else if self.types.contains(&type_ref.name) {
            Type::Named(type_ref.name.clone())
        } else {
            Type::Unknown
        }
    }

    fn is_imported_namespace_member(&self, name: &str) -> bool {
        name.split_once('.')
            .is_some_and(|(namespace, _)| self.imported_namespaces.contains(namespace))
    }

    fn requires_unsupported_default(&self, type_ref: &TypeRef) -> bool {
        if type_ref.function.is_some() {
            return true;
        }

        if BasicType::from_name(&type_ref.name).is_some() {
            return !type_ref.args.is_empty();
        }

        self.structs.contains_key(&type_ref.name) || self.types.contains(&type_ref.name)
    }

    fn type_ref_without_diagnostics(&self, type_ref: Option<&TypeRef>) -> Type {
        let Some(type_ref) = type_ref else {
            return Type::Unknown;
        };

        if let Some(function) = &type_ref.function {
            return Type::Function {
                params: function
                    .params
                    .iter()
                    .map(|param| FunctionTypeParam {
                        type_: self.type_ref_without_diagnostics(Some(&param.type_ref)),
                        mutable: param.mutable,
                    })
                    .collect(),
                return_type: Box::new(
                    self.type_ref_without_diagnostics(Some(&function.return_type)),
                ),
            };
        }

        if !type_ref.args.is_empty() {
            return Type::Unknown;
        }

        if let Some(basic_type) = BasicType::from_name(&type_ref.name) {
            Type::Basic(basic_type)
        } else if self.structs.contains_key(&type_ref.name) {
            Type::Struct(type_ref.name.clone())
        } else if self.enums.contains_key(&type_ref.name) {
            Type::Enum(type_ref.name.clone())
        } else if type_ref.name == "void" {
            Type::Void
        } else if self.types.contains(&type_ref.name) {
            Type::Named(type_ref.name.clone())
        } else {
            Type::Unknown
        }
    }

    fn type_ref_in_context(&self, type_ref: Option<&TypeRef>, self_type: &Type) -> Type {
        if let Some(type_ref) = type_ref
            && let Some(function) = &type_ref.function
        {
            return Type::Function {
                params: function
                    .params
                    .iter()
                    .map(|param| FunctionTypeParam {
                        type_: self.type_ref_in_context(Some(&param.type_ref), self_type),
                        mutable: param.mutable,
                    })
                    .collect(),
                return_type: Box::new(
                    self.type_ref_in_context(Some(&function.return_type), self_type),
                ),
            };
        }

        if type_ref.is_some_and(|type_ref| type_ref.name == "Self" && type_ref.args.is_empty()) {
            self_type.clone()
        } else {
            self.type_ref_without_diagnostics(type_ref)
        }
    }

    fn requires_mutable_capability(&self, type_: &Type) -> bool {
        matches!(type_, Type::Struct(_))
    }

    fn expr_has_mutable_capability(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Identifier(name) => self.lookup(name).is_some_and(|binding| binding.mutable),
            ExprKind::Member { object, .. } => self.expr_has_mutable_capability(object),
            ExprKind::GenericMember { object, .. } => self.expr_has_mutable_capability(object),
            ExprKind::StructInit { name, fields, .. } => {
                let Some(definition) = self.structs.get(name) else {
                    return false;
                };

                fields.iter().all(|field| {
                    definition.fields.get(&field.name).is_none_or(|type_| {
                        !self.requires_mutable_capability(type_)
                            || self.expr_has_mutable_capability(&field.value)
                    })
                })
            }
            ExprKind::Call { callee, args } => {
                if matches!(
                    &callee.kind,
                    ExprKind::Member { name, .. } if name == "clone"
                ) {
                    return true;
                }

                let signature = match &callee.kind {
                    ExprKind::Identifier(name) => self.functions.get(name),
                    ExprKind::Member { object, name } => {
                        if let Some(type_) = self.resolve_type_expression(object) {
                            match &type_ {
                                Type::Struct(struct_name) => self
                                    .structs
                                    .get(struct_name)
                                    .and_then(|struct_| struct_.static_methods.get(name))
                                    .or_else(|| {
                                        self.static_extensions
                                            .get(&extension_name(&type_.name(), name))
                                    })
                                    .or_else(|| {
                                        self.static_trait_methods
                                            .get(&static_trait_method_name(&type_.name(), name))
                                    }),
                                _ => self
                                    .static_extensions
                                    .get(&extension_name(&type_.name(), name))
                                    .or_else(|| {
                                        self.static_trait_methods
                                            .get(&static_trait_method_name(&type_.name(), name))
                                    }),
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                signature.is_some_and(|signature| {
                    args.iter().zip(&signature.params).all(|(arg, param)| {
                        !self.requires_mutable_capability(&param.type_)
                            || self.expr_has_mutable_capability(arg)
                    })
                })
            }
            ExprKind::String(_)
            | ExprKind::Number(_)
            | ExprKind::Bool(_)
            | ExprKind::Binary { .. }
            | ExprKind::Unary { .. } => true,
            ExprKind::Array(_)
            | ExprKind::GenericType { .. }
            | ExprKind::Lambda(_)
            | ExprKind::Match { .. }
            | ExprKind::PostfixIncrement(_)
            | ExprKind::Missing => false,
        }
    }

    fn unsupported(&mut self, span: Span, message: &'static str) {
        if self.unsupported_features.insert(message) {
            self.diagnostics.push(Diagnostic::warning(span, message));
        }
    }

    fn define(&mut self, name: &str, mutable: bool, type_: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), Binding { mutable, type_ });
        }
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding.clone());
            }
        }

        None
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn current_return_type(&self) -> Type {
        self.return_types.last().cloned().unwrap_or(Type::Unknown)
    }
}

fn types_are_compatible(expected_type: &Type, value_type: &Type) -> bool {
    match (expected_type, value_type) {
        (Type::Unknown, _) | (_, Type::Unknown) => true,
        (
            Type::Function {
                params,
                return_type,
            },
            Type::Function {
                params: value_params,
                return_type: value_return_type,
            },
        ) => {
            params.len() == value_params.len()
                && params.iter().zip(value_params).all(|(param, value_param)| {
                    param.mutable == value_param.mutable
                        && types_are_compatible(&param.type_, &value_param.type_)
                })
                && types_are_compatible(return_type, value_return_type)
        }
        _ => expected_type == value_type,
    }
}

fn signatures_match(expected: &FunctionSignature, actual: &FunctionSignature) -> bool {
    expected.mutable_self == actual.mutable_self
        && expected.params.len() == actual.params.len()
        && expected
            .params
            .iter()
            .zip(&actual.params)
            .all(|(expected, actual)| {
                expected.mutable == actual.mutable && expected.type_ == actual.type_
            })
        && expected.return_type == actual.return_type
}

fn signature_with_self_type(signature: &FunctionSignature, self_type: &Type) -> FunctionSignature {
    FunctionSignature {
        params: signature
            .params
            .iter()
            .map(|param| ParamSignature {
                type_: type_with_self_type(&param.type_, self_type),
                mutable: param.mutable,
            })
            .collect(),
        return_type: type_with_self_type(&signature.return_type, self_type),
        mutable_self: signature.mutable_self,
    }
}

fn type_with_self_type(type_: &Type, self_type: &Type) -> Type {
    match type_ {
        Type::Named(name) if name == "Self" => self_type.clone(),
        Type::Function {
            params,
            return_type,
        } => Type::Function {
            params: params
                .iter()
                .map(|param| FunctionTypeParam {
                    type_: type_with_self_type(&param.type_, self_type),
                    mutable: param.mutable,
                })
                .collect(),
            return_type: Box::new(type_with_self_type(return_type, self_type)),
        },
        _ => type_.clone(),
    }
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

fn block_always_returns_value(block: &Block) -> bool {
    block.statements.iter().any(statement_always_returns_value)
}

fn statement_always_returns_value(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return { value: Some(_) } => true,
        StmtKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            block_always_returns_value(then_branch)
                && match else_branch {
                    ElseBranch::Block(block) => block_always_returns_value(block),
                    ElseBranch::If(statement) => statement_always_returns_value(statement),
                }
        }
        StmtKind::Let { .. }
        | StmtKind::Assign { .. }
        | StmtKind::Return { value: None }
        | StmtKind::While { .. }
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::If {
            else_branch: None, ..
        }
        | StmtKind::For { .. }
        | StmtKind::Expr(_) => false,
    }
}
