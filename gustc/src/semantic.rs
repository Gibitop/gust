use std::collections::{HashMap, HashSet};

use crate::ast::{
    BasicType, BinaryOp, Block, Expr, ExprKind, FunctionBody, FunctionDecl, Item, Pattern, Program,
    Stmt, StmtKind, StructDecl, StructInitField, StructMember, TypeRef,
};
use crate::diagnostic::Diagnostic;
use crate::span::Span;

pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let mut analyzer = Analyzer::new();
    analyzer.collect_top_level(program);
    analyzer.validate_program(program);
    analyzer.diagnostics
}

struct Analyzer {
    diagnostics: Vec<Diagnostic>,
    values: HashSet<String>,
    types: HashSet<String>,
    structs: HashMap<String, StructDefinition>,
    functions: HashMap<String, FunctionSignature>,
    unsupported_features: HashSet<&'static str>,
    scopes: Vec<HashMap<String, Binding>>,
    return_types: Vec<Type>,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<Type>,
    return_type: Type,
}

#[derive(Debug, Clone)]
struct StructDefinition {
    fields: HashMap<String, Type>,
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    type_: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Type {
    Basic(BasicType),
    Struct(String),
    Unknown,
}

impl Type {
    fn name(&self) -> String {
        match self {
            Type::Basic(type_) => type_.name().to_string(),
            Type::Struct(name) => name.clone(),
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
            functions: HashMap::new(),
            unsupported_features: HashSet::new(),
            scopes: Vec::new(),
            return_types: Vec::new(),
        }
    }

    fn collect_top_level(&mut self, program: &Program) {
        let mut names: HashMap<String, Span> = HashMap::new();
        let mut main_count = 0;

        for item in &program.items {
            match item {
                Item::Import(item) => {
                    for name in &item.names {
                        self.values.insert(name.clone());
                        self.types.insert(name.clone());
                        self.insert_top_level(&mut names, name, item.span);
                    }
                }
                Item::Enum(item) => {
                    self.types.insert(item.name.clone());
                    self.insert_top_level(&mut names, &item.name, item.span);

                    for variant in &item.variants {
                        self.values.insert(variant.name.clone());
                    }
                }
                Item::Struct(item) => {
                    self.values.insert(item.name.clone());
                    self.types.insert(item.name.clone());
                    self.structs.insert(
                        item.name.clone(),
                        StructDefinition {
                            fields: HashMap::new(),
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
            }
        }

        for item in &program.items {
            match item {
                Item::Struct(item) => self.collect_struct_definition(item),
                Item::Function(item) => {
                    if let Some(name) = &item.name {
                        self.functions.insert(
                            name.clone(),
                            FunctionSignature {
                                params: item
                                    .params
                                    .iter()
                                    .map(|param| {
                                        self.type_ref_without_diagnostics(param.type_ref.as_ref())
                                    })
                                    .collect(),
                                return_type: self
                                    .type_ref_without_diagnostics(item.return_type.as_ref()),
                            },
                        );
                    }
                }
                Item::Import(_) | Item::Enum(_) => {}
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

        for member in &item.members {
            let StructMember::Field(field) = member else {
                continue;
            };

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

        self.structs
            .insert(item.name.clone(), StructDefinition { fields });
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
                    self.unsupported(
                        item.span,
                        "enums are parsed but enum layout and matching are not implemented yet",
                    );

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
                                let type_ = self.validate_type(&field.type_ref);

                                if matches!(type_, Type::Struct(_))
                                    || (matches!(type_, Type::Unknown)
                                        && BasicType::from_name(&field.type_ref.name).is_none()
                                        && self.types.contains(&field.type_ref.name))
                                {
                                    self.diagnostics.push(Diagnostic::error(
                                        field.span,
                                        "struct fields only support basic types for now",
                                    ));
                                }
                            }
                            StructMember::Method(method) => {
                                self.unsupported(
                                    method.span,
                                    "methods are parsed but method dispatch is not implemented yet",
                                );
                                self.validate_function(method, true);
                            }
                        }
                    }
                }
                Item::Function(function) => self.validate_function(function, false),
            }
        }
    }

