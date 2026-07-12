impl Parser {
    fn parse_import(&mut self) -> ImportDecl {
        let start = self.expect_keyword(Keyword::From, "`from`").span;
        let mut path = String::new();

        while !self.at_eof() && self.current_keyword() != Some(Keyword::Import) {
            path.push_str(&self.advance().lexeme);
        }

        self.expect_keyword(Keyword::Import, "`import`");

        let mut names = Vec::new();
        let braced = self.match_kind(&TokenKind::LeftBrace);
        let namespace = if braced {
            while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
                let name_start = self.current().span;
                if let Some(name) = self.consume_identifier() {
                    let alias = if self.match_keyword(Keyword::As) {
                        Some(self.expect_identifier("expected import alias after `as`"))
                    } else {
                        None
                    };
                    names.push(ImportName {
                        name,
                        alias,
                        span: name_start.join(self.previous_span()),
                    });
                } else {
                    self.error_here("expected imported name");
                    break;
                }

                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_kind(&TokenKind::RightBrace, "`}`");
            None
        } else {
            let namespace_start = self.current().span;
            let name = self.expect_identifier("expected module namespace");
            Some(ImportNamespace {
                name,
                span: namespace_start.join(self.previous_span()),
            })
        };

        let end = self.previous_span();

        ImportDecl {
            path,
            names,
            namespace,
            span: start.join(end),
        }
    }

    fn parse_enum(&mut self) -> EnumDecl {
        let start = self.expect_keyword(Keyword::Enum, "`enum`").span;
        let name = self.expect_identifier("expected enum name");
        let (type_params, type_param_bounds) = self.parse_type_params();
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut variants = Vec::new();
        let mut members = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            let member_start = self.position;

            if self.current_keyword() == Some(Keyword::Static) {
                let start = self.expect_keyword(Keyword::Static, "`static`").span;
                self.expect_keyword(Keyword::Fn, "`fn`");
                let name = self.expect_identifier("expected static function name");
                let (type_params, type_param_bounds) = self.parse_type_params();
                members.push(StructMember::StaticMethod(self.parse_function_tail(
                    start,
                    Some(name),
                    type_params,
                    type_param_bounds,
                )));
            } else if self.current_keyword() == Some(Keyword::Fn) {
                members.push(StructMember::Method(self.parse_function(true)));
            } else {
                let variant_start = self.current().span;
                let variant_name = self.expect_identifier("expected enum variant name");
                let payload = if self.match_kind(&TokenKind::LeftParen) {
                    let payload = self.parse_type();
                    self.expect_kind(&TokenKind::RightParen, "`)`");
                    payload
                } else {
                    None
                };
                let span = variant_start.join(self.previous_span());
                variants.push(EnumVariant {
                    name: variant_name,
                    payload,
                    span,
                });
                self.match_kind(&TokenKind::Comma);
            }

            if self.position == member_start {
                self.advance();
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        EnumDecl {
            name,
            type_params,
            type_param_bounds,
            variants,
            members,
            span: start.join(end),
        }
    }

    fn parse_struct(&mut self) -> StructDecl {
        let start = self.expect_keyword(Keyword::Struct, "`struct`").span;
        let name = self.expect_identifier("expected struct name");
        let (type_params, type_param_bounds) = self.parse_type_params();
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut members = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            let member_start = self.position;

            if self.current_keyword() == Some(Keyword::Static) {
                let start = self.expect_keyword(Keyword::Static, "`static`").span;
                self.expect_keyword(Keyword::Fn, "`fn`");
                let name = self.expect_identifier("expected static function name");
                let (type_params, type_param_bounds) = self.parse_type_params();
                members.push(StructMember::StaticMethod(self.parse_function_tail(
                    start,
                    Some(name),
                    type_params,
                    type_param_bounds,
                )));
            } else if self.current_keyword() == Some(Keyword::Fn) {
                members.push(StructMember::Method(self.parse_function(true)));
            } else if self.check_identifier() {
                let field_start = self.current().span;
                let name = self.expect_identifier("expected field name");
                self.expect_kind(&TokenKind::Colon, "`:`");
                let type_ref = self
                    .parse_type()
                    .unwrap_or_else(|| self.missing_type(field_start));
                members.push(StructMember::Field(FieldDecl {
                    name,
                    span: field_start.join(type_ref.span),
                    type_ref,
                }));
                self.match_kind(&TokenKind::Comma);
            } else {
                self.error_here("expected struct field or method");
                self.advance();
            }

            if self.position == member_start {
                self.advance();
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        StructDecl {
            name,
            type_params,
            type_param_bounds,
            members,
            span: start.join(end),
        }
    }

    fn parse_trait(&mut self) -> TraitDecl {
        let start = self.expect_keyword(Keyword::Trait, "`trait`").span;
        let name = self.expect_identifier("expected trait name");
        let (type_params, type_param_bounds) = self.parse_type_params();
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut methods = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            if matches!(self.current_keyword(), Some(Keyword::Fn | Keyword::Static)) {
                methods.push(self.parse_trait_method());
            } else {
                self.error_here("expected trait method");
                self.advance();
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        TraitDecl {
            name,
            type_params,
            type_param_bounds,
            methods,
            span: start.join(end),
        }
    }

    fn parse_trait_method(&mut self) -> TraitMethodDecl {
        let (start, static_) = if self.current_keyword() == Some(Keyword::Static) {
            let start = self.expect_keyword(Keyword::Static, "`static`").span;
            self.expect_keyword(Keyword::Fn, "`fn`");
            (start, true)
        } else {
            (self.expect_keyword(Keyword::Fn, "`fn`").span, false)
        };
        let name = self.expect_callable_name("expected trait method name");
        self.expect_kind(&TokenKind::LeftParen, "`(`");
        let params = self.parse_params();
        self.expect_kind(&TokenKind::RightParen, "`)`");

        let return_type = if self.match_kind(&TokenKind::Colon) {
            self.parse_type()
        } else {
            None
        };

        TraitMethodDecl {
            name,
            static_,
            params,
            return_type,
            span: start.join(self.previous_span()),
        }
    }

    fn parse_impl(&mut self) -> ImplDecl {
        let start = self.expect_keyword(Keyword::Impl, "`impl`").span;
        let (type_params, type_param_bounds) = self.parse_type_params();
        let trait_ref = self
            .parse_type()
            .unwrap_or_else(|| self.missing_type(self.current().span));
        self.expect_keyword(Keyword::For, "`for`");
        let type_ref = self
            .parse_type()
            .unwrap_or_else(|| self.missing_type(self.current().span));
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut methods = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            if matches!(self.current_keyword(), Some(Keyword::Fn | Keyword::Static)) {
                let (function, static_) = if self.current_keyword() == Some(Keyword::Static) {
                    let start = self.expect_keyword(Keyword::Static, "`static`").span;
                    self.expect_keyword(Keyword::Fn, "`fn`");
                    let name = self.expect_callable_name("expected impl method name");
                    (
                        self.parse_function_tail(start, Some(name), Vec::new(), Vec::new()),
                        true,
                    )
                } else {
                    (self.parse_function(true), false)
                };
                let span = function.span;
                methods.push(ImplMember {
                    function,
                    static_,
                    span,
                });
            } else {
                self.error_here("expected impl method");
                self.advance();
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        ImplDecl {
            type_params,
            type_param_bounds,
            trait_ref,
            type_ref,
            methods,
            span: start.join(end),
        }
    }

    fn parse_type_params(&mut self) -> (Vec<String>, Vec<TypeParamBound>) {
        let mut params = Vec::new();
        let mut bounds = Vec::new();
        if !self.match_kind(&TokenKind::Less) {
            return (params, bounds);
        }

        while !self.at_eof() && !self.check_type_greater() {
            let start = self.current().span;
            let param = self.expect_identifier("expected type parameter name");
            params.push(param.clone());
            if self.match_kind(&TokenKind::Colon) {
                loop {
                    let trait_ref = self
                        .parse_type()
                        .unwrap_or_else(|| self.missing_type(self.current().span));
                    bounds.push(TypeParamBound {
                        param: param.clone(),
                        span: start.join(trait_ref.span),
                        trait_ref,
                    });
                    if !self.match_kind(&TokenKind::Plus) {
                        break;
                    }
                }
            }
            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_type_greater();
        (params, bounds)
    }

    fn parse_function(&mut self, named: bool) -> FunctionDecl {
        let start = self.expect_keyword(Keyword::Fn, "`fn`").span;
        let name = if named {
            Some(self.expect_callable_name("expected function name"))
        } else {
            None
        };
        let (type_params, type_param_bounds) = if named {
            self.parse_type_params()
        } else {
            (Vec::new(), Vec::new())
        };

        self.parse_function_tail(start, name, type_params, type_param_bounds)
    }

    fn parse_top_level_function(&mut self) -> Item {
        let start = self.expect_keyword(Keyword::Fn, "`fn`").span;
        self.parse_top_level_function_tail(start, false)
    }

    fn parse_static_extension(&mut self) -> Item {
        let start = self.expect_keyword(Keyword::Static, "`static`").span;
        self.expect_keyword(Keyword::Fn, "`fn`");
        self.parse_top_level_function_tail(start, true)
    }

    fn parse_top_level_function_tail(&mut self, start: Span, static_: bool) -> Item {
        let name_span = self.current().span;
        let first_name = self.expect_identifier("expected function or extension type name");

        if self.match_kind(&TokenKind::Dot) {
            let function_name = self.expect_callable_name("expected extension function name");
            let function =
                self.parse_function_tail(start, Some(function_name), Vec::new(), Vec::new());
            let type_ref = TypeRef {
                name: first_name,
                args: Vec::new(),
                function: None,
                span: name_span,
            };

            Item::Extension(ExtensionDecl {
                span: start.join(function.span),
                type_ref,
                function,
                static_,
            })
        } else {
            if static_ {
                self.error_here("static functions must be declared on a type");
            }
            let (type_params, type_param_bounds) = self.parse_type_params();
            Item::Function(self.parse_function_tail(
                start,
                Some(first_name),
                type_params,
                type_param_bounds,
            ))
        }
    }

    fn parse_function_tail(
        &mut self,
        start: Span,
        name: Option<String>,
        type_params: Vec<String>,
        type_param_bounds: Vec<TypeParamBound>,
    ) -> FunctionDecl {
        self.expect_kind(&TokenKind::LeftParen, "`(`");
        let params = self.parse_params();
        self.expect_kind(&TokenKind::RightParen, "`)`");

        let return_type = if self.match_kind(&TokenKind::Colon) {
            self.parse_type()
        } else {
            None
        };

        let body = if self.check_kind(&TokenKind::LeftBrace) {
            FunctionBody::Block(self.parse_block())
        } else if self.match_kind(&TokenKind::FatArrow) {
            FunctionBody::Expr(Box::new(self.parse_expression()))
        } else {
            self.error_here("expected function body");
            FunctionBody::Expr(Box::new(self.missing_expr(self.current().span)))
        };

        let span = start.join(body.span());

        FunctionDecl {
            name,
            type_params,
            type_param_bounds,
            params,
            return_type,
            body,
            span,
        }
    }

    fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();

        while !self.at_eof() && !self.check_kind(&TokenKind::RightParen) {
            let start = self.current().span;
            let mutable = self.match_keyword(Keyword::Mut);
            let name = self.expect_identifier("expected parameter name");
            let type_ref = if self.match_kind(&TokenKind::Colon) {
                self.parse_type()
            } else {
                None
            };

            params.push(Param {
                name,
                mutable,
                type_ref,
                span: start.join(self.previous_span()),
            });

            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }

        params
    }

    fn parse_struct_init(&mut self, name: String, args: Vec<TypeRef>, start: Span) -> Expr {
        self.expect_kind(&TokenKind::LeftBrace, "`{`");
        let mut fields = Vec::new();

        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            let field_start = self.current().span;
            let name = self.expect_identifier("expected struct field name");
            self.expect_kind(&TokenKind::Colon, "`:`");
            let value = self.parse_expression();
            let span = field_start.join(value.span);
            fields.push(StructInitField { name, value, span });

            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let span = start.join(self.previous_span());

        Expr {
            kind: ExprKind::StructInit { name, args, fields },
            span,
        }
    }

}
