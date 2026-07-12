impl Analyzer {
    fn check_match_arm_usefulness(
        &mut self,
        matrix: &mut UsefulnessMatrix,
        pattern: &Pattern,
        value_type: &Type,
        has_guard: bool,
    ) {
        let Some(deconstructed) = self.deconstruct_pattern(pattern, value_type) else {
            return;
        };

        let useful = self.pattern_is_useful(matrix.rows(), &deconstructed, value_type, &[]);
        if useful {
            if let DeconstructedPat::Or { alternatives, span } = &deconstructed {
                let mut prior_alternatives = Vec::new();
                for alternative in alternatives {
                    if !self.pattern_is_useful(
                        matrix.rows(),
                        alternative,
                        value_type,
                        &prior_alternatives,
                    ) {
                        self.diagnostics.push(Diagnostic::error(
                            *span,
                            format!(
                                "unreachable or-pattern alternative `{}`",
                                self.format_deconstructed_pattern(alternative, value_type)
                            ),
                        ));
                    } else {
                        prior_alternatives.push(alternative.clone());
                    }
                }
            }
        } else {
            self.report_unreachable_match_pattern(matrix, pattern, &deconstructed, value_type);
        }

        if !has_guard {
            if matches!(pattern, Pattern::Wildcard { .. }) {
                matrix.saw_wildcard_branch = true;
            }
            matrix.add(deconstructed);
            if !self.wildcard_is_useful(matrix.rows(), value_type) {
                matrix.type_fully_covered = true;
            }
        }
    }

    fn report_match_exhaustiveness(
        &mut self,
        matrix: &UsefulnessMatrix,
        value_type: &Type,
        match_span: Span,
    ) {
        if matches!(value_type, Type::Unknown) {
            return;
        }

        let Some(witness) = self.first_missing_pattern(matrix.rows(), value_type) else {
            return;
        };

        let missing = self.format_deconstructed_pattern(&witness, value_type);
        let message = match value_type {
            Type::Enum(enum_name) => {
                format!("non-exhaustive match for enum `{enum_name}`; missing `{missing}`")
            }
            Type::Struct(name) => {
                format!("non-exhaustive match for struct `{name}`; missing `{missing}`")
            }
            Type::Basic(BasicType::Bool) => {
                "non-exhaustive match for `bool`; cover `true` and `false` or add a wildcard branch"
                    .to_string()
            }
            Type::Basic(BasicType::String) => {
                "non-exhaustive match for `string`; add a wildcard branch".to_string()
            }
            Type::Basic(type_) if type_.is_integer() => {
                format!(
                    "non-exhaustive match for `{}`; add a wildcard branch",
                    type_.name()
                )
            }
            _ => return,
        };

        self.diagnostics.push(Diagnostic::error(match_span, message));
    }

    fn report_unreachable_match_pattern(
        &mut self,
        matrix: &UsefulnessMatrix,
        pattern: &Pattern,
        deconstructed: &DeconstructedPat,
        value_type: &Type,
    ) {
        let span = pattern.span();
        if matrix.saw_wildcard_branch {
            self.diagnostics.push(Diagnostic::error(
                span,
                "match branches after a wildcard are unreachable",
            ));
            return;
        }
        if matrix.type_fully_covered {
            self.diagnostics.push(Diagnostic::error(
                span,
                "match branches after a covering pattern are unreachable",
            ));
            return;
        }

        match (pattern, deconstructed, value_type) {
            (Pattern::Wildcard { .. }, _, _) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    "duplicate wildcard match branch",
                ));
            }
            (
                Pattern::Variant { .. },
                DeconstructedPat::Ctor {
                    ctor: Constructor::Variant { variant: name, .. },
                    ..
                },
                Type::Enum(_),
            ) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("duplicate match branch for variant `{name}`"),
                ));
            }
            (
                Pattern::String { value, .. },
                _,
                Type::Basic(BasicType::String),
            ) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("duplicate match branch for string `{value}`"),
                ));
            }
            (
                Pattern::Bool { value, .. },
                _,
                Type::Basic(BasicType::Bool),
            ) => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("duplicate match branch for bool `{value}`"),
                ));
            }
            (Pattern::Or { .. }, DeconstructedPat::Or { .. }, Type::Enum(_)) => {
                if let Some(variant) = self.first_fully_covered_variant_name(deconstructed, value_type)
                {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        format!("duplicate match branch for variant `{variant}`"),
                    ));
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        span,
                        "unreachable match pattern",
                    ));
                }
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!(
                        "unreachable match pattern `{}`",
                        self.format_deconstructed_pattern(deconstructed, value_type)
                    ),
                ));
            }
        }
    }

    fn first_fully_covered_variant_name(
        &self,
        pattern: &DeconstructedPat,
        value_type: &Type,
    ) -> Option<String> {
        match pattern {
            DeconstructedPat::Or { alternatives, .. } => alternatives
                .iter()
                .find_map(|alternative| self.first_fully_covered_variant_name(alternative, value_type)),
            DeconstructedPat::Ctor {
                ctor: Constructor::Variant { variant, .. },
                ..
            } => Some(variant.clone()),
            _ => None,
        }
    }

    fn pattern_is_useful(
        &self,
        matrix: &[DeconstructedPat],
        pattern: &DeconstructedPat,
        value_type: &Type,
        extra_rows: &[DeconstructedPat],
    ) -> bool {
        let mut rows = matrix.to_vec();
        rows.extend(extra_rows.iter().cloned());
        self.is_useful(&rows, pattern, value_type).is_some()
    }

    fn wildcard_is_useful(&self, matrix: &[DeconstructedPat], value_type: &Type) -> bool {
        self.is_useful(matrix, &DeconstructedPat::Wildcard, value_type)
            .is_some()
    }

    fn first_missing_pattern(
        &self,
        matrix: &[DeconstructedPat],
        value_type: &Type,
    ) -> Option<DeconstructedPat> {
        self.is_useful(matrix, &DeconstructedPat::Wildcard, value_type)
    }

    fn is_useful(
        &self,
        matrix: &[DeconstructedPat],
        pattern: &DeconstructedPat,
        value_type: &Type,
    ) -> Option<DeconstructedPat> {
        self.is_useful_row(
            &matrix.iter().map(|row| vec![row.clone()]).collect::<Vec<_>>(),
            &[pattern.clone()],
            &[value_type.clone()],
        )
        .map(|witness| witness.into_iter().next().unwrap_or(DeconstructedPat::Wildcard))
    }

    fn is_useful_row(
        &self,
        matrix: &[Vec<DeconstructedPat>],
        row: &[DeconstructedPat],
        tys: &[Type],
    ) -> Option<Vec<DeconstructedPat>> {
        if row.is_empty() {
            return if matrix.is_empty() {
                Some(Vec::new())
            } else {
                None
            };
        }

        let pattern = &row[0];
        let value_type = &tys[0];

        match pattern {
            DeconstructedPat::Or { alternatives, .. } => {
                for alternative in alternatives {
                    let mut specialized_row = vec![alternative.clone()];
                    specialized_row.extend_from_slice(&row[1..]);
                    if let Some(witness) = self.is_useful_row(matrix, &specialized_row, tys) {
                        return Some(witness);
                    }
                }
                None
            }
            DeconstructedPat::Ctor { ctor, fields, .. } => {
                let specialized = self.specialize_matrix(matrix, ctor, value_type);
                let mut specialized_row = fields.clone();
                specialized_row.extend_from_slice(&row[1..]);
                let mut specialized_tys = self.constructor_field_types(ctor, value_type);
                specialized_tys.extend_from_slice(&tys[1..]);
                let witness =
                    self.is_useful_row(&specialized, &specialized_row, &specialized_tys)?;
                Some(self.reconstruct_constructor_witness(ctor, fields.len(), witness))
            }
            DeconstructedPat::Wildcard => {
                let heads = self.matrix_head_constructors(matrix);
                if let Some(all) = self.split_constructors(value_type) {
                    let missing = all
                        .iter()
                        .filter(|ctor| !heads.iter().any(|head| head == *ctor))
                        .cloned()
                        .collect::<Vec<_>>();
                    if missing.is_empty() {
                        for ctor in &all {
                            let specialized = self.specialize_matrix(matrix, ctor, value_type);
                            let field_tys = self.constructor_field_types(ctor, value_type);
                            let mut specialized_row = field_tys
                                .iter()
                                .map(|_| DeconstructedPat::Wildcard)
                                .collect::<Vec<_>>();
                            specialized_row.extend_from_slice(&row[1..]);
                            let mut specialized_tys = field_tys;
                            specialized_tys.extend_from_slice(&tys[1..]);
                            if let Some(witness) =
                                self.is_useful_row(&specialized, &specialized_row, &specialized_tys)
                            {
                                return Some(self.reconstruct_constructor_witness(
                                    ctor,
                                    self.constructor_arity(ctor, value_type),
                                    witness,
                                ));
                            }
                        }
                        None
                    } else {
                        let defaulted = self.default_matrix(matrix);
                        let rest_witness = self.is_useful_row(&defaulted, &row[1..], &tys[1..])?;
                        let ctor = &missing[0];
                        let field_tys = self.constructor_field_types(ctor, value_type);
                        let fields = field_tys
                            .iter()
                            .map(|_| DeconstructedPat::Wildcard)
                            .collect::<Vec<_>>();
                        let mut witness = vec![DeconstructedPat::Ctor {
                            ctor: ctor.clone(),
                            fields,
                        }];
                        witness.extend(rest_witness);
                        Some(witness)
                    }
                } else {
                    let defaulted = self.default_matrix(matrix);
                    let rest_witness = self.is_useful_row(&defaulted, &row[1..], &tys[1..])?;
                    let mut witness = vec![DeconstructedPat::Wildcard];
                    witness.extend(rest_witness);
                    Some(witness)
                }
            }
        }
    }

    fn reconstruct_constructor_witness(
        &self,
        ctor: &Constructor,
        arity: usize,
        mut witness: Vec<DeconstructedPat>,
    ) -> Vec<DeconstructedPat> {
        let fields = witness.drain(..arity.min(witness.len())).collect::<Vec<_>>();
        let mut result = vec![DeconstructedPat::Ctor {
            ctor: ctor.clone(),
            fields
        }];
        result.append(&mut witness);
        result
    }

    fn specialize_matrix(
        &self,
        matrix: &[Vec<DeconstructedPat>],
        ctor: &Constructor,
        value_type: &Type,
    ) -> Vec<Vec<DeconstructedPat>> {
        let mut specialized = Vec::new();
        for row in matrix {
            self.specialize_row(row, ctor, value_type, &mut specialized);
        }
        specialized
    }

    fn specialize_row(
        &self,
        row: &[DeconstructedPat],
        ctor: &Constructor,
        value_type: &Type,
        out: &mut Vec<Vec<DeconstructedPat>>,
    ) {
        let Some(head) = row.first() else {
            return;
        };
        let rest = &row[1..];
        match head {
            DeconstructedPat::Or { alternatives, .. } => {
                for alternative in alternatives {
                    let mut expanded = vec![alternative.clone()];
                    expanded.extend_from_slice(rest);
                    self.specialize_row(&expanded, ctor, value_type, out);
                }
            }
            DeconstructedPat::Wildcard => {
                let arity = self.constructor_arity(ctor, value_type);
                let mut specialized = (0..arity)
                    .map(|_| DeconstructedPat::Wildcard)
                    .collect::<Vec<_>>();
                specialized.extend_from_slice(rest);
                out.push(specialized);
            }
            DeconstructedPat::Ctor {
                ctor: row_ctor,
                fields,
                ..
            } => {
                if self.constructor_covers(row_ctor, ctor) {
                    let mut specialized = fields.clone();
                    specialized.extend_from_slice(rest);
                    out.push(specialized);
                }
            }
        }
    }

    fn default_matrix(&self, matrix: &[Vec<DeconstructedPat>]) -> Vec<Vec<DeconstructedPat>> {
        let mut defaulted = Vec::new();
        for row in matrix {
            self.default_row(row, &mut defaulted);
        }
        defaulted
    }

    fn default_row(&self, row: &[DeconstructedPat], out: &mut Vec<Vec<DeconstructedPat>>) {
        let Some(head) = row.first() else {
            return;
        };
        let rest = &row[1..];
        match head {
            DeconstructedPat::Or { alternatives, .. } => {
                for alternative in alternatives {
                    let mut expanded = vec![alternative.clone()];
                    expanded.extend_from_slice(rest);
                    self.default_row(&expanded, out);
                }
            }
            DeconstructedPat::Wildcard => {
                out.push(rest.to_vec());
            }
            DeconstructedPat::Ctor { .. } => {}
        }
    }

    fn matrix_head_constructors(&self, matrix: &[Vec<DeconstructedPat>]) -> Vec<Constructor> {
        let mut ctors = Vec::new();
        for row in matrix {
            self.collect_head_constructors(row.first(), &mut ctors);
        }
        ctors
    }

    fn collect_head_constructors(
        &self,
        pattern: Option<&DeconstructedPat>,
        ctors: &mut Vec<Constructor>,
    ) {
        let Some(pattern) = pattern else {
            return;
        };
        match pattern {
            DeconstructedPat::Or { alternatives, .. } => {
                for alternative in alternatives {
                    self.collect_head_constructors(Some(alternative), ctors);
                }
            }
            DeconstructedPat::Ctor { ctor, .. } => {
                if !ctors.iter().any(|existing| existing == ctor) {
                    ctors.push(ctor.clone());
                }
            }
            DeconstructedPat::Wildcard => {}
        }
    }

    fn constructor_covers(&self, row_ctor: &Constructor, target: &Constructor) -> bool {
        if row_ctor == target {
            return true;
        }
        match (row_ctor, target) {
            (
                Constructor::IntRange {
                    start,
                    end,
                    inclusive,
                },
                Constructor::Int(value),
            ) => integer_range_contains(start, end, *inclusive, value),
            (
                Constructor::IntRange {
                    start: row_start,
                    end: row_end,
                    inclusive: row_inclusive,
                },
                Constructor::IntRange {
                    start,
                    end,
                    inclusive,
                },
            ) => {
                row_start == start && row_end == end && row_inclusive == inclusive
            }
            _ => false,
        }
    }

    fn split_constructors(&self, value_type: &Type) -> Option<Vec<Constructor>> {
        match value_type {
            Type::Basic(BasicType::Bool) => Some(vec![
                Constructor::Bool(true),
                Constructor::Bool(false),
            ]),
            Type::Enum(enum_name) => {
                let definition = self.enums.get(enum_name)?;
                let mut variants = definition.variants.keys().cloned().collect::<Vec<_>>();
                variants.sort();
                Some(
                    variants
                        .into_iter()
                        .map(|variant| Constructor::Variant {
                            enum_name: enum_name.clone(),
                            variant,
                        })
                        .collect(),
                )
            }
            Type::Struct(name) => Some(vec![Constructor::Struct { name: name.clone() }]),
            Type::Basic(BasicType::String) => None,
            Type::Basic(type_) if type_.is_integer() => None,
            _ => None,
        }
    }

    fn constructor_arity(&self, ctor: &Constructor, value_type: &Type) -> usize {
        self.constructor_field_types(ctor, value_type).len()
    }

    fn constructor_field_types(&self, ctor: &Constructor, value_type: &Type) -> Vec<Type> {
        match (ctor, value_type) {
            (
                Constructor::Variant {
                    enum_name,
                    variant,
                },
                Type::Enum(value_enum),
            ) if enum_name == value_enum => self
                .enums
                .get(enum_name)
                .and_then(|enum_| enum_.variants.get(variant))
                .and_then(|payload| payload.clone())
                .into_iter()
                .collect(),
            (Constructor::Struct { name }, Type::Struct(value_name)) if name == value_name => {
                let Some(struct_) = self.structs.get(name) else {
                    return Vec::new();
                };
                let mut fields = struct_.fields.keys().cloned().collect::<Vec<_>>();
                fields.sort();
                fields
                    .into_iter()
                    .filter_map(|field| struct_.fields.get(&field).cloned())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn deconstruct_pattern(
        &self,
        pattern: &Pattern,
        value_type: &Type,
    ) -> Option<DeconstructedPat> {
        match (pattern, value_type) {
            (Pattern::Or { alternatives, span }, _) => {
                let mut deconstructed = Vec::new();
                for alternative in alternatives {
                    deconstructed.push(self.deconstruct_pattern(alternative, value_type)?);
                }
                Some(DeconstructedPat::Or {
                    alternatives: deconstructed,
                    span: *span,
                })
            }
            (Pattern::Wildcard { .. } | Pattern::Binding { .. }, _) => {
                Some(DeconstructedPat::Wildcard)
            }
            (
                Pattern::Variant {
                    enum_name,
                    variant,
                    payload,
                    ..
                },
                Type::Enum(value_enum),
            ) if enum_name == value_enum => {
                let payload_type = self
                    .enums
                    .get(enum_name)?
                    .variants
                    .get(variant)?
                    .clone();
                let fields = match (payload, payload_type) {
                    (Some(payload), Some(payload_type)) => {
                        vec![self.deconstruct_pattern(payload, &payload_type)?]
                    }
                    (None, None) => Vec::new(),
                    _ => return None,
                };
                Some(DeconstructedPat::Ctor {
                    ctor: Constructor::Variant {
                        enum_name: enum_name.clone(),
                        variant: variant.clone(),
                    },
                    fields
                })
            }
            (
                Pattern::Struct {
                    name,
                    fields,
                    has_rest,
                    ..
                },
                Type::Struct(value_name),
            ) if name == value_name => {
                let struct_ = self.structs.get(name)?;
                let mut field_names = struct_.fields.keys().cloned().collect::<Vec<_>>();
                field_names.sort();
                let field_patterns = fields
                    .iter()
                    .map(|field| (field.name.as_str(), &field.pattern))
                    .collect::<HashMap<_, _>>();
                if !*has_rest
                    && field_names
                        .iter()
                        .any(|field| !field_patterns.contains_key(field.as_str()))
                {
                    return None;
                }
                let mut deconstructed_fields = Vec::new();
                for field_name in &field_names {
                    let field_type = struct_.fields.get(field_name)?;
                    if let Some(field_pattern) = field_patterns.get(field_name.as_str()) {
                        deconstructed_fields
                            .push(self.deconstruct_pattern(field_pattern, field_type)?);
                    } else {
                        deconstructed_fields.push(DeconstructedPat::Wildcard);
                    }
                }
                Some(DeconstructedPat::Ctor {
                    ctor: Constructor::Struct { name: name.clone() },
                    fields: deconstructed_fields
                })
            }
            (Pattern::Bool { value, .. }, Type::Basic(BasicType::Bool)) => {
                Some(DeconstructedPat::Ctor {
                    ctor: Constructor::Bool(*value),
                    fields: Vec::new()
                })
            }
            (Pattern::String { value, .. }, Type::Basic(BasicType::String)) => {
                Some(DeconstructedPat::Ctor {
                    ctor: Constructor::String(value.clone()),
                    fields: Vec::new()
                })
            }
            (Pattern::Number { value, .. }, Type::Basic(type_)) if type_.is_integer() => {
                Some(DeconstructedPat::Ctor {
                    ctor: Constructor::Int(value.clone()),
                    fields: Vec::new()
                })
            }
            (
                Pattern::Range {
                    start,
                    end,
                    inclusive,
                    ..
                },
                Type::Basic(type_),
            ) if type_.is_integer() => Some(DeconstructedPat::Ctor {
                ctor: Constructor::IntRange {
                    start: start.clone(),
                    end: end.clone(),
                    inclusive: *inclusive,
                },
                fields: Vec::new()
            }),
            _ => None,
        }
    }

    fn format_deconstructed_pattern(&self, pattern: &DeconstructedPat, value_type: &Type) -> String {
        match (pattern, value_type) {
            (DeconstructedPat::Wildcard, _) => "_".to_string(),
            (DeconstructedPat::Or { alternatives, .. }, _) => alternatives
                .iter()
                .map(|alternative| self.format_deconstructed_pattern(alternative, value_type))
                .collect::<Vec<_>>()
                .join(" | "),
            (
                DeconstructedPat::Ctor {
                    ctor: Constructor::Variant {
                        enum_name,
                        variant,
                    },
                    fields,
                },
                Type::Enum(_),
            ) => {
                if fields.is_empty() {
                    format!("{variant}")
                } else {
                    let payload_type = self
                        .enums
                        .get(enum_name)
                        .and_then(|enum_| enum_.variants.get(variant))
                        .and_then(|payload| payload.clone());
                    let payload = fields
                        .first()
                        .zip(payload_type.as_ref())
                        .map(|(field, payload_type)| {
                            self.format_deconstructed_pattern(field, payload_type)
                        })
                        .unwrap_or_else(|| "_".to_string());
                    if payload == "_" {
                        format!("{variant}(_)")
                    } else if matches!(
                        fields.first(),
                        Some(DeconstructedPat::Ctor {
                            ctor: Constructor::Variant { .. },
                            ..
                        })
                    ) {
                        if let Some(Type::Enum(payload_enum)) = payload_type {
                            format!("{variant}({payload_enum}.{payload})")
                        } else {
                            format!("{variant}({payload})")
                        }
                    } else {
                        format!("{variant}({payload})")
                    }
                }
            }
            (
                DeconstructedPat::Ctor {
                    ctor: Constructor::Struct { name },
                    fields,
                },
                Type::Struct(_),
            ) => {
                let Some(struct_) = self.structs.get(name) else {
                    return format!("{name} {{ ... }}");
                };
                let mut field_names = struct_.fields.keys().cloned().collect::<Vec<_>>();
                field_names.sort();
                let mut parts = Vec::new();
                let mut has_non_wildcard = false;
                for (field_name, field_pattern) in field_names.iter().zip(fields) {
                    if matches!(field_pattern, DeconstructedPat::Wildcard) {
                        continue;
                    }
                    has_non_wildcard = true;
                    let field_type = struct_.fields.get(field_name).unwrap();
                    parts.push(format!(
                        "{field_name}: {}",
                        self.format_deconstructed_pattern(field_pattern, field_type)
                    ));
                }
                if !has_non_wildcard {
                    format!("{name} {{ ... }}")
                } else if parts.len() == field_names.len() {
                    format!("{name} {{ {} }}", parts.join(", "))
                } else {
                    parts.push("...".to_string());
                    format!("{name} {{ {} }}", parts.join(", "))
                }
            }
            (
                DeconstructedPat::Ctor {
                    ctor: Constructor::Bool(value),
                    ..
                },
                _,
            ) => value.to_string(),
            (
                DeconstructedPat::Ctor {
                    ctor: Constructor::String(value),
                    ..
                },
                _,
            ) => format!("\"{value}\""),
            (
                DeconstructedPat::Ctor {
                    ctor: Constructor::Int(value),
                    ..
                },
                _,
            ) => value.clone(),
            (
                DeconstructedPat::Ctor {
                    ctor:
                        Constructor::IntRange {
                            start,
                            end,
                            inclusive,
                        },
                    ..
                },
                _,
            ) => {
                if *inclusive {
                    format!("{start}..={end}")
                } else {
                    format!("{start}..{end}")
                }
            }
            _ => "_".to_string(),
        }
    }
}

#[derive(Default)]
struct UsefulnessMatrix {
    rows: Vec<DeconstructedPat>,
    saw_wildcard_branch: bool,
    type_fully_covered: bool,
}

impl UsefulnessMatrix {
    fn rows(&self) -> &[DeconstructedPat] {
        &self.rows
    }

    fn add(&mut self, pattern: DeconstructedPat) {
        match pattern {
            DeconstructedPat::Or { alternatives, .. } => {
                self.rows.extend(alternatives);
            }
            pattern => self.rows.push(pattern),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Constructor {
    Variant {
        enum_name: String,
        variant: String,
    },
    Struct {
        name: String,
    },
    Bool(bool),
    String(String),
    Int(String),
    IntRange {
        start: String,
        end: String,
        inclusive: bool,
    },
}

#[derive(Debug, Clone)]
enum DeconstructedPat {
    Wildcard,
    Ctor {
        ctor: Constructor,
        fields: Vec<DeconstructedPat>,
    },
    Or {
        alternatives: Vec<DeconstructedPat>,
        span: Span,
    },
}

fn integer_range_contains(start: &str, end: &str, inclusive: bool, value: &str) -> bool {
    let Ok(start) = start.parse::<i128>() else {
        return false;
    };
    let Ok(end) = end.parse::<i128>() else {
        return false;
    };
    let Ok(value) = value.parse::<i128>() else {
        return false;
    };
    if inclusive {
        value >= start && value <= end
    } else {
        value >= start && value < end
    }
}
