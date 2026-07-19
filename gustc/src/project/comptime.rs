use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::build::compile_c_source_to_binary;
use crate::c_codegen::{CCodegenOptions, CComptimeOptions, emit_c_for_comptime};
use crate::lower::lower_program_with_source_files;

static NEXT_COMPTIME_RUNNER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
enum ComptimeValue {
    Void,
    Bool(bool),
    Number(String),
    String(String),
    Char(u32),
    Struct {
        name: String,
        fields: Vec<(String, ComptimeValue)>,
    },
    Enum {
        name: String,
        variant: String,
        payload: Option<Box<ComptimeValue>>,
    },
    Closure {
        name: String,
        captures: Vec<(String, ComptimeValue)>,
    },
    Unsupported(String),
}

impl ComptimeValue {
    fn to_expr(
        &self,
        span: Span,
        lambdas: &HashMap<String, FunctionDecl>,
    ) -> Result<Expr, String> {
        match self {
            ComptimeValue::Void => Ok(Expr {
                kind: ExprKind::Missing,
                span,
            }),
            ComptimeValue::Bool(value) => Ok(Expr {
                kind: ExprKind::Bool(*value),
                span,
            }),
            ComptimeValue::Number(value) => Ok(Expr {
                kind: ExprKind::Number(value.clone()),
                span,
            }),
            ComptimeValue::String(value) => Ok(Expr {
                kind: ExprKind::String(value.clone()),
                span,
            }),
            ComptimeValue::Char(value) => Ok(Expr {
                kind: ExprKind::Char(*value),
                span,
            }),
            ComptimeValue::Struct { name, fields } => Ok(Expr {
                kind: ExprKind::StructInit {
                    name: name.clone(),
                    args: Vec::new(),
                    fields: fields
                        .iter()
                        .map(|(name, value)| {
                            Ok(StructInitField {
                                name: name.clone(),
                                value: value.to_expr(span, lambdas)?,
                                span,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                },
                span,
            }),
            ComptimeValue::Enum {
                name,
                variant,
                payload,
            } => {
                let callee = Expr {
                    span,
                    kind: ExprKind::Member {
                        object: Box::new(Expr {
                            span,
                            kind: ExprKind::Identifier(name.clone()),
                        }),
                        name: variant.clone(),
                    },
                };
                if let Some(payload) = payload {
                    Ok(Expr {
                        span,
                        kind: ExprKind::Call {
                            callee: Box::new(callee),
                            args: vec![payload.to_expr(span, lambdas)?],
                        },
                    })
                } else {
                    Ok(callee)
                }
            }
            ComptimeValue::Closure { name, captures } => {
                let Some(function) = lambdas.get(name) else {
                    return Err(
                        "comptime result cannot be materialized as Gust source".to_string()
                    );
                };
                if captures.is_empty() {
                    return Ok(Expr {
                        kind: ExprKind::Lambda(function.clone()),
                        span,
                    });
                }

                let mut statements = Vec::new();
                for (name, value) in captures {
                    statements.push(Stmt {
                        span,
                        kind: StmtKind::Let {
                            name: name.clone(),
                            mutable: true,
                            type_annotation: None,
                            value: Some(value.to_expr(span, lambdas)?),
                        },
                    });
                }
                statements.push(Stmt {
                    span,
                    kind: StmtKind::Return {
                        value: Some(Expr {
                            kind: ExprKind::Lambda(function.clone()),
                            span,
                        }),
                    },
                });
                Ok(Expr {
                    span,
                    kind: ExprKind::Block(Block { statements, span }),
                })
            }
            ComptimeValue::Unsupported(message) => Err(message.clone()),
        }
    }
}

#[derive(Clone)]
struct ComptimeSite {
    id: usize,
    package: usize,
    span: Span,
    expr: Expr,
    direct_static_name: Option<String>,
    runtime_local_uses: Vec<RuntimeLocalUse>,
    runtime_static_uses: Vec<RuntimeLocalUse>,
}

#[derive(Clone)]
struct RuntimeLocalUse {
    name: String,
    span: Span,
}

struct ComptimeReader<'bytes> {
    bytes: &'bytes [u8],
    position: usize,
}

fn expand_comptime(
    program: &mut Program,
    item_packages: &[usize],
    packages: &[Package],
    root_package: usize,
    diagnostics: &mut Vec<Diagnostic>,
    source_files: Vec<LoweringSourceFile>,
) {
    let sites = collect_comptime_sites(program, item_packages, root_package);
    if sites.is_empty() {
        return;
    }

    let mut expanded_comptime_statics = HashSet::new();
    for site in &sites {
        if let Some(reference) = site.runtime_local_uses.first() {
            diagnostics.push(Diagnostic::error(
                reference.span,
                format!(
                    "`comptime` expressions cannot read runtime local `{}`; comptime code is compiled and run during compilation, before locals from the surrounding runtime scope exist. Move `{}` into the comptime block, make it a top-level `let` initialized by `comptime`, or compute this value at runtime outside `comptime`.",
                    reference.name, reference.name
                ),
            ));
            return;
        }
        if let Some(reference) = site
            .runtime_static_uses
            .iter()
            .find(|reference| !expanded_comptime_statics.contains(&reference.name))
        {
            diagnostics.push(Diagnostic::error(
                reference.span,
                format!(
                    "`comptime` expressions cannot read runtime static `{}`; top-level `let` initializers run at runtime, so their values are not available while compiling. Initialize `{}` with `comptime` first, or move the computation out of `comptime`.",
                    reference.name, reference.name
                ),
            ));
            return;
        }
        match run_comptime_site(
            program,
            item_packages,
            packages,
            site,
            &source_files,
            &expanded_comptime_statics,
        ) {
            Ok(value) => match value.to_expr(site.span, &collect_lambda_sources(program)) {
                Ok(expr) => {
                    replace_comptime_at_span(program, site.span, expr);
                    if let Some(name) = &site.direct_static_name {
                        expanded_comptime_statics.insert(name.clone());
                    }
                }
                Err(message) => diagnostics.push(Diagnostic::error(site.span, message)),
            },
            Err(message) => diagnostics.push(Diagnostic::error(site.span, message)),
        }
        if diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
        {
            return;
        }
    }
}

pub(crate) fn expand_comptime_for_source(program: &mut Program, diagnostics: &mut Vec<Diagnostic>) {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let packages = vec![Package {
        root: root.clone(),
        src: root.clone(),
        project: false,
        std: false,
        no_std: false,
        dependencies: HashMap::new(),
        aliases: HashSet::new(),
        comptime_permissions: ComptimePermissions {
            fs: PermissionValue::All,
            env: PermissionValue::All,
        },
    }];
    let item_packages = vec![0; program.items.len()];
    expand_comptime(
        program,
        &item_packages,
        &packages,
        0,
        diagnostics,
        Vec::new(),
    );
}

fn collect_comptime_sites(
    program: &Program,
    item_packages: &[usize],
    root_package: usize,
) -> Vec<ComptimeSite> {
    let mut sites = Vec::new();
    let static_names = program
        .items
        .iter()
        .filter_map(|item| {
            let Item::StaticVar(static_) = item else {
                return None;
            };
            Some(static_.name.clone())
        })
        .collect::<HashSet<_>>();
    for (index, item) in program.items.iter().enumerate() {
        let package = item_packages.get(index).copied().unwrap_or(root_package);
        collect_comptime_sites_in_item(item, package, &static_names, &mut sites);
    }
    sites
}

fn collect_comptime_sites_in_item(
    item: &Item,
    package: usize,
    static_names: &HashSet<String>,
    sites: &mut Vec<ComptimeSite>,
) {
    match item {
        Item::Enum(item) => {
            for member in &item.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        collect_comptime_sites_in_function(function, package, static_names, sites);
                    }
                    StructMember::Field(_) => {}
                }
            }
        }
        Item::Struct(item) => {
            for member in &item.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        collect_comptime_sites_in_function(function, package, static_names, sites);
                    }
                    StructMember::Field(_) => {}
                }
            }
        }
        Item::Trait(item) => {
            for method in &item.methods {
                if let Some(body) = &method.body {
                    let mut runtime_scopes = vec![params_scope(&method.params)];
                    collect_comptime_sites_in_body(
                        body,
                        package,
                        static_names,
                        &mut runtime_scopes,
                        None,
                        sites,
                    );
                }
            }
        }
        Item::Impl(item) => {
            for member in &item.methods {
                collect_comptime_sites_in_function(
                    &member.function,
                    package,
                    static_names,
                    sites,
                );
            }
        }
        Item::Extension(item) => {
            collect_comptime_sites_in_function(&item.function, package, static_names, sites)
        }
        Item::Function(function) => {
            collect_comptime_sites_in_function(function, package, static_names, sites)
        }
        Item::StaticVar(item) => {
            let mut runtime_scopes = Vec::new();
            let direct_static_name = matches!(item.value.kind, ExprKind::Comptime(_))
                .then_some(item.name.as_str());
            collect_comptime_sites_in_expr(
                &item.value,
                package,
                static_names,
                &mut runtime_scopes,
                direct_static_name,
                sites,
            );
        }
        Item::Import(_) => {}
    }
}

