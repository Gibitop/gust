use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    Block, ElseBranch, EnumDecl, Expr, ExprKind, FunctionBody, FunctionDecl, Item, MatchBranchBody,
    Pattern, Program, Stmt, StmtKind, StructDecl, StructMember, TraitDecl, TypeRef,
};
use crate::diagnostic::Diagnostic;

pub fn monomorphize(program: &Program) -> Result<Program, Vec<Diagnostic>> {
    Monomorphizer::new(program).run(program)
}

struct Monomorphizer {
    struct_templates: HashMap<String, StructDecl>,
    enum_templates: HashMap<String, EnumDecl>,
    trait_templates: HashMap<String, TraitDecl>,
    function_templates: HashMap<String, FunctionDecl>,
    concrete_structs: HashSet<String>,
    concrete_struct_defs: HashMap<String, StructDecl>,
    concrete_enums: HashMap<String, EnumDecl>,
    concrete_traits: HashSet<String>,
    pending: VecDeque<PendingSpecialization>,
    emitted: HashSet<String>,
    specializations: HashMap<String, (String, Vec<TypeRef>)>,
    scopes: Vec<HashMap<String, TypeRef>>,
    return_types: Vec<TypeRef>,
    self_types: Vec<TypeRef>,
    inferred_returns: Vec<Option<Vec<TypeRef>>>,
    member_returns: HashMap<(String, String, bool), TypeRef>,
    function_returns: HashMap<String, TypeRef>,
    function_params: HashMap<String, Vec<Option<TypeRef>>>,
    generic_function_returns: HashMap<String, TypeRef>,
    generic_method_returns: HashMap<(String, String, bool), TypeRef>,
    expected_expr_types: HashMap<crate::span::Span, TypeRef>,
    diagnostics: Vec<Diagnostic>,
}

