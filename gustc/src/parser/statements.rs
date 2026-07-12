impl Parser {
    fn parse_block(&mut self) -> Block {
        let start = self.expect_kind(&TokenKind::LeftBrace, "`{`").span;
        let mut statements = Vec::new();

        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            let position = self.position;
            statements.push(self.parse_statement());

            if self.position == position {
                self.advance();
            }
        }

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        Block {
            statements,
            span: start.join(end),
        }
    }

    fn parse_statement(&mut self) -> Stmt {
        match self.current_keyword() {
            Some(Keyword::Let) => self.parse_let_statement(),
            Some(Keyword::Return) => self.parse_return_statement(),
            Some(Keyword::If) => self.parse_if_statement(),
            Some(Keyword::While) => self.parse_while_statement(),
            Some(Keyword::Break) => self.parse_break_statement(),
            Some(Keyword::Continue) => self.parse_continue_statement(),
            Some(Keyword::For) => self.parse_for_statement(),
            _ => {
                let target = self.parse_expression();

                if let Some(op) = self.match_assignment_operator() {
                    let value = self.parse_expression();
                    Stmt {
                        span: target.span.join(value.span),
                        kind: StmtKind::Assign { target, op, value },
                    }
                } else {
                    Stmt {
                        span: target.span,
                        kind: StmtKind::Expr(target),
                    }
                }
            }
        }
    }

    fn parse_let_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::Let, "`let`").span;
        let mutable = self.match_keyword(Keyword::Mut);
        let name = self.expect_identifier("expected binding name");
        let type_annotation = if self.match_kind(&TokenKind::Colon) {
            self.parse_type()
        } else {
            None
        };

        let value = if self.match_kind(&TokenKind::Equal) {
            Some(self.parse_expression())
        } else {
            if type_annotation.is_none() {
                self.error_here("expected `=` or type annotation in let statement");
            }

            None
        };
        let span = start.join(
            value
                .as_ref()
                .map_or_else(|| self.previous_span(), |value| value.span),
        );

        Stmt {
            kind: StmtKind::Let {
                name,
                mutable,
                type_annotation,
                value,
            },
            span,
        }
    }

    fn parse_return_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::Return, "`return`").span;
        let value = if self.check_kind(&TokenKind::RightBrace) || self.at_eof() {
            None
        } else {
            Some(self.parse_expression())
        };
        let end = value.as_ref().map_or(start, |expr| expr.span);

        Stmt {
            kind: StmtKind::Return { value },
            span: start.join(end),
        }
    }

    fn parse_for_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::For, "`for`").span;
        let name = self.expect_identifier("expected loop binding name");
        self.expect_keyword(Keyword::In, "`in`");
        let iterable = self.parse_expression_without_struct_init();
        let body = self.parse_block();
        let span = start.join(body.span);

        Stmt {
            kind: StmtKind::For {
                name,
                iterable,
                body,
            },
            span,
        }
    }

    fn parse_while_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::While, "`while`").span;
        let condition = self.parse_expression_without_struct_init();
        let body = self.parse_block();
        let span = start.join(body.span);

        Stmt {
            kind: StmtKind::While { condition, body },
            span,
        }
    }

    fn parse_break_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::Break, "`break`").span;

        Stmt {
            kind: StmtKind::Break,
            span: start,
        }
    }

    fn parse_continue_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::Continue, "`continue`").span;

        Stmt {
            kind: StmtKind::Continue,
            span: start,
        }
    }

    fn parse_if_statement(&mut self) -> Stmt {
        let start = self.expect_keyword(Keyword::If, "`if`").span;
        let condition = self.parse_expression_without_struct_init();
        let then_branch = self.parse_block();
        let else_branch = if self.match_keyword(Keyword::Else) {
            if self.current_keyword() == Some(Keyword::If) {
                Some(ElseBranch::If(Box::new(self.parse_if_statement())))
            } else {
                Some(ElseBranch::Block(self.parse_block()))
            }
        } else {
            None
        };
        let end = match &else_branch {
            Some(ElseBranch::Block(block)) => block.span,
            Some(ElseBranch::If(statement)) => statement.span,
            None => then_branch.span,
        };

        Stmt {
            kind: StmtKind::If {
                condition,
                then_branch,
                else_branch,
            },
            span: start.join(end),
        }
    }

}
