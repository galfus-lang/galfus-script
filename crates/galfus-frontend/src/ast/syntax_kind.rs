#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxNodeKind {
    // Root
    SourceFile,

    // Items
    ImportItem,
    ExportItem,
    FunctionItem,
    TypeAliasItem,
    StructItem,
    EnumItem,
    ChoiceItem,
    ConstraintItem,
    VarItem,
    ConstItem,

    // Imports
    NamespaceImport,
    NamedImportList,
    NamedImport,
    ImportAlias,
    ImportSource,

    // Paths / names
    Identifier,
    Path,

    // Declarations
    FunctionStamp,
    FunctionAnchor,
    StructFieldList,
    StructField,
    StructFieldConst,
    StructExpansion,
    WeakStructField,
    StructFieldDefault,
    EnumVariantList,
    EnumVariant,
    ChoiceVariantList,
    ChoiceVariant,
    ChoicePayload,
    ChoicePayloadItem,

    // Parameters
    ParameterList,
    Parameter,
    RestParameter,
    ParameterDefault,

    // Generics / constraints / decorators
    GenericParameterList,
    GenericParameter,
    GenericParameterConstraint,
    BasicConstraint,
    ConstraintMemberList,
    ConstraintField,
    ConstraintFunctionSignature,
    SatisfiesClause,
    Decorator,
    DecoratorList,

    // Keyword metadata
    KeywordMetadataList,
    KeywordMetadataFlag,
    KeywordMetadataPair,
    KeywordMetadataType,

    // Statements
    Block,
    ReturnStatement,
    BreakStatement,
    ContinueStatement,
    VarStatement,
    ConstStatement,
    ExpressionStatement,
    IfStatement,
    ElseClause,
    ForStatement,
    ForBinding,
    LoopStatement,
    AssignmentStatement,

    // Patterns
    BindingPattern,
    TypePattern,
    TypePatternBinding,
    VariantPattern,
    VariantPatternPayload,
    LiteralPattern,
    StructBindingPattern,
    StructBindingField,
    TupleBindingPattern,
    ArrayBindingPattern,
    RestBindingPattern,
    WildcardPattern,

    // Nominal Struct Patterns
    StructPattern,
    StructPatternField,

    // Binding helpers
    TypeAnnotation,
    Initializer,

    // Types
    TypeNull,
    NamedType,
    GenericType,
    TypeArgumentList,
    ArrayType,
    UnionType,
    FunctionType,
    FunctionTypeParameterList,
    GroupedType,
    TupleType,

    // Arguments
    Argument,
    OmittedArgument,
    ArgumentList,

    // Expressions
    CallExpression,
    PathExpression,
    NameExpression,
    MemberExpression,
    IndexExpression,
    GroupedExpression,
    TupleExpression,
    CopyExpression,
    CastExpression,
    UnaryExpression,
    BinaryExpression,
    ArrowFunctionExpression,
    GenericExpression,
    GenericArgumentList,
    EnumDiscriminant,
    RangeExpression,
    RangeStep,
    NullSafeMemberExpression,
    MatchExpression,
    MatchArmList,
    MatchArm,
    InstanceofExpression,
    InstanceofArmList,
    InstanceofArm,
    TypeofExpression,
    TypeofArmList,
    TypeofArm,
    NewArrayExpression,

    // Operators
    UnaryOperator,
    BinaryOperator,
    AssignmentOperator,
    RangeOperator,

    // Literals
    ArrayLiteral,
    ArrayElement,
    SpreadArrayElement,
    StructLiteral,
    StructLiteralFieldList,
    StructLiteralField,
    StructLiteralFieldShorthand,
    SpreadStructLiteralField,
    InferredStructLiteral,
    IntegerLiteral,
    FloatLiteral,
    BoolLiteral,
    NullLiteral,
    StringLiteral,

    // Wildcard Expression
    WildcardExpression,
}