enum PendingSpecialization {
    Struct(String, Vec<TypeRef>),
    Enum(String, Vec<TypeRef>),
    Trait(String, Vec<TypeRef>),
    Function(String, Vec<TypeRef>),
    Method {
        receiver: String,
        name: String,
        static_: bool,
        args: Vec<TypeRef>,
    },
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
        let function_templates: HashMap<String, FunctionDecl> = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(item) = item else {
                    return None;
                };
                (!item.type_params.is_empty())
                    .then(|| item.name.clone().map(|name| (name, item.clone())))
                    .flatten()
            })
            .collect();
        let trait_templates = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Trait(item) = item else {
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
        let concrete_struct_defs = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Struct(item) = item else {
                    return None;
                };
                item.type_params
                    .is_empty()
                    .then(|| (item.name.clone(), item.clone()))
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
        let concrete_traits = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Trait(item) = item else {
                    return None;
                };
                item.type_params.is_empty().then(|| item.name.clone())
            })
            .collect();
        let function_returns = program
            .items
            .iter()
            .filter_map(|item| {
                let Item::Function(function) = item else {
                    return None;
                };
                if !function.type_params.is_empty() {
                    return None;
                }
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
                if !function.type_params.is_empty() {
                    return None;
                }
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
        let generic_function_returns = function_templates
            .iter()
            .filter_map(|(name, function)| {
                function
                    .return_type
                    .clone()
                    .map(|return_type| (name.clone(), return_type))
            })
            .collect();

        Self {
            struct_templates,
            enum_templates,
            trait_templates,
            function_templates,
            concrete_structs,
            concrete_struct_defs,
            concrete_enums,
            concrete_traits,
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
            generic_function_returns,
            generic_method_returns: HashMap::new(),
            expected_expr_types: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self, program: &Program) -> Result<Program, Vec<Diagnostic>> {
        self.infer_generic_function_returns();
        self.infer_generic_method_returns();
        self.validate_templates();

        let mut items = Vec::new();
        for item in &program.items {
            if matches!(item, Item::Struct(item) if !item.type_params.is_empty())
                || matches!(item, Item::Enum(item) if !item.type_params.is_empty())
                || matches!(item, Item::Trait(item) if !item.type_params.is_empty())
                || matches!(item, Item::Function(item) if !item.type_params.is_empty())
            {
                continue;
            }

            let mut item = item.clone();
            self.rewrite_item(&mut item, &HashMap::new());
            items.push(item);
        }

        while let Some(pending) = self.pending.pop_front() {
            match pending {
                PendingSpecialization::Struct(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
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
                        function: None,
                        span: specialized.span,
                    });
                    for member in &mut specialized.members {
                        match member {
                            StructMember::Field(field) => {
                                self.rewrite_type(&mut field.type_ref, &substitutions);
                            }
                            StructMember::Method(function)
                            | StructMember::StaticMethod(function) => {
                                if function.type_params.is_empty() {
                                    self.rewrite_function(function, &substitutions);
                                }
                            }
                        }
                    }
                    self.infer_specialized_member_returns(&mut specialized);
                    self.self_types.pop();
                    items.push(Item::Struct(specialized));
                }
                PendingSpecialization::Enum(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
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
                PendingSpecialization::Trait(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
                    let Some(template) = self.trait_templates.get(&name).cloned() else {
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
                    for method in &mut specialized.methods {
                        for param in &mut method.params {
                            if let Some(type_ref) = &mut param.type_ref {
                                self.rewrite_type(type_ref, &substitutions);
                            }
                        }
                        if let Some(return_type) = &mut method.return_type {
                            self.rewrite_type(return_type, &substitutions);
                        }
                    }
                    items.push(Item::Trait(specialized));
                }
                PendingSpecialization::Function(name, args) => {
                    let specialized_name = specialized_name(&name, &args);
                    if !self.emitted.insert(specialized_name.clone()) {
                        continue;
                    }
                    let Some(template) = self.function_templates.get(&name).cloned() else {
                        continue;
                    };
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let mut specialized = template;
                    specialized.name = Some(specialized_name.clone());
                    specialized.type_params.clear();
                    for param in &mut specialized.params {
                        if let Some(type_ref) = &mut param.type_ref {
                            *type_ref = substitute_type(type_ref, &substitutions);
                        }
                    }
                    if let Some(return_type) = &mut specialized.return_type {
                        *return_type = substitute_type(return_type, &substitutions);
                    } else if let Some(return_type) = self.generic_function_returns.get(&name) {
                        specialized.return_type =
                            Some(substitute_type(return_type, &substitutions));
                    }
                    if let Some(return_type) = &specialized.return_type {
                        self.function_returns
                            .insert(specialized_name.clone(), return_type.clone());
                    }
                    self.function_params.insert(
                        specialized_name,
                        specialized
                            .params
                            .iter()
                            .map(|param| param.type_ref.clone())
                            .collect(),
                    );
                    self.rewrite_function(&mut specialized, &substitutions);
                    items.push(Item::Function(specialized));
                }
                PendingSpecialization::Method {
                    receiver,
                    name,
                    static_,
                    args,
                } => {
                    let method_name = specialized_name(&name, &args);
                    let emitted_name = format!("{receiver}.{method_name}");
                    if !self.emitted.insert(emitted_name) {
                        continue;
                    }
                    self.emit_method_specialization(&mut items, &receiver, &name, static_, &args);
                }
            }
        }

        prune_unused_generic_methods(&mut items, &self.emitted);
        prune_generic_method_templates(&mut items);

        if self.diagnostics.is_empty() {
            Ok(Program { items })
        } else {
            Err(self.diagnostics)
        }
    }

    fn validate_templates(&mut self) {
        for template in self.struct_templates.values().cloned().collect::<Vec<_>>() {
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
            self.validate_method_type_params(&template);
        }
        for template in self
            .concrete_struct_defs
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.validate_method_type_params(&template);
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
        for template in self.trait_templates.values() {
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "duplicate type parameter `{name}` in trait `{}`",
                            template.name
                        ),
                    ));
                }
            }
            let used = template
                .methods
                .iter()
                .flat_map(|method| {
                    method
                        .params
                        .iter()
                        .filter_map(|param| param.type_ref.as_ref())
                        .chain(method.return_type.as_ref())
                })
                .flat_map(type_names)
                .collect::<HashSet<_>>();
            for name in &template.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!(
                            "unused type parameter `{name}` in trait `{}`",
                            template.name
                        ),
                    ));
                }
            }
        }
        for template in self.function_templates.values() {
            let function_name = template.name.as_deref().unwrap_or("<anonymous>");
            let mut names = HashSet::new();
            for name in &template.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!("duplicate type parameter `{name}` in function `{function_name}`"),
                    ));
                }
            }
            let used = template
                .params
                .iter()
                .filter_map(|param| param.type_ref.as_ref())
                .chain(self.generic_function_returns.get(function_name))
                .flat_map(type_names)
                .collect::<HashSet<_>>();
            for name in &template.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        template.span,
                        format!("unused type parameter `{name}` in function `{function_name}`"),
                    ));
                }
            }
        }
    }

    fn validate_method_type_params(&mut self, template: &StructDecl) {
        let struct_params = template.type_params.iter().cloned().collect::<HashSet<_>>();
        for member in &template.members {
            let function = match member {
                StructMember::Method(function) | StructMember::StaticMethod(function) => function,
                StructMember::Field(_) => continue,
            };
            if function.type_params.is_empty() {
                continue;
            }
            let function_name = function.name.as_deref().unwrap_or("<anonymous>");
            let mut names = HashSet::new();
            for name in &function.type_params {
                if !names.insert(name) {
                    self.diagnostics.push(Diagnostic::error(
                        function.span,
                        format!("duplicate type parameter `{name}` in method `{function_name}`"),
                    ));
                }
                if struct_params.contains(name) {
                    self.diagnostics.push(Diagnostic::error(
                        function.span,
                        format!(
                            "type parameter `{name}` in method `{function_name}` conflicts with struct `{}`",
                            template.name
                        ),
                    ));
                }
            }
            let used = function
                .params
                .iter()
                .filter_map(|param| param.type_ref.as_ref())
                .chain(function.return_type.as_ref().or_else(|| {
                    self.generic_method_returns.get(&(
                        template.name.clone(),
                        function_name.to_string(),
                        matches!(member, StructMember::StaticMethod(_)),
                    ))
                }))
                .flat_map(type_names)
                .collect::<HashSet<_>>();
            for name in &function.type_params {
                if !used.contains(name.as_str()) {
                    self.diagnostics.push(Diagnostic::error(
                        function.span,
                        format!("unused type parameter `{name}` in method `{function_name}`"),
                    ));
                }
            }
        }
    }

    fn infer_generic_method_returns(&mut self) {
        let mut templates = Vec::new();
        for template in self.struct_templates.values() {
            for member in &template.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function)
                        if !function.type_params.is_empty() =>
                    {
                        templates.push((
                            template.name.clone(),
                            function.clone(),
                            matches!(member, StructMember::StaticMethod(_)),
                        ));
                    }
                    StructMember::Field(_)
                    | StructMember::Method(_)
                    | StructMember::StaticMethod(_) => {}
                }
            }
        }
        for template in self.concrete_struct_defs.values() {
            for member in &template.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function)
                        if !function.type_params.is_empty() =>
                    {
                        templates.push((
                            template.name.clone(),
                            function.clone(),
                            matches!(member, StructMember::StaticMethod(_)),
                        ));
                    }
                    StructMember::Field(_)
                    | StructMember::Method(_)
                    | StructMember::StaticMethod(_) => {}
                }
            }
        }

        for (owner, function, static_) in &templates {
            if let (Some(name), Some(return_type)) = (&function.name, &function.return_type) {
                self.generic_method_returns
                    .insert((owner.clone(), name.clone(), *static_), return_type.clone());
            }
        }

        for _ in 0..templates.len() {
            let mut changed = false;
            for (owner, function, static_) in &templates {
                let Some(name) = &function.name else {
                    continue;
                };
                let key = (owner.clone(), name.clone(), *static_);
                if self.generic_method_returns.contains_key(&key) {
                    continue;
                }
                let Some(return_type) =
                    self.infer_rewritten_function_return(function, owner, !static_)
                else {
                    continue;
                };
                self.generic_method_returns.insert(key, return_type);
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    fn infer_generic_function_returns(&mut self) {
        for _ in 0..self.function_templates.len() {
            let mut changed = false;
            for (name, template) in self.function_templates.clone() {
                if self.generic_function_returns.contains_key(&name) {
                    continue;
                }
                let Some(return_type) = self.infer_rewritten_function_return(&template, "", false)
                else {
                    continue;
                };
                self.generic_function_returns.insert(name, return_type);
                changed = true;
            }
            if !changed {
                break;
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
                    function: None,
                    span: item.span,
                });
                for member in &mut item.members {
                    match member {
                        StructMember::Field(field) => {
                            self.rewrite_type(&mut field.type_ref, substitutions);
                        }
                        StructMember::Method(function) | StructMember::StaticMethod(function) => {
                            if function.type_params.is_empty() {
                                self.rewrite_function(function, substitutions);
                            }
                        }
                    }
                }
                self.self_types.pop();
            }
            Item::Trait(item) => {
                for method in &mut item.methods {
                    for param in &mut method.params {
                        if let Some(type_ref) = &mut param.type_ref {
                            self.rewrite_type(type_ref, substitutions);
                        }
                    }
                    if let Some(return_type) = &mut method.return_type {
                        self.rewrite_type(return_type, substitutions);
                    }
                }
            }
            Item::Impl(item) => {
                self.rewrite_type(&mut item.trait_ref, substitutions);
                self.rewrite_type(&mut item.type_ref, substitutions);
                self.self_types.push(item.type_ref.clone());
                for member in &mut item.methods {
                    self.rewrite_function(&mut member.function, substitutions);
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
                if let Some(return_type) = self.return_types.last().cloned() {
                    self.apply_expr_context(expr, &return_type);
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
                    if let Some(return_type) = self.return_types.last().cloned() {
                        self.apply_expr_context(value, &return_type);
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
            StmtKind::While { condition, body } => {
                self.rewrite_expr(condition, substitutions);
                self.rewrite_block(body, substitutions);
            }
            StmtKind::For { iterable, body, .. } => {
                self.rewrite_expr(iterable, substitutions);
                self.rewrite_block(body, substitutions);
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) => self.rewrite_expr(expr, substitutions),
        }
    }

    fn rewrite_expr(&mut self, expr: &mut Expr, substitutions: &HashMap<String, TypeRef>) {
        let generic_function_call = match &expr.kind {
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Identifier(name) if self.function_templates.contains_key(name) => {
                    Some((name.clone(), None))
                }
                ExprKind::GenericType { name, args }
                    if self.function_templates.contains_key(name) =>
                {
                    Some((name.clone(), Some(args.clone())))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((function_name, explicit_args)) = generic_function_call {
            let expected_return = self.expected_expr_types.remove(&expr.span);
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic function call was matched above")
            };
            let mut args_rewritten = false;
            let type_args = if let Some(mut type_args) = explicit_args {
                for type_arg in &mut type_args {
                    self.rewrite_type(type_arg, substitutions);
                }
                self.validate_function_type_arguments(&function_name, &type_args, expr.span)
                    .then_some(type_args)
            } else {
                match self.infer_function_type_arguments(
                    &function_name,
                    args,
                    expected_return.as_ref(),
                ) {
                    Ok(mut type_args) => {
                        for type_arg in &mut type_args {
                            self.rewrite_type(type_arg, substitutions);
                        }
                        Some(type_args)
                    }
                    Err(_) => {
                        for arg in args.iter_mut() {
                            self.rewrite_expr(arg, substitutions);
                        }
                        args_rewritten = true;
                        match self.infer_function_type_arguments(
                            &function_name,
                            args,
                            expected_return.as_ref(),
                        ) {
                            Ok(mut type_args) => {
                                for type_arg in &mut type_args {
                                    self.rewrite_type(type_arg, substitutions);
                                }
                                Some(type_args)
                            }
                            Err(message) => {
                                self.diagnostics.push(Diagnostic::error(expr.span, message));
                                None
                            }
                        }
                    }
                }
            };
            if let Some(type_args) = type_args {
                self.apply_generic_function_argument_contexts(
                    &function_name,
                    &type_args,
                    args,
                    substitutions,
                );
                if !args_rewritten {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                }
                self.specialize_function(&function_name, &type_args);
                callee.kind = ExprKind::Identifier(specialized_name(&function_name, &type_args));
            } else if !args_rewritten {
                for arg in args.iter_mut() {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            return;
        }

        let generic_method_call = match &expr.kind {
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::GenericMember { object, name, args } => self
                    .infer_type_expression_ref(object)
                    .map(|receiver| {
                        (
                            object.clone(),
                            receiver,
                            name.clone(),
                            true,
                            Some(args.clone()),
                        )
                    })
                    .or_else(|| {
                        self.infer_expr_type(object).map(|receiver| {
                            (
                                object.clone(),
                                receiver,
                                name.clone(),
                                false,
                                Some(args.clone()),
                            )
                        })
                    }),
                ExprKind::Member { object, name } => self
                    .infer_type_expression_ref(object)
                    .and_then(|receiver| {
                        self.method_template(&receiver, name, true)
                            .filter(|(_, _, function)| !function.type_params.is_empty())
                            .map(|_| (object.clone(), receiver, name.clone(), true, None))
                    })
                    .or_else(|| {
                        self.infer_expr_type(object)
                            .and_then(|receiver| {
                                self.method_template(&receiver, name, false)
                                    .filter(|(_, _, function)| !function.type_params.is_empty())
                                    .map(|_| receiver)
                            })
                            .map(|receiver| (object.clone(), receiver, name.clone(), false, None))
                    }),
                _ => None,
            },
            _ => None,
        };
        if let Some((_, receiver, method_name, static_, explicit_args)) = generic_method_call {
            let expected_return = self.expected_expr_types.remove(&expr.span);
            let ExprKind::Call { callee, args } = &mut expr.kind else {
                unreachable!("generic method call was matched above")
            };
            let mut args_rewritten = false;
            let type_args = if let Some(mut type_args) = explicit_args {
                for type_arg in &mut type_args {
                    self.rewrite_type(type_arg, substitutions);
                }
                self.validate_method_type_arguments(
                    &receiver,
                    &method_name,
                    static_,
                    &type_args,
                    expr.span,
                )
                .then_some(type_args)
            } else {
                match self.infer_method_type_arguments(
                    &receiver,
                    &method_name,
                    static_,
                    args,
                    expected_return.as_ref(),
                ) {
                    Ok(mut type_args) => {
                        for type_arg in &mut type_args {
                            self.rewrite_type(type_arg, substitutions);
                        }
                        Some(type_args)
                    }
                    Err(_) => {
                        for arg in args.iter_mut() {
                            self.rewrite_expr(arg, substitutions);
                        }
                        args_rewritten = true;
                        match self.infer_method_type_arguments(
                            &receiver,
                            &method_name,
                            static_,
                            args,
                            expected_return.as_ref(),
                        ) {
                            Ok(mut type_args) => {
                                for type_arg in &mut type_args {
                                    self.rewrite_type(type_arg, substitutions);
                                }
                                Some(type_args)
                            }
                            Err(message) => {
                                self.diagnostics.push(Diagnostic::error(expr.span, message));
                                None
                            }
                        }
                    }
                }
            };
            if let Some(type_args) = type_args {
                self.apply_generic_method_argument_contexts(
                    &receiver,
                    &method_name,
                    static_,
                    &type_args,
                    args,
                    substitutions,
                );
                if !args_rewritten {
                    for arg in args.iter_mut() {
                        self.rewrite_expr(arg, substitutions);
                    }
                }
                if let Some((receiver, _, _)) =
                    self.method_template(&receiver, &method_name, static_)
                {
                    self.specialize_method(&receiver.name, &method_name, static_, &type_args);
                }
                let mut object = match &mut callee.kind {
                    ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
                        (**object).clone()
                    }
                    _ => unreachable!("generic method call requires a member callee"),
                };
                self.rewrite_expr(&mut object, substitutions);
                callee.kind = ExprKind::Member {
                    object: Box::new(object),
                    name: specialized_name(&method_name, &type_args),
                };
            } else if !args_rewritten {
                for arg in args.iter_mut() {
                    self.rewrite_expr(arg, substitutions);
                }
            }
            return;
        }

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
            ExprKind::GenericMember { object, args, .. } => {
                self.rewrite_expr(object, substitutions);
                for arg in args {
                    self.rewrite_type(arg, substitutions);
                }
            }
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
        if let Some(function) = &mut type_ref.function {
            for param in &mut function.params {
                self.rewrite_type(&mut param.type_ref, substitutions);
            }
            self.rewrite_type(&mut function.return_type, substitutions);
            return;
        }

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
        } else if self.trait_templates.contains_key(&type_ref.name) {
            let name = type_ref.name.clone();
            self.specialize_trait(&name, &type_ref.args, type_ref.span);
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
        } else if self.concrete_traits.contains(&type_ref.name) && !type_ref.args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                type_ref.span,
                format!("trait `{}` does not accept type arguments", type_ref.name),
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
                if !function.type_params.is_empty() {
                    continue;
                }
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
                if !function.type_params.is_empty() {
                    continue;
                }
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
            function: None,
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
                StmtKind::While { body, .. } => {
                    return_types.extend(self.infer_block_return_types(body));
                }
                StmtKind::Assign { .. }
                | StmtKind::Return { value: None }
                | StmtKind::Break
                | StmtKind::Continue
                | StmtKind::Expr(_) => {}
            }
        }
        self.scopes.pop();
        return_types
    }

    fn apply_expr_context(&mut self, expr: &mut Expr, expected: &TypeRef) {
        self.expected_expr_types.insert(expr.span, expected.clone());
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

    fn infer_function_type_arguments(
        &self,
        function_name: &str,
        args: &[Expr],
        expected_return: Option<&TypeRef>,
    ) -> Result<Vec<TypeRef>, String> {
        let template = &self.function_templates[function_name];
        let mut constraints = template
            .params
            .iter()
            .zip(args)
            .filter_map(|(param, arg)| {
                let expected = param.type_ref.as_ref()?;
                self.infer_expr_type(arg)
                    .map(|actual| (expected.clone(), actual))
            })
            .collect::<Vec<_>>();
        let return_type = template
            .return_type
            .as_ref()
            .or_else(|| self.generic_function_returns.get(function_name));
        if let (Some(return_type), Some(expected_return)) = (return_type, expected_return) {
            constraints.push((return_type.clone(), expected_return.clone()));
        }
        self.solve_type_arguments(function_name, &template.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic function `{function_name}`: {reason}; write `{function_name}<Type>(...)` or add a concrete expected type"
                )
            })
    }

    fn apply_generic_function_argument_contexts(
        &mut self,
        function_name: &str,
        type_args: &[TypeRef],
        args: &mut [Expr],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        let template = self.function_templates[function_name].clone();
        let function_substitutions = template
            .type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect::<HashMap<_, _>>();
        for (param, arg) in template.params.iter().zip(args) {
            let Some(type_ref) = &param.type_ref else {
                continue;
            };
            let mut expected = substitute_type(type_ref, &function_substitutions);
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(arg, &expected);
        }
    }

    fn infer_method_type_arguments(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        args: &[Expr],
        expected_return: Option<&TypeRef>,
    ) -> Result<Vec<TypeRef>, String> {
        let (receiver, struct_substitutions, function) = self
            .method_template(receiver, method_name, static_)
            .ok_or_else(|| format!("unknown generic method `{method_name}`"))?;
        let mut constraints = function
            .params
            .iter()
            .filter(|param| !(param.name == "self" && param.type_ref.is_none()))
            .zip(args)
            .filter_map(|(param, arg)| {
                let expected = param.type_ref.as_ref()?;
                self.infer_expr_type(arg)
                    .map(|actual| (substitute_type(expected, &struct_substitutions), actual))
            })
            .collect::<Vec<_>>();
        let return_type = self.method_return_type(&receiver, &function, static_);
        if let (Some(return_type), Some(expected_return)) = (return_type, expected_return) {
            constraints.push((return_type, expected_return.clone()));
        }
        self.solve_type_arguments(method_name, &function.type_params, constraints)
            .map_err(|reason| {
                format!(
                    "cannot infer type arguments for generic method `{}.{method_name}`: {reason}; write `.{method_name}<Type>(...)` or add a concrete expected type",
                    receiver.name
                )
            })
    }

    fn apply_generic_method_argument_contexts(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        type_args: &[TypeRef],
        args: &mut [Expr],
        substitutions: &HashMap<String, TypeRef>,
    ) {
        let Some((_, mut method_substitutions, function)) =
            self.method_template(receiver, method_name, static_)
        else {
            return;
        };
        method_substitutions.extend(
            function
                .type_params
                .iter()
                .cloned()
                .zip(type_args.iter().cloned()),
        );
        for (param, arg) in function
            .params
            .iter()
            .filter(|param| !(param.name == "self" && param.type_ref.is_none()))
            .zip(args)
        {
            let Some(type_ref) = &param.type_ref else {
                continue;
            };
            let mut expected = substitute_type(type_ref, &method_substitutions);
            self.rewrite_type(&mut expected, substitutions);
            self.apply_expr_context(arg, &expected);
        }
    }

    fn validate_method_type_arguments(
        &mut self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
        span: crate::span::Span,
    ) -> bool {
        let Some((receiver, _, function)) = self.method_template(receiver, method_name, static_)
        else {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!("unknown generic method `{method_name}`"),
            ));
            return false;
        };
        let expected = function.type_params.len();
        if args.len() == expected {
            return true;
        }
        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "generic method `{}.{method_name}` expects {expected} type arguments, got {}",
                receiver.name,
                args.len()
            ),
        ));
        false
    }

    fn method_template(
        &self,
        receiver: &TypeRef,
        method_name: &str,
        static_: bool,
    ) -> Option<(TypeRef, HashMap<String, TypeRef>, FunctionDecl)> {
        let receiver = self.expanded_type(receiver);
        if let Some((generic_name, args)) = self.specializations.get(&receiver.name) {
            let template = self.struct_templates.get(generic_name)?;
            let substitutions = template
                .type_params
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let mut function = template.members.iter().find_map(|member| match member {
                StructMember::Method(function) if !static_ => {
                    (function.name.as_deref() == Some(method_name)).then(|| function.clone())
                }
                StructMember::StaticMethod(function) if static_ => {
                    (function.name.as_deref() == Some(method_name)).then(|| function.clone())
                }
                StructMember::Field(_)
                | StructMember::Method(_)
                | StructMember::StaticMethod(_) => None,
            })?;
            if function.return_type.is_none()
                && let Some(return_type) = self.generic_method_returns.get(&(
                    generic_name.clone(),
                    method_name.to_string(),
                    static_,
                ))
            {
                function.return_type = Some(return_type.clone());
            }
            return Some((receiver, substitutions, function));
        }

        if !receiver.args.is_empty()
            && let Some(template) = self.struct_templates.get(&receiver.name)
        {
            let substitutions = template
                .type_params
                .iter()
                .cloned()
                .zip(receiver.args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let mut function = template.members.iter().find_map(|member| match member {
                StructMember::Method(function) if !static_ => {
                    (function.name.as_deref() == Some(method_name)).then(|| function.clone())
                }
                StructMember::StaticMethod(function) if static_ => {
                    (function.name.as_deref() == Some(method_name)).then(|| function.clone())
                }
                StructMember::Field(_)
                | StructMember::Method(_)
                | StructMember::StaticMethod(_) => None,
            })?;
            if function.return_type.is_none()
                && let Some(return_type) = self.generic_method_returns.get(&(
                    receiver.name.clone(),
                    method_name.to_string(),
                    static_,
                ))
            {
                function.return_type = Some(return_type.clone());
            }
            return Some((
                TypeRef {
                    name: specialized_name(&receiver.name, &receiver.args),
                    args: Vec::new(),
                    function: None,
                    span: receiver.span,
                },
                substitutions,
                function,
            ));
        }

        let template = self.concrete_struct_defs.get(&receiver.name)?;
        let mut function = template.members.iter().find_map(|member| match member {
            StructMember::Method(function) if !static_ => {
                (function.name.as_deref() == Some(method_name)).then(|| function.clone())
            }
            StructMember::StaticMethod(function) if static_ => {
                (function.name.as_deref() == Some(method_name)).then(|| function.clone())
            }
            StructMember::Field(_) | StructMember::Method(_) | StructMember::StaticMethod(_) => {
                None
            }
        })?;
        if function.return_type.is_none()
            && let Some(return_type) = self.generic_method_returns.get(&(
                receiver.name.clone(),
                method_name.to_string(),
                static_,
            ))
        {
            function.return_type = Some(return_type.clone());
        }
        Some((receiver, HashMap::new(), function))
    }

    fn method_return_type(
        &mut self,
        receiver: &TypeRef,
        function: &FunctionDecl,
        static_: bool,
    ) -> Option<TypeRef> {
        function
            .return_type
            .clone()
            .or_else(|| self.infer_rewritten_function_return(function, &receiver.name, !static_))
    }

    fn specialize_method(
        &mut self,
        receiver: &str,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
    ) {
        let specialized_method = specialized_name(method_name, args);
        let key = (receiver.to_string(), specialized_method.clone(), static_);
        if !self.member_returns.contains_key(&key) {
            let receiver_type = TypeRef {
                name: receiver.to_string(),
                args: Vec::new(),
                function: None,
                span: args
                    .first()
                    .map_or_else(|| crate::span::Span::new(0, 0), |arg| arg.span),
            };
            if let Some((_, mut substitutions, function)) =
                self.method_template(&receiver_type, method_name, static_)
            {
                substitutions.extend(
                    function
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned()),
                );
                if let Some(return_type) =
                    self.method_return_type(&receiver_type, &function, static_)
                {
                    self.member_returns
                        .insert(key, substitute_type(&return_type, &substitutions));
                }
            }
        }
        self.pending.push_back(PendingSpecialization::Method {
            receiver: receiver.to_string(),
            name: method_name.to_string(),
            static_,
            args: args.to_vec(),
        });
    }

    fn emit_method_specialization(
        &mut self,
        items: &mut [Item],
        receiver: &str,
        method_name: &str,
        static_: bool,
        args: &[TypeRef],
    ) {
        let receiver_type = TypeRef {
            name: receiver.to_string(),
            args: Vec::new(),
            function: None,
            span: args
                .first()
                .map_or_else(|| crate::span::Span::new(0, 0), |arg| arg.span),
        };
        let Some((_, mut substitutions, mut function)) =
            self.method_template(&receiver_type, method_name, static_)
        else {
            return;
        };
        substitutions.extend(
            function
                .type_params
                .iter()
                .cloned()
                .zip(args.iter().cloned()),
        );
        function.name = Some(specialized_name(method_name, args));
        function.type_params.clear();
        self.self_types.push(receiver_type.clone());
        self.rewrite_function(&mut function, &substitutions);
        self.self_types.pop();
        if function.return_type.is_none()
            && let Some(return_type) =
                self.infer_rewritten_function_return(&function, receiver, !static_)
        {
            function.return_type = Some(return_type);
        }
        if let Some(return_type) = &function.return_type {
            self.member_returns.insert(
                (
                    receiver.to_string(),
                    function.name.clone().unwrap_or_default(),
                    static_,
                ),
                return_type.clone(),
            );
        }
        let Some(Item::Struct(struct_)) = items
            .iter_mut()
            .find(|item| matches!(item, Item::Struct(struct_) if struct_.name == receiver))
        else {
            return;
        };
        if static_ {
            struct_.members.push(StructMember::StaticMethod(function));
        } else {
            struct_.members.push(StructMember::Method(function));
        }
    }

    fn validate_function_type_arguments(
        &mut self,
        function_name: &str,
        args: &[TypeRef],
        span: crate::span::Span,
    ) -> bool {
        let expected = self.function_templates[function_name].type_params.len();
        if args.len() == expected {
            return true;
        }
        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "generic function `{function_name}` expects {expected} type arguments, got {}",
                args.len()
            ),
        ));
        false
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
            function: None,
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
            ExprKind::StructInit { name, args, fields } => {
                if name == "Self"
                    && args.is_empty()
                    && let Some(type_ref) = self.lookup_local_type("Self")
                {
                    return Some(type_ref);
                }
                if self.struct_templates.contains_key(name) {
                    let args = if args.is_empty() {
                        self.infer_struct_type_arguments(name, fields).ok()?
                    } else {
                        args.clone()
                    };
                    return Some(TypeRef {
                        name: name.clone(),
                        args,
                        function: None,
                        span: expr.span,
                    });
                }
                if args.is_empty() {
                    Some(self.expanded_type(&inferred(name)))
                } else {
                    Some(TypeRef {
                        name: name.clone(),
                        args: args.clone(),
                        function: None,
                        span: expr.span,
                    })
                }
            }
            ExprKind::GenericType { name, args } => {
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
                        function: None,
                        span: expr.span,
                    })
                }
            }
            ExprKind::Member { object, name } => {
                if let ExprKind::GenericType {
                    name: enum_name,
                    args,
                } = &object.kind
                    && self.enum_templates.contains_key(enum_name)
                    && self.enum_templates[enum_name]
                        .variants
                        .iter()
                        .any(|variant| variant.name == *name)
                {
                    return Some(TypeRef {
                        name: enum_name.clone(),
                        args: args.clone(),
                        function: None,
                        span: expr.span,
                    });
                }
                if let ExprKind::Identifier(enum_name) = &object.kind
                    && self.enum_variant_payload(enum_name, name).is_some()
                {
                    return Some(inferred(enum_name));
                }
                let object_type = self.infer_expr_type(object)?;
                self.generic_member_type(&object_type, name, false)
            }
            ExprKind::GenericMember { object, name, args } => {
                let receiver = self.infer_expr_type(object)?;
                let (_, mut substitutions, function) =
                    self.method_template(&receiver, name, false)?;
                substitutions.extend(
                    function
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned()),
                );
                let return_type = function.return_type.as_ref()?;
                Some(substitute_type(return_type, &substitutions))
            }
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Member { object, name } = &callee.kind {
                    match &object.kind {
                        ExprKind::Identifier(enum_name)
                            if self.enum_templates.contains_key(enum_name)
                                && self.lookup_local_type(enum_name).is_none() =>
                        {
                            let ExprKind::Call { args, .. } = &expr.kind else {
                                unreachable!("call expression was matched above")
                            };
                            if let Ok(type_args) =
                                self.infer_enum_type_arguments(enum_name, name, args)
                            {
                                return Some(TypeRef {
                                    name: enum_name.clone(),
                                    args: type_args,
                                    function: None,
                                    span: expr.span,
                                });
                            }
                        }
                        ExprKind::GenericType {
                            name: enum_name,
                            args,
                        } if self.enum_templates.contains_key(enum_name) => {
                            return Some(TypeRef {
                                name: enum_name.clone(),
                                args: args.clone(),
                                function: None,
                                span: expr.span,
                            });
                        }
                        _ => {}
                    }
                }
                if let ExprKind::Identifier(name) = &callee.kind {
                    if let Some(template_return) = self.generic_function_returns.get(name)
                        && let ExprKind::Call { args, .. } = &expr.kind
                        && let Ok(type_args) = self.infer_function_type_arguments(name, args, None)
                    {
                        let template = &self.function_templates[name];
                        let substitutions = template
                            .type_params
                            .iter()
                            .cloned()
                            .zip(type_args)
                            .collect::<HashMap<_, _>>();
                        return Some(substitute_type(template_return, &substitutions));
                    }
                    if let Some(return_type) = self.function_returns.get(name) {
                        return Some(return_type.clone());
                    }
                }
                if let ExprKind::GenericType { name, args } = &callee.kind
                    && let Some(template_return) = self.generic_function_returns.get(name)
                {
                    let template = &self.function_templates[name];
                    let substitutions = template
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    return Some(substitute_type(template_return, &substitutions));
                }
                if let ExprKind::GenericMember { object, name, args } = &callee.kind {
                    let receiver = self.infer_expr_type(object)?;
                    let (_, mut substitutions, function) =
                        self.method_template(&receiver, name, false)?;
                    substitutions.extend(
                        function
                            .type_params
                            .iter()
                            .cloned()
                            .zip(args.iter().cloned()),
                    );
                    let return_type = function.return_type.as_ref()?;
                    return Some(substitute_type(return_type, &substitutions));
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
        if !receiver.args.is_empty() {
            let concrete_name = specialized_name(&receiver.name, &receiver.args);
            if let Some(return_type) =
                self.member_returns
                    .get(&(concrete_name, member_name.to_string(), static_))
            {
                return Some(return_type.clone());
            }
        }
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

    fn infer_type_expression_ref(&self, expr: &Expr) -> Option<TypeRef> {
        match &expr.kind {
            ExprKind::Identifier(name)
                if self.concrete_structs.contains(name)
                    || self.struct_templates.contains_key(name) =>
            {
                Some(TypeRef {
                    name: name.clone(),
                    args: Vec::new(),
                    function: None,
                    span: expr.span,
                })
            }
            ExprKind::GenericType { name, args } if self.struct_templates.contains_key(name) => {
                Some(TypeRef {
                    name: name.clone(),
                    args: args.clone(),
                    function: None,
                    span: expr.span,
                })
            }
            _ => None,
        }
    }

    fn expanded_type(&self, type_ref: &TypeRef) -> TypeRef {
        if type_ref.args.is_empty()
            && let Some((name, args)) = self.specializations.get(&type_ref.name)
        {
            return TypeRef {
                name: name.clone(),
                args: args.clone(),
                function: None,
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

        self.pending.push_back(PendingSpecialization::Struct(
            name.to_string(),
            args.to_vec(),
        ));
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
            .push_back(PendingSpecialization::Enum(name.to_string(), args.to_vec()));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }

    fn specialize_trait(&mut self, name: &str, args: &[TypeRef], span: crate::span::Span) {
        let expected = self.trait_templates[name].type_params.len();
        if args.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "generic trait `{name}` expects {expected} type arguments, got {}",
                    args.len()
                ),
            ));
            return;
        }

        self.pending.push_back(PendingSpecialization::Trait(
            name.to_string(),
            args.to_vec(),
        ));
        self.specializations.insert(
            specialized_name(name, args),
            (name.to_string(), args.to_vec()),
        );
    }

    fn specialize_function(&mut self, name: &str, args: &[TypeRef]) {
        let specialized_name = specialized_name(name, args);
        if !self.function_params.contains_key(&specialized_name) {
            let template = self.function_templates[name].clone();
            let substitutions = template
                .type_params
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let params = template
                .params
                .iter()
                .map(|param| {
                    param
                        .type_ref
                        .as_ref()
                        .map(|type_ref| substitute_type(type_ref, &substitutions))
                })
                .collect();
            self.function_params
                .insert(specialized_name.clone(), params);
            if let Some(return_type) = &template.return_type {
                self.function_returns.insert(
                    specialized_name,
                    substitute_type(return_type, &substitutions),
                );
            } else if let Some(return_type) = self.generic_function_returns.get(name) {
                self.function_returns.insert(
                    specialized_name,
                    substitute_type(return_type, &substitutions),
                );
            }
        }
        self.pending.push_back(PendingSpecialization::Function(
            name.to_string(),
            args.to_vec(),
        ));
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
                Item::Import(_)
                | Item::Enum(_)
                | Item::Trait(_)
                | Item::Impl(_)
                | Item::Extension(_) => {}
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
                Item::Impl(item) => {
                    for member in &item.methods {
                        self.visit_function(
                            &member.function,
                            Some(&item.type_ref.name),
                            !member.static_,
                        );
                    }
                }
                Item::Import(_) | Item::Enum(_) | Item::Struct(_) | Item::Trait(_) => {}
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
                StmtKind::While { condition, body } => {
                    self.visit_expr(condition, locals);
                    self.visit_block(body, &mut locals.clone());
                }
                StmtKind::For { iterable, body, .. } => {
                    self.visit_expr(iterable, locals);
                    self.visit_block(body, &mut locals.clone());
                }
                StmtKind::Break | StmtKind::Continue => {}
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
            ExprKind::GenericMember { object, .. } => self.visit_expr(object, locals),
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

fn prune_generic_method_templates(items: &mut [Item]) {
    for item in items {
        let Item::Struct(item) = item else {
            continue;
        };
        item.members.retain(|member| match member {
            StructMember::Method(function) | StructMember::StaticMethod(function) => {
                function.type_params.is_empty()
            }
            StructMember::Field(_) => true,
        });
    }
}

fn concrete_type_name(type_ref: &TypeRef) -> Option<String> {
    type_ref.args.is_empty().then(|| type_ref.name.clone())
}

fn type_names(type_ref: &TypeRef) -> Vec<&str> {
    if let Some(function) = &type_ref.function {
        let mut names = Vec::new();
        for param in &function.params {
            names.extend(type_names(&param.type_ref));
        }
        names.extend(type_names(&function.return_type));
        return names;
    }
    let mut names = vec![type_ref.name.as_str()];
    for arg in &type_ref.args {
        names.extend(type_names(arg));
    }
    names
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
        function: type_ref
            .function
            .as_ref()
            .map(|function| crate::ast::FunctionTypeRef {
                params: function
                    .params
                    .iter()
                    .map(|param| crate::ast::FunctionTypeParam {
                        mutable: param.mutable,
                        type_ref: substitute_type(&param.type_ref, substitutions),
                    })
                    .collect(),
                return_type: Box::new(substitute_type(&function.return_type, substitutions)),
            }),
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
