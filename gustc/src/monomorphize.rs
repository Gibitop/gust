use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, Item, MatchBranchBody, Program,
    Stmt, StmtKind, StructDecl, StructMember, TypeRef,
};
use crate::diagnostic::Diagnostic;

pub fn monomorphize(program: &Program) -> Result<Program, Vec<Diagnostic>> {
    Monomorphizer::new(program).run(program)
}

struct Monomorphizer {
    templates: HashMap<String, StructDecl>,
    concrete_structs: HashSet<String>,
    pending: VecDeque<(String, Vec<TypeRef>)>,
    emitted: HashSet<String>,
    specializations: HashMap<String, (String, Vec<TypeRef>)>,
    diagnostics: Vec<Diagnostic>,
}

impl Monomorphizer {
    fn new(program: &Program) -> Self {
        let templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty()).then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let concrete_structs = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                item.type_params.is_empty().then(|| item.name.clone())
            })
            .collect();

        Self {
            templates,
            concrete_structs,
            pending: VecDeque::new(),
            emitted: HashSet::new(),
            specializations: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self, program: &Program) -> Result<Program, Vec<Diagnostic>> {
        self.validate_templates();
        for item in &program.items {
            if let Item::Enum(item) = item
                && !item.type_params.is_empty()
            {
                self.diagnostics.push(Diagnostic::error(
                    item.span,
                    "generic enums are not implemented yet",
                ));
            }
        }

        let mut items = Vec::new();
        for item in &program.items {
            if matches!(item, Item::Struct(item) if !item.type_params.is_empty()) {
                continue;
            }

            let mut item = item.clone();
            self.rewrite_item(&mut item, &HashMap::new());
            items.push(item);
        }

        while let Some((name, args)) = self.pending.pop_front() {
            let specialized_name = specialized_name(&name, &args);
            if !self.emitted.insert(specialized_name.clone()) {
                continue;
            }

            let Some(template) = self.templates.get(&name).cloned() else {
                continue;
            };
            let substitutions = template
                .type_params
                .iter()
                .cloned()
                .zip(args)
                .collect::<HashMap<_, _>>();
            let mut specialized = template;
            specialized.name = specialized_name;
            specialized.type_params.clear();
            for member in &mut specialized.members {
                match member {
                    StructMember::Field(field) => {
                        self.rewrite_type(&mut field.type_ref, &substitutions);
                    }
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        self.rewrite_function(function, &substitutions);
                    }
                }
            }
            items.push(Item::Struct(specialized));
        }

        prune_unused_generic_methods(&mut items, &self.emitted);

        if self.diagnostics.is_empty() {
            Ok(Program { items })
        } else {
            Err(self.diagnostics)
        }
    }

    fn validate_templates(&mut self) {
        for template in self.templates.values() {
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "duplicate type parameter `{name}` in struct `{}`",
                            template.name
                        ),
                    ));
                }
            }
        }
    }

    fn rewrite_item(&mut self, item: &mut Item, substitutions: &HashMap<String, TypeRef>) {
        match item {
            Item::Import(_) => {}
            Item::Enum(item) => {
                for variant in &mut item.variants {
                    if let Some(payload) = &mut variant.payload {
                        self.rewrite_type(payload, substitutions);
                    }
                }
            }
            Item::Struct(item) => {
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => {
                            self.rewrite_type(&mut field.type_ref, substitutions);
                        }
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            self.rewrite_function(function, substitutions);
                        }
                    }
                }
            }
            Item::Extension(item) => {
                self.rewrite_type(&mut item.type_ref, substitutions);
                self.rewrite_function(&mut item.function, substitutions);
            }
            Item::Function(function) => self.rewrite_function(function, substitutions),
        }
    }

    fn rewrite_function(
        &mut self,
        function: &mut FunctionDecl,
        substitutions: &HashMap<String, TypeRef>,
    ) {
        for param in &mut function.params {
            if let Some(type_ref) = &mut param.type_ref {
                self.rewrite_type(type_ref, substitutions);
            }
        }
        if let Some(return_type) = &mut function.return_type {
            self.rewrite_type(return_type, substitutions);
        }
        match &mut function.body {
            FunctionBody::Block(block) => self.rewrite_block(block, substitutions),
            FunctionBody::Expr(expr) => self.rewrite_expr(expr, substitutions),
        }
    }

    fn rewrite_block(&mut self, block: &mut Block, substitutions: &HashMap<String, TypeRef>) {
        for statement in &mut block.statements {
            self.rewrite_statement(statement, substitutions);
        }
    }

    fn rewrite_statement(
        &mut self,
        statement: &mut Stmt,
        substitutions: &HashMap<String, TypeRef>,
    ) {
        match &mut statement.kind {
            StmtKind::Let {
                type_annotation,
                value,
                ..
            } => {
                if let Some(type_ref) = type_annotation {
                    self.rewrite_type(type_ref, substitutions);
                    if let Some(value) = value {
                        self.apply_literal_context(value, type_ref);
                    }
                }
                if let Some(value) = value {
                    self.rewrite_expr(value, substitutions);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                self.rewrite_expr(target, substitutions);
                self.rewrite_expr(value, substitutions);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    self.rewrite_expr(value, substitutions);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.rewrite_expr(condition, substitutions);
                self.rewrite_block(then_branch, substitutions);
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.rewrite_block(block, substitutions),
                        ElseBranch::If(statement) => {
                            self.rewrite_statement(statement, substitutions);
                        }
                    }
                }
            }
            StmtKind::For { iterable, body, .. } => {
                self.rewrite_expr(iterable, substitutions);
                self.rewrite_block(body, substitutions);
            }
            StmtKind::Expr(expr) => self.rewrite_expr(expr, substitutions),
        }
    }

    fn rewrite_expr(&mut self, expr: &mut Expr, substitutions: &HashMap<String, TypeRef>) {
        if let ExprKind::GenericType { name, args } = &mut expr.kind {
            for arg in args.iter_mut() {
                self.rewrite_type(arg, substitutions);
            }
            if self.templates.contains_key(name) {
                self.specialize(name, args, expr.span);
                *name = specialized_name(name, args);
                expr.kind = ExprKind::Identifier(name.clone());
            } else if self.concrete_structs.contains(name) {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("struct `{name}` does not accept type arguments"),
                ));
            } else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown generic struct `{name}`"),
                ));
            }
            return;
        }

        match &mut expr.kind {
            ExprKind::Array(items) => {
                for item in items {
                    self.rewrite_expr(item, substitutions);
                }
            }
            ExprKind::Call { callee, args } => {
                self.rewrite_expr(callee, substitutions);
                for arg in args {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            ExprKind::Member { object, .. } => self.rewrite_expr(object, substitutions),
            ExprKind::StructInit { name, args, fields } => {
                for arg in args.iter_mut() {
                    self.rewrite_type(arg, substitutions);
                }
                if self.templates.contains_key(name) {
                    if args.is_empty() {
                        self.diagnostics.push(Diagnostic::error(
                            expr.span,
                            format!(
                                "cannot infer type arguments for generic struct `{name}`; write `{name}<Type> {{ ... }}` or add a concrete type annotation"
                            ),
                        ));
                    } else {
                        self.specialize(name, args, expr.span);
                        *name = specialized_name(name, args);
                        args.clear();
                    }
                } else if self.concrete_structs.contains(name) && !args.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("struct `{name}` does not accept type arguments"),
                    ));
                }
                for field in fields {
                    self.rewrite_expr(&mut field.value, substitutions);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left, substitutions);
                self.rewrite_expr(right, substitutions);
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.rewrite_expr(operand, substitutions);
            }
            ExprKind::Match { value, branches } => {
                self.rewrite_expr(value, substitutions);
                for branch in branches {
                    match &mut branch.body {
                        MatchBranchBody::Expr(expr) => self.rewrite_expr(expr, substitutions),
                        MatchBranchBody::Block(block) => self.rewrite_block(block, substitutions),
                    }
                }
            }
            ExprKind::Lambda(function) => self.rewrite_function(function, substitutions),
            ExprKind::Identifier(_)
            | ExprKind::GenericType { .. }
            | ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Bool(_)
            | ExprKind::Missing => {}
        }
    }

    fn rewrite_type(&mut self, type_ref: &mut TypeRef, substitutions: &HashMap<String, TypeRef>) {
        if type_ref.args.is_empty()
            && let Some(substitution) = substitutions.get(&type_ref.name)
        {
            let span = type_ref.span;
            *type_ref = substitution.clone();
            type_ref.span = span;
            return;
        }

        for arg in &mut type_ref.args {
            self.rewrite_type(arg, substitutions);
        }

        if self.templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        } else if self.concrete_structs.contains(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("struct `{}` does not accept type arguments", type_ref.name),
            ));
        }
    }

    fn apply_literal_context(&self, expr: &mut Expr, expected: &TypeRef) {
        let ExprKind::StructInit { name, args, .. } = &mut expr.kind else {
            return;
        };
        if !args.is_empty() {
            return;
        }
        let Some((generic_name, concrete_args)) = self.specializations.get(&expected.name) else {
            return;
        };
        if name == generic_name {
            *args = concrete_args.clone();
        }
    }

    fn specialize(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.templates[name].type_params.len();
        if args.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "generic struct `{name}` expects {expected} type arguments, got {}",
                    args.len()
                ),
            ));
            return;
        }

        self.pending.push_back((name.to_string(), args.to_vec()));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }
}

