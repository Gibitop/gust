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
    Trait(TraitDecl),
    Impl(ImplDecl),
    Extension(ExtensionDecl),
    Function(FunctionDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Import(item) => item.span,
            Item::Enum(item) => item.span,
            Item::Struct(item) => item.span,
            Item::Trait(item) => item.span,
            Item::Impl(item) => item.span,
            Item::Extension(item) => item.span,
            Item::Function(item) => item.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: String,
    pub names: Vec<ImportName>,
    pub namespace: Option<ImportNamespace>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportNamespace {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportName {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub type_param_bounds: Vec<TypeParamBound>,
    pub variants: Vec<EnumVariant>,
    pub members: Vec<StructMember>,
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
    pub type_params: Vec<String>,
    pub type_param_bounds: Vec<TypeParamBound>,
    pub members: Vec<StructMember>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub type_param_bounds: Vec<TypeParamBound>,
    pub methods: Vec<TraitMethodDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TraitMethodDecl {
    pub name: String,
    pub static_: bool,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImplDecl {
    pub type_params: Vec<String>,
    pub type_param_bounds: Vec<TypeParamBound>,
    pub trait_ref: TypeRef,
    pub type_ref: TypeRef,
    pub methods: Vec<ImplMember>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImplMember {
    pub function: FunctionDecl,
    pub static_: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StructMember {
    Field(FieldDecl),
    Method(FunctionDecl),
    StaticMethod(FunctionDecl),
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub type_ref: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ExtensionDecl {
    pub type_ref: TypeRef,
    pub function: FunctionDecl,
    pub static_: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Option<String>,
    pub type_params: Vec<String>,
    pub type_param_bounds: Vec<TypeParamBound>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: FunctionBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeParamBound {
    pub param: String,
    pub trait_ref: TypeRef,
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
        value: Option<Expr>,
    },
    Assign {
        target: Expr,
        op: Option<BinaryOp>,
        value: Expr,
    },
    Return {
        value: Option<Expr>,
    },
    If {
        condition: Expr,
        then_branch: Block,
        else_branch: Option<ElseBranch>,
    },
    While {
        condition: Expr,
        body: Block,
    },
    Break,
    Continue,
    For {
        name: String,
        iterable: Expr,
        body: Block,
    },
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub enum ElseBranch {
    Block(Block),
    If(Box<Stmt>),
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
    Char(u32),
    Bool(bool),
    Array(Vec<Expr>),
    CollectionLiteral {
        items: Vec<Expr>,
        collection: TypeRef,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Member {
        object: Box<Expr>,
        name: String,
    },
    GenericMember {
        object: Box<Expr>,
        name: String,
        args: Vec<TypeRef>,
    },
    GenericType {
        name: String,
        args: Vec<TypeRef>,
    },
    StructInit {
        name: String,
        args: Vec<TypeRef>,
        fields: Vec<StructInitField>,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
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
    Subtract,
    Multiply,
    Divide,
    Remainder,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    LogicalAnd,
    LogicalOr,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl BinaryOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Remainder => "%",
            BinaryOp::BitwiseAnd => "&",
            BinaryOp::BitwiseOr => "|",
            BinaryOp::BitwiseXor => "^",
            BinaryOp::ShiftLeft => "<<",
            BinaryOp::ShiftRight => ">>",
            BinaryOp::LogicalAnd => "&&",
            BinaryOp::LogicalOr => "||",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
}

#[derive(Debug, Clone)]
pub struct MatchBranch {
    pub pattern: Pattern,
    pub body: MatchBranchBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum MatchBranchBody {
    Expr(Box<Expr>),
    Block(Block),
}

impl MatchBranchBody {
    pub fn span(&self) -> Span {
        match self {
            MatchBranchBody::Expr(expr) => expr.span,
            MatchBranchBody::Block(block) => block.span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Variant {
        enum_name: String,
        variant: String,
        payload: Option<Box<Pattern>>,
        span: Span,
    },
    Struct {
        name: String,
        fields: Vec<StructPatternField>,
        has_rest: bool,
        span: Span,
    },
    Binding {
        name: String,
        mutable: bool,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Number {
        value: String,
        span: Span,
    },
    Range {
        start: String,
        end: String,
        inclusive: bool,
        span: Span,
    },
    Wildcard {
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct StructPatternField {
    pub name: String,
    pub pattern: Pattern,
    pub span: Span,
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Variant { span, .. }
            | Pattern::Struct { span, .. }
            | Pattern::Binding { span, .. }
            | Pattern::String { span, .. }
            | Pattern::Number { span, .. }
            | Pattern::Range { span, .. }
            | Pattern::Wildcard { span } => *span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeRef>,
    pub function: Option<FunctionTypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionTypeRef {
    pub params: Vec<FunctionTypeParam>,
    pub return_type: Box<TypeRef>,
}

#[derive(Debug, Clone)]
pub struct FunctionTypeParam {
    pub mutable: bool,
    pub type_ref: TypeRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BasicType {
    String,
    Char,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
}

impl BasicType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "string" => Some(Self::String),
            "char" => Some(Self::Char),
            "bool" => Some(Self::Bool),
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            "u128" => Some(Self::U128),
            "usize" => Some(Self::Usize),
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "i128" => Some(Self::I128),
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Char => "char",
            Self::Bool => "bool",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Usize => "usize",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::U128
                | Self::Usize
                | Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::I128
                | Self::F32
                | Self::F64
        )
    }

    pub fn is_signed_numeric(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::F32 | Self::F64
        )
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn is_integer(self) -> bool {
        self.is_numeric() && !self.is_float()
    }
}

pub fn number_literal_is_float(value: &str) -> bool {
    value.contains(['.', 'e', 'E'])
}
