struct Monomorphizer {
    struct_templates: HashMap<String, StructDecl>,
    enum_templates: HashMap<String, EnumDecl>,
    trait_declarations: HashMap<String, TraitDecl>,
    trait_templates: HashMap<String, TraitDecl>,
    impl_declarations: Vec<ImplDecl>,
    impl_templates: Vec<ImplDecl>,
    extensions: Vec<crate::ast::ExtensionDecl>,
    trait_default_extension_start: usize,
    function_templates: HashMap<String, FunctionDecl>,
    concrete_structs: HashSet<String>,
    concrete_struct_defs: HashMap<String, StructDecl>,
    concrete_enums: HashMap<String, EnumDecl>,
    concrete_traits: HashSet<String>,
    pending: VecDeque<PendingSpecialization>,
    emitted: HashSet<String>,
    specializations: HashMap<String, (String, Vec<TypeRef>)>,
    trait_specializations: HashMap<
        String,
        (
            String,
            Vec<TypeRef>,
            Vec<crate::ast::AssociatedTypeBinding>,
        ),
    >,
    scopes: Vec<HashMap<String, TypeRef>>,
    return_types: Vec<TypeRef>,
    self_types: Vec<TypeRef>,
    inferred_returns: Vec<Option<Vec<TypeRef>>>,
    member_returns: HashMap<(String, String, bool), TypeRef>,
    function_returns: HashMap<String, TypeRef>,
    function_params: HashMap<String, Vec<Option<TypeRef>>>,
    generic_function_returns: HashMap<String, TypeRef>,
    generic_method_returns: HashMap<(String, String, bool), TypeRef>,
    trait_method_returns: HashMap<(String, String), TypeRef>,
    expected_expr_types: HashMap<crate::span::Span, TypeRef>,
    inferred_expr_types: HashMap<crate::span::Span, TypeRef>,
    impl_receiver_types: Vec<TypeRef>,
    bound_checks: Vec<BoundCheck>,
    diagnostics: Vec<Diagnostic>,
}

struct BoundCheck {
    owner: String,
    type_ref: TypeRef,
    trait_ref: TypeRef,
    span: crate::span::Span,
}

struct GenericTraitMethodResolution {
    trait_name: String,
    trait_args: Vec<TypeRef>,
    associated_type_bindings: Vec<crate::ast::AssociatedTypeBinding>,
    params: Vec<TypeRef>,
    return_type: TypeRef,
    impl_type_params: Vec<String>,
    impl_type_param_bounds: Vec<TypeParamBound>,
    impl_type_args: Vec<TypeRef>,
}

struct ExtensionResolution {
    template_index: usize,
    receiver: TypeRef,
    receiver_type_params: Vec<String>,
    receiver_type_args: Vec<TypeRef>,
    function_type_args: Vec<TypeRef>,
    params: Vec<TypeRef>,
    return_type: Option<TypeRef>,
}

enum PendingSpecialization {
    Struct(String, Vec<TypeRef>),
    Enum(String, Vec<TypeRef>),
    Trait(String, Vec<TypeRef>, Vec<crate::ast::AssociatedTypeBinding>),
    Function(String, Vec<TypeRef>),
    Impl {
        trait_ref: TypeRef,
        type_ref: TypeRef,
    },
    Method {
        receiver: String,
        name: String,
        static_: bool,
        args: Vec<TypeRef>,
    },
    Extension {
        template_index: usize,
        receiver: TypeRef,
        function_args: Vec<TypeRef>,
    },
}