#[derive(Default)]
struct StructShape {
    fields: HashMap<String, String>,
    methods: HashMap<String, Option<String>>,
    static_methods: HashMap<String, Option<String>>,
}

struct MethodReachability<'items> {
    structs: HashMap<String, StructShape>,
    functions: HashMap<String, Option<String>>,
    generic_structs: &'items HashSet<String>,
    generic_methods: HashMap<(String, String, bool), FunctionDecl>,
    used: HashSet<(String, String, bool)>,
    pending: VecDeque<(String, String, bool)>,
}

impl<'items> MethodReachability<'items> {
    fn new(items: &[Item], generic_structs: &'items HashSet<String>) -> Self {
        let mut structs = HashMap::new();
        let mut functions = HashMap::new();
        let mut generic_methods = HashMap::new();

        for item in items {
            match item {
                Item::Struct(item) => {
                    let fields = item
                        .members
                        .iter()
                        .filter_map(|member| {
                            let StructMember::Field(field) = member else {
                                return None;
                            };
                            concrete_type_name(&field.type_ref)
                                .map(|type_| (field.name.clone(), type_))
                        })
                        .collect();
                    let methods = item
                        .members
                        .iter()
                        .filter_map(|member| {
                            let function = match member {
                                StructMember::Method(function) => function,
                                StructMember::Field(_) | StructMember::StaticMethod(_) => {
                                    return None;
                                }
                            };
                            let name = function.name.clone()?;
                            Some((
                                name.clone(),
                                function.return_type.as_ref().and_then(|type_ref| {
                                    if type_ref.name == "Self" && type_ref.args.is_empty() {
                                        Some(item.name.clone())
                                    } else {
                                        concrete_type_name(type_ref)
                                    }
                                }),
                            ))
                        })
                        .collect();
                    let static_methods = item
                        .members
                        .iter()
                        .filter_map(|member| {
                            let StructMember::StaticMethod(function) = member else {
                                return None;
                            };
                            let name = function.name.clone()?;
                            Some((
                                name,
                                function.return_type.as_ref().and_then(|type_ref| {
                                    if type_ref.name == "Self" && type_ref.args.is_empty() {
                                        Some(item.name.clone())
                                    } else {
                                        concrete_type_name(type_ref)
                                    }
                                }),
                            ))
                        })
                        .collect();
                    if generic_structs.contains(&item.name) {
                        for member in &item.members {
                            match member {
                                StructMember::Method(function) => {
                                    if let Some(name) = &function.name {
                                        generic_methods.insert(
                                            (item.name.clone(), name.clone(), false),
                                            function.clone(),
                                        );
                                    }
                                }
                                StructMember::StaticMethod(function) => {
                                    if let Some(name) = &function.name {
                                        generic_methods.insert(
                                            (item.name.clone(), name.clone(), true),
                                            function.clone(),
                                        );
                                    }
                                }
                                StructMember::Field(_) => {}
                            }
                        }
                    }
                    structs.insert(
                        item.name.clone(),
                        StructShape {
                            fields,
                            methods,
                            static_methods,
                        },
                    );
                }
                Item::Function(function) => {
                    if let Some(name) = &function.name {
                        functions.insert(
                            name.clone(),
                            function.return_type.as_ref().and_then(concrete_type_name),
                        );
                    }
                }
                Item::Import(_) | Item::Enum(_) | Item::Extension(_) => {}
            }
        }

        Self {
            structs,
            functions,
            generic_structs,
            generic_methods,
            used: HashSet::new(),
            pending: VecDeque::new(),
        }
    }