fn collect_comptime_sites_in_function(
    function: &FunctionDecl,
    package: usize,
    static_names: &HashSet<String>,
    sites: &mut Vec<ComptimeSite>,
) {
    let mut runtime_scopes = vec![params_scope(&function.params)];
    collect_comptime_sites_in_body(
        &function.body,
        package,
        static_names,
        &mut runtime_scopes,
        None,
        sites,
    );
}

fn params_scope(params: &[Param]) -> HashSet<String> {
    params.iter().map(|param| param.name.clone()).collect()
}

fn collect_comptime_sites_in_body(
    body: &FunctionBody,
    package: usize,
    static_names: &HashSet<String>,
    runtime_scopes: &mut Vec<HashSet<String>>,
    direct_static_name: Option<&str>,
    sites: &mut Vec<ComptimeSite>,
) {
    match body {
        FunctionBody::Block(block) => collect_comptime_sites_in_block(
            block,
            package,
            static_names,
            runtime_scopes,
            direct_static_name,
            sites,
        ),
        FunctionBody::Expr(expr) => collect_comptime_sites_in_expr(
            expr,
            package,
            static_names,
            runtime_scopes,
            direct_static_name,
            sites,
        ),
    }
}

fn collect_comptime_sites_in_block(
    block: &Block,
    package: usize,
    static_names: &HashSet<String>,
    runtime_scopes: &mut Vec<HashSet<String>>,
    direct_static_name: Option<&str>,
    sites: &mut Vec<ComptimeSite>,
) {
    runtime_scopes.push(HashSet::new());
    for (index, statement) in block.statements.iter().enumerate() {
        let statement_direct_static_name = if index == 0 {
            direct_static_name
        } else {
            None
        };
        collect_comptime_sites_in_statement(
            statement,
            package,
            static_names,
            runtime_scopes,
            statement_direct_static_name,
            sites,
        );
    }
    runtime_scopes.pop();
}

fn collect_comptime_sites_in_statement(
    statement: &Stmt,
    package: usize,
    static_names: &HashSet<String>,
    runtime_scopes: &mut Vec<HashSet<String>>,
    direct_static_name: Option<&str>,
    sites: &mut Vec<ComptimeSite>,
) {
    match &statement.kind {
        StmtKind::Let { name, value, .. } => {
            if let Some(value) = value {
                collect_comptime_sites_in_expr(
                    value,
                    package,
                    static_names,
                    runtime_scopes,
                    direct_static_name,
                    sites,
                );
            }
            runtime_scopes
                .last_mut()
                .expect("block collection always has a scope")
                .insert(name.clone());
        }
        StmtKind::Return { value } => {
            if let Some(value) = value {
                collect_comptime_sites_in_expr(
                    value,
                    package,
                    static_names,
                    runtime_scopes,
                    direct_static_name,
                    sites,
                );
            }
        }
        StmtKind::Assign { target, value, .. } => {
            collect_comptime_sites_in_expr(target, package, static_names, runtime_scopes, None, sites);
            collect_comptime_sites_in_expr(
                value,
                package,
                static_names,
                runtime_scopes,
                direct_static_name,
                sites,
            );
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_comptime_sites_in_expr(condition, package, static_names, runtime_scopes, None, sites);
            collect_comptime_sites_in_block(
                then_branch,
                package,
                static_names,
                runtime_scopes,
                None,
                sites,
            );
            if let Some(else_branch) = else_branch {
                match else_branch {
                    ElseBranch::Block(block) => collect_comptime_sites_in_block(
                        block,
                        package,
                        static_names,
                        runtime_scopes,
                        None,
                        sites,
                    ),
                    ElseBranch::If(statement) => {
                        collect_comptime_sites_in_statement(
                            statement,
                            package,
                            static_names,
                            runtime_scopes,
                            None,
                            sites,
                        );
                    }
                }
            }
        }
        StmtKind::While { condition, body } => {
            collect_comptime_sites_in_expr(condition, package, static_names, runtime_scopes, None, sites);
            collect_comptime_sites_in_block(body, package, static_names, runtime_scopes, None, sites);
        }
        StmtKind::For {
            name,
            iterable,
            body,
        } => {
            collect_comptime_sites_in_expr(iterable, package, static_names, runtime_scopes, None, sites);
            runtime_scopes.push(HashSet::from([name.clone()]));
            collect_comptime_sites_in_block(body, package, static_names, runtime_scopes, None, sites);
            runtime_scopes.pop();
        }
        StmtKind::Block(block) => {
            collect_comptime_sites_in_block(block, package, static_names, runtime_scopes, None, sites)
        }
        StmtKind::Expr(expr) => collect_comptime_sites_in_expr(
            expr,
            package,
            static_names,
            runtime_scopes,
            direct_static_name,
            sites,
        ),
        StmtKind::Break | StmtKind::Continue => {}
    }
}

