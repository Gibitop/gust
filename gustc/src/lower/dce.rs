struct LoweredReachability<'program> {
    program: &'program LoweredProgram,
    functions: HashMap<String, &'program LoweredFunction>,
    closure_functions: HashMap<String, &'program LoweredClosureFunction>,
    live_functions: HashSet<String>,
    live_closure_functions: HashSet<String>,
    live_trait_impls: HashSet<(String, String)>,
    live_dynamic_methods: HashSet<(String, String)>,
    pending_functions: VecDeque<String>,
    pending_closure_functions: VecDeque<String>,
}

impl<'program> LoweredReachability<'program> {
    fn new(program: &'program LoweredProgram) -> Self {
        Self {
            program,
            functions: program
                .functions
                .iter()
                .map(|function| (function.name.clone(), function))
                .collect(),
            closure_functions: program
                .closure_functions
                .iter()
                .map(|function| (function.name.clone(), function))
                .collect(),
            live_functions: HashSet::new(),
            live_closure_functions: HashSet::new(),
            live_trait_impls: HashSet::new(),
            live_dynamic_methods: HashSet::new(),
            pending_functions: VecDeque::new(),
            pending_closure_functions: VecDeque::new(),
        }
    }

    fn find(mut self) -> LoweredReachableItems {
        for statement in &self.program.statements {
            self.visit_statement(statement);
        }
        self.drain();

        loop {
            let mut changed = false;
            let live_dynamic_methods = self
                .live_dynamic_methods
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            for (trait_name, method_name) in live_dynamic_methods {
                if self.mark_trait_method_impls(&trait_name, &method_name) {
                    changed = true;
                }
            }

            let live_trait_impls = self.live_trait_impls.iter().cloned().collect::<Vec<_>>();
            for (trait_name, self_type) in live_trait_impls {
                if self.mark_trait_impl_methods(&trait_name, &self_type) {
                    changed = true;
                }
            }

            self.drain();
            if !changed
                && self.pending_functions.is_empty()
                && self.pending_closure_functions.is_empty()
            {
                break;
            }
        }

        LoweredReachableItems {
            functions: self.live_functions,
            closure_functions: self.live_closure_functions,
            trait_impls: self.live_trait_impls,
            dynamic_methods: self.live_dynamic_methods,
        }
    }

    fn drain(&mut self) {
        while !self.pending_functions.is_empty() || !self.pending_closure_functions.is_empty() {
            while let Some(name) = self.pending_functions.pop_front() {
                let Some(function) = self.functions.get(&name) else {
                    continue;
                };
                let function = (*function).clone();
                for statement in &function.statements {
                    self.visit_statement(statement);
                }
                self.visit_expr(&function.return_value);
            }

            while let Some(name) = self.pending_closure_functions.pop_front() {
                let Some(function) = self.closure_functions.get(&name) else {
                    continue;
                };
                let function = (*function).clone();
                for statement in &function.statements {
                    self.visit_statement(statement);
                }
                self.visit_expr(&function.return_value);
            }
        }
    }

    fn mark_function(&mut self, name: &str) -> bool {
        if self.live_functions.insert(name.to_string()) {
            self.pending_functions.push_back(name.to_string());
            true
        } else {
            false
        }
    }

    fn mark_closure_function(&mut self, name: &str) -> bool {
        if self.live_closure_functions.insert(name.to_string()) {
            self.pending_closure_functions.push_back(name.to_string());
            true
        } else {
            false
        }
    }

    fn mark_trait_impl(&mut self, trait_name: &str, self_type: &LoweredType) {
        self.live_trait_impls
            .insert((trait_name.to_string(), self_type.name()));
    }

    fn mark_dynamic_method(&mut self, trait_name: &str, method: &str) {
        self.live_dynamic_methods
            .insert((trait_name.to_string(), method.to_string()));
    }