    fn validate_function(&mut self, function: &FunctionDecl, has_self: bool) {
        self.push_scope();

        if has_self {
            self.define("self", false, Type::Unknown);
        }

        for param in &function.params {
            if param.mutable {
                self.unsupported(
                    param.span,
                    "mutable parameters are parsed but mutation lowering is not implemented yet",
                );
            }

            let type_ = param
                .type_ref
                .as_ref()
                .map_or(Type::Unknown, |type_ref| self.validate_type(type_ref));

            self.define(&param.name, param.mutable, type_);
        }

        let return_type = function
            .return_type
            .as_ref()
            .map_or(Type::Unknown, |type_ref| self.validate_type(type_ref));
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
                if *mutable {
                    self.unsupported(
                        statement.span,
                        "mutable bindings are parsed but mutation lowering is not implemented yet",
                    );
                }

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

                self.define(name, *mutable, annotated_type.unwrap_or(value_type));
            }
            StmtKind::Return { value } => {
                let expected_type = self.current_return_type();

                if let Some(value) = value {
                    let value_type =
                        self.validate_expr_with_context(value, Some(expected_type.clone()));
                    self.report_type_mismatch(value.span, expected_type, value_type);
                } else if matches!(expected_type, Type::Basic(_)) {
                    self.diagnostics.push(Diagnostic::error(
                        statement.span,
                        "return value required for this function",
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
                self.push_scope();
                self.define(name, false, Type::Unknown);

                for statement in &body.statements {
                    self.validate_statement(statement);
                }

                self.pop_scope();
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
            ExprKind::Number(_) => match expected_type {
                Some(Type::Basic(type_)) if type_.is_numeric() => Type::Basic(type_),
                Some(Type::Unknown) => Type::Unknown,
                Some(Type::Basic(_)) | Some(Type::Struct(_)) | None => Type::Basic(BasicType::I32),
            },
            ExprKind::String(_) => Type::Basic(BasicType::String),
            ExprKind::Bool(_) => Type::Basic(BasicType::Bool),
            ExprKind::Missing => Type::Unknown,
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
                if let ExprKind::Identifier(name) = &callee.kind {
                    return self.validate_call(expr, name, args);
                }

                self.validate_expr(callee);

                for arg in args {
                    self.validate_expr(arg);
                }

                Type::Unknown
            }
            ExprKind::Member { object, name } => self.validate_member(expr.span, object, name),
            ExprKind::StructInit { name, fields } => {
                self.validate_struct_init(expr.span, name, fields)
            }
            ExprKind::Binary { left, op, right } => {
                let left_type = self.validate_expr(left);
                let right_type = self.validate_expr(right);

                match op {
                    BinaryOp::Add => {
                        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown)
                        {
                            Type::Unknown
                        } else if left_type == Type::Basic(BasicType::String)
                            && right_type == Type::Basic(BasicType::String)
                        {
                            Type::Basic(BasicType::String)
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                expr.span,
                                "operator + only supports String operands for now",
                            ));
                            Type::Unknown
                        }
                    }
                    BinaryOp::GreaterEqual => Type::Unknown,
                }
            }
            ExprKind::Match { value, branches } => {
                self.unsupported(
                    expr.span,
                    "match expressions are parsed but pattern lowering is not implemented yet",
                );
                self.validate_expr(value);

                for branch in branches {
                    self.push_scope();
                    self.validate_pattern(&branch.pattern);
                    self.validate_expr(&branch.value);
                    self.pop_scope();
                }

                Type::Unknown
            }
            ExprKind::Lambda(function) => {
                self.unsupported(
                    expr.span,
                    "lambda functions are parsed but closure lowering is not implemented yet",
                );
                self.validate_function(function, false);
                Type::Unknown
            }
            ExprKind::PostfixIncrement(target) => {
                self.unsupported(
                    expr.span,
                    "increment expressions are parsed but mutation lowering is not implemented yet",
                );
                self.validate_expr(target);

                if let Some(name) = root_identifier(target) {
                    if let Some(binding) = self.lookup(name) {
                        if !binding.mutable {
                            self.diagnostics.push(Diagnostic::error(
                                expr.span,
                                format!("cannot mutate immutable binding `{name}`"),
                            ));
                        }
                    }
                }

                Type::Unknown
            }
        }
    }

    fn validate_call(&mut self, expr: &Expr, name: &str, args: &[Expr]) -> Type {
        let Some(signature) = self.functions.get(name).cloned() else {
            if self.values.contains(name) {
                for arg in args {
                    self.validate_expr(arg);
                }

                return Type::Unknown;
            }

            if self.lookup(name).is_none() {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown name `{name}`"),
                ));
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

        for (arg, expected_type) in args.iter().zip(signature.params) {
            let arg_type = self.validate_expr_with_context(arg, Some(expected_type.clone()));
            self.report_type_mismatch(arg.span, expected_type, arg_type);
        }

        signature.return_type
    }

    fn validate_struct_init(&mut self, span: Span, name: &str, fields: &[StructInitField]) -> Type {
        let Some(definition) = self.structs.get(name).cloned() else {
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
            Type::Basic(_) => {
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
        if !matches!(return_type, Type::Basic(_)) {
            return;
        }

        let FunctionBody::Block(block) = &function.body else {
            return;
        };

        if matches!(
            block.statements.last().map(|statement| &statement.kind),
            Some(StmtKind::Return { value: Some(_) })
        ) {
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

        if expected_type != value_type {
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

    fn validate_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Identifier {
                name,
                binding,
                span,
            } => {
                if !self.values.contains(name) {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        format!("unknown pattern `{name}`"),
                    ));
                }

                if let Some(binding) = binding {
                    self.define(binding, false, Type::Unknown);
                }
            }
        }
    }

    fn validate_type(&mut self, type_ref: &TypeRef) -> Type {
        let basic_type = BasicType::from_name(&type_ref.name);

        if basic_type.is_none() && !self.types.contains(&type_ref.name) {
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

        if !type_ref.args.is_empty() {
            Type::Unknown
        } else if let Some(basic_type) = basic_type {
            Type::Basic(basic_type)
        } else if self.structs.contains_key(&type_ref.name) {
            Type::Struct(type_ref.name.clone())
        } else {
            Type::Unknown
        }
    }

    fn requires_unsupported_default(&self, type_ref: &TypeRef) -> bool {
        if BasicType::from_name(&type_ref.name).is_some() {
            return !type_ref.args.is_empty();
        }

        self.structs.contains_key(&type_ref.name) || self.types.contains(&type_ref.name)
    }

    fn type_ref_without_diagnostics(&self, type_ref: Option<&TypeRef>) -> Type {
        let Some(type_ref) = type_ref else {
            return Type::Unknown;
        };

        if !type_ref.args.is_empty() {
            return Type::Unknown;
        }

        if let Some(basic_type) = BasicType::from_name(&type_ref.name) {
            Type::Basic(basic_type)
        } else if self.structs.contains_key(&type_ref.name) {
            Type::Struct(type_ref.name.clone())
        } else {
            Type::Unknown
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

fn root_identifier(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name),
        ExprKind::Member { object, .. } => root_identifier(object),
        _ => None,
    }
}
