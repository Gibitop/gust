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

    fn parse_export(&mut self) -> Item {
        let start = self.expect_keyword(Keyword::Export, "`export`").span;
        match self.current_keyword() {
            Some(Keyword::Enum) => Item::Enum(self.parse_enum(true, Some(start))),
            Some(Keyword::Struct) => Item::Struct(self.parse_struct(true, Some(start))),
            Some(Keyword::Trait) => Item::Trait(self.parse_trait(true, Some(start))),
            Some(Keyword::Fn) => self.parse_top_level_function(true, Some(start)),
            Some(Keyword::Static) => self.parse_static_extension(true, Some(start)),
            _ => {
                self.error_here("expected exported enum, struct, trait, or function");
                self.advance();
                self.synchronize_top_level();
                Item::Function(FunctionDecl {
                    name: Some("<missing>".to_string()),
                    exported: true,
                    type_params: Vec::new(),
                    type_param_bounds: Vec::new(),
                    params: Vec::new(),
                    return_type: None,
                    body: FunctionBody::Expr(Box::new(self.missing_expr(start))),
                    span: start,
                })
            }
        }
    }

    fn parse_enum(&mut self, exported: bool, export_start: Option<Span>) -> EnumDecl {
        let start = if let Some(start) = export_start {
            self.expect_keyword(Keyword::Enum, "`enum`");
            start
        } else {
            self.expect_keyword(Keyword::Enum, "`enum`").span
        };
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
            exported,
            type_params,
            type_param_bounds,
            variants,
            members,
            span: start.join(end),
        }
    }

    fn parse_struct(&mut self, exported: bool, export_start: Option<Span>) -> StructDecl {
        let start = if let Some(start) = export_start {
            self.expect_keyword(Keyword::Struct, "`struct`");
            start
        } else {
            self.expect_keyword(Keyword::Struct, "`struct`").span
        };
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
            } else if self.current_keyword() == Some(Keyword::Internal)
                || self.check_identifier()
            {
                let internal = self.match_keyword(Keyword::Internal);
                let field_start = self.current().span;
                let name = self.expect_identifier("expected field name");
                self.expect_kind(&TokenKind::Colon, "`:`");
                let type_ref = self
                    .parse_type()
                    .unwrap_or_else(|| self.missing_type(field_start));
                members.push(StructMember::Field(FieldDecl {
                    name,
                    internal,
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
            exported,
            type_params,
            type_param_bounds,
            members,
            span: start.join(end),
        }
    }

    fn parse_trait(&mut self, exported: bool, export_start: Option<Span>) -> TraitDecl {
        let start = if let Some(start) = export_start {
            self.expect_keyword(Keyword::Trait, "`trait`");
            start
        } else {
            self.expect_keyword(Keyword::Trait, "`trait`").span
        };
        let name = self.expect_identifier("expected trait name");
        let (type_params, type_param_bounds) = self.parse_type_params();
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut associated_types = Vec::new();
        let mut methods = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            if matches!(self.current_keyword(), Some(Keyword::Fn | Keyword::Static)) {
                methods.push(self.parse_trait_method());
            } else if self.current_keyword() == Some(Keyword::Type) {
                let type_start = self.expect_keyword(Keyword::Type, "`type`").span;
                let name = self.expect_identifier("expected associated type name");
                let (type_params, type_param_bounds) = self.parse_type_params();
                let mut bounds = Vec::new();
                if self.match_kind(&TokenKind::Colon) {
                    loop {
                        let bound = self
                            .parse_type()
                            .unwrap_or_else(|| self.missing_type(self.current().span));
                        bounds.push(bound);
                        if !self.match_kind(&TokenKind::Plus) {
                            break;
                        }
                    }
                }
                let default = if self.match_kind(&TokenKind::Equal) {
                    Some(
                        self.parse_type()
                            .unwrap_or_else(|| self.missing_type(self.current().span)),
                    )
                } else {
                    None
                };
                associated_types.push(AssociatedTypeDecl {
                    name,
                    type_params,
                    type_param_bounds,
                    bounds,
                    default,
                    span: type_start.join(self.previous_span()),
                });
            } else {
                self.error_here("expected trait method or associated type");
                self.advance();
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        TraitDecl {
            name,
            exported,
            type_params,
            type_param_bounds,
            associated_types,
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
        let (type_params, type_param_bounds) = self.parse_type_params();
        self.expect_kind(&TokenKind::LeftParen, "`(`");
        let params = self.parse_params();
        self.expect_kind(&TokenKind::RightParen, "`)`");

        let return_type = if self.match_kind(&TokenKind::Colon) {
            self.parse_type()
        } else {
            None
        };

        let body = if self.check_kind(&TokenKind::LeftBrace) {
            Some(FunctionBody::Block(self.parse_block()))
        } else if self.match_kind(&TokenKind::FatArrow) {
            Some(FunctionBody::Expr(Box::new(self.parse_expression())))
        } else {
            None
        };

        TraitMethodDecl {
            name,
            static_,
            type_params,
            type_param_bounds,
            params,
            return_type,
            span: start.join(body.as_ref().map_or_else(|| self.previous_span(), FunctionBody::span)),
            body,
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

        let mut associated_types = Vec::new();
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
            } else if self.current_keyword() == Some(Keyword::Type) {
                let type_start = self.expect_keyword(Keyword::Type, "`type`").span;
                let name = self.expect_identifier("expected associated type name");
                let (type_params, type_param_bounds) = self.parse_type_params();
                self.expect_kind(&TokenKind::Colon, "`:`");
                let type_ref = self
                    .parse_type()
                    .unwrap_or_else(|| self.missing_type(self.current().span));
                associated_types.push(AssociatedTypeDef {
                    name,
                    type_params,
                    type_param_bounds,
                    span: type_start.join(type_ref.span),
                    type_ref,
                });
            } else {
                self.error_here("expected impl method or associated type definition");
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
            associated_types,
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

    fn parse_top_level_function(&mut self, exported: bool, export_start: Option<Span>) -> Item {
        let start = if let Some(start) = export_start {
            self.expect_keyword(Keyword::Fn, "`fn`");
            start
        } else {
            self.expect_keyword(Keyword::Fn, "`fn`").span
        };
        self.parse_top_level_function_tail(start, false, exported)
    }

    fn parse_static_extension(&mut self, exported: bool, export_start: Option<Span>) -> Item {
        let start = if let Some(start) = export_start {
            self.expect_keyword(Keyword::Static, "`static`");
            start
        } else {
            self.expect_keyword(Keyword::Static, "`static`").span
        };
        self.expect_keyword(Keyword::Fn, "`fn`");
        self.parse_top_level_function_tail(start, true, exported)
    }

    fn parse_top_level_function_tail(&mut self, start: Span, static_: bool, exported: bool) -> Item {
        let name_span = self.current().span;
        let first_name = self.expect_identifier("expected function or extension type name");
        let after_name = self.position;
        let diagnostic_count = self.diagnostics.len();
        let (type_ref, extension_type_params, extension_type_param_bounds) =
            self.parse_extension_target_type(first_name.clone(), name_span);

        if self.match_kind(&TokenKind::Dot) {
            let function_name = self.expect_callable_name("expected extension function name");
            let (function_type_params, function_type_param_bounds) = self.parse_type_params();
            let mut function = self.parse_function_tail(
                start,
                Some(function_name),
                function_type_params,
                function_type_param_bounds,
            );
            function.exported = exported;

            Item::Extension(ExtensionDecl {
                span: start.join(function.span),
                type_ref,
                exported,
                type_params: extension_type_params,
                type_param_bounds: extension_type_param_bounds,
                function,
                static_,
            })
        } else {
            self.position = after_name;
            self.diagnostics.truncate(diagnostic_count);
            if static_ {
                self.error_here("static functions must be declared on a type");
            }
            let (type_params, type_param_bounds) = self.parse_type_params();
            let mut function = self.parse_function_tail(
                start,
                Some(first_name),
                type_params,
                type_param_bounds,
            );
            function.exported = exported;
            Item::Function(function)
        }
    }

    fn parse_extension_target_type(
        &mut self,
        name: String,
        name_span: Span,
    ) -> (TypeRef, Vec<String>, Vec<TypeParamBound>) {
        let mut args = Vec::new();
        let mut bindings = Vec::new();
        let mut type_params = Vec::new();
        let mut type_param_bounds = Vec::new();
        let mut end = name_span;

        if self.match_kind(&TokenKind::Less) {
            while !self.at_eof() && !self.check_type_greater() {
                if self.current_keyword() == Some(Keyword::Type) {
                    let binding_start = self.expect_keyword(Keyword::Type, "`type`").span;
                    let binding_name = self.expect_identifier("expected associated type name");
                    self.expect_kind(&TokenKind::Colon, "`:`");
                    let type_ref = self
                        .parse_type()
                        .unwrap_or_else(|| self.missing_type(self.current().span));
                    bindings.push(AssociatedTypeBinding {
                        name: binding_name,
                        span: binding_start.join(type_ref.span),
                        type_ref,
                    });
                } else {
                    let Some(arg) = self.parse_type() else {
                        break;
                    };
                    if arg.args.is_empty()
                        && arg.function.is_none()
                        && self.match_kind(&TokenKind::Colon)
                    {
                        if !type_params.contains(&arg.name) {
                            type_params.push(arg.name.clone());
                        }
                        loop {
                            let trait_ref = self
                                .parse_type()
                                .unwrap_or_else(|| self.missing_type(self.current().span));
                            type_param_bounds.push(TypeParamBound {
                                param: arg.name.clone(),
                                span: arg.span.join(trait_ref.span),
                                trait_ref,
                            });
                            if !self.match_kind(&TokenKind::Plus) {
                                break;
                            }
                        }
                    }
                    args.push(arg);
                }
                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }

            end = self.expect_type_greater().span;
        }

        (
            TypeRef {
                name,
                args,
                bindings,
                function: None,
                span: name_span.join(end),
            },
            type_params,
            type_param_bounds,
        )
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
            exported: false,
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