    fn find(mut self, items: &[Item]) -> HashSet<(String, String, bool)> {
        for item in items {
            match item {
                Item::Struct(item) if !self.generic_structs.contains(&item.name) => {
                    for member in &item.members {
                        match member {
                            StructMember::Method(function) => {
                                self.visit_function(function, Some(&item.name), true);
                            }
                            StructMember::StaticMethod(function) => {
                                self.visit_function(function, Some(&item.name), false);
                            }
                            StructMember::Field(_) => {}
                        }
                    }
                }
                Item::Function(function) => self.visit_function(function, None, false),
                Item::Extension(item) => self.visit_function(&item.function, None, false),
                Item::Import(_) | Item::Enum(_) | Item::Struct(_) => {}
            }
        }

        while let Some(key) = self.pending.pop_front() {
            let Some(function) = self.generic_methods.get(&key).cloned() else {
                continue;
            };
            self.visit_function(&function, Some(&key.0), !key.2);
        }

        self.used
    }

    fn visit_function(
        &mut self,
        function: &FunctionDecl,
        owner_type: Option<&str>,
        has_self: bool,
    ) {
        let mut locals = HashMap::new();
        if let Some(owner_type) = owner_type {
            locals.insert("Self".to_string(), owner_type.to_string());
            if has_self {
                locals.insert("self".to_string(), owner_type.to_string());
            }
        }
        for param in &function.params {
            if let Some(type_ref) = &param.type_ref
                && let Some(type_) = self.type_name(type_ref)
            {
                locals.insert(param.name.clone(), type_);
            }
        }

        match &function.body {
            FunctionBody::Block(block) => self.visit_block(block, &mut locals),
            FunctionBody::Expr(expr) => {
                self.visit_expr(expr, &mut locals);
            }
        }
    }

