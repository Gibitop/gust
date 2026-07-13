struct Analyzer {
    diagnostics: Vec<Diagnostic>,
    values: HashSet<String>,
    types: HashSet<String>,
    structs: HashMap<String, StructDefinition>,
    enums: HashMap<String, EnumDefinition>,
    traits: HashMap<String, TraitDefinition>,
    functions: HashMap<String, FunctionSignature>,
    extensions: HashMap<String, FunctionSignature>,
    static_extensions: HashMap<String, FunctionSignature>,
    trait_methods: HashMap<String, FunctionSignature>,
    qualified_trait_methods: HashMap<String, FunctionSignature>,
    static_trait_methods: HashMap<String, FunctionSignature>,
    qualified_static_trait_methods: HashMap<String, FunctionSignature>,
    trait_impls: HashSet<(String, String)>,
    imported_namespaces: HashSet<String>,
    unsupported_features: HashSet<&'static str>,
    scopes: Vec<HashMap<String, Binding>>,
    return_types: Vec<Type>,
    self_types: Vec<Type>,
    direct_struct_methods: Vec<String>,
    loop_depth: usize,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<ParamSignature>,
    return_type: Type,
    mutable_self: bool,
}

#[derive(Debug, Clone)]
struct ParamSignature {
    type_: Type,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct StructDefinition {
    fields: HashMap<String, StructField>,
    methods: HashMap<String, FunctionSignature>,
    static_methods: HashMap<String, FunctionSignature>,
}

#[derive(Debug, Clone)]
struct StructField {
    type_: Type,
    internal: bool,
}

#[derive(Debug, Clone)]
struct EnumDefinition {
    variants: HashMap<String, Option<Type>>,
    methods: HashMap<String, FunctionSignature>,
    static_methods: HashMap<String, FunctionSignature>,
}

#[derive(Debug, Clone)]
struct TraitDefinition {
    methods: HashMap<String, FunctionSignature>,
    static_methods: HashMap<String, FunctionSignature>,
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    type_: Type,
    origin: BindingOrigin,
}

#[derive(Debug, Clone)]
enum BindingOrigin {
    Local,
    MatchPayload {
        enum_name: String,
        variant: String,
        mutable_available: bool,
    },
}

fn is_self_param(param: &crate::ast::Param) -> bool {
    param.name == "self"
}

fn has_mutable_receiver(function: &FunctionDecl) -> bool {
    function
        .params
        .iter()
        .any(|param| is_self_param(param) && param.mutable && param.type_ref.is_none())
}
