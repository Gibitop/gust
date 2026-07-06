use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    Block, ElseBranch, EnumDecl, Expr, ExprKind, FunctionBody, FunctionDecl, Item, MatchBranchBody,
    Pattern, Program, Stmt, StmtKind, StructDecl, StructMember, TypeRef,
};
use crate::diagnostic::Diagnostic;

pub fn monomorphize(program: &Program) -> Result<Program, Vec<Diagnostic>> {
    Monomorphizer::new(program).run(program)
}

struct Monomorphizer {
    struct_templates: HashMap<String, StructDecl>,
    enum_templates: HashMap<String, EnumDecl>,
    concrete_structs: HashSet<String>,
    concrete_enums: HashMap<String, EnumDecl>,
    pending: VecDeque<(GenericKind, String, Vec<TypeRef>)>,
    emitted: HashSet<String>,
    specializations: HashMap<String, (String, Vec<TypeRef>)>,
    scopes: Vec<HashMap<String, TypeRef>>,
    return_types: Vec<TypeRef>,
    self_types: Vec<TypeRef>,
    inferred_returns: Vec<Option<Vec<TypeRef>>>,
    member_returns: HashMap<(String, String, bool), TypeRef>,
    function_returns: HashMap<String, TypeRef>,
    function_params: HashMap<String, Vec<Option<TypeRef>>>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy)]
enum GenericKind {
    Struct,
    Enum,
}