impl SyntaxNodeKind {
    pub fn is_item(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::ImportItem
                | SyntaxNodeKind::ExportItem
                | SyntaxNodeKind::FunctionItem
                | SyntaxNodeKind::TypeAliasItem
                | SyntaxNodeKind::StructItem
                | SyntaxNodeKind::EnumItem
                | SyntaxNodeKind::ChoiceItem
                | SyntaxNodeKind::ConstraintItem
                | SyntaxNodeKind::VarItem
                | SyntaxNodeKind::ConstItem
        )
    }

    pub fn is_statement(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::ReturnStatement
                | SyntaxNodeKind::BreakStatement
                | SyntaxNodeKind::ContinueStatement
                | SyntaxNodeKind::VarStatement
                | SyntaxNodeKind::ConstStatement
                | SyntaxNodeKind::ExpressionStatement
                | SyntaxNodeKind::IfStatement
                | SyntaxNodeKind::ForStatement
                | SyntaxNodeKind::LoopStatement
                | SyntaxNodeKind::AssignmentStatement
        )
    }

    pub fn is_expression(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::CallExpression
                | SyntaxNodeKind::PathExpression
                | SyntaxNodeKind::NameExpression
                | SyntaxNodeKind::MemberExpression
                | SyntaxNodeKind::IndexExpression
                | SyntaxNodeKind::GroupedExpression
                | SyntaxNodeKind::TupleExpression
                | SyntaxNodeKind::CopyExpression
                | SyntaxNodeKind::CastExpression
                | SyntaxNodeKind::UnaryExpression
                | SyntaxNodeKind::BinaryExpression
                | SyntaxNodeKind::ArrowFunctionExpression
                | SyntaxNodeKind::GenericExpression
                | SyntaxNodeKind::ArrayLiteral
                | SyntaxNodeKind::StructLiteral
                | SyntaxNodeKind::InferredStructLiteral
                | SyntaxNodeKind::IntegerLiteral
                | SyntaxNodeKind::FloatLiteral
                | SyntaxNodeKind::BoolLiteral
                | SyntaxNodeKind::NullLiteral
                | SyntaxNodeKind::StringLiteral
                | SyntaxNodeKind::RangeExpression
                | SyntaxNodeKind::NullSafeMemberExpression
                | SyntaxNodeKind::MatchExpression
                | SyntaxNodeKind::InstanceofExpression
                | SyntaxNodeKind::NewArrayExpression
                | SyntaxNodeKind::WildcardExpression
        )
    }

    pub fn is_type(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::TypeNull
                | SyntaxNodeKind::NamedType
                | SyntaxNodeKind::Path
                | SyntaxNodeKind::GenericType
                | SyntaxNodeKind::ArrayType
                | SyntaxNodeKind::UnionType
                | SyntaxNodeKind::FunctionType
                | SyntaxNodeKind::GroupedType
                | SyntaxNodeKind::TupleType
        )
    }

    pub fn is_literal(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::IntegerLiteral
                | SyntaxNodeKind::FloatLiteral
                | SyntaxNodeKind::BoolLiteral
                | SyntaxNodeKind::NullLiteral
                | SyntaxNodeKind::StringLiteral
        )
    }

    pub fn is_operator(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::UnaryOperator
                | SyntaxNodeKind::BinaryOperator
                | SyntaxNodeKind::AssignmentOperator
                | SyntaxNodeKind::RangeOperator
        )
    }

    pub fn is_list(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::NamedImportList
                | SyntaxNodeKind::StructFieldList
                | SyntaxNodeKind::EnumVariantList
                | SyntaxNodeKind::ChoiceVariantList
                | SyntaxNodeKind::ParameterList
                | SyntaxNodeKind::GenericParameterList
                | SyntaxNodeKind::ConstraintMemberList
                | SyntaxNodeKind::MatchArmList
                | SyntaxNodeKind::InstanceofArmList
                | SyntaxNodeKind::TypeArgumentList
                | SyntaxNodeKind::FunctionTypeParameterList
                | SyntaxNodeKind::ArgumentList
                | SyntaxNodeKind::GenericArgumentList
                | SyntaxNodeKind::StructLiteralFieldList
                | SyntaxNodeKind::DecoratorList
                | SyntaxNodeKind::KeywordMetadataList
        )
    }

    pub fn is_pattern(self) -> bool {
        matches!(
            self,
            SyntaxNodeKind::BindingPattern
                | SyntaxNodeKind::WildcardPattern
                | SyntaxNodeKind::StructBindingPattern
                | SyntaxNodeKind::StructBindingField
                | SyntaxNodeKind::TupleBindingPattern
                | SyntaxNodeKind::ArrayBindingPattern
                | SyntaxNodeKind::RestBindingPattern
                | SyntaxNodeKind::TypePattern
                | SyntaxNodeKind::TypePatternBinding
                | SyntaxNodeKind::VariantPattern
                | SyntaxNodeKind::VariantPatternPayload
                | SyntaxNodeKind::LiteralPattern
                | SyntaxNodeKind::StructPattern
                | SyntaxNodeKind::StructPatternField
        )
    }
}