fn collect_comptime_sites_in_expr(
    expr: &Expr,
    package: usize,
    static_names: &HashSet<String>,
    runtime_scopes: &mut Vec<HashSet<String>>,
    direct_static_name: Option<&str>,
    sites: &mut Vec<ComptimeSite>,
) {
    match &expr.kind {
        ExprKind::Comptime(inner) => {
            let id = sites.len();
            sites.push(ComptimeSite {
                id,
                package,
                span: expr.span,
                expr: (**inner).clone(),
                direct_static_name: direct_static_name.map(str::to_string),
                runtime_local_uses: runtime_local_uses_in_comptime(inner, runtime_scopes),
                runtime_static_uses: runtime_static_uses_in_comptime(
                    inner,
                    runtime_scopes,
                    static_names,
                ),
            });
        }
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_comptime_sites_in_expr(
                    item,
                    package,
                    static_names,
                    runtime_scopes,
                    None,
                    sites,
                );
            }
        }
        ExprKind::Call { callee, args } => {
            collect_comptime_sites_in_expr(
                callee,
                package,
                static_names,
                runtime_scopes,
                None,
                sites,
            );
            for arg in args {
                collect_comptime_sites_in_expr(
                    arg,
                    package,
                    static_names,
                    runtime_scopes,
                    None,
                    sites,
                );
            }
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            collect_comptime_sites_in_expr(
                object,
                package,
                static_names,
                runtime_scopes,
                None,
                sites,
            )
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_comptime_sites_in_expr(
                    &field.value,
                    package,
                    static_names,
                    runtime_scopes,
                    None,
                    sites,
                );
            }
        }
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            collect_comptime_sites_in_expr(start, package, static_names, runtime_scopes, None, sites);
            collect_comptime_sites_in_expr(end, package, static_names, runtime_scopes, None, sites);
        }
        ExprKind::Cast { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::PostfixIncrement(value) => {
            collect_comptime_sites_in_expr(value, package, static_names, runtime_scopes, None, sites)
        }
        ExprKind::Match { value, branches } => {
            collect_comptime_sites_in_expr(value, package, static_names, runtime_scopes, None, sites);
            for branch in branches {
                runtime_scopes.push(pattern_scope(&branch.pattern));
                if let Some(guard) = &branch.guard {
                    collect_comptime_sites_in_expr(
                        guard,
                        package,
                        static_names,
                        runtime_scopes,
                        None,
                        sites,
                    );
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => {
                        collect_comptime_sites_in_expr(
                            expr,
                            package,
                            static_names,
                            runtime_scopes,
                            None,
                            sites,
                        )
                    }
                    MatchBranchBody::Block(block) => {
                        collect_comptime_sites_in_block(
                            block,
                            package,
                            static_names,
                            runtime_scopes,
                            None,
                            sites,
                        )
                    }
                }
                runtime_scopes.pop();
            }
        }
        ExprKind::Lambda(function) => {
            runtime_scopes.push(params_scope(&function.params));
            collect_comptime_sites_in_body(
                &function.body,
                package,
                static_names,
                runtime_scopes,
                None,
                sites,
            );
            runtime_scopes.pop();
        }
        ExprKind::Block(block) => collect_comptime_sites_in_block(
            block,
            package,
            static_names,
            runtime_scopes,
            None,
            sites,
        ),
        ExprKind::Identifier(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn runtime_local_uses_in_comptime(
    expr: &Expr,
    runtime_scopes: &[HashSet<String>],
) -> Vec<RuntimeLocalUse> {
    let mut uses = Vec::new();
    let mut comptime_scopes = Vec::new();
    collect_runtime_local_uses_in_comptime_expr(
        expr,
        runtime_scopes,
        &mut comptime_scopes,
        &mut uses,
    );
    uses
}

fn runtime_static_uses_in_comptime(
    expr: &Expr,
    runtime_scopes: &[HashSet<String>],
    static_names: &HashSet<String>,
) -> Vec<RuntimeLocalUse> {
    let mut uses = Vec::new();
    let mut comptime_scopes = Vec::new();
    collect_runtime_static_uses_in_comptime_expr(
        expr,
        runtime_scopes,
        static_names,
        &mut comptime_scopes,
        &mut uses,
    );
    uses
}

fn collect_runtime_static_uses_in_comptime_expr(
    expr: &Expr,
    runtime_scopes: &[HashSet<String>],
    static_names: &HashSet<String>,
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            if !name_visible(comptime_scopes, name)
                && !name_visible(runtime_scopes, name)
                && static_names.contains(name)
            {
                uses.push(RuntimeLocalUse {
                    name: name.clone(),
                    span: expr.span,
                });
            }
        }
        ExprKind::Comptime(inner) => collect_runtime_static_uses_in_comptime_expr(
            inner,
            runtime_scopes,
            static_names,
            comptime_scopes,
            uses,
        ),
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_runtime_static_uses_in_comptime_expr(
                    item,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
            }
        }
        ExprKind::Call { callee, args } => {
            collect_runtime_static_uses_in_comptime_expr(
                callee,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            );
            for arg in args {
                collect_runtime_static_uses_in_comptime_expr(
                    arg,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
            }
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            collect_runtime_static_uses_in_comptime_expr(
                object,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            )
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_runtime_static_uses_in_comptime_expr(
                    &field.value,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
            }
        }
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            collect_runtime_static_uses_in_comptime_expr(
                start,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            );
            collect_runtime_static_uses_in_comptime_expr(
                end,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            );
        }
        ExprKind::Cast { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::PostfixIncrement(value) => collect_runtime_static_uses_in_comptime_expr(
            value,
            runtime_scopes,
            static_names,
            comptime_scopes,
            uses,
        ),
        ExprKind::Match { value, branches } => {
            collect_runtime_static_uses_in_comptime_expr(
                value,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            );
            for branch in branches {
                comptime_scopes.push(pattern_scope(&branch.pattern));
                if let Some(guard) = &branch.guard {
                    collect_runtime_static_uses_in_comptime_expr(
                        guard,
                        runtime_scopes,
                        static_names,
                        comptime_scopes,
                        uses,
                    );
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => collect_runtime_static_uses_in_comptime_expr(
                        expr,
                        runtime_scopes,
                        static_names,
                        comptime_scopes,
                        uses,
                    ),
                    MatchBranchBody::Block(block) => collect_runtime_static_uses_in_comptime_block(
                        block,
                        runtime_scopes,
                        static_names,
                        comptime_scopes,
                        uses,
                    ),
                }
                comptime_scopes.pop();
            }
        }
        ExprKind::Lambda(function) => {
            comptime_scopes.push(params_scope(&function.params));
            collect_runtime_static_uses_in_comptime_body(
                &function.body,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            );
            comptime_scopes.pop();
        }
        ExprKind::Block(block) => collect_runtime_static_uses_in_comptime_block(
            block,
            runtime_scopes,
            static_names,
            comptime_scopes,
            uses,
        ),
        ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn collect_runtime_static_uses_in_comptime_body(
    body: &FunctionBody,
    runtime_scopes: &[HashSet<String>],
    static_names: &HashSet<String>,
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    match body {
        FunctionBody::Block(block) => collect_runtime_static_uses_in_comptime_block(
            block,
            runtime_scopes,
            static_names,
            comptime_scopes,
            uses,
        ),
        FunctionBody::Expr(expr) => collect_runtime_static_uses_in_comptime_expr(
            expr,
            runtime_scopes,
            static_names,
            comptime_scopes,
            uses,
        ),
    }
}

fn collect_runtime_static_uses_in_comptime_block(
    block: &Block,
    runtime_scopes: &[HashSet<String>],
    static_names: &HashSet<String>,
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    comptime_scopes.push(HashSet::new());
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value {
                    collect_runtime_static_uses_in_comptime_expr(
                        value,
                        runtime_scopes,
                        static_names,
                        comptime_scopes,
                        uses,
                    );
                }
                comptime_scopes
                    .last_mut()
                    .expect("comptime block collection always has a scope")
                    .insert(name.clone());
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    collect_runtime_static_uses_in_comptime_expr(
                        value,
                        runtime_scopes,
                        static_names,
                        comptime_scopes,
                        uses,
                    );
                }
            }
            StmtKind::Assign { target, value, .. } => {
                collect_runtime_static_uses_in_comptime_expr(
                    target,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
                collect_runtime_static_uses_in_comptime_expr(
                    value,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_runtime_static_uses_in_comptime_expr(
                    condition,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
                collect_runtime_static_uses_in_comptime_block(
                    then_branch,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => collect_runtime_static_uses_in_comptime_block(
                            block,
                            runtime_scopes,
                            static_names,
                            comptime_scopes,
                            uses,
                        ),
                        ElseBranch::If(statement) => {
                            collect_runtime_static_uses_in_comptime_statement(
                                statement,
                                runtime_scopes,
                                static_names,
                                comptime_scopes,
                                uses,
                            );
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                collect_runtime_static_uses_in_comptime_expr(
                    condition,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
                collect_runtime_static_uses_in_comptime_block(
                    body,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                collect_runtime_static_uses_in_comptime_expr(
                    iterable,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
                comptime_scopes.push(HashSet::from([name.clone()]));
                collect_runtime_static_uses_in_comptime_block(
                    body,
                    runtime_scopes,
                    static_names,
                    comptime_scopes,
                    uses,
                );
                comptime_scopes.pop();
            }
            StmtKind::Block(block) => collect_runtime_static_uses_in_comptime_block(
                block,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            ),
            StmtKind::Expr(expr) => collect_runtime_static_uses_in_comptime_expr(
                expr,
                runtime_scopes,
                static_names,
                comptime_scopes,
                uses,
            ),
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
    comptime_scopes.pop();
}

fn collect_runtime_static_uses_in_comptime_statement(
    statement: &Stmt,
    runtime_scopes: &[HashSet<String>],
    static_names: &HashSet<String>,
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    collect_runtime_static_uses_in_comptime_block(
        &Block {
            statements: vec![statement.clone()],
            span: statement.span,
        },
        runtime_scopes,
        static_names,
        comptime_scopes,
        uses,
    );
}

fn collect_runtime_local_uses_in_comptime_expr(
    expr: &Expr,
    runtime_scopes: &[HashSet<String>],
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            if !name_visible(comptime_scopes, name) && name_visible(runtime_scopes, name) {
                uses.push(RuntimeLocalUse {
                    name: name.clone(),
                    span: expr.span,
                });
            }
        }
        ExprKind::Comptime(inner) => {
            collect_runtime_local_uses_in_comptime_expr(inner, runtime_scopes, comptime_scopes, uses)
        }
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_runtime_local_uses_in_comptime_expr(
                    item,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
            }
        }
        ExprKind::Call { callee, args } => {
            collect_runtime_local_uses_in_comptime_expr(
                callee,
                runtime_scopes,
                comptime_scopes,
                uses,
            );
            for arg in args {
                collect_runtime_local_uses_in_comptime_expr(
                    arg,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
            }
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            collect_runtime_local_uses_in_comptime_expr(
                object,
                runtime_scopes,
                comptime_scopes,
                uses,
            )
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_runtime_local_uses_in_comptime_expr(
                    &field.value,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
            }
        }
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            collect_runtime_local_uses_in_comptime_expr(
                start,
                runtime_scopes,
                comptime_scopes,
                uses,
            );
            collect_runtime_local_uses_in_comptime_expr(
                end,
                runtime_scopes,
                comptime_scopes,
                uses,
            );
        }
        ExprKind::Cast { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::PostfixIncrement(value) => collect_runtime_local_uses_in_comptime_expr(
            value,
            runtime_scopes,
            comptime_scopes,
            uses,
        ),
        ExprKind::Match { value, branches } => {
            collect_runtime_local_uses_in_comptime_expr(
                value,
                runtime_scopes,
                comptime_scopes,
                uses,
            );
            for branch in branches {
                comptime_scopes.push(pattern_scope(&branch.pattern));
                if let Some(guard) = &branch.guard {
                    collect_runtime_local_uses_in_comptime_expr(
                        guard,
                        runtime_scopes,
                        comptime_scopes,
                        uses,
                    );
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => collect_runtime_local_uses_in_comptime_expr(
                        expr,
                        runtime_scopes,
                        comptime_scopes,
                        uses,
                    ),
                    MatchBranchBody::Block(block) => collect_runtime_local_uses_in_comptime_block(
                        block,
                        runtime_scopes,
                        comptime_scopes,
                        uses,
                    ),
                }
                comptime_scopes.pop();
            }
        }
        ExprKind::Lambda(function) => {
            comptime_scopes.push(params_scope(&function.params));
            collect_runtime_local_uses_in_comptime_body(
                &function.body,
                runtime_scopes,
                comptime_scopes,
                uses,
            );
            comptime_scopes.pop();
        }
        ExprKind::Block(block) => collect_runtime_local_uses_in_comptime_block(
            block,
            runtime_scopes,
            comptime_scopes,
            uses,
        ),
        ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn collect_runtime_local_uses_in_comptime_body(
    body: &FunctionBody,
    runtime_scopes: &[HashSet<String>],
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    match body {
        FunctionBody::Block(block) => collect_runtime_local_uses_in_comptime_block(
            block,
            runtime_scopes,
            comptime_scopes,
            uses,
        ),
        FunctionBody::Expr(expr) => collect_runtime_local_uses_in_comptime_expr(
            expr,
            runtime_scopes,
            comptime_scopes,
            uses,
        ),
    }
}

fn collect_runtime_local_uses_in_comptime_block(
    block: &Block,
    runtime_scopes: &[HashSet<String>],
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    comptime_scopes.push(HashSet::new());
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { name, value, .. } => {
                if let Some(value) = value {
                    collect_runtime_local_uses_in_comptime_expr(
                        value,
                        runtime_scopes,
                        comptime_scopes,
                        uses,
                    );
                }
                comptime_scopes
                    .last_mut()
                    .expect("comptime block collection always has a scope")
                    .insert(name.clone());
            }
            StmtKind::Return { value } => {
                if let Some(value) = value {
                    collect_runtime_local_uses_in_comptime_expr(
                        value,
                        runtime_scopes,
                        comptime_scopes,
                        uses,
                    );
                }
            }
            StmtKind::Assign { target, value, .. } => {
                collect_runtime_local_uses_in_comptime_expr(
                    target,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
                collect_runtime_local_uses_in_comptime_expr(
                    value,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_runtime_local_uses_in_comptime_expr(
                    condition,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
                collect_runtime_local_uses_in_comptime_block(
                    then_branch,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => collect_runtime_local_uses_in_comptime_block(
                            block,
                            runtime_scopes,
                            comptime_scopes,
                            uses,
                        ),
                        ElseBranch::If(statement) => {
                            collect_runtime_local_uses_in_comptime_statement(
                                statement,
                                runtime_scopes,
                                comptime_scopes,
                                uses,
                            );
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                collect_runtime_local_uses_in_comptime_expr(
                    condition,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
                collect_runtime_local_uses_in_comptime_block(
                    body,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
            }
            StmtKind::For {
                name,
                iterable,
                body,
            } => {
                collect_runtime_local_uses_in_comptime_expr(
                    iterable,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
                comptime_scopes.push(HashSet::from([name.clone()]));
                collect_runtime_local_uses_in_comptime_block(
                    body,
                    runtime_scopes,
                    comptime_scopes,
                    uses,
                );
                comptime_scopes.pop();
            }
            StmtKind::Block(block) => collect_runtime_local_uses_in_comptime_block(
                block,
                runtime_scopes,
                comptime_scopes,
                uses,
            ),
            StmtKind::Expr(expr) => collect_runtime_local_uses_in_comptime_expr(
                expr,
                runtime_scopes,
                comptime_scopes,
                uses,
            ),
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
    comptime_scopes.pop();
}

fn collect_runtime_local_uses_in_comptime_statement(
    statement: &Stmt,
    runtime_scopes: &[HashSet<String>],
    comptime_scopes: &mut Vec<HashSet<String>>,
    uses: &mut Vec<RuntimeLocalUse>,
) {
    collect_runtime_local_uses_in_comptime_block(
        &Block {
            statements: vec![statement.clone()],
            span: statement.span,
        },
        runtime_scopes,
        comptime_scopes,
        uses,
    );
}

fn pattern_scope(pattern: &Pattern) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_pattern_scope(pattern, &mut names);
    names
}

fn collect_pattern_scope(pattern: &Pattern, names: &mut HashSet<String>) {
    match pattern {
        Pattern::Or { alternatives, .. } => {
            for alternative in alternatives {
                collect_pattern_scope(alternative, names);
            }
        }
        Pattern::Variant { payload, .. } => {
            if let Some(payload) = payload {
                collect_pattern_scope(payload, names);
            }
        }
        Pattern::Struct { fields, .. } => {
            for field in fields {
                collect_pattern_scope(&field.pattern, names);
            }
        }
        Pattern::Binding { name, .. } => {
            names.insert(name.clone());
        }
        Pattern::String { .. }
        | Pattern::Bool { .. }
        | Pattern::Number { .. }
        | Pattern::Range { .. }
        | Pattern::Wildcard { .. } => {}
    }
}

fn name_visible(scopes: &[HashSet<String>], name: &str) -> bool {
    scopes.iter().rev().any(|scope| scope.contains(name))
}

fn run_comptime_site(
    program: &Program,
    item_packages: &[usize],
    packages: &[Package],
    site: &ComptimeSite,
    source_files: &[LoweringSourceFile],
    expanded_comptime_statics: &HashSet<String>,
) -> Result<ComptimeValue, String> {
    let mut runner = program.clone();
    let mut runner_items = Vec::new();
    for (index, mut item) in runner.items.into_iter().enumerate() {
        if let Item::StaticVar(static_) = &item {
            if expr_contains_comptime(&static_.value)
                || !expanded_comptime_statics.contains(&static_.name)
            {
                continue;
            }
        }
        let package = item_packages.get(index).copied().unwrap_or(site.package);
        prepare_runner_item(&mut item, package, packages);
        runner_items.push(item);
    }
    runner.items = runner_items;
    let mut entry_expr = site.expr.clone();
    prepare_runner_expr(&mut entry_expr, site.package, packages);
    let entry_return_type = infer_runner_entry_type(&entry_expr, &runner);

    let entry_name = format!("__gust_comptime_site_{}", site.id);
    runner.items.retain(|item| {
        !matches!(item, Item::Function(function) if function.name.as_deref() == Some("main"))
    });
    runner.items.push(Item::Function(FunctionDecl {
        name: Some(entry_name.clone()),
        exported: false,
        type_params: Vec::new(),
        type_param_bounds: Vec::new(),
        params: Vec::new(),
        return_type: entry_return_type,
        body: FunctionBody::Block(Block {
            span: site.span,
            statements: vec![Stmt {
                span: site.span,
                kind: StmtKind::Return {
                    value: Some(entry_expr),
                },
            }],
        }),
        span: site.span,
    }));
    runner.items.push(Item::Function(FunctionDecl {
        name: Some("main".to_string()),
        exported: false,
        type_params: Vec::new(),
        type_param_bounds: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body: FunctionBody::Block(Block {
            statements: vec![Stmt {
                span: site.span,
                kind: StmtKind::If {
                    condition: Expr {
                        span: site.span,
                        kind: ExprKind::Bool(false),
                    },
                    then_branch: Block {
                        span: site.span,
                        statements: vec![Stmt {
                            span: site.span,
                            kind: StmtKind::Expr(Expr {
                                span: site.span,
                                kind: ExprKind::Call {
                                    callee: Box::new(Expr {
                                        span: site.span,
                                        kind: ExprKind::Identifier(entry_name.clone()),
                                    }),
                                    args: Vec::new(),
                                },
                            }),
                        }],
                    },
                    else_branch: None,
                },
            }],
            span: site.span,
        }),
        span: site.span,
    }));

    let diagnostics = validate(&runner);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(format!(
            "failed to compile comptime runner: {}",
            diagnostics
                .into_iter()
                .filter(|diagnostic| diagnostic.severity == Severity::Error)
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    let lowered = lower_program_with_source_files(&runner, source_files.to_vec()).map_err(|diagnostics| {
        format!(
            "failed to lower comptime runner: {}",
            diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    let temp_dir = std::env::temp_dir();
    let unique = unique_comptime_id(site.id);
    let c_path = temp_dir.join(format!("{unique}.c"));
    let exe_path = temp_dir.join(format!("{unique}.bin"));
    let result_path = temp_dir.join(format!("{unique}.result"));
    let c_source = emit_c_for_comptime(
        &lowered,
        CCodegenOptions {
            gc_stress: false,
        },
        CComptimeOptions {
            entry_name,
            result_path: result_path.to_string_lossy().into_owned(),
        },
    );
    compile_c_source_to_binary(&c_source, &c_path, &exe_path)
        .map_err(|error| format!("failed to compile comptime runner C: {error}"))?;

    let output = Command::new(&exe_path)
        .output()
        .map_err(|error| format!("failed to run comptime runner: {error}"))?;
    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&exe_path);
    if !output.status.success() {
        let mut message = format!("comptime runner failed with status {}", output.status);
        if !output.stderr.is_empty() {
            message.push_str(": ");
            message.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        if !output.stdout.is_empty() {
            message.push_str(": stdout: ");
            message.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        return Err(message);
    }

    let bytes = fs::read(&result_path)
        .map_err(|error| format!("failed to read comptime result artifact: {error}"))?;
    let _ = fs::remove_file(&result_path);
    decode_comptime_value(&bytes)
}

fn unique_comptime_id(site: usize) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let counter = NEXT_COMPTIME_RUNNER.fetch_add(1, Ordering::Relaxed);
    format!("gustc-comptime-{}-{site}-{counter}-{nanos}", std::process::id())
}

fn expr_contains_comptime(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Comptime(_) => true,
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            items.iter().any(expr_contains_comptime)
        }
        ExprKind::Call { callee, args } => {
            expr_contains_comptime(callee) || args.iter().any(expr_contains_comptime)
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            expr_contains_comptime(object)
        }
        ExprKind::StructInit { fields, .. } => fields
            .iter()
            .any(|field| expr_contains_comptime(&field.value)),
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            expr_contains_comptime(start) || expr_contains_comptime(end)
        }
        ExprKind::Cast { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::PostfixIncrement(value) => expr_contains_comptime(value),
        ExprKind::Match { value, branches } => {
            expr_contains_comptime(value)
                || branches.iter().any(|branch| {
                    branch.guard.as_ref().is_some_and(expr_contains_comptime)
                        || match &branch.body {
                            MatchBranchBody::Expr(expr) => expr_contains_comptime(expr),
                            MatchBranchBody::Block(block) => block_contains_comptime(block),
                        }
                })
        }
        ExprKind::Lambda(function) => body_contains_comptime(&function.body),
        ExprKind::Block(block) => block_contains_comptime(block),
        ExprKind::Identifier(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => false,
    }
}

fn body_contains_comptime(body: &FunctionBody) -> bool {
    match body {
        FunctionBody::Block(block) => block_contains_comptime(block),
        FunctionBody::Expr(expr) => expr_contains_comptime(expr),
    }
}

fn block_contains_comptime(block: &Block) -> bool {
    block.statements.iter().any(|statement| match &statement.kind {
        StmtKind::Let { value, .. } | StmtKind::Return { value } => {
            value.as_ref().is_some_and(expr_contains_comptime)
        }
        StmtKind::Assign { target, value, .. } => {
            expr_contains_comptime(target) || expr_contains_comptime(value)
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_contains_comptime(condition)
                || block_contains_comptime(then_branch)
                || else_branch.as_ref().is_some_and(|else_branch| match else_branch {
                    ElseBranch::Block(block) => block_contains_comptime(block),
                    ElseBranch::If(statement) => {
                        block_contains_comptime(&Block {
                            statements: vec![(**statement).clone()],
                            span: statement.span,
                        })
                    }
                })
        }
        StmtKind::While { condition, body } => {
            expr_contains_comptime(condition) || block_contains_comptime(body)
        }
        StmtKind::For { iterable, body, .. } => {
            expr_contains_comptime(iterable) || block_contains_comptime(body)
        }
        StmtKind::Block(block) => block_contains_comptime(block),
        StmtKind::Expr(expr) => expr_contains_comptime(expr),
        StmtKind::Break | StmtKind::Continue => false,
    })
}

fn infer_runner_entry_type(expr: &Expr, program: &Program) -> Option<TypeRef> {
    let mut static_types = HashMap::new();
    for item in &program.items {
        let Item::StaticVar(static_) = item else {
            continue;
        };
        if let Some(type_ref) = static_
            .type_annotation
            .clone()
            .or_else(|| infer_runner_expr_type(&static_.value, &static_types))
        {
            static_types.insert(static_.name.clone(), type_ref);
        }
    }

    infer_runner_expr_type(expr, &static_types)
}

fn infer_runner_expr_type(expr: &Expr, static_types: &HashMap<String, TypeRef>) -> Option<TypeRef> {
    let inferred = |name: &str| TypeRef {
        name: name.to_string(),
        args: Vec::new(),
        bindings: Vec::new(),
        function: None,
        span: expr.span,
    };

    match &expr.kind {
        ExprKind::Identifier(name) => static_types.get(name).cloned(),
        ExprKind::Number(value) => Some(inferred(if crate::ast::number_literal_is_float(value) {
            "f64"
        } else {
            "i32"
        })),
        ExprKind::String(_) => Some(inferred("string")),
        ExprKind::Char(_) => Some(inferred("char")),
        ExprKind::Bool(_) => Some(inferred("bool")),
        ExprKind::Binary { left, op, right } => match op {
            BinaryOp::LogicalAnd
            | BinaryOp::LogicalOr
            | BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => Some(inferred("bool")),
            _ => {
                let left = infer_runner_expr_type(left, static_types)?;
                let right = infer_runner_expr_type(right, static_types)?;
                (left.name == right.name && left.args.is_empty() && right.args.is_empty())
                    .then_some(left)
            }
        },
        ExprKind::Cast { type_ref, .. } => Some(type_ref.clone()),
        ExprKind::Block(block) => block.statements.iter().find_map(|statement| {
            let StmtKind::Return { value: Some(value) } = &statement.kind else {
                return None;
            };
            infer_runner_expr_type(value, static_types)
        }),
        _ => None,
    }
}

fn collect_lambda_sources(program: &Program) -> HashMap<String, FunctionDecl> {
    let mut lambdas = HashMap::new();
    let mut next_id = 0usize;
    for item in &program.items {
        collect_lambda_sources_in_item(item, &mut next_id, &mut lambdas);
    }
    lambdas
}

fn collect_lambda_sources_in_item(
    item: &Item,
    next_id: &mut usize,
    lambdas: &mut HashMap<String, FunctionDecl>,
) {
    match item {
        Item::Enum(item) => {
            for member in &item.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        collect_lambda_sources_in_body(&function.body, next_id, lambdas);
                    }
                    StructMember::Field(_) => {}
                }
            }
        }
        Item::Struct(item) => {
            for member in &item.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        collect_lambda_sources_in_body(&function.body, next_id, lambdas);
                    }
                    StructMember::Field(_) => {}
                }
            }
        }
        Item::Trait(item) => {
            for method in &item.methods {
                if let Some(body) = &method.body {
                    collect_lambda_sources_in_body(body, next_id, lambdas);
                }
            }
        }
        Item::Impl(item) => {
            for member in &item.methods {
                collect_lambda_sources_in_body(&member.function.body, next_id, lambdas);
            }
        }
        Item::Extension(item) => collect_lambda_sources_in_body(&item.function.body, next_id, lambdas),
        Item::Function(function) => collect_lambda_sources_in_body(&function.body, next_id, lambdas),
        Item::StaticVar(item) => collect_lambda_sources_in_expr(&item.value, next_id, lambdas),
        Item::Import(_) => {}
    }
}

fn collect_lambda_sources_in_body(
    body: &FunctionBody,
    next_id: &mut usize,
    lambdas: &mut HashMap<String, FunctionDecl>,
) {
    match body {
        FunctionBody::Block(block) => collect_lambda_sources_in_block(block, next_id, lambdas),
        FunctionBody::Expr(expr) => collect_lambda_sources_in_expr(expr, next_id, lambdas),
    }
}

fn collect_lambda_sources_in_block(
    block: &Block,
    next_id: &mut usize,
    lambdas: &mut HashMap<String, FunctionDecl>,
) {
    for statement in &block.statements {
        match &statement.kind {
            StmtKind::Let { value, .. } | StmtKind::Return { value } => {
                if let Some(value) = value {
                    collect_lambda_sources_in_expr(value, next_id, lambdas);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                collect_lambda_sources_in_expr(target, next_id, lambdas);
                collect_lambda_sources_in_expr(value, next_id, lambdas);
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_lambda_sources_in_expr(condition, next_id, lambdas);
                collect_lambda_sources_in_block(then_branch, next_id, lambdas);
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => {
                            collect_lambda_sources_in_block(block, next_id, lambdas)
                        }
                        ElseBranch::If(statement) => {
                            collect_lambda_sources_in_statement(statement, next_id, lambdas)
                        }
                    }
                }
            }
            StmtKind::While { condition, body } => {
                collect_lambda_sources_in_expr(condition, next_id, lambdas);
                collect_lambda_sources_in_block(body, next_id, lambdas);
            }
            StmtKind::For { iterable, body, .. } => {
                collect_lambda_sources_in_expr(iterable, next_id, lambdas);
                collect_lambda_sources_in_block(body, next_id, lambdas);
            }
            StmtKind::Block(block) => collect_lambda_sources_in_block(block, next_id, lambdas),
            StmtKind::Expr(expr) => collect_lambda_sources_in_expr(expr, next_id, lambdas),
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

fn collect_lambda_sources_in_statement(
    statement: &Stmt,
    next_id: &mut usize,
    lambdas: &mut HashMap<String, FunctionDecl>,
) {
    let block = Block {
        statements: vec![statement.clone()],
        span: statement.span,
    };
    collect_lambda_sources_in_block(&block, next_id, lambdas);
}

fn collect_lambda_sources_in_expr(
    expr: &Expr,
    next_id: &mut usize,
    lambdas: &mut HashMap<String, FunctionDecl>,
) {
    match &expr.kind {
        ExprKind::Lambda(function) => {
            lambdas.insert(format!("lambda{}", *next_id), function.clone());
            *next_id += 1;
            collect_lambda_sources_in_body(&function.body, next_id, lambdas);
        }
        ExprKind::Block(block) => {
            if lambda_source_block_uses_iife(block) {
                *next_id += 1;
            }
            collect_lambda_sources_in_block(block, next_id, lambdas);
        }
        ExprKind::Comptime(expr) => collect_lambda_sources_in_expr(expr, next_id, lambdas),
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                collect_lambda_sources_in_expr(item, next_id, lambdas);
            }
        }
        ExprKind::Call { callee, args } => {
            collect_lambda_sources_in_expr(callee, next_id, lambdas);
            for arg in args {
                collect_lambda_sources_in_expr(arg, next_id, lambdas);
            }
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            collect_lambda_sources_in_expr(object, next_id, lambdas)
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                collect_lambda_sources_in_expr(&field.value, next_id, lambdas);
            }
        }
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            collect_lambda_sources_in_expr(start, next_id, lambdas);
            collect_lambda_sources_in_expr(end, next_id, lambdas);
        }
        ExprKind::Cast { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::PostfixIncrement(value) => {
            collect_lambda_sources_in_expr(value, next_id, lambdas)
        }
        ExprKind::Match { value, branches } => {
            collect_lambda_sources_in_expr(value, next_id, lambdas);
            for branch in branches {
                if let Some(guard) = &branch.guard {
                    collect_lambda_sources_in_expr(guard, next_id, lambdas);
                }
                match &branch.body {
                    MatchBranchBody::Expr(expr) => {
                        collect_lambda_sources_in_expr(expr, next_id, lambdas)
                    }
                    MatchBranchBody::Block(block) => {
                        collect_lambda_sources_in_block(block, next_id, lambdas)
                    }
                }
            }
        }
        ExprKind::Identifier(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn lambda_source_block_uses_iife(block: &Block) -> bool {
    let Some((last_statement, setup_statements)) = block.statements.split_last() else {
        return false;
    };
    setup_statements.iter().any(lambda_source_statement_contains_return)
        || !matches!(last_statement.kind, StmtKind::Return { value: Some(_) })
}

fn lambda_source_statement_contains_return(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Return { .. } => true,
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            lambda_source_block_contains_return(then_branch)
                || else_branch.as_ref().is_some_and(|else_branch| match else_branch {
                    ElseBranch::Block(block) => lambda_source_block_contains_return(block),
                    ElseBranch::If(statement) => {
                        lambda_source_statement_contains_return(statement)
                    }
                })
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } | StmtKind::Block(body) => {
            lambda_source_block_contains_return(body)
        }
        StmtKind::Let { .. }
        | StmtKind::Assign { .. }
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Expr(_) => false,
    }
}

fn lambda_source_block_contains_return(block: &Block) -> bool {
    block
        .statements
        .iter()
        .any(lambda_source_statement_contains_return)
}

fn prepare_runner_item(item: &mut Item, package: usize, packages: &[Package]) {
    match item {
        Item::Enum(item) => {
            for member in &mut item.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        prepare_runner_body(&mut function.body, package, packages);
                    }
                    StructMember::Field(_) => {}
                }
            }
        }
        Item::Struct(item) => {
            for member in &mut item.members {
                match member {
                    StructMember::Method(function) | StructMember::StaticMethod(function) => {
                        prepare_runner_body(&mut function.body, package, packages);
                    }
                    StructMember::Field(_) => {}
                }
            }
        }
        Item::Trait(item) => {
            for method in &mut item.methods {
                if let Some(body) = &mut method.body {
                    prepare_runner_body(body, package, packages);
                }
            }
        }
        Item::Impl(item) => {
            for member in &mut item.methods {
                prepare_runner_body(&mut member.function.body, package, packages);
            }
        }
        Item::Extension(item) => prepare_runner_body(&mut item.function.body, package, packages),
        Item::Function(function) => prepare_runner_body(&mut function.body, package, packages),
        Item::StaticVar(item) => prepare_runner_expr(&mut item.value, package, packages),
        Item::Import(_) => {}
    }
}

fn prepare_runner_body(body: &mut FunctionBody, package: usize, packages: &[Package]) {
    match body {
        FunctionBody::Block(block) => prepare_runner_block(block, package, packages),
        FunctionBody::Expr(expr) => prepare_runner_expr(expr, package, packages),
    }
}

fn prepare_runner_block(block: &mut Block, package: usize, packages: &[Package]) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            StmtKind::Let { value, .. } | StmtKind::Return { value } => {
                if let Some(value) = value {
                    prepare_runner_expr(value, package, packages);
                }
            }
            StmtKind::Assign { target, value, .. } => {
                prepare_runner_expr(target, package, packages);
                prepare_runner_expr(value, package, packages);
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                prepare_runner_expr(condition, package, packages);
                prepare_runner_block(then_branch, package, packages);
                if let Some(else_branch) = else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => prepare_runner_block(block, package, packages),
                        ElseBranch::If(statement) => prepare_runner_statement(statement, package, packages),
                    }
                }
            }
            StmtKind::While { condition, body } => {
                prepare_runner_expr(condition, package, packages);
                prepare_runner_block(body, package, packages);
            }
            StmtKind::For { iterable, body, .. } => {
                prepare_runner_expr(iterable, package, packages);
                prepare_runner_block(body, package, packages);
            }
            StmtKind::Block(block) => prepare_runner_block(block, package, packages),
            StmtKind::Expr(expr) => prepare_runner_expr(expr, package, packages),
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

fn prepare_runner_statement(statement: &mut Stmt, package: usize, packages: &[Package]) {
    match &mut statement.kind {
        StmtKind::Let { value, .. } | StmtKind::Return { value } => {
            if let Some(value) = value {
                prepare_runner_expr(value, package, packages);
            }
        }
        StmtKind::Assign { target, value, .. } => {
            prepare_runner_expr(target, package, packages);
            prepare_runner_expr(value, package, packages);
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            prepare_runner_expr(condition, package, packages);
            prepare_runner_block(then_branch, package, packages);
            if let Some(else_branch) = else_branch {
                match else_branch {
                    ElseBranch::Block(block) => prepare_runner_block(block, package, packages),
                    ElseBranch::If(statement) => prepare_runner_statement(statement, package, packages),
                }
            }
        }
        StmtKind::While { condition, body } => {
            prepare_runner_expr(condition, package, packages);
            prepare_runner_block(body, package, packages);
        }
        StmtKind::For { iterable, body, .. } => {
            prepare_runner_expr(iterable, package, packages);
            prepare_runner_block(body, package, packages);
        }
        StmtKind::Block(block) => prepare_runner_block(block, package, packages),
        StmtKind::Expr(expr) => prepare_runner_expr(expr, package, packages),
        StmtKind::Break | StmtKind::Continue => {}
    }
}

fn prepare_runner_expr(expr: &mut Expr, package: usize, packages: &[Package]) {
    if let Some(value) = comptime_permission_value(expr, package, packages) {
        expr.kind = ExprKind::Bool(value);
        return;
    }

    match &mut expr.kind {
        ExprKind::Comptime(inner) => {
            let mut replacement = (**inner).clone();
            prepare_runner_expr(&mut replacement, package, packages);
            *expr = replacement;
        }
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => {
            for item in items {
                prepare_runner_expr(item, package, packages);
            }
        }
        ExprKind::Call { callee, args } => {
            prepare_runner_expr(callee, package, packages);
            for arg in args {
                prepare_runner_expr(arg, package, packages);
            }
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            prepare_runner_expr(object, package, packages)
        }
        ExprKind::StructInit { fields, .. } => {
            for field in fields {
                prepare_runner_expr(&mut field.value, package, packages);
            }
        }
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            prepare_runner_expr(start, package, packages);
            prepare_runner_expr(end, package, packages);
        }
        ExprKind::Cast { value, .. }
        | ExprKind::Unary { operand: value, .. }
        | ExprKind::PostfixIncrement(value) => prepare_runner_expr(value, package, packages),
        ExprKind::Match { value, branches } => {
            prepare_runner_expr(value, package, packages);
            for branch in branches {
                if let Some(guard) = &mut branch.guard {
                    prepare_runner_expr(guard, package, packages);
                }
                match &mut branch.body {
                    MatchBranchBody::Expr(expr) => prepare_runner_expr(expr, package, packages),
                    MatchBranchBody::Block(block) => prepare_runner_block(block, package, packages),
                }
            }
        }
        ExprKind::Lambda(function) => prepare_runner_body(&mut function.body, package, packages),
        ExprKind::Block(block) => prepare_runner_block(block, package, packages),
        ExprKind::Identifier(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => {}
    }
}

fn comptime_permission_value(expr: &Expr, package: usize, packages: &[Package]) -> Option<bool> {
    let ExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    if args.len() != 1 {
        return None;
    }
    let ExprKind::String(value) = &args[0].kind else {
        return None;
    };
    match expression_path(callee).as_deref() {
        Some("comptime.permissions.fs.canRead") => Some(can_read_fs(packages, package, value)),
        Some("comptime.permissions.env.canRead") => Some(can_read_env(packages, package, value)),
        _ => None,
    }
}

fn replace_comptime_at_span(program: &mut Program, span: Span, value: Expr) -> bool {
    for item in &mut program.items {
        if replace_comptime_at_span_in_item(item, span, value.clone()) {
            return true;
        }
    }
    false
}

fn replace_comptime_at_span_in_item(item: &mut Item, span: Span, value: Expr) -> bool {
    match item {
        Item::Enum(item) => item.members.iter_mut().any(|member| match member {
            StructMember::Method(function) | StructMember::StaticMethod(function) => {
                replace_comptime_at_span_in_body(&mut function.body, span, value.clone())
            }
            StructMember::Field(_) => false,
        }),
        Item::Struct(item) => item.members.iter_mut().any(|member| match member {
            StructMember::Method(function) | StructMember::StaticMethod(function) => {
                replace_comptime_at_span_in_body(&mut function.body, span, value.clone())
            }
            StructMember::Field(_) => false,
        }),
        Item::Trait(item) => item.methods.iter_mut().any(|method| {
            method
                .body
                .as_mut()
                .is_some_and(|body| replace_comptime_at_span_in_body(body, span, value.clone()))
        }),
        Item::Impl(item) => item.methods.iter_mut().any(|member| {
            replace_comptime_at_span_in_body(&mut member.function.body, span, value.clone())
        }),
        Item::Extension(item) => replace_comptime_at_span_in_body(&mut item.function.body, span, value),
        Item::Function(function) => replace_comptime_at_span_in_body(&mut function.body, span, value),
        Item::StaticVar(item) => replace_comptime_at_span_in_expr(&mut item.value, span, value),
        Item::Import(_) => false,
    }
}

fn replace_comptime_at_span_in_body(body: &mut FunctionBody, span: Span, value: Expr) -> bool {
    match body {
        FunctionBody::Block(block) => replace_comptime_at_span_in_block(block, span, value),
        FunctionBody::Expr(expr) => replace_comptime_at_span_in_expr(expr, span, value),
    }
}

fn replace_comptime_at_span_in_block(block: &mut Block, span: Span, value: Expr) -> bool {
    for statement in &mut block.statements {
        let replaced = match &mut statement.kind {
            StmtKind::Let { value: expr, .. } | StmtKind::Return { value: expr } => expr
                .as_mut()
                .is_some_and(|expr| replace_comptime_at_span_in_expr(expr, span, value.clone())),
            StmtKind::Assign { target, value: expr, .. } => {
                replace_comptime_at_span_in_expr(target, span, value.clone())
                    || replace_comptime_at_span_in_expr(expr, span, value.clone())
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                replace_comptime_at_span_in_expr(condition, span, value.clone())
                    || replace_comptime_at_span_in_block(then_branch, span, value.clone())
                    || else_branch.as_mut().is_some_and(|else_branch| match else_branch {
                        ElseBranch::Block(block) => {
                            replace_comptime_at_span_in_block(block, span, value.clone())
                        }
                        ElseBranch::If(statement) => {
                            replace_comptime_at_span_in_statement(statement, span, value.clone())
                        }
                    })
            }
            StmtKind::While { condition, body } => {
                replace_comptime_at_span_in_expr(condition, span, value.clone())
                    || replace_comptime_at_span_in_block(body, span, value.clone())
            }
            StmtKind::For { iterable, body, .. } => {
                replace_comptime_at_span_in_expr(iterable, span, value.clone())
                    || replace_comptime_at_span_in_block(body, span, value.clone())
            }
            StmtKind::Block(block) => replace_comptime_at_span_in_block(block, span, value.clone()),
            StmtKind::Expr(expr) => replace_comptime_at_span_in_expr(expr, span, value.clone()),
            StmtKind::Break | StmtKind::Continue => false,
        };
        if replaced {
            return true;
        }
    }
    false
}

fn replace_comptime_at_span_in_statement(statement: &mut Stmt, span: Span, value: Expr) -> bool {
    match &mut statement.kind {
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            replace_comptime_at_span_in_expr(condition, span, value.clone())
                || replace_comptime_at_span_in_block(then_branch, span, value.clone())
                || else_branch.as_mut().is_some_and(|else_branch| match else_branch {
                    ElseBranch::Block(block) => {
                        replace_comptime_at_span_in_block(block, span, value.clone())
                    }
                    ElseBranch::If(statement) => {
                        replace_comptime_at_span_in_statement(statement, span, value.clone())
                    }
                })
        }
        _ => false,
    }
}

fn replace_comptime_at_span_in_expr(expr: &mut Expr, span: Span, value: Expr) -> bool {
    if expr.span == span && matches!(expr.kind, ExprKind::Comptime(_)) {
        *expr = value;
        return true;
    }

    match &mut expr.kind {
        ExprKind::Comptime(inner) => replace_comptime_at_span_in_expr(inner, span, value),
        ExprKind::Array(items) | ExprKind::CollectionLiteral { items, .. } => items
            .iter_mut()
            .any(|item| replace_comptime_at_span_in_expr(item, span, value.clone())),
        ExprKind::Call { callee, args } => {
            replace_comptime_at_span_in_expr(callee, span, value.clone())
                || args
                    .iter_mut()
                    .any(|arg| replace_comptime_at_span_in_expr(arg, span, value.clone()))
        }
        ExprKind::Member { object, .. } | ExprKind::GenericMember { object, .. } => {
            replace_comptime_at_span_in_expr(object, span, value)
        }
        ExprKind::StructInit { fields, .. } => fields
            .iter_mut()
            .any(|field| replace_comptime_at_span_in_expr(&mut field.value, span, value.clone())),
        ExprKind::Range { start, end, .. } | ExprKind::Binary { left: start, right: end, .. } => {
            replace_comptime_at_span_in_expr(start, span, value.clone())
                || replace_comptime_at_span_in_expr(end, span, value)
        }
        ExprKind::Cast { value: expr, .. }
        | ExprKind::Unary { operand: expr, .. }
        | ExprKind::PostfixIncrement(expr) => replace_comptime_at_span_in_expr(expr, span, value),
        ExprKind::Match { value: expr, branches } => {
            replace_comptime_at_span_in_expr(expr, span, value.clone())
                || branches.iter_mut().any(|branch| {
                    branch
                        .guard
                        .as_mut()
                        .is_some_and(|guard| {
                            replace_comptime_at_span_in_expr(guard, span, value.clone())
                        })
                        || match &mut branch.body {
                            MatchBranchBody::Expr(expr) => {
                                replace_comptime_at_span_in_expr(expr, span, value.clone())
                            }
                            MatchBranchBody::Block(block) => {
                                replace_comptime_at_span_in_block(block, span, value.clone())
                            }
                        }
                })
        }
        ExprKind::Lambda(function) => replace_comptime_at_span_in_body(&mut function.body, span, value),
        ExprKind::Block(block) => replace_comptime_at_span_in_block(block, span, value),
        ExprKind::Identifier(_)
        | ExprKind::GenericType { .. }
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Missing => false,
    }
}

fn decode_comptime_value(bytes: &[u8]) -> Result<ComptimeValue, String> {
    if !bytes.starts_with(b"GCT1") {
        return Err("comptime runner wrote an invalid result artifact".to_string());
    }
    let mut reader = ComptimeReader {
        bytes,
        position: 4,
    };
    reader.read_value()
}

impl<'bytes> ComptimeReader<'bytes> {
    fn read_value(&mut self) -> Result<ComptimeValue, String> {
        let tag = self.read_byte()?;
        match tag {
            0 => Ok(ComptimeValue::Void),
            1 => Ok(ComptimeValue::Bool(self.read_byte()? != 0)),
            2 => {
                let _type_name = self.read_string()?;
                Ok(ComptimeValue::Number(self.read_string()?))
            }
            3 => Ok(ComptimeValue::String(self.read_string()?)),
            4 => Ok(ComptimeValue::Char(self.read_u32()?)),
            6 => {
                let name = self.read_string()?;
                let count = self.read_u64()? as usize;
                let mut fields = Vec::new();
                for _ in 0..count {
                    let name = self.read_string()?;
                    let value = self.read_value()?;
                    fields.push((name, value));
                }
                Ok(ComptimeValue::Struct { name, fields })
            }
            7 => {
                let name = self.read_string()?;
                let variant = self.read_string()?;
                let has_payload = self.read_byte()? != 0;
                let payload = if has_payload {
                    Some(Box::new(self.read_value()?))
                } else {
                    None
                };
                Ok(ComptimeValue::Enum {
                    name,
                    variant,
                    payload,
                })
            }
            9 => {
                let name = self.read_string()?;
                let count = self.read_u64()? as usize;
                let mut captures = Vec::new();
                for _ in 0..count {
                    let name = self.read_string()?;
                    let value = self.read_value()?;
                    captures.push((name, value));
                }
                Ok(ComptimeValue::Closure { name, captures })
            }
            255 => Ok(ComptimeValue::Unsupported(self.read_string()?)),
            _ => Err(format!("unknown comptime result tag {tag}")),
        }
    }

    fn read_byte(&mut self) -> Result<u8, String> {
        let Some(value) = self.bytes.get(self.position).copied() else {
            return Err("truncated comptime result artifact".to_string());
        };
        self.position += 1;
        Ok(value)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let mut value = 0u32;
        for shift in 0..4 {
            value |= u32::from(self.read_byte()?) << (shift * 8);
        }
        Ok(value)
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in 0..8 {
            value |= u64::from(self.read_byte()?) << (shift * 8);
        }
        Ok(value)
    }

    fn read_string(&mut self) -> Result<String, String> {
        let len = self.read_u64()? as usize;
        let end = self.position.saturating_add(len);
        if end > self.bytes.len() {
            return Err("truncated comptime result string".to_string());
        }
        let value = std::str::from_utf8(&self.bytes[self.position..end])
            .map_err(|_| "comptime result string is not valid UTF-8".to_string())?
            .to_string();
        self.position = end;
        Ok(value)
    }
}

fn expression_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name.clone()),
        ExprKind::Member { object, name } => {
            let mut path = expression_path(object)?;
            path.push('.');
            path.push_str(name);
            Some(path)
        }
        _ => None,
    }
}

fn can_read_fs(packages: &[Package], package: usize, path: &str) -> bool {
    let Some(package) = packages.get(package) else {
        return false;
    };
    permission_allows_path(&package.comptime_permissions.fs, &package.root, path)
}

fn can_read_env(packages: &[Package], package: usize, name: &str) -> bool {
    let Some(package) = packages.get(package) else {
        return false;
    };
    permission_allows_name(&package.comptime_permissions.env, name)
}

fn permission_allows_name(permission: &PermissionValue, name: &str) -> bool {
    match permission {
        PermissionValue::All => true,
        PermissionValue::None => false,
        PermissionValue::Patterns(patterns) => patterns.iter().any(|pattern| pattern == name),
    }
}

fn permission_allows_path(permission: &PermissionValue, root: &Path, path: &str) -> bool {
    match permission {
        PermissionValue::All => true,
        PermissionValue::None => false,
        PermissionValue::Patterns(patterns) => patterns.iter().any(|pattern| {
            path_pattern_matches(pattern, path)
                || root
                    .join(path)
                    .strip_prefix(root)
                    .ok()
                    .and_then(|path| path.to_str())
                    .is_some_and(|path| path_pattern_matches(pattern, path))
        }),
    }
}

fn path_pattern_matches(pattern: &str, path: &str) -> bool {
    glob_matches(pattern.as_bytes(), path.as_bytes())
}

fn glob_matches(pattern: &[u8], value: &[u8]) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }

    if pattern.starts_with(b"**") {
        let rest = &pattern[2..];
        return glob_matches(rest, value)
            || (!value.is_empty() && glob_matches(pattern, &value[1..]));
    }

    if pattern[0] == b'*' {
        let rest = &pattern[1..];
        return glob_matches(rest, value)
            || (!value.is_empty() && value[0] != b'/' && glob_matches(pattern, &value[1..]));
    }

    !value.is_empty() && pattern[0] == value[0] && glob_matches(&pattern[1..], &value[1..])
}
