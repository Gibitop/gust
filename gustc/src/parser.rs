use crate::ast::{
    BinaryOp, Block, ElseBranch, EnumDecl, EnumVariant, Expr, ExprKind, ExtensionDecl, FieldDecl,
    FunctionBody, FunctionDecl, ImportDecl, Item, MatchBranch, MatchBranchBody, Param, Pattern,
    Program, Stmt, StmtKind, StructDecl, StructInitField, StructMember, TypeRef, UnaryOp,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{Keyword, Token, TokenKind};
use crate::span::Span;

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
    diagnostics: Vec<Diagnostic>,
    allow_struct_init: bool,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            diagnostics: Vec::new(),
            allow_struct_init: true,
        }
    }

    pub fn parse(mut self) -> (Program, Vec<Diagnostic>) {
        let mut items = Vec::new();

        while !self.at_eof() {
            let start = self.position;

            match self.current_keyword() {
                Some(Keyword::From) => items.push(Item::Import(self.parse_import())),
                Some(Keyword::Enum) => items.push(Item::Enum(self.parse_enum())),
                Some(Keyword::Struct) => items.push(Item::Struct(self.parse_struct())),
                Some(Keyword::Fn) => items.push(self.parse_top_level_function()),
                Some(Keyword::Static) => items.push(self.parse_static_extension()),
                _ => {
                    self.error_here("expected a top-level declaration");
                    self.advance();
                    self.synchronize_top_level();
                }
            }

            if self.position == start {
                self.advance();
            }
        }

        (Program { items }, self.diagnostics)
    }

    fn parse_import(&mut self) -> ImportDecl {
        let start = self.expect_keyword(Keyword::From, "`from`").span;
        let mut path = String::new();

        while !self.at_eof() && self.current_keyword() != Some(Keyword::Import) {
            path.push_str(&self.advance().lexeme);
        }

        self.expect_keyword(Keyword::Import, "`import`");

        let mut names = Vec::new();
        let braced = self.match_kind(&TokenKind::LeftBrace);

        while !self.at_eof() && (!braced || !self.check_kind(&TokenKind::RightBrace)) {
            if let Some(name) = self.consume_identifier() {
                names.push(name);
            } else {
                self.error_here("expected imported name");
                break;
            }

            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
        }

        if braced {
            self.expect_kind(&TokenKind::RightBrace, "`}`");
        }

        let end = self.previous_span();

        ImportDecl {
            path,
            names,
            span: start.join(end),
        }
    }

    fn parse_enum(&mut self) -> EnumDecl {
        let start = self.expect_keyword(Keyword::Enum, "`enum`").span;
        let name = self.expect_identifier("expected enum name");
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut variants = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
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

        self.expect_kind(&TokenKind::RightBrace, "`}`");
        let end = self.previous_span();

        EnumDecl {
            name,
            variants,
            span: start.join(end),
        }
    }

    fn parse_struct(&mut self) -> StructDecl {
        let start = self.expect_keyword(Keyword::Struct, "`struct`").span;
        let name = self.expect_identifier("expected struct name");
        self.expect_kind(&TokenKind::LeftBrace, "`{`");

        let mut members = Vec::new();
        while !self.at_eof() && !self.check_kind(&TokenKind::RightBrace) {
            let member_start = self.position;

            if self.current_keyword() == Some(Keyword::Static) {
                let start = self.expect_keyword(Keyword::Static, "`static`").span;
                self.expect_keyword(Keyword::Fn, "`fn`");
                let name = self.expect_identifier("expected static function name");
                members.push(StructMember::StaticMethod(
                    self.parse_function_tail(start, Some(name)),
                ));
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
            members,
            span: start.join(end),
        }
    }

    fn parse_function(&mut self, named: bool) -> FunctionDecl {
        let start = self.expect_keyword(Keyword::Fn, "`fn`").span;
        let name = if named {
            Some(self.expect_identifier("expected function name"))
        } else {
            None
        };

        self.parse_function_tail(start, name)
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
            let function_name = self.expect_identifier("expected extension function name");
            let function = self.parse_function_tail(start, Some(function_name));
            let type_ref = TypeRef {
                name: first_name,
                args: Vec::new(),
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
            Item::Function(self.parse_function_tail(start, Some(first_name)))
        }
    }

    fn parse_function_tail(&mut self, start: Span, name: Option<String>) -> FunctionDecl {
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

    fn parse_expression(&mut self) -> Expr {
        self.parse_binary_expression(0)
    }

    fn parse_expression_without_struct_init(&mut self) -> Expr {
        let previous = self.allow_struct_init;
        self.allow_struct_init = false;
        let expr = self.parse_expression();
        self.allow_struct_init = previous;
        expr
    }

    fn parse_binary_expression(&mut self, min_precedence: u8) -> Expr {
        let mut left = self.parse_unary_expression();

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
                let name = self.expect_identifier("expected member name");
                expr = Expr {
                    span: expr.span.join(name_span),
                    kind: ExprKind::Member {
                        object: Box::new(expr),
                        name,
                    },
                };
            } else if self.match_kind(&TokenKind::PlusPlus) {
                expr = Expr {
                    span: expr.span.join(self.previous_span()),
                    kind: ExprKind::PostfixIncrement(Box::new(expr)),
                };
            } else if self.allow_struct_init && self.check_kind(&TokenKind::LeftBrace) {
                if let ExprKind::Identifier(name) = &expr.kind {
                    let name = name.clone();
                    expr = self.parse_struct_init(name, expr.span);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        expr
    }

    fn parse_struct_init(&mut self, name: String, start: Span) -> Expr {
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
            kind: ExprKind::StructInit { name, fields },
            span,
        }
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
            TokenKind::Keyword(Keyword::False) => Expr {
                kind: ExprKind::Bool(false),
                span: token.span,
            },
            TokenKind::Keyword(Keyword::True) => Expr {
                kind: ExprKind::Bool(true),
                span: token.span,
            },
            TokenKind::LeftBracket => self.finish_array(token.span),
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
            TokenKind::Keyword(Keyword::Match) => self.finish_match(token.span),
            _ => {
                self.diagnostics
                    .push(Diagnostic::error(token.span, "expected expression"));
                self.missing_expr(token.span)
            }
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
            self.expect_kind(&TokenKind::FatArrow, "`=>`");
            let body = if self.check_kind(&TokenKind::LeftBrace) {
                MatchBranchBody::Block(self.parse_block())
            } else {
                MatchBranchBody::Expr(self.parse_expression())
            };
            let span = pattern.span().join(body.span());
            branches.push(MatchBranch {
                pattern,
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

    fn parse_pattern(&mut self) -> Pattern {
        let start = self.current().span;
        if let TokenKind::StringLiteral(value) = self.current().kind.clone() {
            self.advance();
            return Pattern::String { value, span: start };
        }

        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "_") {
            self.advance();
            return Pattern::Wildcard { span: start };
        }

        let enum_name = self.expect_identifier("expected enum name in match pattern");
        self.expect_kind(&TokenKind::Dot, "`.`");
        let variant = self.expect_identifier("expected enum variant in match pattern");
        let binding = if self.match_kind(&TokenKind::LeftParen) {
            let binding = self.expect_identifier("expected pattern binding");
            self.expect_kind(&TokenKind::RightParen, "`)`");
            Some(binding)
        } else {
            None
        };
        let span = start.join(self.previous_span());

        Pattern::Variant {
            enum_name,
            variant,
            binding,
            span,
        }
    }

    fn parse_type(&mut self) -> Option<TypeRef> {
        let start = self.current().span;
        let name = if let Some(name) = self.consume_identifier() {
            name
        } else {
            self.error_here("expected type name");
            return None;
        };

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
            span: start.join(end),
        })
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

    fn missing_type(&self, span: Span) -> TypeRef {
        TypeRef {
            name: "<missing>".to_string(),
            args: Vec::new(),
            span,
        }
    }

    fn synchronize_top_level(&mut self) {
        while !self.at_eof() {
            if matches!(
                self.current_keyword(),
                Some(
                    Keyword::From | Keyword::Enum | Keyword::Struct | Keyword::Fn | Keyword::Static,
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

fn simple_kind_eq(left: &TokenKind, right: &TokenKind) -> bool {
    matches!(
        (left, right),
        (TokenKind::LeftParen, TokenKind::LeftParen)
            | (TokenKind::RightParen, TokenKind::RightParen)
            | (TokenKind::LeftBrace, TokenKind::LeftBrace)
            | (TokenKind::RightBrace, TokenKind::RightBrace)
            | (TokenKind::LeftBracket, TokenKind::LeftBracket)
            | (TokenKind::RightBracket, TokenKind::RightBracket)
            | (TokenKind::Colon, TokenKind::Colon)
            | (TokenKind::Comma, TokenKind::Comma)
            | (TokenKind::Dot, TokenKind::Dot)
            | (TokenKind::Slash, TokenKind::Slash)
            | (TokenKind::SlashEqual, TokenKind::SlashEqual)
            | (TokenKind::Plus, TokenKind::Plus)
            | (TokenKind::PlusPlus, TokenKind::PlusPlus)
            | (TokenKind::PlusEqual, TokenKind::PlusEqual)
            | (TokenKind::Minus, TokenKind::Minus)
            | (TokenKind::MinusEqual, TokenKind::MinusEqual)
            | (TokenKind::Star, TokenKind::Star)
            | (TokenKind::StarEqual, TokenKind::StarEqual)
            | (TokenKind::Percent, TokenKind::Percent)
            | (TokenKind::PercentEqual, TokenKind::PercentEqual)
            | (TokenKind::Equal, TokenKind::Equal)
            | (TokenKind::EqualEqual, TokenKind::EqualEqual)
            | (TokenKind::Bang, TokenKind::Bang)
            | (TokenKind::BangEqual, TokenKind::BangEqual)
            | (TokenKind::Ampersand, TokenKind::Ampersand)
            | (TokenKind::AmpersandEqual, TokenKind::AmpersandEqual)
            | (TokenKind::AndAnd, TokenKind::AndAnd)
            | (TokenKind::Pipe, TokenKind::Pipe)
            | (TokenKind::PipeEqual, TokenKind::PipeEqual)
            | (TokenKind::OrOr, TokenKind::OrOr)
            | (TokenKind::Caret, TokenKind::Caret)
            | (TokenKind::CaretEqual, TokenKind::CaretEqual)
            | (TokenKind::FatArrow, TokenKind::FatArrow)
            | (TokenKind::ShiftLeft, TokenKind::ShiftLeft)
            | (TokenKind::ShiftLeftEqual, TokenKind::ShiftLeftEqual)
            | (TokenKind::ShiftRight, TokenKind::ShiftRight)
            | (TokenKind::ShiftRightEqual, TokenKind::ShiftRightEqual)
            | (TokenKind::LessEqual, TokenKind::LessEqual)
            | (TokenKind::GreaterEqual, TokenKind::GreaterEqual)
            | (TokenKind::Less, TokenKind::Less)
            | (TokenKind::Greater, TokenKind::Greater)
            | (TokenKind::Eof, TokenKind::Eof)
    )
}
