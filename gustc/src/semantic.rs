use std::collections::{HashMap, HashSet};

use crate::ast::{
    Block, Expr, ExprKind, FunctionBody, FunctionDecl, Item, Pattern, Program, Stmt, StmtKind,
    StructMember, TypeRef,
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
    unsupported_features: HashSet<&'static str>,
    scopes: Vec<HashMap<String, Binding>>,
}

#[derive(Debug, Clone, Copy)]
struct Binding {
    mutable: bool,
}

impl Analyzer {
    fn new() -> Self {
        let values = HashSet::from(["io".to_string()]);
        let types = HashSet::from([
            "String".to_string(),
            "u32".to_string(),
            "void".to_string(),
            "ArrayList".to_string(),
        ]);

        Self {
            diagnostics: Vec::new(),
            values,
            types,
            unsupported_features: HashSet::new(),
            scopes: Vec::new(),
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
                    self.unsupported(
                        item.span,
                        "structs are parsed but struct layout is not implemented yet",
                    );

                    for member in &item.members {
                        match member {
                            StructMember::Field(field) => self.validate_type(&field.type_ref),
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
            self.define("self", false);
        }

        for param in &function.params {
            if param.mutable {
                self.unsupported(
                    param.span,
                    "mutable parameters are parsed but mutation lowering is not implemented yet",
                );
            }

            if let Some(type_ref) = &param.type_ref {
                self.validate_type(type_ref);
            }

            self.define(&param.name, param.mutable);
        }

        if let Some(type_ref) = &function.return_type {
            self.validate_type(type_ref);
        }

        match &function.body {
            FunctionBody::Block(block) => self.validate_block(block),
            FunctionBody::Expr(expr) => self.validate_expr(expr),
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
                if *mutable {
                    self.unsupported(
                        statement.span,
                        "mutable bindings are parsed but mutation lowering is not implemented yet",
                    );
                }

                if let Some(type_ref) = type_annotation {
                    self.validate_type(type_ref);
                }

                self.validate_expr(value);
                self.define(name, *mutable);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    self.validate_expr(value);
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
                self.define(name, false);

                for statement in &body.statements {
                    self.validate_statement(statement);
                }

                self.pop_scope();
            }
            StmtKind::Expr(expr) => self.validate_expr(expr),
        }
    }

    fn validate_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Identifier(name) => {
                if !self.is_declared(name) {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown name `{name}`"),
                    ));
                }
            }
            ExprKind::Number(_) | ExprKind::String(_) | ExprKind::Missing => {}
            ExprKind::Array(items) => {
                self.unsupported(
                    expr.span,
                    "array literals are parsed but collection lowering is not implemented yet",
                );

                for item in items {
                    self.validate_expr(item);
                }
            }
            ExprKind::Call { callee, args } => {
                self.validate_expr(callee);

                for arg in args {
                    self.validate_expr(arg);
                }
            }
            ExprKind::Member { object, .. } => self.validate_expr(object),
            ExprKind::StructInit { name, fields } => {
                self.unsupported(
                    expr.span,
                    "struct literals are parsed but construction is not implemented yet",
                );

                if !self.types.contains(name) {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("unknown type `{name}`"),
                    ));
                }

                for field in fields {
                    self.validate_expr(&field.value);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.validate_expr(left);
                self.validate_expr(right);
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
            }
            ExprKind::Lambda(function) => {
                self.unsupported(
                    expr.span,
                    "lambda functions are parsed but closure lowering is not implemented yet",
                );
                self.validate_function(function, false);
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
            }
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
                    self.define(binding, false);
                }
            }
        }
    }

    fn validate_type(&mut self, type_ref: &TypeRef) {
        if !self.types.contains(&type_ref.name) {
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
    }

    fn unsupported(&mut self, span: Span, message: &'static str) {
        if self.unsupported_features.insert(message) {
            self.diagnostics.push(Diagnostic::warning(span, message));
        }
    }

    fn define(&mut self, name: &str, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), Binding { mutable });
        }
    }

    fn is_declared(&self, name: &str) -> bool {
        self.lookup(name).is_some() || self.values.contains(name)
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(*binding);
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
}

fn root_identifier(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name),
        ExprKind::Member { object, .. } => root_identifier(object),
        _ => None,
    }
}
