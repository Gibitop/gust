use crate::span::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Import(ImportDecl),
    Enum(EnumDecl),
    Struct(StructDecl),
    Function(FunctionDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Import(item) => item.span,
            Item::Enum(item) => item.span,
            Item::Struct(item) => item.span,
            Item::Function(item) => item.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: String,
    pub names: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub payload: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub members: Vec<StructMember>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StructMember {
    Field(FieldDecl),
    Method(FunctionDecl),
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub type_ref: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: FunctionBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub mutable: bool,
    pub type_ref: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    Block(Block),
    Expr(Box<Expr>),
}

impl FunctionBody {
    pub fn span(&self) -> Span {
        match self {
            FunctionBody::Block(block) => block.span,
            FunctionBody::Expr(expr) => expr.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let {
        name: String,
        mutable: bool,
        type_annotation: Option<TypeRef>,
        value: Expr,
    },
    Return {
        value: Option<Expr>,
    },
    For {
        name: String,
        iterable: Expr,
        body: Block,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Identifier(String),
    Number(String),
    String(String),
    Array(Vec<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
    StructInit {
        name: String,
        fields: Vec<StructInitField>,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Match {
        value: Box<Expr>,
        branches: Vec<MatchBranch>,
    },
    Lambda(FunctionDecl),
    PostfixIncrement(Box<Expr>),
    Missing,
}

#[derive(Debug, Clone)]
pub struct StructInitField {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    GreaterEqual,
}

#[derive(Debug, Clone)]
pub struct MatchBranch {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Identifier {
        name: String,
        binding: Option<String>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Identifier { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeRef>,
    pub span: Span,
}
