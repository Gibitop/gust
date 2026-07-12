impl Parser {
    fn parse_pattern(&mut self) -> Pattern {
        self.parse_pattern_with_bindings(false)
    }

    fn parse_pattern_with_bindings(&mut self, allow_binding: bool) -> Pattern {
        let start = self.current().span;
        if allow_binding && self.match_keyword(Keyword::Mut) {
            let name = self.expect_identifier("expected pattern binding");
            return Pattern::Binding {
                name,
                mutable: true,
                span: start.join(self.previous_span()),
            };
        }

        if let TokenKind::StringLiteral(value) = self.current().kind.clone() {
            self.advance();
            return Pattern::String { value, span: start };
        }

        if let TokenKind::Number(value) = self.current().kind.clone() {
            self.advance();
            let inclusive = if self.match_kind(&TokenKind::DotDotEqual) {
                Some(true)
            } else if self.match_kind(&TokenKind::DotDot) {
                Some(false)
            } else {
                None
            };
            if let Some(inclusive) = inclusive {
                let end = if let TokenKind::Number(value) = self.current().kind.clone() {
                    self.advance();
                    value
                } else {
                    self.error_here("expected range pattern end");
                    String::new()
                };
                return Pattern::Range {
                    start: value,
                    end,
                    inclusive,
                    span: start.join(self.previous_span()),
                };
            }
            return Pattern::Number { value, span: start };
        }

        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "_") {
            self.advance();
            return Pattern::Wildcard { span: start };
        }

        let mut path = vec![self.expect_identifier("expected enum name in match pattern")];
        while self.match_kind(&TokenKind::Dot) {
            path.push(self.expect_identifier("expected enum variant in match pattern"));
        }
        let variant = path.pop().unwrap_or_default();
        let enum_name = path.join(".");
        if enum_name.is_empty() {
            if allow_binding {
                return Pattern::Binding {
                    name: variant,
                    mutable: false,
                    span: start,
                };
            }
            self.error_here("expected `.` and enum variant in match pattern");
        }
        let payload = if self.match_kind(&TokenKind::LeftParen) {
            let payload = self.parse_pattern_with_bindings(true);
            self.expect_kind(&TokenKind::RightParen, "`)`");
            Some(Box::new(payload))
        } else {
            None
        };
        let span = start.join(self.previous_span());

        Pattern::Variant {
            enum_name,
            variant,
            payload,
            span,
        }
    }

}
