
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProgram {
    pub structs: Vec<LoweredStruct>,
    pub enums: Vec<LoweredEnum>,
    pub traits: Vec<LoweredTrait>,
    pub statics: Vec<LoweredStaticVar>,
    pub functions: Vec<LoweredFunction>,
    pub closure_functions: Vec<LoweredClosureFunction>,
    pub statements: Vec<LoweredStatement>,
    pub main_location: LoweredSourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredSourceLocation {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStaticVar {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStruct {
    pub name: String,
    pub fields: Vec<LoweredField>,
    pub raw_buffer_element: Option<LoweredType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredField {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredEnum {
    pub name: String,
    pub variants: Vec<LoweredVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredVariant {
    pub name: String,
    pub payload: Option<LoweredType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTrait {
    pub name: String,
    pub methods: Vec<LoweredTraitMethod>,
    pub impls: Vec<LoweredTraitImpl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTraitMethod {
    pub name: String,
    pub params: Vec<LoweredParamSignature>,
    pub return_type: LoweredType,
    pub mutable_self: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTraitImpl {
    pub self_type: LoweredType,
    pub methods: Vec<LoweredTraitImplMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredTraitImplMethod {
    pub name: String,
    pub function_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredFunction {
    pub name: String,
    pub location: LoweredSourceLocation,
    pub params: Vec<LoweredParam>,
    pub return_type: LoweredType,
    pub statements: Vec<LoweredStatement>,
    pub return_value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredParam {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredClosureFunction {
    pub name: String,
    pub captures: Vec<LoweredClosureCapture>,
    pub params: Vec<LoweredParam>,
    pub return_type: LoweredType,
    pub statements: Vec<LoweredStatement>,
    pub return_value: LoweredExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredClosureCapture {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredStatement {
    Local {
        name: String,
        value: LoweredExpr,
    },
    LocalCell {
        name: String,
        value: LoweredExpr,
    },
    Assignment {
        target: LoweredExpr,
        value: LoweredExpr,
    },
    Println(LoweredExpr),
    Panic {
        message: LoweredExpr,
        location: LoweredSourceLocation,
    },
    Expr(LoweredExpr),
    Return(Option<LoweredExpr>),
    If {
        condition: LoweredExpr,
        then_branch: Vec<LoweredStatement>,
        else_branch: Option<Vec<LoweredStatement>>,
    },
    While {
        condition: LoweredExpr,
        body: Vec<LoweredStatement>,
    },
    Break,
    Continue,
    Match {
        value: LoweredExpr,
        temp_name: String,
        decision: Box<LoweredMatchDecision>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredExpr {
    pub type_: LoweredType,
    pub kind: LoweredExprKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredType {
    Basic(BasicType),
    Struct(String),
    Enum(String),
    Trait(String),
    Function {
        params: Vec<LoweredFunctionTypeParam>,
        return_type: Box<LoweredType>,
    },
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredFunctionTypeParam {
    pub type_: LoweredType,
    pub mutable: bool,
}

impl LoweredType {
    pub fn name(&self) -> String {
        match self {
            LoweredType::Basic(type_) => type_.name().to_string(),
            LoweredType::Struct(name) => name.clone(),
            LoweredType::Enum(name) => name.clone(),
            LoweredType::Trait(name) => name.clone(),
            LoweredType::Function {
                params,
                return_type,
            } => {
                let params = params
                    .iter()
                    .map(|param| {
                        if param.mutable {
                            format!("mut {}", param.type_.name())
                        } else {
                            param.type_.name()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fn({params}): {}", return_type.name())
            }
            LoweredType::Void => "void".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredExprKind {
    Void,
    StringLiteral(String),
    BoolLiteral(bool),
    NumberLiteral(String),
    Local(String),
    LocalCell(String),
    CapturedLocal {
        env_name: String,
        name: String,
    },
    PostfixIncrement(Box<LoweredExpr>),
    StringConcat(Box<LoweredExpr>, Box<LoweredExpr>),
    Not(Box<LoweredExpr>),
    Negate(Box<LoweredExpr>),
    Cast {
        value: Box<LoweredExpr>,
        type_: LoweredType,
    },
    Arithmetic {
        left: Box<LoweredExpr>,
        op: BinaryOp,
        right: Box<LoweredExpr>,
    },
    Logical {
        left: Box<LoweredExpr>,
        op: BinaryOp,
        right: Box<LoweredExpr>,
    },
    Comparison {
        left: Box<LoweredExpr>,
        op: BinaryOp,
        right: Box<LoweredExpr>,
    },
    StructLiteral {
        name: String,
        fields: Vec<LoweredStructFieldValue>,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
        payload: Option<Box<LoweredExpr>>,
    },
    Match {
        value: Box<LoweredExpr>,
        temp_name: String,
        decision: Box<LoweredMatchDecision>,
    },
    FieldAccess {
        object: Box<LoweredExpr>,
        field: String,
    },
    Clone(Box<LoweredExpr>),
    NumberToString(Box<LoweredExpr>),
    Call {
        name: String,
        args: Vec<LoweredExpr>,
        location: LoweredSourceLocation,
    },
    CollectionLiteral {
        constructor: String,
        add: String,
        items: Vec<LoweredExpr>,
        location: LoweredSourceLocation,
    },
    TraitObject {
        trait_name: String,
        self_type: LoweredType,
        value: Box<LoweredExpr>,
    },
    DynamicCall {
        object: Box<LoweredExpr>,
        method: String,
        args: Vec<LoweredExpr>,
        location: LoweredSourceLocation,
    },
    Closure {
        name: String,
        captures: Vec<LoweredClosureCapture>,
    },
    IndirectCall {
        callee: Box<LoweredExpr>,
        args: Vec<LoweredExpr>,
        location: LoweredSourceLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredMatchDecision {
    Arms {
        arms: Vec<LoweredMatchDecision>,
    },
    Test {
        subject: String,
        test: LoweredMatchTest,
        then: Box<LoweredMatchDecision>,
        else_: Box<LoweredMatchDecision>,
    },
    Bind {
        name: String,
        type_: LoweredType,
        source: LoweredMatchBindSource,
        declare: bool,
        then: Box<LoweredMatchDecision>,
    },
    Or {
        bindings: Vec<LoweredMatchOrBinding>,
        alternatives: Vec<LoweredMatchDecision>,
        then: Box<LoweredMatchDecision>,
        else_: Box<LoweredMatchDecision>,
    },
    Matched,
    Body {
        statements: Vec<LoweredStatement>,
        value: Option<LoweredExpr>,
    },
    Fail,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredMatchTest {
    EnumTag {
        enum_name: String,
        variant: String,
    },
    StringEq(String),
    BoolEq(bool),
    NumberEq {
        value: String,
        type_: BasicType,
    },
    Range {
        start: String,
        end: String,
        inclusive: bool,
        type_: BasicType,
    },
    Guard(Box<LoweredExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredMatchBindSource {
    EnumPayload {
        subject: String,
        variant: String,
    },
    StructField {
        subject: String,
        field: String,
    },
    Subject(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredMatchOrBinding {
    pub name: String,
    pub type_: LoweredType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweredPattern {
    Or(Vec<LoweredPattern>),
    Variant {
        enum_name: String,
        variant: String,
        payload: Option<Box<LoweredPattern>>,
    },
    Struct {
        name: String,
        fields: Vec<LoweredStructPatternField>,
    },
    Binding {
        name: String,
    },
    String(String),
    Bool(bool),
    Number {
        value: String,
        type_: BasicType,
    },
    Range {
        start: String,
        end: String,
        inclusive: bool,
        type_: BasicType,
    },
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStructPatternField {
    pub name: String,
    pub pattern: LoweredPattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredStructFieldValue {
    pub name: String,
    pub value: LoweredExpr,
}