impl Monomorphizer {
    fn new(program: &Program) -> Self {
        let struct_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty()).then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let enum_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Enum(item) = item else {
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
        let concrete_enums = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Enum(item) = item else {
                    return None;
                };
                item.type_params
                    .is_empty()
                    .then(|| (item.name.clone(), item.clone()))
            })
            .collect();
        let function_returns = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(function) = item else {
                    return None;
                };
                Some((
                    function.name.clone()?,
                    function.return_type.as_ref()?.clone(),
                ))
            })
            .collect();
        let function_params = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(function) = item else {
                    return None;
                };
                Some((
                    function.name.clone()?,
                    function
                        .params
                        .iter()
                        .map(|param| param.type_ref.clone())
                        .collect(),
                ))
            })
            .collect();

        Self {
            struct_templates,
            enum_templates,
            concrete_structs,
            concrete_enums,
            pending: VecDeque::new(),
            emitted: HashSet::new(),
            specializations: HashMap::new(),
            scopes: Vec::new(),
            return_types: Vec::new(),
            self_types: Vec::new(),
            inferred_returns: Vec::new(),
            member_returns: HashMap::new(),
            function_returns,
            function_params,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self, program: &Program) -> Result<Program, Vec<Diagnostic>> {
        self.validate_templates();

        let mut items = Vec::new();
        for item in &program.items {
            if matches!(item, Item::Struct(item) if !item.type_params.is_empty())
                || matches!(item, Item::Enum(item) if !item.type_params.is_empty())
            {
                continue;
            }

            let mut item = item.clone();
            self.rewrite_item(&mut item, &HashMap::new());
            items.push(item);
        }

        while let Some((kind, name, args)) = self.pending.pop_front() {
            let specialized_name = specialized_name(&name, &args);
            if !self.emitted.insert(specialized_name.clone()) {
                continue;
            }

            match kind {
                GenericKind::Struct => {
                    let Some(template) = self.struct_templates.get(&name).cloned() else {
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
                    self.self_types.push(TypeRef {
                        name: specialized.name.clone(),
                        args: Vec::new(),
                        span: specialized.span,
                    });
                    for member in &mut specialized.members {
                        match member {
                            StructMember::Field(field) => {
                                self.rewrite_type(&mut field.type_ref, &substitutions);
                            }
                            StructMember::Method(function)
                            | StructMember::StaticMethod(function) => {
                                self.rewrite_function(function, &substitutions);
                            }
                        }
                    }
                    self.infer_specialized_member_returns(&mut specialized);
                    self.self_types.pop();
                    items.push(Item::Struct(specialized));
                }
                GenericKind::Enum => {
                    let Some(template) = self.enum_templates.get(&name).cloned() else {
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
                    for variant in &mut specialized.variants {
                        if let Some(payload) = &mut variant.payload {
                            self.rewrite_type(payload, &substitutions);
                        }
                    }
                    self.concrete_enums
                        .insert(specialized.name.clone(), specialized.clone());
                    items.push(Item::Enum(specialized));
                }
            }
        }

        prune_unused_generic_methods(&mut items, &self.emitted);

        if self.diagnostics.is_empty() {
            Ok(Program { items })
        } else {
            Err(self.diagnostics)
        }
    }

    fn validate_templates(&mut self) {
        for template in self.struct_templates.values() {
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
        for template in self.enum_templates.values() {
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "duplicate type parameter `{name}` in enum `{}`",
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
                self.self_types.push(TypeRef {
                    name: item.name.clone(),
                    args: Vec::new(),
                    span: item.span,
                });
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
                self.self_types.pop();
            }
            Item::Extension(item) => {
                self.rewrite_type(&mut item.type_ref, substitutions);
                self.rewrite_function(&mut item.function, substitutions);
            }
            Item::Function(function) => {
                self.rewrite_function(function, substitutions);
                if let (Some(name), Some(return_type)) = (&function.name, &function.return_type) {
                    self.function_returns
                        .insert(name.clone(), return_type.clone());
                }
                if let Some(name) = &function.name {
                    self.function_params.insert(
                        name.clone(),
                        function
                            .params
                            .iter()
                            .map(|param| param.type_ref.clone())
                            .collect(),
                    );
                }
            }
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
        let mut function_scope = function
            .params
            .iter()
            .filter_map(|param| {
                param
                    .type_ref
                    .as_ref()
                    .map(|type_ref| (param.name.clone(), type_ref.clone()))
            })
            .collect::<HashMap<_, _>>();
        if let Some(self_type) = self.self_types.last() {
            function_scope.insert("Self".to_string(), self_type.clone());
            function_scope.insert("self".to_string(), self_type.clone());
        }
        self.scopes.push(function_scope);
        let had_explicit_return = function.return_type.is_some();
        if let Some(return_type) = &function.return_type {
            self.return_types.push(return_type.clone());
        }
        let infer_return = !had_explicit_return
            && self
                .self_types
                .last()
                .is_some_and(|type_ref| self.specializations.contains_key(&type_ref.name));
        self.inferred_returns
            .push(infer_return.then(Vec::<TypeRef>::new));
        match &mut function.body {
            FunctionBody::Block(block) => self.rewrite_block(block, substitutions),
            FunctionBody::Expr(expr) => {
                if let Some(return_type) = self.return_types.last() {
                    self.apply_expr_context(expr, return_type);
                }
                self.rewrite_expr(expr, substitutions);
                if infer_return
                    && let Some(type_ref) = self.infer_expr_type(expr)
                    && let Some(Some(return_types)) = self.inferred_returns.last_mut()
                {
                    return_types.push(type_ref);
                }
            }
        }
        if let Some(Some(return_types)) = self.inferred_returns.pop()
            && let Some(return_type) = consistent_type(&return_types)
        {
            function.return_type = Some(return_type);
        }
        if had_explicit_return {
            self.return_types.pop();
        }
        self.scopes.pop();
    }

    fn rewrite_block(&mut self, block: &mut Block, substitutions: &HashMap<String, TypeRef>) {
        self.scopes.push(HashMap::new());
        for statement in &mut block.statements {
            self.rewrite_statement(statement, substitutions);
        }
        self.scopes.pop();
    }

    fn rewrite_statement(
        &mut self,
        statement: &mut Stmt,
        substitutions: &HashMap<String, TypeRef>,
    ) {
        match &mut statement.kind {
            StmtKind::Let {
                name,
                type_annotation,
                value,
                ..
            } => {
                if let Some(type_ref) = type_annotation {
                    self.rewrite_type(type_ref, substitutions);
                    if let Some(value) = value {
                        self.apply_expr_context(value, type_ref);
                    }
                }
                if let Some(value) = value {
                    self.rewrite_expr(value, substitutions);
                }
                let inferred_type = type_annotation
                    .clone()
                    .or_else(|| value.as_ref().and_then(|value| self.infer_expr_type(value)));
                if let Some(type_ref) = inferred_type
                    && let Some(scope) = self.scopes.last_mut()
                {
                    scope.insert(name.clone(), type_ref);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                if let Some(expected) = self.infer_expr_type(target) {
                    self.apply_expr_context(value, &expected);
                }
                self.rewrite_expr(target, substitutions);
                self.rewrite_expr(value, substitutions);
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    if let Some(return_type) = self.return_types.last() {
                        self.apply_expr_context(value, return_type);
                    }
                    self.rewrite_expr(value, substitutions);
                    if let Some(type_ref) = self.infer_expr_type(value)
                        && let Some(Some(return_types)) = self.inferred_returns.last_mut()
                    {
                        return_types.push(type_ref);
                    }
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
            if self.struct_templates.contains_key(name) {
                self.specialize_struct(name, args, expr.span);
                *name = specialized_name(name, args);
                expr.kind = ExprKind::Identifier(name.clone());
            } else if self.enum_templates.contains_key(name) {
                self.specialize_enum(name, args, expr.span);
                *name = specialized_name(name, args);
                expr.kind = ExprKind::Identifier(name.clone());
            } else if self.concrete_structs.contains(name) {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("struct `{name}` does not accept type arguments"),
                ));
            } else if self.concrete_enums.contains_key(name) {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("enum `{name}` does not accept type arguments"),
                ));
            } else {
                self.diagnostics.push(Diagnostic::error(
                    expr.span,
                    format!("unknown generic type `{name}`"),
                ));
            }
            return;
        }

        let generic_variant_call = match &expr.kind {
            ExprKind::Call { callee, .. } => {
                let ExprKind::Member { object, name } = &callee.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                let ExprKind::Identifier(type_name) = &object.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                (self.enum_templates.contains_key(type_name)
                    && self.lookup_local_type(type_name).is_none())
                .then(|| (type_name.clone(), name.clone()))
            }
            _ => None,
        };
        if let Some((type_name, variant_name)) = generic_variant_call {
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic variant call was matched above")
            };
            for arg in args.iter_mut() {
                self.rewrite_expr(arg, substitutions);
            }
            match self.infer_enum_type_arguments(&type_name, &variant_name, args) {
                Ok(mut type_args) => {
                    for type_arg in &mut type_args {
                        self.rewrite_type(type_arg, substitutions);
                    }
                    self.specialize_enum(&type_name, &type_args, expr.span);
                    let ExprKind::Member { object, .. } = &mut callee.kind else {
                        unreachable!("generic variant call requires a member callee")
                    };
                    object.kind = ExprKind::Identifier(specialized_name(&type_name, &type_args));
                }
                Err(message) => self.diagnostics.push(Diagnostic::error(expr.span, message)),
            }
            return;
        }

        if let ExprKind::Member { object, name } = &expr.kind
            && let ExprKind::Identifier(type_name) = &object.kind
            && self.enum_templates.contains_key(type_name)
            && self.lookup_local_type(type_name).is_none()
        {
            let message = self
                .infer_enum_type_arguments(type_name, name, &[])
                .err()
                .unwrap_or_else(|| {
                    format!(
                        "cannot infer type arguments for generic enum `{type_name}`; write `{type_name}<Type>.{name}` or add a concrete expected type"
                    )
                });
            self.diagnostics.push(Diagnostic::error(expr.span, message));
            return;
        }

        let generic_static_call = match &expr.kind {
            ExprKind::Call { callee, .. } => {
                let ExprKind::Member { object, name } = &callee.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                let ExprKind::Identifier(type_name) = &object.kind else {
                    return self.rewrite_expr_children(expr, substitutions);
                };
                (self.struct_templates.contains_key(type_name)
                    && self.lookup_local_type(type_name).is_none())
                .then(|| (type_name.clone(), name.clone()))
            }
            _ => None,
        };
        if let Some((type_name, method_name)) = generic_static_call {
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic static call was matched above")
            };
            for arg in args.iter_mut() {
                self.rewrite_expr(arg, substitutions);
            }
            match self.infer_static_type_arguments(&type_name, &method_name, args) {
                Ok(mut type_args) => {
                    for type_arg in &mut type_args {
                        self.rewrite_type(type_arg, substitutions);
                    }
                    self.specialize_struct(&type_name, &type_args, expr.span);
                    let ExprKind::Member { object, .. } = &mut callee.kind else {
                        unreachable!("generic static call requires a member callee")
                    };
                    object.kind = ExprKind::Identifier(specialized_name(&type_name, &type_args));
                }
                Err(message) => self.diagnostics.push(Diagnostic::error(expr.span, message)),
            }
            return;
        }

        self.rewrite_expr_children(expr, substitutions);
    }

    fn rewrite_expr_children(&mut self, expr: &mut Expr, substitutions: &HashMap<String, TypeRef>) {
        match &mut expr.kind {
            ExprKind::Array(items) => {
                for item in items {
                    self.rewrite_expr(item, substitutions);
                }
            }
            ExprKind::Call { callee, args } => {
                self.rewrite_expr(callee, substitutions);
                let function_contexts = if let ExprKind::Identifier(name) = &callee.kind {
                    self.function_params.get(name).cloned()
                } else {
                    None
                };
                let payload_context = if let ExprKind::Member { object, name } = &callee.kind
                    && let ExprKind::Identifier(enum_name) = &object.kind
                {
                    self.enum_variant_payload(enum_name, name).flatten()
                } else {
                    None
                };
                if let (Some(mut expected), Some(arg)) = (payload_context, args.first_mut()) {
                    self.rewrite_type(&mut expected, substitutions);
                    self.apply_expr_context(arg, &expected);
                }
                if let Some(contexts) = function_contexts {
                    for (arg, expected) in args.iter_mut().zip(contexts) {
                        let Some(mut expected) = expected else {
                            continue;
                        };
                        self.rewrite_type(&mut expected, substitutions);
                        self.apply_expr_context(arg, &expected);
                    }
                }
                for arg in args {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            ExprKind::Member { object, .. } => self.rewrite_expr(object, substitutions),
            ExprKind::StructInit { name, args, fields } => {
                for arg in args.iter_mut() {
                    self.rewrite_type(arg, substitutions);
                }
                if self.struct_templates.contains_key(name) && !args.is_empty() {
                    self.apply_struct_field_contexts(name, args, fields, substitutions);
                }
                for field in fields.iter_mut() {
                    self.rewrite_expr(&mut field.value, substitutions);
                }
                if self.struct_templates.contains_key(name) {
                    if args.is_empty() {
                        match self.infer_struct_type_arguments(name, fields) {
                            Ok(mut inferred_args) => {
                                for inferred_arg in &mut inferred_args {
                                    self.rewrite_type(inferred_arg, substitutions);
                                }
                                *args = inferred_args;
                            }
                            Err(message) => {
                                self.diagnostics.push(Diagnostic::error(expr.span, message));
                            }
                        }
                    }
                    if !args.is_empty() {
                        self.specialize_struct(name, args, expr.span);
                        *name = specialized_name(name, args);
                        args.clear();
                    } else {
                        return;
                    }
                } else if self.concrete_structs.contains(name) && !args.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        format!("struct `{name}` does not accept type arguments"),
                    ));
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
                let mut value_type = self.infer_expr_type(value);
                if let Some(type_ref) = &mut value_type
                    && self.enum_templates.contains_key(&type_ref.name)
                    && !type_ref.args.is_empty()
                {
                    let name = type_ref.name.clone();
                    self.specialize_enum(&name, &type_ref.args, type_ref.span);
                    type_ref.name = specialized_name(&name, &type_ref.args);
                    type_ref.args.clear();
                }
                for branch in branches {
                    let mut scope = HashMap::new();
                    if let Pattern::Variant {
                        enum_name,
                        variant,
                        binding,
                        ..
                    } = &mut branch.pattern
                    {
                        if let Some(value_type) = &value_type
                            && let Some((generic_name, _)) =
                                self.specializations.get(&value_type.name)
                            && enum_name == generic_name
                            && self.enum_templates.contains_key(generic_name)
                        {
                            *enum_name = value_type.name.clone();
                        }
                        if let Some(binding) = binding
                            && binding != "_"
                            && let Some(Some(payload)) =
                                self.enum_variant_payload(enum_name, variant)
                        {
                            scope.insert(binding.clone(), payload);
                        }
                    }
                    self.scopes.push(scope);
                    match &mut branch.body {
                        MatchBranchBody::Expr(expr) => self.rewrite_expr(expr, substitutions),
                        MatchBranchBody::Block(block) => self.rewrite_block(block, substitutions),
                    }
                    self.scopes.pop();
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

        if self.struct_templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize_struct(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        } else if self.enum_templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize_enum(&name, &type_ref.args, type_ref.span);
            type_ref.name = specialized_name(&name, &type_ref.args);
            type_ref.args.clear();
        } else if self.concrete_structs.contains(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("struct `{}` does not accept type arguments", type_ref.name),
            ));
        } else if self.concrete_enums.contains_key(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("enum `{}` does not accept type arguments", type_ref.name),
            ));
        }
    }

    fn infer_specialized_member_returns(&mut self, struct_: &mut StructDecl) {
        for _ in 0..struct_.members.len() {
            let mut changed = false;
            for member in &struct_.members {
                let (function, static_) = match member {
                    StructMember::Method(function) => (function, false),
                    StructMember::StaticMethod(function) => (function, true),
                    StructMember::Field(_) => continue,
                };
                if let Some(name) = &function.name
                    && let Some(return_type) = &function.return_type
                {
                    self.member_returns.insert(
                        (struct_.name.clone(), name.clone(), static_),
                        return_type.clone(),
                    );
                }
            }
            for member in &mut struct_.members {
                let (function, static_) = match member {
                    StructMember::Method(function) => (function, false),
                    StructMember::StaticMethod(function) => (function, true),
                    StructMember::Field(_) => continue,
                };
                if function.return_type.is_some() {
                    continue;
                }
                let Some(return_type) =
                    self.infer_rewritten_function_return(function, &struct_.name, !static_)
                else {
                    continue;
                };
                function.return_type = Some(return_type.clone());
                if let Some(name) = &function.name {
                    self.member_returns
                        .insert((struct_.name.clone(), name.clone(), static_), return_type);
                }
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    fn infer_rewritten_function_return(
        &mut self,
        function: &FunctionDecl,
        self_type: &str,
        has_self: bool,
    ) -> Option<TypeRef> {
        let self_type = TypeRef {
            name: self_type.to_string(),
            args: Vec::new(),
            span: function.span,
        };
        let mut scope = function
            .params
            .iter()
            .filter_map(|param| {
                param
                    .type_ref
                    .as_ref()
                    .map(|type_ref| (param.name.clone(), type_ref.clone()))
            })
            .collect::<HashMap<_, _>>();
        scope.insert("Self".to_string(), self_type.clone());
        if has_self {
            scope.insert("self".to_string(), self_type);
        }
        self.scopes.push(scope);
        let return_types = match &function.body {
            FunctionBody::Expr(expr) => self.infer_expr_type(expr).into_iter().collect(),
            FunctionBody::Block(block) => self.infer_block_return_types(block),
        };
        self.scopes.pop();
        consistent_type(&return_types)
    }

    fn infer_block_return_types(&mut self, block: &Block) -> Vec<TypeRef> {
        self.scopes.push(HashMap::new());
        let mut return_types = Vec::new();
        for statement in &block.statements {
            match &statement.kind {
                StmtKind::Let {
                    name,
                    type_annotation,
                    value,
                    ..
                } => {
                    let type_ref = type_annotation
                        .clone()
                        .or_else(|| value.as_ref().and_then(|value| self.infer_expr_type(value)));
                    if let Some(type_ref) = type_ref
                        && let Some(scope) = self.scopes.last_mut()
                    {
                        scope.insert(name.clone(), type_ref);
                    }
                }
                StmtKind::Return { value: Some(value) } => {
                    if let Some(type_ref) = self.infer_expr_type(value) {
                        return_types.push(type_ref);
                    }
                }
                StmtKind::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    return_types.extend(self.infer_block_return_types(then_branch));
                    if let Some(else_branch) = else_branch {
                        match else_branch {
                            ElseBranch::Block(block) => {
                                return_types.extend(self.infer_block_return_types(block));
                            }
                            ElseBranch::If(statement) => {
                                let block = Block {
                                    statements: vec![(**statement).clone()],
                                    span: statement.span,
                                };
                                return_types.extend(self.infer_block_return_types(&block));
                            }
                        }
                    }
                }
                StmtKind::For { body, .. } => {
                    return_types.extend(self.infer_block_return_types(body));
                }
                StmtKind::Assign { .. } | StmtKind::Return { value: None } | StmtKind::Expr(_) => {}
            }
        }
        self.scopes.pop();
        return_types
    }

    fn apply_expr_context(&self, expr: &mut Expr, expected: &TypeRef) {
        let Some((generic_name, concrete_args)) = self.specializations.get(&expected.name) else {
            return;
        };
        match &mut expr.kind {
            ExprKind::StructInit { name, args, .. } if args.is_empty() && name == generic_name => {
                *args = concrete_args.clone();
            }
            ExprKind::Member { object, .. } => {
                if let ExprKind::Identifier(name) = &object.kind
                    && name == generic_name
                {
                    object.kind = ExprKind::GenericType {
                        name: name.clone(),
                        args: concrete_args.clone(),
                    };
                }
            }
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Member { object, .. } = &mut callee.kind
                    && let ExprKind::Identifier(name) = &object.kind
                    && name == generic_name
                {
                    object.kind = ExprKind::GenericType {
                        name: name.clone(),
                        args: concrete_args.clone(),
                    };
                }
            }
            _ => {}
        }
    }

    fn apply_struct_field_contexts(
        &mut self,
        name: &str,
        args: &[TypeRef],
        fields: &mut [crate::ast::StructInitField],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        let template = self.struct_templates[name].clone();
        let field_substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for field in fields {
            let Some(mut expected) = template.members.iter().find_map(|member| {
                let StructMember::Field(expected) = member else {
                    return None;
                };
                (expected.name == field.name)
                    .then(|| substitute_type(&expected.type_ref, &field_substitutions))
            }) else {
                continue;
            };
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(&mut field.value, &expected);
        }
    }

    fn infer_struct_type_arguments(
        &self,
        name: &str,
        fields: &[crate::ast::StructInitField],
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.struct_templates[name];
        let mut constraints = Vec::new();
        for field in fields {
            let Some(expected) = template.members.iter().find_map(|member| {
                let StructMember::Field(expected) = member else {
                    return None;
                };
                (expected.name == field.name).then_some(&expected.type_ref)
            }) else {
                continue;
            };
            if let Some(actual) = self.infer_expr_type(&field.value) {
                constraints.push((expected.clone(), actual));
            }
        }
        self.solve_type_arguments(name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic struct `{name}`: {reason}; write `{name}<Type> {{ ... }}` or add a concrete type annotation"
                )
            })
    }

    fn infer_static_type_arguments(
        &self,
        type_name: &str,
        method_name: &str,
        args: &[Expr],
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.struct_templates[type_name];
        let Some(function) = template.members.iter().find_map(|member| {
            let StructMember::StaticMethod(function) = member else {
                return None;
            };
            (function.name.as_deref() == Some(method_name)).then_some(function)
        }) else {
            return Err(format!(
                "unknown static function `{method_name}` for generic struct `{type_name}`"
            ));
        };
        let constraints = function
            .params
            .iter()
            .filter_map(|param| param.type_ref.as_ref())
            .zip(args)
            .filter_map(|(expected, arg)| {
                self.infer_expr_type(arg)
                    .map(|actual| (expected.clone(), actual))
            })
            .collect();
        self.solve_type_arguments(type_name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic static call `{type_name}.{method_name}`: {reason}; write `{type_name}<Type>.{method_name}(...)` or add a concrete expected type"
                )
            })
    }

    fn infer_enum_type_arguments(
        &self,
        type_name: &str,
        variant_name: &str,
        args: &[Expr],
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.enum_templates[type_name];
        let Some(variant) = template
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
        else {
            return Err(format!("unknown variant `{type_name}.{variant_name}`"));
        };
        let expected_count = usize::from(variant.payload.is_some());
        if args.len() != expected_count {
            return Err(format!(
                "enum variant `{type_name}.{variant_name}` expects {expected_count} arguments, got {}",
                args.len()
            ));
        }
        let constraints = variant
            .payload
            .iter()
            .zip(args)
            .filter_map(|(expected, arg)| {
                self.infer_expr_type(arg)
                    .map(|actual| (expected.clone(), actual))
            })
            .collect();
        self.solve_type_arguments(type_name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic enum `{type_name}`: {reason}; write `{type_name}<Type>.{variant_name}(...)` or add a concrete expected type"
                )
            })
    }

    fn solve_type_arguments(
        &self,
        _type_name: &str,
        params: &[String],
        constraints: Vec<(TypeRef, TypeRef)>,
    ) -> Result<Vec<TypeRef>, String> {
        let param_names = params.iter().cloned().collect::<HashSet<_>>();
        let mut inferred = HashMap::new();
        for (expected, actual) in constraints {
            self.unify_type(&expected, &actual, &param_names, &mut inferred)?;
        }
        let missing = params
            .iter()
            .filter(|param| !inferred.contains_key(*param))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "no concrete type was found for {}",
                missing
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(params
            .iter()
            .map(|param| inferred[param].clone())
            .collect::<Vec<_>>())
    }

    fn unify_type(
        &self,
        expected: &TypeRef,
        actual: &TypeRef,
        params: &HashSet<String>,
        inferred: &mut HashMap<String, TypeRef>,
    ) -> Result<(), String> {
        let actual = self.expanded_type(actual);
        if expected.args.is_empty() && params.contains(&expected.name) {
            if let Some(previous) = inferred.get(&expected.name)
                && type_name(&self.expanded_type(previous)) != type_name(&actual)
            {
                return Err(format!(
                    "conflicting types `{}` and `{}` were inferred for `{}`",
                    type_name(&self.expanded_type(previous)),
                    type_name(&actual),
                    expected.name
                ));
            }
            inferred.insert(expected.name.clone(), actual);
            return Ok(());
        }

        let expected = self.expanded_type(expected);
        if expected.name != actual.name || expected.args.len() != actual.args.len() {
            return Ok(());
        }
        for (expected, actual) in expected.args.iter().zip(&actual.args) {
            self.unify_type(expected, actual, params, inferred)?;
        }
        Ok(())
    }

    fn infer_expr_type(&self, expr: &Expr) -> Option<TypeRef> {
        let inferred = |name: &str| TypeRef {
            name: name.to_string(),
            args: Vec::new(),
            span: expr.span,
        };
        match &expr.kind {
            ExprKind::Identifier(name) => self.lookup_local_type(name),
            ExprKind::Number(value) => {
                Some(inferred(if crate::ast::number_literal_is_float(value) {
                    "f64"
                } else {
                    "i32"
                }))
            }
            ExprKind::String(_) => Some(inferred("String")),
            ExprKind::Bool(_) => Some(inferred("bool")),
            ExprKind::StructInit { name, args, .. } | ExprKind::GenericType { name, args } => {
                if name == "Self"
                    && args.is_empty()
                    && let Some(type_ref) = self.lookup_local_type("Self")
                {
                    return Some(type_ref);
                }
                if args.is_empty() {
                    Some(self.expanded_type(&inferred(name)))
                } else {
                    Some(TypeRef {
                        name: name.clone(),
                        args: args.clone(),
                        span: expr.span,
                    })
                }
            }
            ExprKind::Member { object, name } => {
                if let ExprKind::Identifier(enum_name) = &object.kind
                    && self.enum_variant_payload(enum_name, name).is_some()
                {
                    return Some(inferred(enum_name));
                }
                let object_type = self.infer_expr_type(object)?;
                self.generic_member_type(&object_type, name, false)
            }
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Identifier(name) = &callee.kind
                    && let Some(return_type) = self.function_returns.get(name)
                {
                    return Some(return_type.clone());
                }
                let ExprKind::Member { object, name } = &callee.kind else {
                    return None;
                };
                if name == "clone" {
                    return self.infer_expr_type(object);
                }
                if let ExprKind::Identifier(type_name) = &object.kind {
                    if self.enum_variant_payload(type_name, name).is_some() {
                        return Some(inferred(type_name));
                    }
                    let type_ref = if type_name == "Self" {
                        self.lookup_local_type("Self")
                    } else {
                        Some(inferred(type_name))
                    };
                    if let Some(type_ref) = type_ref
                        && self.specializations.contains_key(&type_ref.name)
                    {
                        return self.generic_member_type(&type_ref, name, true);
                    }
                }
                let object_type = self.infer_expr_type(object)?;
                self.generic_member_type(&object_type, name, false)
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.infer_expr_type(operand)
            }
            ExprKind::Binary { left, right, .. } => {
                let left = self.infer_expr_type(left)?;
                let right = self.infer_expr_type(right)?;
                (type_name(&left) == type_name(&right)).then_some(left)
            }
            ExprKind::Match { branches, .. } => branches.iter().find_map(|branch| {
                let MatchBranchBody::Expr(expr) = &branch.body else {
                    return None;
                };
                self.infer_expr_type(expr)
            }),
            ExprKind::Array(_) | ExprKind::Lambda(_) | ExprKind::Missing => None,
        }
    }

    fn generic_member_type(
        &self,
        receiver: &TypeRef,
        member_name: &str,
        static_: bool,
    ) -> Option<TypeRef> {
        if let Some(return_type) =
            self.member_returns
                .get(&(receiver.name.clone(), member_name.to_string(), static_))
        {
            return Some(return_type.clone());
        }
        let receiver = self.expanded_type(receiver);
        let template = self.struct_templates.get(&receiver.name)?;
        let substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(receiver.args.iter().cloned())
            .collect::<HashMap<_, _>>();
        let return_type = template.members.iter().find_map(|member| {
            let function = match member {
                StructMember::Method(function) if !static_ => function,
                StructMember::StaticMethod(function) if static_ => function,
                StructMember::Field(field) if !static_ && field.name == member_name => {
                    return Some(field.type_ref.clone());
                }
                _ => return None,
            };
            (function.name.as_deref() == Some(member_name))
                .then(|| function.return_type.clone())
                .flatten()
        })?;
        if return_type.name == "Self" && return_type.args.is_empty() {
            return Some(receiver);
        }
        Some(substitute_type(&return_type, &substitutions))
    }

    fn expanded_type(&self, type_ref: &TypeRef) -> TypeRef {
        if type_ref.args.is_empty()
            && let Some((name, args)) = self.specializations.get(&type_ref.name)
        {
            return TypeRef {
                name: name.clone(),
                args: args.clone(),
                span: type_ref.span,
            };
        }
        type_ref.clone()
    }

    fn enum_variant_payload(&self, enum_name: &str, variant_name: &str) -> Option<Option<TypeRef>> {
        if let Some(enum_) = self.concrete_enums.get(enum_name) {
            return enum_
                .variants
                .iter()
                .find(|variant| variant.name == variant_name)
                .map(|variant| variant.payload.clone());
        }
        let (generic_name, args) = self.specializations.get(enum_name)?;
        let template = self.enum_templates.get(generic_name)?;
        let substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        template
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
            .map(|variant| {
                variant
                    .payload
                    .as_ref()
                    .map(|payload| substitute_type(payload, &substitutions))
            })
    }

    fn lookup_local_type(&self, name: &str) -> Option<TypeRef> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn specialize_struct(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.struct_templates[name].type_params.len();
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

        self.pending
            .push_back((GenericKind::Struct, name.to_string(), args.to_vec()));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }

    fn specialize_enum(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.enum_templates[name].type_params.len();
        if args.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "generic enum `{name}` expects {expected} type arguments, got {}",
                    args.len()
                ),
            ));
            return;
        }

        self.pending
            .push_back((GenericKind::Enum, name.to_string(), args.to_vec()));
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

fn consistent_type(types: &[TypeRef]) -> Option<TypeRef> {
    let first = types.first()?;
    types
        .iter()
        .all(|type_ref| type_name(type_ref) == type_name(first))
        .then(|| first.clone())
}

fn substitute_type(type_ref: &TypeRef, substitutions: &HashMap<String, TypeRef>) -> TypeRef {
    if type_ref.args.is_empty()
        && let Some(substitution) = substitutions.get(&type_ref.name)
    {
        let mut substitution = substitution.clone();
        substitution.span = type_ref.span;
        return substitution;
    }

    TypeRef {
        name: type_ref.name.clone(),
        args: type_ref
            .args
            .iter()
            .map(|arg| substitute_type(arg, substitutions))
            .collect(),
        span: type_ref.span,
    }
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
