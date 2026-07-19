impl Parser {
    fn parse_expression(&mut self) -> Expr {
        self.parse_range_expression()
    }

    fn parse_expression_without_struct_init(&mut self) -> Expr {
        let previous = self.allow_struct_init;
        self.allow_struct_init = false;
        let expr = self.parse_expression();
        self.allow_struct_init = previous;
        expr
    }

    fn parse_range_expression(&mut self) -> Expr {
        let start = self.parse_binary_expression(0);
        let inclusive = if self.match_kind(&TokenKind::DotDotEqual) {
            true
        } else if self.match_kind(&TokenKind::DotDot) {
            false
        } else {
            return start;
        };
        let end = self.parse_binary_expression(0);
        let span = start.span.join(end.span);

        Expr {
            kind: ExprKind::Range {
                start: Box::new(start),
                end: Box::new(end),
                inclusive,
            },
            span,
        }
    }

    fn parse_binary_expression(&mut self, min_precedence: u8) -> Expr {
        let mut left = self.parse_cast_expression();

        while let Some((op, precedence)) = self.current_binary_op() {
            if precedence < min_precedence {
                break;
            }

            self.advance();
            let right = self.parse_binary_expression(precedence + 1);
            let span = left.span.join(right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        left
    }

    fn parse_cast_expression(&mut self) -> Expr {
        let mut expr = self.parse_unary_expression();

        while self.match_keyword(Keyword::As) {
            let type_ref = self
                .parse_type()
                .unwrap_or_else(|| self.missing_type(self.current().span));
            let span = expr.span.join(type_ref.span);
            expr = Expr {
                kind: ExprKind::Cast {
                    value: Box::new(expr),
                    type_ref,
                },
                span,
            };
        }

        expr
    }

    fn parse_unary_expression(&mut self) -> Expr {
        let op = if self.match_kind(&TokenKind::Bang) {
            Some(UnaryOp::Not)
        } else if self.match_kind(&TokenKind::Minus) {
            Some(UnaryOp::Negate)
        } else {
            None
        };

        if let Some(op) = op {
            let start = self.previous_span();
            let operand = self.parse_unary_expression();
            let span = start.join(operand.span);

            return Expr {
                kind: ExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
                span,
            };
        }

        self.parse_postfix_expression()
    }

    fn parse_postfix_expression(&mut self) -> Expr {
        let mut expr = self.parse_primary_expression();

        loop {
            if self.match_kind(&TokenKind::LeftParen) {
                let mut args = Vec::new();

                while !self.at_eof() && !self.check_kind(&TokenKind::RightParen) {
                    args.push(self.parse_expression());

                    if !self.match_kind(&TokenKind::Comma) {
                        break;
                    }
                }

                self.expect_kind(&TokenKind::RightParen, "`)`");
                let span = expr.span.join(self.previous_span());
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
            } else if self.match_kind(&TokenKind::Dot) {
                let name_span = self.current().span;
                let name = self.expect_callable_name("expected member name");
                expr = Expr {
                    span: expr.span.join(name_span),
                    kind: ExprKind::Member {
                        object: Box::new(expr),
                        name,
                    },
                };
            } else if self.match_kind(&TokenKind::LeftBracket) {
                let key = self.parse_expression();
                self.expect_kind(&TokenKind::RightBracket, "`]`");
                let span = expr.span.join(self.previous_span());
                expr = Expr {
                    span,
                    kind: ExprKind::Call {
                        callee: Box::new(Expr {
                            span,
                            kind: ExprKind::Member {
                                object: Box::new(expr),
                                name: INDEX_METHOD.to_string(),
                            },
                        }),
                        args: vec![key],
                    },
                };
            } else if self.check_kind(&TokenKind::Less)
                && matches!(expr.kind, ExprKind::Member { .. })
            {
                let position = self.position;
                let diagnostic_count = self.diagnostics.len();
                let args = self.parse_type_args();
                if let Some(args) = args {
                    if self.check_kind(&TokenKind::LeftParen) && !args.is_empty() {
                        let ExprKind::Member { object, name } = expr.kind else {
                            unreachable!("generic member must start from a member expression")
                        };
                        expr = Expr {
                            span: expr.span.join(self.previous_span()),
                            kind: ExprKind::GenericMember { object, name, args },
                        };
                    } else {
                        self.position = position;
                        self.diagnostics.truncate(diagnostic_count);
                        break;
                    }
                } else {
                    self.position = position;
                    self.diagnostics.truncate(diagnostic_count);
                    break;
                }
            } else if self.match_kind(&TokenKind::PlusPlus) {
                if indexed_access_parts(&expr).is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        expr.span,
                        "increment through indexed access is not supported; read, increment, and assign the indexed value explicitly",
                    ));
                }
                expr = Expr {
                    span: expr.span.join(self.previous_span()),
                    kind: ExprKind::PostfixIncrement(Box::new(expr)),
                };
            } else if (self.allow_struct_init && self.check_kind(&TokenKind::LeftBrace))
                || self.check_kind(&TokenKind::Less)
            {
                if let Some(name) = expression_path(&expr) {
                    let position = self.position;
                    let diagnostic_count = self.diagnostics.len();
                    let args = if self.check_kind(&TokenKind::Less) {
                        self.parse_type_args()
                    } else {
                        Some(Vec::new())
                    };
                    if self.diagnostics.len() != diagnostic_count {
                        self.position = position;
                        self.diagnostics.truncate(diagnostic_count);
                        break;
                    }
                    if let Some(args) = args {
                        if self.allow_struct_init && self.check_kind(&TokenKind::LeftBrace) {
                            expr = self.parse_struct_init(name, args, expr.span);
                        } else if !args.is_empty() {
                            expr = Expr {
                                span: expr.span.join(self.previous_span()),
                                kind: ExprKind::GenericType { name, args },
                            };
                        } else {
                            self.position = position;
                            self.diagnostics.truncate(diagnostic_count);
                            break;
                        }
                    } else {
                        self.position = position;
                        self.diagnostics.truncate(diagnostic_count);
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        expr
    }

    fn parse_primary_expression(&mut self) -> Expr {
        let token = self.advance();

        match token.kind {
            TokenKind::Identifier(name) => Expr {
                kind: ExprKind::Identifier(name),
                span: token.span,
            },
            TokenKind::Number(value) => Expr {
                kind: ExprKind::Number(value),
                span: token.span,
            },
            TokenKind::StringLiteral(value) => Expr {
                kind: ExprKind::String(value),
                span: token.span,
            },
            TokenKind::InterpolatedString(parts) => {
                self.finish_interpolated_string(token.span, parts)
            }
            TokenKind::CharLiteral(value) => Expr {
                kind: ExprKind::Char(value),
                span: token.span,
            },
            TokenKind::Keyword(Keyword::False) => Expr {
                kind: ExprKind::Bool(false),
                span: token.span,
            },
            TokenKind::Keyword(Keyword::True) => Expr {
                kind: ExprKind::Bool(true),
                span: token.span,
            },
            TokenKind::LeftBracket => self.finish_array(token.span),
            TokenKind::LeftBrace => {
                self.position = self.position.saturating_sub(1);
                let block = self.parse_block();
                Expr {
                    span: block.span,
                    kind: ExprKind::Block(block),
                }
            }
            TokenKind::LeftParen => {
                let expr = self.parse_expression();
                self.expect_kind(&TokenKind::RightParen, "`)`");
                expr
            }
            TokenKind::Keyword(Keyword::Fn) => {
                self.position = self.position.saturating_sub(1);
                let function = self.parse_function(false);
                Expr {
                    span: function.span,
                    kind: ExprKind::Lambda(function),
                }
            }
            TokenKind::Keyword(Keyword::Comptime) => {
                if self.check_kind(&TokenKind::Dot) {
                    return Expr {
                        kind: ExprKind::Identifier("comptime".to_string()),
                        span: token.span,
                    };
                }
                let value = self.parse_expression();
                Expr {
                    span: token.span.join(value.span),
                    kind: ExprKind::Comptime(Box::new(value)),
                }
            }
            TokenKind::Keyword(Keyword::Match) => self.finish_match(token.span),
            _ => {
                self.diagnostics
                    .push(Diagnostic::error(token.span, "expected expression"));
                self.missing_expr(token.span)
            }
        }
    }

    fn finish_interpolated_string(
        &mut self,
        span: Span,
        parts: Vec<InterpolatedStringPart>,
    ) -> Expr {
        let mut expr: Option<Expr> = None;

        for part in parts {
            let part_expr = match part {
                InterpolatedStringPart::Text(value) => Expr {
                    kind: ExprKind::String(value),
                    span,
                },
                InterpolatedStringPart::Path { segments, span } => {
                    let value = self.interpolation_path_expr(segments, span);
                    self.interpolated_value_to_string(value)
                }
                InterpolatedStringPart::Expr {
                    source,
                    source_start,
                    span,
                } => {
                    let value = self.interpolation_expression_expr(&source, source_start, span);
                    self.interpolated_value_to_string(value)
                }
            };

            expr = Some(if let Some(left) = expr {
                let span = left.span.join(part_expr.span);
                Expr {
                    kind: ExprKind::Binary {
                        left: Box::new(left),
                        op: BinaryOp::Add,
                        right: Box::new(part_expr),
                    },
                    span,
                }
            } else {
                part_expr
            });
        }

        expr.unwrap_or(Expr {
            kind: ExprKind::String(String::new()),
            span,
        })
    }

    fn interpolation_path_expr(
        &mut self,
        segments: Vec<InterpolatedPathSegment>,
        span: Span,
    ) -> Expr {
        let mut segments = segments.into_iter();
        let Some(first) = segments.next() else {
            return self.missing_expr(span);
        };
        let mut expr = Expr {
            kind: ExprKind::Identifier(first.name),
            span: first.span,
        };

        for segment in segments {
            expr = Expr {
                span: expr.span.join(segment.span),
                kind: ExprKind::Member {
                    object: Box::new(expr),
                    name: segment.name,
                },
            };
        }

        expr
    }

    fn interpolation_expression_expr(
        &mut self,
        source: &str,
        source_start: usize,
        span: Span,
    ) -> Expr {
        let (mut tokens, lexer_diagnostics) = Lexer::new(source).tokenize();
        for diagnostic in lexer_diagnostics {
            self.diagnostics.push(Diagnostic::error(
                offset_span(diagnostic.span, source_start),
                diagnostic.message,
            ));
        }
        for token in &mut tokens {
            token.span = offset_span(token.span, source_start);
        }

        let mut parser = Parser::new(tokens);
        let expr = parser.parse_expression();
        if !parser.at_eof() {
            parser.error_here("expected end of interpolation expression");
        }
        self.diagnostics.extend(parser.diagnostics);

        if matches!(expr.kind, ExprKind::Missing) {
            Expr { span, ..expr }
        } else {
            expr
        }
    }

    fn interpolated_value_to_string(&self, value: Expr) -> Expr {
        let span = value.span;
        Expr {
            span,
            kind: ExprKind::Call {
                callee: Box::new(Expr {
                    span,
                    kind: ExprKind::Member {
                        object: Box::new(value),
                        name: "toString".to_string(),
                    },
                }),
                args: Vec::new(),
            },
        }
    }

    fn finish_array(&mut self, start: Span) -> Expr {
        let mut items = Vec::new();

        while !self.at_eof() && !self.check_kind(&TokenKind::RightBracket) {
            items.push(self.parse_expression());

            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_kind(&TokenKind::RightBracket, "`]`");
        let span = start.join(self.previous_span());

        Expr {
            kind: ExprKind::Array(items),
            span,
        }
    }

    fn finish_match(&mut self, start: Span) -> Expr {
        let value = self.parse_expression_without_struct_init();
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut branches = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            let pattern = self.parse_pattern();
            let guard = if self.match_keyword(Keyword::If) {
                Some(self.parse_expression())
            } else {
                None
            };
            self.expect_kind(&TokenKind::FatArrow, "`=>`");
            let body = if self.check_kind(&TokenKind::LeftBrace) {
                MatchBranchBody::Block(self.parse_block())
            } else if self.current_keyword() == Some(Keyword::Break) {
                let statement = self.parse_break_statement();
                MatchBranchBody::Block(Block {
                    span: statement.span,
                    statements: vec![statement],
                })
            } else {
                MatchBranchBody::Expr(Box::new(self.parse_expression()))
            };
            let span = pattern.span().join(body.span());
            branches.push(MatchBranch {
                pattern,
                guard,
                body,
                span,
            });
            self.match_kind(&TokenKind::Comma);
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let span = start.join(self.previous_span());

        Expr {
            kind: ExprKind::Match {
                value: Box::new(value),
                branches,
            },
            span,
        }
    }

    fn current_binary_op(&self) -> Option<(BinaryOp, u8)> {
        match self.current().kind {
            TokenKind::Plus => Some((BinaryOp::Add, 10)),
            TokenKind::Minus => Some((BinaryOp::Subtract, 10)),
            TokenKind::Star => Some((BinaryOp::Multiply, 11)),
            TokenKind::Slash => Some((BinaryOp::Divide, 11)),
            TokenKind::Percent => Some((BinaryOp::Remainder, 11)),
            TokenKind::Ampersand => Some((BinaryOp::BitwiseAnd, 8)),
            TokenKind::Pipe => Some((BinaryOp::BitwiseOr, 6)),
            TokenKind::Caret => Some((BinaryOp::BitwiseXor, 7)),
            TokenKind::ShiftLeft => Some((BinaryOp::ShiftLeft, 9)),
            TokenKind::ShiftRight => Some((BinaryOp::ShiftRight, 9)),
            TokenKind::AndAnd => Some((BinaryOp::LogicalAnd, 3)),
            TokenKind::OrOr => Some((BinaryOp::LogicalOr, 2)),
            TokenKind::EqualEqual => Some((BinaryOp::Equal, 4)),
            TokenKind::BangEqual => Some((BinaryOp::NotEqual, 4)),
            TokenKind::Less => Some((BinaryOp::Less, 5)),
            TokenKind::LessEqual => Some((BinaryOp::LessEqual, 5)),
            TokenKind::Greater => Some((BinaryOp::Greater, 5)),
            TokenKind::GreaterEqual => Some((BinaryOp::GreaterEqual, 5)),
            _ => None,
        }
    }

    fn missing_expr(&self, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Missing,
            span,
        }
    }

}

fn offset_span(span: Span, offset: usize) -> Span {
    Span::new(span.start + offset, span.end + offset)
}
