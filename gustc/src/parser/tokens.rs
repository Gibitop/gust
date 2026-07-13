impl Parser {
    fn missing_type(&self, span: Span) -> TypeRef {
        TypeRef {
            name: "<missing>".to_string(),
            args: Vec::new(),
            bindings: Vec::new(),
            function: None,
            span,
        }
    }

    fn synchronize_top_level(&mut self) {
        while !self.at_eof() {
            if matches!(
                self.current_keyword(),
                Some(
                    Keyword::From
                        | Keyword::Enum
                        | Keyword::Struct
                        | Keyword::Trait
                        | Keyword::Impl
                        | Keyword::Fn
                        | Keyword::Static,
                )
            ) {
                return;
            }

            self.advance();
        }
    }

    fn expect_identifier(&mut self, message: &str) -> String {
        if let Some(name) = self.consume_identifier() {
            return name;
        }

        self.error_here(message);
        "<missing>".to_string()
    }

    fn expect_callable_name(&mut self, message: &str) -> String {
        if self.current_keyword() == Some(Keyword::From) {
            self.advance();
            return "from".to_string();
        }
        self.expect_identifier(message)
    }

    fn consume_identifier(&mut self) -> Option<String> {
        match self.current().kind.clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Some(name)
            }
            _ => None,
        }
    }

    fn expect_keyword(&mut self, keyword: Keyword, expected: &str) -> Token {
        if self.current_keyword() == Some(keyword) {
            return self.advance();
        }

        self.error_here(format!("expected {expected}"));
        self.synthetic_current()
    }

    fn expect_kind(&mut self, kind: &TokenKind, expected: &str) -> Token {
        if self.check_kind(kind) {
            return self.advance();
        }

        self.error_here(format!("expected {expected}"));
        self.synthetic_current()
    }

    fn match_keyword(&mut self, keyword: Keyword) -> bool {
        if self.current_keyword() != Some(keyword) {
            return false;
        }

        self.advance();
        true
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if !self.check_kind(kind) {
            return false;
        }

        self.advance();
        true
    }

    fn match_assignment_operator(&mut self) -> Option<Option<BinaryOp>> {
        let op = match self.current().kind {
            TokenKind::Equal => None,
            TokenKind::PlusEqual => Some(BinaryOp::Add),
            TokenKind::MinusEqual => Some(BinaryOp::Subtract),
            TokenKind::StarEqual => Some(BinaryOp::Multiply),
            TokenKind::SlashEqual => Some(BinaryOp::Divide),
            TokenKind::PercentEqual => Some(BinaryOp::Remainder),
            TokenKind::AmpersandEqual => Some(BinaryOp::BitwiseAnd),
            TokenKind::PipeEqual => Some(BinaryOp::BitwiseOr),
            TokenKind::CaretEqual => Some(BinaryOp::BitwiseXor),
            TokenKind::ShiftLeftEqual => Some(BinaryOp::ShiftLeft),
            TokenKind::ShiftRightEqual => Some(BinaryOp::ShiftRight),
            _ => return None,
        };
        self.advance();
        Some(op)
    }

    fn check_identifier(&self) -> bool {
        matches!(self.current().kind, TokenKind::Identifier(_))
    }

    fn check_kind(&self, kind: &TokenKind) -> bool {
        simple_kind_eq(&self.current().kind, kind)
    }

    fn current_keyword(&self) -> Option<Keyword> {
        match self.current().kind {
            TokenKind::Keyword(keyword) => Some(keyword),
            _ => None,
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();

        if !matches!(token.kind, TokenKind::Eof) {
            self.position += 1;
        }

        token
    }

    fn previous_span(&self) -> Span {
        self.tokens
            .get(self.position.saturating_sub(1))
            .map_or_else(|| self.current().span, |token| token.span)
    }

    fn synthetic_current(&self) -> Token {
        Token {
            kind: TokenKind::Identifier(String::new()),
            span: self.current().span,
            lexeme: String::new(),
        }
    }

    fn error_here(&mut self, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(self.current().span, message));
    }
}