    fn mark_trait_method_impls(&mut self, trait_name: &str, method_name: &str) -> bool {
        let mut changed = false;
        let function_names = self
            .program
            .traits
            .iter()
            .filter(|trait_| trait_.name == trait_name)
            .flat_map(|trait_| &trait_.impls)
            .filter_map(|impl_| {
                impl_
                    .methods
                    .iter()
                    .find(|method| method.name == method_name)
                    .map(|method| method.function_name.clone())
            })
            .collect::<Vec<_>>();

        for function_name in function_names {
            if self.mark_function(&function_name) {
                changed = true;
            }
        }
        changed
    }

    fn mark_trait_impl_methods(&mut self, trait_name: &str, self_type: &str) -> bool {
        let mut changed = false;
        let function_names = self
            .program
            .traits
            .iter()
            .filter(|trait_| trait_.name == trait_name)
            .flat_map(|trait_| &trait_.impls)
            .filter(|impl_| impl_.self_type.name() == self_type)
            .flat_map(|impl_| impl_.methods.iter().map(|method| method.function_name.clone()))
            .collect::<Vec<_>>();

        for function_name in function_names {
            if self.mark_function(&function_name) {
                changed = true;
            }
        }
        changed
    }

    fn visit_statement(&mut self, statement: &LoweredStatement) {
        match statement {
            LoweredStatement::Local { value, .. }
            | LoweredStatement::LocalCell { value, .. }
            | LoweredStatement::Println(value)
            | LoweredStatement::Panic { message: value, .. }
            | LoweredStatement::Expr(value) => self.visit_expr(value),
            LoweredStatement::Assignment { target, value } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            LoweredStatement::Return(value) => {
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }
            LoweredStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expr(condition);
                for statement in then_branch {
                    self.visit_statement(statement);
                }
                if let Some(else_branch) = else_branch {
                    for statement in else_branch {
                        self.visit_statement(statement);
                    }
                }
            }
            LoweredStatement::While { condition, body } => {
                self.visit_expr(condition);
                for statement in body {
                    self.visit_statement(statement);
                }
            }
            LoweredStatement::Break | LoweredStatement::Continue => {}
            LoweredStatement::Match {
                value, decision, ..
            } => {
                self.visit_expr(value);
                self.visit_match_decision(decision);
            }
        }
    }

    fn visit_expr(&mut self, expr: &LoweredExpr) {
        match &expr.kind {
            LoweredExprKind::PostfixIncrement(operand)
            | LoweredExprKind::Not(operand)
            | LoweredExprKind::Negate(operand)
            | LoweredExprKind::FieldAccess {
                object: operand, ..
            }
            | LoweredExprKind::Clone(operand)
            | LoweredExprKind::NumberToString(operand)
            | LoweredExprKind::Cast { value: operand, .. } => self.visit_expr(operand),
            LoweredExprKind::StringConcat(left, right)
            | LoweredExprKind::Arithmetic { left, right, .. }
            | LoweredExprKind::Logical { left, right, .. }
            | LoweredExprKind::Comparison { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            LoweredExprKind::StructLiteral { fields, .. } => {
                for field in fields {
                    self.visit_expr(&field.value);
                }
            }
            LoweredExprKind::EnumLiteral { payload, .. } => {
                if let Some(payload) = payload {
                    self.visit_expr(payload);
                }
            }
            LoweredExprKind::Match {
                value, decision, ..
            } => {
                self.visit_expr(value);
                self.visit_match_decision(decision);
            }
            LoweredExprKind::Call { name, args, .. } => {
                self.mark_function(name);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            LoweredExprKind::CollectionLiteral {
                constructor,
                add,
                items,
                ..
            } => {
                self.mark_function(constructor);
                self.mark_function(add);
                for item in items {
                    self.visit_expr(item);
                }
            }
            LoweredExprKind::TraitObject {
                trait_name,
                self_type,
                value,
            } => {
                self.mark_trait_impl(trait_name, self_type);
                self.visit_expr(value);
            }
            LoweredExprKind::DynamicCall {
                object,
                method,
                args,
                ..
            } => {
                if let LoweredType::Trait(trait_name) = &object.type_ {
                    self.mark_dynamic_method(trait_name, method);
                }
                self.visit_expr(object);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            LoweredExprKind::Closure { name, .. } => {
                self.mark_closure_function(name);
            }
            LoweredExprKind::IndirectCall { callee, args, .. } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            LoweredExprKind::Void
            | LoweredExprKind::StringLiteral(_)
            | LoweredExprKind::BoolLiteral(_)
            | LoweredExprKind::NumberLiteral(_)
            | LoweredExprKind::Local(_)
            | LoweredExprKind::LocalCell(_)
            | LoweredExprKind::CapturedLocal { .. } => {}
        }
    }

    fn visit_match_decision(&mut self, decision: &LoweredMatchDecision) {
        match decision {
            LoweredMatchDecision::Arms { arms } => {
                for arm in arms {
                    self.visit_match_decision(arm);
                }
            }
            LoweredMatchDecision::Test {
                test, then, else_, ..
            } => {
                if let LoweredMatchTest::Guard(guard) = test {
                    self.visit_expr(guard);
                }
                self.visit_match_decision(then);
                self.visit_match_decision(else_);
            }
            LoweredMatchDecision::Bind { then, .. } => self.visit_match_decision(then),
            LoweredMatchDecision::Or {
                alternatives,
                then,
                else_,
                ..
            } => {
                for alternative in alternatives {
                    self.visit_match_decision(alternative);
                }
                self.visit_match_decision(then);
                self.visit_match_decision(else_);
            }
            LoweredMatchDecision::Body { statements, value } => {
                for statement in statements {
                    self.visit_statement(statement);
                }
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }
            LoweredMatchDecision::Matched | LoweredMatchDecision::Fail | LoweredMatchDecision::End => {}
        }
    }
}

struct LoweredReachableItems {
    functions: HashSet<String>,
    closure_functions: HashSet<String>,
    trait_impls: HashSet<(String, String)>,
    dynamic_methods: HashSet<(String, String)>,
}

struct LoweredTypeReachability<'program> {
    program: &'program LoweredProgram,
    structs: HashMap<String, &'program LoweredStruct>,
    enums: HashMap<String, &'program LoweredEnum>,
    traits: HashMap<String, &'program LoweredTrait>,
    live_structs: HashSet<String>,
    live_enums: HashSet<String>,
    live_traits: HashSet<String>,
    pending_types: VecDeque<LoweredType>,
}

impl<'program> LoweredTypeReachability<'program> {
    fn new(program: &'program LoweredProgram) -> Self {
        Self {
            program,
            structs: program
                .structs
                .iter()
                .map(|struct_| (struct_.name.clone(), struct_))
                .collect(),
            enums: program
                .enums
                .iter()
                .map(|enum_| (enum_.name.clone(), enum_))
                .collect(),
            traits: program
                .traits
                .iter()
                .map(|trait_| (trait_.name.clone(), trait_))
                .collect(),
            live_structs: HashSet::new(),
            live_enums: HashSet::new(),
            live_traits: HashSet::new(),
            pending_types: VecDeque::new(),
        }
    }

    fn find(mut self, reachable: &LoweredReachableItems) -> LoweredReachableTypes {
        for static_ in &self.program.statics {
            self.mark_type(&static_.type_);
        }
        for statement in &self.program.statements {
            self.visit_statement(statement);
        }

        for function in &self.program.functions {
            if !reachable.functions.contains(&function.name) {
                continue;
            }
            for param in &function.params {
                self.mark_type(&param.type_);
            }
            self.mark_type(&function.return_type);
            for statement in &function.statements {
                self.visit_statement(statement);
            }
            self.visit_expr(&function.return_value);
        }

        for function in &self.program.closure_functions {
            if !reachable.closure_functions.contains(&function.name) {
                continue;
            }
            for capture in &function.captures {
                self.mark_type(&capture.type_);
            }
            for param in &function.params {
                self.mark_type(&param.type_);
            }
            self.mark_type(&function.return_type);
            for statement in &function.statements {
                self.visit_statement(statement);
            }
            self.visit_expr(&function.return_value);
        }

        for (trait_name, self_type) in &reachable.trait_impls {
            self.mark_type(&LoweredType::Trait(trait_name.clone()));
            self.mark_type(&LoweredType::Struct(self_type.clone()));
        }
        for (trait_name, _) in &reachable.dynamic_methods {
            self.mark_type(&LoweredType::Trait(trait_name.clone()));
        }

        self.drain();

        LoweredReachableTypes {
            structs: self.live_structs,
            enums: self.live_enums,
            traits: self.live_traits,
        }
    }

    fn drain(&mut self) {
        while let Some(type_) = self.pending_types.pop_front() {
            match type_ {
                LoweredType::Struct(name) => {
                    if !self.live_structs.insert(name.clone()) {
                        continue;
                    }
                    if let Some(struct_) = self.structs.get(&name) {
                        let fields = struct_.fields.clone();
                        let raw_buffer_element = struct_.raw_buffer_element.clone();
                        for field in fields {
                            self.mark_type(&field.type_);
                        }
                        if let Some(element) = raw_buffer_element {
                            self.mark_type(&element);
                        }
                    }
                }
                LoweredType::Enum(name) => {
                    if !self.live_enums.insert(name.clone()) {
                        continue;
                    }
                    if let Some(enum_) = self.enums.get(&name) {
                        let variants = enum_.variants.clone();
                        for variant in variants {
                            if let Some(payload) = &variant.payload {
                                self.mark_type(payload);
                            }
                        }
                    }
                }
                LoweredType::Trait(name) => {
                    if !self.live_traits.insert(name.clone()) {
                        continue;
                    }
                    if let Some(trait_) = self.traits.get(&name) {
                        let methods = trait_.methods.clone();
                        for method in methods {
                            for param in &method.params {
                                self.mark_type(&param.type_);
                            }
                            self.mark_type(&method.return_type);
                        }
                    }
                }
                LoweredType::Function {
                    params,
                    return_type,
                } => {
                    for param in params {
                        self.mark_type(&param.type_);
                    }
                    self.mark_type(&return_type);
                }
                LoweredType::Basic(_) | LoweredType::Void => {}
            }
        }
    }

    fn mark_type(&mut self, type_: &LoweredType) {
        self.pending_types.push_back(type_.clone());
    }

    fn visit_statement(&mut self, statement: &LoweredStatement) {
        match statement {
            LoweredStatement::Local { value, .. }
            | LoweredStatement::LocalCell { value, .. }
            | LoweredStatement::Println(value)
            | LoweredStatement::Panic { message: value, .. }
            | LoweredStatement::Expr(value) => self.visit_expr(value),
            LoweredStatement::Assignment { target, value } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            LoweredStatement::Return(value) => {
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }
            LoweredStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expr(condition);
                for statement in then_branch {
                    self.visit_statement(statement);
                }
                if let Some(else_branch) = else_branch {
                    for statement in else_branch {
                        self.visit_statement(statement);
                    }
                }
            }
            LoweredStatement::While { condition, body } => {
                self.visit_expr(condition);
                for statement in body {
                    self.visit_statement(statement);
                }
            }
            LoweredStatement::Break | LoweredStatement::Continue => {}
            LoweredStatement::Match {
                value, decision, ..
            } => {
                self.visit_expr(value);
                self.visit_match_decision(decision);
            }
        }
    }

    fn visit_expr(&mut self, expr: &LoweredExpr) {
        self.mark_type(&expr.type_);
        match &expr.kind {
            LoweredExprKind::PostfixIncrement(operand)
            | LoweredExprKind::Not(operand)
            | LoweredExprKind::Negate(operand)
            | LoweredExprKind::FieldAccess {
                object: operand, ..
            }
            | LoweredExprKind::Clone(operand)
            | LoweredExprKind::NumberToString(operand)
            | LoweredExprKind::Cast { value: operand, .. } => self.visit_expr(operand),
            LoweredExprKind::StringConcat(left, right)
            | LoweredExprKind::Arithmetic { left, right, .. }
            | LoweredExprKind::Logical { left, right, .. }
            | LoweredExprKind::Comparison { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            LoweredExprKind::StructLiteral { name, fields } => {
                self.mark_type(&LoweredType::Struct(name.clone()));
                for field in fields {
                    self.visit_expr(&field.value);
                }
            }
            LoweredExprKind::EnumLiteral {
                enum_name, payload, ..
            } => {
                self.mark_type(&LoweredType::Enum(enum_name.clone()));
                if let Some(payload) = payload {
                    self.visit_expr(payload);
                }
            }
            LoweredExprKind::Match {
                value, decision, ..
            } => {
                self.visit_expr(value);
                self.visit_match_decision(decision);
            }
            LoweredExprKind::Call { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            LoweredExprKind::CollectionLiteral { items, .. } => {
                for item in items {
                    self.visit_expr(item);
                }
            }
            LoweredExprKind::TraitObject {
                trait_name,
                self_type,
                value,
            } => {
                self.mark_type(&LoweredType::Trait(trait_name.clone()));
                self.mark_type(self_type);
                self.visit_expr(value);
            }
            LoweredExprKind::DynamicCall { object, args, .. } => {
                self.visit_expr(object);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            LoweredExprKind::Closure { captures, .. } => {
                for capture in captures {
                    self.mark_type(&capture.type_);
                }
            }
            LoweredExprKind::IndirectCall { callee, args, .. } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            LoweredExprKind::Void
            | LoweredExprKind::StringLiteral(_)
            | LoweredExprKind::BoolLiteral(_)
            | LoweredExprKind::NumberLiteral(_)
            | LoweredExprKind::Local(_)
            | LoweredExprKind::LocalCell(_)
            | LoweredExprKind::CapturedLocal { .. } => {}
        }
    }

    fn visit_match_decision(&mut self, decision: &LoweredMatchDecision) {
        match decision {
            LoweredMatchDecision::Arms { arms } => {
                for arm in arms {
                    self.visit_match_decision(arm);
                }
            }
            LoweredMatchDecision::Test {
                test, then, else_, ..
            } => {
                match test {
                    LoweredMatchTest::EnumTag { enum_name, .. } => {
                        self.mark_type(&LoweredType::Enum(enum_name.clone()));
                    }
                    LoweredMatchTest::Guard(guard) => self.visit_expr(guard),
                    LoweredMatchTest::StringEq(_)
                    | LoweredMatchTest::BoolEq(_)
                    | LoweredMatchTest::NumberEq { .. }
                    | LoweredMatchTest::Range { .. } => {}
                }
                self.visit_match_decision(then);
                self.visit_match_decision(else_);
            }
            LoweredMatchDecision::Bind { type_, then, .. } => {
                self.mark_type(type_);
                self.visit_match_decision(then);
            }
            LoweredMatchDecision::Or {
                bindings,
                alternatives,
                then,
                else_,
            } => {
                for binding in bindings {
                    self.mark_type(&binding.type_);
                }
                for alternative in alternatives {
                    self.visit_match_decision(alternative);
                }
                self.visit_match_decision(then);
                self.visit_match_decision(else_);
            }
            LoweredMatchDecision::Body { statements, value } => {
                for statement in statements {
                    self.visit_statement(statement);
                }
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }
            LoweredMatchDecision::Matched | LoweredMatchDecision::Fail | LoweredMatchDecision::End => {}
        }
    }
}

struct LoweredReachableTypes {
    structs: HashSet<String>,
    enums: HashSet<String>,
    traits: HashSet<String>,
}

fn prune_dead_lowered_items(mut program: LoweredProgram) -> LoweredProgram {
    let reachable = LoweredReachability::new(&program).find();
    let reachable_types = LoweredTypeReachability::new(&program).find(&reachable);

    program
        .functions
        .retain(|function| reachable.functions.contains(&function.name));
    program
        .closure_functions
        .retain(|function| reachable.closure_functions.contains(&function.name));
    program
        .structs
        .retain(|struct_| reachable_types.structs.contains(&struct_.name));
    program
        .enums
        .retain(|enum_| reachable_types.enums.contains(&enum_.name));
    program.traits.retain_mut(|trait_| {
        if !reachable_types.traits.contains(&trait_.name) {
            return false;
        }

        trait_.impls.retain(|impl_| {
            reachable
                .trait_impls
                .contains(&(trait_.name.clone(), impl_.self_type.name()))
        });
        true
    });

    program
}