    fn visit_block(&mut self, block: &Block, locals: &mut HashMap<String, String>) {
        for statement in &block.statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    type_annotation,
                    value,
                    ..
                } => {
                    let value_type = value
                        .as_ref()
                        .and_then(|value| self.visit_expr(value, locals));
                    let type_ = type_annotation
                        .as_ref()
                        .and_then(|type_ref| self.type_name(type_ref))
                        .or(value_type);
                    if let Some(type_) = type_ {
                        locals.insert(name.clone(), type_);
                    }
                }
                StmtKind::Assign { target, value, .. } => {
                    self.visit_expr(target, locals);
                    self.visit_expr(value, locals);
                }
                StmtKind::Return { value } => {
                    if let Some(value) = value {
                        self.visit_expr(value, locals);
                    }
                }
                StmtKind::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    self.visit_expr(condition, locals);
                    self.visit_block(then_branch, &mut locals.clone());
                    if let Some(else_branch) = else_branch {
                        match else_branch {
                            ElseBranch::Block(block) => {
                                self.visit_block(block, &mut locals.clone());
                            }
                            ElseBranch::If(statement) => {
                                let block = Block {
                                    statements: vec![(**statement).clone()],
                                    span: statement.span,
                                };
                                self.visit_block(&block, &mut locals.clone());
                            }
                        }
                    }
                }
                StmtKind::For { iterable, body, .. } => {
                    self.visit_expr(iterable, locals);
                    self.visit_block(body, &mut locals.clone());
                }
                StmtKind::Expr(expr) => {
                    self.visit_expr(expr, locals);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr, locals: &mut HashMap<String, String>) -> Option<String> {
        match &expr.kind {
            ExprKind::Identifier(name) => locals.get(name).cloned(),
            ExprKind::StructInit { name, fields, .. } => {
                for field in fields {
                    self.visit_expr(&field.value, locals);
                }
                self.structs.contains_key(name).then(|| name.clone())
            }
            ExprKind::Member { object, name } => {
                let object_type = self.visit_expr(object, locals)?;
                self.structs.get(&object_type)?.fields.get(name).cloned()
            }
            ExprKind::Call { callee, args } => {
                for arg in args {
                    self.visit_expr(arg, locals);
                }
                match &callee.kind {
                    ExprKind::Identifier(name) => self.functions.get(name).cloned().flatten(),
                    ExprKind::Member { object, name } => {
                        if let ExprKind::Identifier(identifier) = &object.kind
                            && (identifier == "Self" || !locals.contains_key(identifier))
                        {
                            let type_name = if identifier == "Self" {
                                locals.get(identifier).map(String::as_str)
                            } else {
                                Some(identifier.as_str())
                            };
                            let Some(type_name) =
                                type_name.filter(|name| self.structs.contains_key(*name))
                            else {
                                return None;
                            };
                            self.use_method(type_name, name, true);
                            return self
                                .structs
                                .get(type_name)
                                .and_then(|shape| shape.static_methods.get(name))
                                .cloned()
                                .flatten();
                        }
                        let object_type = self.visit_expr(object, locals);
                        if name == "clone" {
                            return object_type;
                        }
                        if let Some(object_type) = object_type {
                            self.use_method(&object_type, name, false);
                            self.structs
                                .get(&object_type)
                                .and_then(|shape| shape.methods.get(name))
                                .cloned()
                                .flatten()
                        } else {
                            self.use_matching_methods(name);
                            None
                        }
                    }
                    _ => {
                        self.visit_expr(callee, locals);
                        None
                    }
                }
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.visit_expr(item, locals);
                }
                None
            }
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left, locals);
                self.visit_expr(right, locals);
                None
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.visit_expr(operand, locals)
            }
            ExprKind::Match { value, branches } => {
                self.visit_expr(value, locals);
                let mut type_ = None;
                for branch in branches {
                    let branch_type = match &branch.body {
                        MatchBranchBody::Expr(expr) => self.visit_expr(expr, locals),
                        MatchBranchBody::Block(block) => {
                            self.visit_block(block, &mut locals.clone());
                            None
                        }
                    };
                    if type_.is_none() {
                        type_ = branch_type;
                    }
                }
                type_
            }
            ExprKind::Lambda(function) => {
                self.visit_function(function, None, false);
                None
            }
            ExprKind::String(_) => Some("String".to_string()),
            ExprKind::Number(value) => Some(
                if crate::ast::number_literal_is_float(value) {
                    "f64"
                } else {
                    "i32"
                }
                .to_string(),
            ),
            ExprKind::Bool(_) => Some("bool".to_string()),
            ExprKind::GenericType { .. } | ExprKind::Missing => None,
        }
    }

    fn use_method(&mut self, struct_name: &str, method_name: &str, static_: bool) {
        let key = (struct_name.to_string(), method_name.to_string(), static_);
        if self.generic_methods.contains_key(&key) && self.used.insert(key.clone()) {
            self.pending.push_back(key);
        }
    }

    fn use_matching_methods(&mut self, method_name: &str) {
        let matching = self
            .generic_methods
            .keys()
            .filter(|(_, name, static_)| name == method_name && !static_)
            .cloned()
            .collect::<Vec<_>>();
        for (struct_name, method_name, static_) in matching {
            self.use_method(&struct_name, &method_name, static_);
        }
    }

    fn type_name(&self, type_ref: &TypeRef) -> Option<String> {
        concrete_type_name(type_ref)
    }
}

