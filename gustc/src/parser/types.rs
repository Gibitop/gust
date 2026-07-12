impl Parser {
    fn parse_type_args(&mut self) -> Option<Vec<TypeRef>> {
        if !self.match_kind(&TokenKind::Less) {
            return None;
        }

        let mut args = Vec::new();
        while !self.at_eof() && !self.check_type_greater() {
            args.push(self.parse_type()?);
            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_type_greater();
        Some(args)
    }

    fn parse_type(&mut self) -> Option<TypeRef> {
        let start = self.current().span;
        if self.match_keyword(Keyword::Fn) {
            self.expect_kind(&TokenKind::LeftParen, "`(`");
            let mut params = Vec::new();
            while !self.at_eof() && !self.check_kind(&TokenKind::RightParen) {
                let mutable = self.match_keyword(Keyword::Mut);
                let type_ref = self
                    .parse_type()
                    .unwrap_or_else(|| self.missing_type(self.current().span));
                params.push(FunctionTypeParam { mutable, type_ref });

                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect_kind(&TokenKind::RightParen, "`)`");
            self.expect_kind(&TokenKind::Colon, "`:`");
            let return_type = self
                .parse_type()
                .unwrap_or_else(|| self.missing_type(self.current().span));
            let span = start.join(return_type.span);
            return Some(TypeRef {
                name: "fn".to_string(),
                args: Vec::new(),
                function: Some(FunctionTypeRef {
                    params,
                    return_type: Box::new(return_type),
                }),
                span,
            });
        }

        let mut name = if let Some(name) = self.consume_identifier() {
            name
        } else {
            self.error_here("expected type name");
            return None;
        };
        while self.match_kind(&TokenKind::Dot) {
            name.push('.');
            name.push_str(&self.expect_identifier("expected type name after `.`"));
        }

        let mut args = Vec::new();
        let mut end = self.previous_span();
        if self.match_kind(&TokenKind::Less) {
            while !self.at_eof() && !self.check_type_greater() {
                if let Some(type_ref) = self.parse_type() {
                    args.push(type_ref);
                }

                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }

            end = self.expect_type_greater().span;
        }

        Some(TypeRef {
            name,
            args,
            function: None,
            span: start.join(end),
        })
    }

    fn expect_type_greater(&mut self) -> Token {
        let token = self.current().clone();

        match token.kind {
            TokenKind::Greater => self.advance(),
            TokenKind::GreaterEqual => {
                self.tokens[self.position] = Token {
                    kind: TokenKind::Equal,
                    span: Span::new(token.span.start + 1, token.span.end),
                    lexeme: "=".to_string(),
                };
                Token {
                    kind: TokenKind::Greater,
                    span: Span::new(token.span.start, token.span.start + 1),
                    lexeme: ">".to_string(),
                }
            }
            TokenKind::ShiftRight | TokenKind::ShiftRightEqual => {
                self.tokens[self.position] = Token {
                    kind: if matches!(token.kind, TokenKind::ShiftRightEqual) {
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    },
                    span: Span::new(token.span.start + 1, token.span.end),
                    lexeme: token.lexeme[1..].to_string(),
                };
                Token {
                    kind: TokenKind::Greater,
                    span: Span::new(token.span.start, token.span.start + 1),
                    lexeme: ">".to_string(),
                }
            }
            _ => {
                self.error_here("expected `>`");
                self.synthetic_current()
            }
        }
    }

    fn check_type_greater(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::ShiftRight
                | TokenKind::ShiftRightEqual
        )
    }

}
