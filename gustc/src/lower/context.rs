
#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
    params: Vec<LoweredParamSignature>,
    return_type: LoweredType,
    return_type_known: bool,
    mutable_self: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredParamSignature {
    pub type_: LoweredType,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
struct LoweringLocal {
    type_: LoweredType,
    mutable: bool,
    replacement: Option<LoweredExpr>,
    captured: bool,
}

#[derive(Debug, Default)]
struct LoweringContext {
    diagnostics: Vec<Diagnostic>,
    closure_functions: Vec<LoweredClosureFunction>,
    next_closure_id: usize,
}

thread_local! {
    static CLOSURE_LOWERING: RefCell<LoweringContext> = RefCell::new(LoweringContext::default());
    static CAPTURED_NAMES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}
