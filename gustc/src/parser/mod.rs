use crate::ast::{
    AssociatedTypeBinding, AssociatedTypeDecl, AssociatedTypeDef, BinaryOp, Block, ElseBranch,
    EnumDecl, EnumVariant, Expr, ExprKind, ExtensionDecl, FieldDecl, FunctionBody, FunctionDecl,
    FunctionTypeParam, FunctionTypeRef, INDEX_METHOD, INDEX_SET_METHOD, ImplDecl, ImplMember,
    ImportDecl, ImportName, ImportNamespace, Item, MatchBranch, MatchBranchBody, Param, Pattern,
    Program, Stmt, StmtKind, StructDecl, StructInitField, StructMember, TraitDecl, TraitMethodDecl,
    TypeParamBound, TypeRef, UnaryOp,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{
    InterpolatedPathSegment, InterpolatedStringPart, Keyword, Lexer, Token, TokenKind,
};
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
                Some(Keyword::Trait) => items.push(Item::Trait(self.parse_trait())),
                Some(Keyword::Impl) => items.push(Item::Impl(self.parse_impl())),
                Some(Keyword::Fn) => items.push(self.parse_top_level_function()),
                Some(Keyword::Static) => items.push(self.parse_static_extension()),
                Some(Keyword::Type) => {
                    self.error_here(
                        "associated-type definitions are only allowed inside trait impls",
                    );
                    self.advance();
                    self.synchronize_top_level();
                }
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
}
include!("declarations.rs");
include!("statements.rs");
include!("expressions.rs");
include!("patterns.rs");
include!("types.rs");
include!("tokens.rs");

fn expression_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name.clone()),
        ExprKind::Member { object, name } => Some(format!("{}.{name}", expression_path(object)?)),
        _ => None,
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
            | (TokenKind::DotDot, TokenKind::DotDot)
            | (TokenKind::DotDotEqual, TokenKind::DotDotEqual)
            | (TokenKind::Ellipsis, TokenKind::Ellipsis)
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
