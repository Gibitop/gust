use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{
    BasicType, BinaryOp, Block, ElseBranch, Expr, ExprKind, FunctionBody, FunctionDecl, Item,
    MatchBranchBody, Pattern, Program, Stmt, StmtKind, StructDecl, StructInitField, StructMember,
    TraitDecl, TraitMethodDecl, TypeRef, UnaryOp, number_literal_is_float,
};
use crate::diagnostic::Diagnostic;
use crate::span::{SourceMap, Span};

pub fn lower_program(program: &Program) -> Result<LoweredProgram, Vec<Diagnostic>> {
    lower_program_with_source_files(program, Vec::new())
}

pub fn lower_program_with_source(
    program: &Program,
    path: impl Into<String>,
    source: impl Into<String>,
) -> Result<LoweredProgram, Vec<Diagnostic>> {
    lower_program_with_source_files(
        program,
        vec![LoweringSourceFile {
            path: path.into(),
            source: source.into(),
            offset: 0,
        }],
    )
}

pub fn lower_program_with_source_files(
    program: &Program,
    source_files: Vec<LoweringSourceFile>,
) -> Result<LoweredProgram, Vec<Diagnostic>> {
    SOURCE_FILES.with(|files| *files.borrow_mut() = source_files);
    let result = crate::monomorphize::monomorphize(program)
        .and_then(|program| lower_monomorphized_program(&program));
    SOURCE_FILES.with(|files| files.borrow_mut().clear());
    result
}

fn lower_source_location(span: Span) -> LoweredSourceLocation {
    SOURCE_FILES.with(|files| {
        let files = files.borrow();
        let Some(file) = files
            .iter()
            .rev()
            .find(|file| span.start >= file.offset)
            .or_else(|| files.first())
        else {
            return LoweredSourceLocation {
                path: "<source>".to_string(),
                line: 1,
                column: 1,
            };
        };
        let local_start = span.start.saturating_sub(file.offset);
        let location = SourceMap::new(&file.source).location(local_start);
        LoweredSourceLocation {
            path: file.path.clone(),
            line: location.line,
            column: location.column,
        }
    })
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
                    let lowered_name = if trait_has_positional_type_arguments(&item.trait_ref.name)
                    {
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

        Ok(prune_dead_lowered_items(LoweredProgram {
            structs,
            enums,
            traits,
            functions,
            closure_functions,
            statements,
            main_location: lower_source_location(main.span),
        }))
    } else {
        Err(diagnostics)
    }
}

include!("ir.rs");
include!("dce.rs");
include!("context.rs");
include!("names.rs");
include!("items.rs");
include!("types.rs");
include!("returns.rs");
include!("captures.rs");
include!("statements.rs");
include!("loops.rs");
include!("expressions.rs");
include!("patterns.rs");
include!("decision.rs");
