
#[derive(Default)]
struct StructShape {
    fields: HashMap<String, String>,
    methods: HashMap<String, Option<String>>,
    static_methods: HashMap<String, Option<String>>,
}

struct MethodReachability<'items> {
    structs: HashMap<String, StructShape>,
    enums: HashMap<String, StructShape>,
    functions: HashMap<String, Option<String>>,
    generic_structs: &'items HashSet<String>,
    generic_methods: HashMap<(String, String, bool), FunctionDecl>,
    used: HashSet<(String, String, bool)>,
    pending: VecDeque<(String, String, bool)>,
}

impl<'items> MethodReachability<'items> {
    fn new(items: &[Item], generic_structs: &'items HashSet<String>) -> Self {
        let mut structs = HashMap::new();
        let mut enums = HashMap::new();
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
                Item::Enum(item) => {
                    let fields = HashMap::new();
                    let methods = item
                        .members
                        .iter()
                        .filter_map(|member| {
                            let StructMember::Method(function) = member else {
                                return None;
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
                    enums.insert(
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
                Item::Import(_) | Item::Trait(_) | Item::Impl(_) | Item::Extension(_) => {}
            }
        }

        Self {
            structs,
            enums,
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
                Item::Enum(item) if !self.generic_structs.contains(&item.name) => {
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
                        .map(|type_| {
                            if type_ == "Self" {
                                locals.get("Self").cloned().unwrap_or(type_)
                            } else {
                                type_
                            }
                        })
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
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                self.visit_expr(start, locals);
                self.visit_expr(end, locals);
                let source_name = if *inclusive {
                    "RangeInclusive"
                } else {
                    "Range"
                };
                self.structs
                    .keys()
                    .find(|name| {
                        *name == source_name || name.ends_with(&format!("::{source_name}"))
                    })
                    .cloned()
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
                        let static_type_name = match &object.kind {
                            ExprKind::Identifier(identifier) if identifier == "Self" => {
                                locals.get(identifier).cloned()
                            }
                            ExprKind::Identifier(identifier)
                                if !locals.contains_key(identifier)
                                    && (self.structs.contains_key(identifier)
                                        || self.enums.contains_key(identifier)) =>
                            {
                                Some(identifier.clone())
                            }
                            ExprKind::GenericType { name, args } => {
                                let type_name = specialized_name(name, args);
                                (self.structs.contains_key(&type_name)
                                    || self.enums.contains_key(&type_name))
                                .then_some(type_name)
                            }
                            _ => None,
                        };
                        if let Some(type_name) = static_type_name {
                            self.use_method(&type_name, name, true);
                            return self
                                .structs
                                .get(&type_name)
                                .and_then(|shape| shape.static_methods.get(name))
                                .or_else(|| {
                                    self.enums
                                        .get(&type_name)
                                        .and_then(|shape| shape.static_methods.get(name))
                                })
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
                                .or_else(|| {
                                    self.enums
                                        .get(&object_type)
                                        .and_then(|shape| shape.methods.get(name))
                                })
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
            ExprKind::CollectionLiteral { items, collection } => {
                for item in items {
                    self.visit_expr(item, locals);
                }
                Some(collection.name.clone())
            }
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left, locals);
                self.visit_expr(right, locals);
                None
            }
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncrement(operand) => {
                self.visit_expr(operand, locals)
            }
            ExprKind::Cast { value, type_ref } => {
                self.visit_expr(value, locals);
                Some(type_ref.name.clone())
            }
            ExprKind::Match { value, branches } => {
                self.visit_expr(value, locals);
                let mut type_ = None;
                for branch in branches {
                    if let Some(guard) = &branch.guard {
                        self.visit_expr(guard, locals);
                    }
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
            ExprKind::String(_) => Some("string".to_string()),
            ExprKind::Char(_) => Some("char".to_string()),
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
        match item {
            Item::Struct(item) => {
                if !generic_structs.contains(&item.name) {
                    continue;
                }
                item.members.retain(|member| match member {
                    StructMember::Method(function) => function.name.as_ref().is_some_and(|name| {
                        used.contains(&(item.name.clone(), name.clone(), false))
                    }),
                    StructMember::StaticMethod(function) => {
                        function.name.as_ref().is_some_and(|name| {
                            used.contains(&(item.name.clone(), name.clone(), true))
                        })
                    }
                    StructMember::Field(_) => true,
                });
            }
            Item::Enum(item) => {
                if !generic_structs.contains(&item.name) {
                    continue;
                }
                item.members.retain(|member| match member {
                    StructMember::Method(function) => function.name.as_ref().is_some_and(|name| {
                        used.contains(&(item.name.clone(), name.clone(), false))
                    }),
                    StructMember::StaticMethod(function) => {
                        function.name.as_ref().is_some_and(|name| {
                            used.contains(&(item.name.clone(), name.clone(), true))
                        })
                    }
                    StructMember::Field(_) => true,
                });
            }
            _ => {}
        }
    }
}

fn prune_generic_method_templates(items: &mut [Item]) {
    for item in items {
        match item {
            Item::Struct(item) => {
                item.members.retain(|member| match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        function.type_params.is_empty()
                    }
                    StructMember::Field(_) => true,
                });
            }
            Item::Enum(item) => {
                item.members.retain(|member| match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        function.type_params.is_empty()
                    }
                    StructMember::Field(_) => true,
                });
            }
            _ => {}
        }
    }
}