fn prune_unused_generic_methods(items: &mut [Item], generic_structs: &HashSet<String>) {
    let used = MethodReachability::new(items, generic_structs).find(items);
    for item in items {
        let Item::Struct(item) = item else {
            continue;
        };
        if !generic_structs.contains(&item.name) {
            continue;
        }
        item.members.retain(|member| match member {
            StructMember::Method(function) => function
                .name
                .as_ref()
                .is_some_and(|name| used.contains(&(item.name.clone(), name.clone(), false))),
            StructMember::StaticMethod(function) => function
                .name
                .as_ref()
                .is_some_and(|name| used.contains(&(item.name.clone(), name.clone(), true))),
            StructMember::Field(_) => true,
        });
    }
}

fn concrete_type_name(type_ref: &TypeRef) -> Option<String> {
    type_ref.args.is_empty().then(|| type_ref.name.clone())
}

fn specialized_name(name: &str, args: &[TypeRef]) -> String {
    let args = args.iter().map(type_name).collect::<Vec<_>>().join(", ");
    format!("{name}<{args}>")
}

fn type_name(type_ref: &TypeRef) -> String {
    if type_ref.args.is_empty() {
        type_ref.name.clone()
    } else {
        specialized_name(&type_ref.name, &type_ref.args)
    }
}
