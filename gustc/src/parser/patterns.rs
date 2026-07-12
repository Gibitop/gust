impl Parser {
    fn parse_pattern(&mut self) -> Pattern {
        let start = self.current().span;
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
            self.error_here("expected `.` and enum variant in match pattern");
        }
        let (binding, binding_mutable) = if self.match_kind(&TokenKind::LeftParen) {
            let binding_mutable = self.match_keyword(Keyword::Mut);
            let binding = self.expect_identifier("expected pattern binding");
            self.expect_kind(&TokenKind::RightParen, "`)`");
            (Some(binding), binding_mutable)
        } else {
            (None, false)
        };
        let span = start.join(self.previous_span());

        Pattern::Variant {
            enum_name,
            variant,
            binding,
            binding_mutable,
            span,
        }
    }

}
