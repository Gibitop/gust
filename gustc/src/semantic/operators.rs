impl Analyzer {
    fn validate_assignment_value(
        &mut self,
        span: Span,
        target: &Expr,
        op: Option<BinaryOp>,
        value: &Expr,
        expected_type: Type,
    ) -> Type {
        if let Some(op) = op {
            if matches!(
                op,
                BinaryOp::BitwiseAnd
                    | BinaryOp::BitwiseOr
                    | BinaryOp::BitwiseXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
            ) {
                self.validate_bitwise(span, target, op, value, Some(expected_type))
            } else {
                self.validate_arithmetic(span, target, op, value, Some(expected_type))
            }
        } else {
            self.validate_expr_with_context(value, Some(expected_type))
        }
    }

    fn validate_arithmetic(
        &mut self,
        span: Span,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        expected_type: Option<Type>,
    ) -> Type {
        let contextual_type = expected_type.filter(|type_| {
            matches!(type_, Type::Basic(type_) if type_.is_numeric())
                || op == BinaryOp::Add && *type_ == Type::Basic(BasicType::String)
        });
        let (left_type, right_type) = if let Some(type_) = contextual_type {
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if number_pair_contains_float(left, right) {
            let type_ = Type::Basic(BasicType::F64);
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if matches!(left.kind, ExprKind::Number(_))
            && !matches!(right.kind, ExprKind::Number(_))
        {
            let right_type = self.validate_expr(right);
            let left_type = self.validate_expr_with_context(left, Some(right_type.clone()));
            (left_type, right_type)
        } else {
            let left_type = self.validate_expr(left);
            let right_type = self.validate_expr_with_context(right, Some(left_type.clone()));
            (left_type, right_type)
        };

        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
            return Type::Unknown;
        }

        if left_type != right_type {
            self.report_type_mismatch(right.span, left_type, right_type);
            return Type::Unknown;
        }

        if matches!(&left_type, Type::Basic(type_) if type_.is_numeric())
            || op == BinaryOp::Add && left_type == Type::Basic(BasicType::String)
        {
            return left_type;
        }

        let requirement = if op == BinaryOp::Add {
            "only supports numeric or string operands"
        } else {
            "only supports numeric operands"
        };
        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "operator {} {requirement}, got `{}`",
                op.symbol(),
                left_type.name()
            ),
        ));
        Type::Unknown
    }

    fn validate_bitwise(
        &mut self,
        span: Span,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        expected_type: Option<Type>,
    ) -> Type {
        let contextual_type =
            expected_type.filter(|type_| matches!(type_, Type::Basic(type_) if type_.is_integer()));
        let (left_type, right_type) = if let Some(type_) = contextual_type {
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if matches!(left.kind, ExprKind::Number(_))
            && !matches!(right.kind, ExprKind::Number(_))
        {
            let right_type = self.validate_expr(right);
            let left_type = self.validate_expr_with_context(left, Some(right_type.clone()));
            (left_type, right_type)
        } else {
            let left_type = self.validate_expr(left);
            let right_type = self.validate_expr_with_context(right, Some(left_type.clone()));
            (left_type, right_type)
        };

        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
            return Type::Unknown;
        }

        if left_type != right_type {
            self.report_type_mismatch(right.span, left_type, right_type);
            return Type::Unknown;
        }

        if matches!(&left_type, Type::Basic(type_) if type_.is_integer()) {
            return left_type;
        }

        self.diagnostics.push(Diagnostic::error(
            span,
            format!(
                "operator {} only supports integer operands, got `{}`",
                op.symbol(),
                left_type.name()
            ),
        ));
        Type::Unknown
    }

    fn validate_comparison(&mut self, span: Span, left: &Expr, op: BinaryOp, right: &Expr) -> Type {
        let (left_type, right_type) = if number_pair_contains_float(left, right) {
            let type_ = Type::Basic(BasicType::F64);
            let left_type = self.validate_expr_with_context(left, Some(type_.clone()));
            let right_type = self.validate_expr_with_context(right, Some(type_));
            (left_type, right_type)
        } else if matches!(left.kind, ExprKind::Number(_))
            && !matches!(right.kind, ExprKind::Number(_))
        {
            let right_type = self.validate_expr(right);
            let left_type = self.validate_expr_with_context(left, Some(right_type.clone()));
            (left_type, right_type)
        } else {
            let left_type = self.validate_expr(left);
            let right_type = self.validate_expr_with_context(right, Some(left_type.clone()));
            (left_type, right_type)
        };

        if matches!(left_type, Type::Unknown) || matches!(right_type, Type::Unknown) {
            return Type::Unknown;
        }

        if left_type != right_type {
            self.report_type_mismatch(right.span, left_type, right_type);
            return Type::Unknown;
        }

        let supported = match op {
            BinaryOp::Equal | BinaryOp::NotEqual => {
                matches!(
                    left_type,
                    Type::Basic(BasicType::String | BasicType::Char | BasicType::Bool)
                ) || matches!(&left_type, Type::Basic(type_) if type_.is_numeric())
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                matches!(&left_type, Type::Basic(type_) if type_.is_numeric())
            }
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Remainder
            | BinaryOp::BitwiseAnd
            | BinaryOp::BitwiseOr
            | BinaryOp::BitwiseXor
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight
            | BinaryOp::LogicalAnd
            | BinaryOp::LogicalOr => {
                unreachable!("non-comparison operator is validated separately")
            }
        };

        if !supported {
            let requirement = match op {
                BinaryOp::Equal | BinaryOp::NotEqual => {
                    "only supports numeric, bool, and string operands"
                }
                BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual => "only supports numeric operands",
                BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::BitwiseAnd
                | BinaryOp::BitwiseOr
                | BinaryOp::BitwiseXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
                | BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr => {
                    unreachable!("non-comparison operator is validated separately")
                }
            };
            self.diagnostics.push(Diagnostic::error(
                span,
                format!(
                    "operator {} {requirement}, got `{}`",
                    op.symbol(),
                    left_type.name()
                ),
            ));
            return Type::Unknown;
        }

        Type::Basic(BasicType::Bool)
    }

}
