use serde::{Deserialize, Serialize};

use crate::span::SourceSpan;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    pub source: String,
    pub script: Script,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub span: SourceSpan,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    Block {
        span: SourceSpan,
        body: Vec<Stmt>,
    },
    VariableDecl {
        span: SourceSpan,
        kind: BindingKind,
        declarators: Vec<Declarator>,
    },
    FunctionDecl {
        span: SourceSpan,
        function: FunctionExpr,
    },
    Expression {
        span: SourceSpan,
        expression: Expr,
    },
    If {
        span: SourceSpan,
        test: Expr,
        consequent: Box<Stmt>,
        alternate: Option<Box<Stmt>>,
    },
    While {
        span: SourceSpan,
        test: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        span: SourceSpan,
        body: Box<Stmt>,
        test: Expr,
    },
    For {
        span: SourceSpan,
        init: Option<ForInit>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    ForOf {
        span: SourceSpan,
        await_each: bool,
        head: ForOfHead,
        iterable: Expr,
        body: Box<Stmt>,
    },
    ForIn {
        span: SourceSpan,
        head: ForOfHead,
        object: Expr,
        body: Box<Stmt>,
    },
    Break {
        span: SourceSpan,
    },
    Continue {
        span: SourceSpan,
    },
    Return {
        span: SourceSpan,
        value: Option<Expr>,
    },
    Throw {
        span: SourceSpan,
        value: Expr,
    },
    Try {
        span: SourceSpan,
        body: Box<Stmt>,
        catch: Option<CatchClause>,
        finally: Option<Box<Stmt>>,
    },
    Switch {
        span: SourceSpan,
        discriminant: Expr,
        cases: Vec<SwitchCase>,
    },
    Empty {
        span: SourceSpan,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForInit {
    VariableDecl {
        kind: BindingKind,
        declarators: Vec<Declarator>,
    },
    Expression(Expr),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForOfHead {
    Binding { kind: BindingKind, pattern: Pattern },
    Assignment { target: AssignTarget },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Declarator {
    pub span: SourceSpan,
    pub pattern: Pattern,
    pub initializer: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingKind {
    Let,
    Const,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatchClause {
    pub span: SourceSpan,
    pub parameter: Option<Pattern>,
    pub body: Box<Stmt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCase {
    pub span: SourceSpan,
    pub test: Option<Expr>,
    pub consequent: Vec<Stmt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    Identifier {
        span: SourceSpan,
        name: String,
    },
    Object {
        span: SourceSpan,
        properties: Vec<ObjectPatternProperty>,
        rest: Option<Box<Pattern>>,
    },
    Array {
        span: SourceSpan,
        elements: Vec<Option<Pattern>>,
        rest: Option<Box<Pattern>>,
    },
    Default {
        span: SourceSpan,
        target: Box<Pattern>,
        default_value: Expr,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectPatternProperty {
    pub span: SourceSpan,
    pub key: PropertyName,
    pub value: Pattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyName {
    Identifier(String),
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionExpr {
    pub span: SourceSpan,
    pub name: Option<String>,
    pub length: usize,
    pub display_source: String,
    pub params: Vec<Pattern>,
    pub rest: Option<Pattern>,
    pub param_init: Vec<Stmt>,
    pub body: Vec<Stmt>,
    pub is_async: bool,
    pub is_arrow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Undefined {
        span: SourceSpan,
    },
    Null {
        span: SourceSpan,
    },
    Bool {
        span: SourceSpan,
        value: bool,
    },
    Number {
        span: SourceSpan,
        value: f64,
    },
    BigInt {
        span: SourceSpan,
        value: String,
    },
    String {
        span: SourceSpan,
        value: String,
    },
    RegExp {
        span: SourceSpan,
        pattern: String,
        flags: String,
    },
    Identifier {
        span: SourceSpan,
        name: String,
    },
    This {
        span: SourceSpan,
    },
    Array {
        span: SourceSpan,
        elements: Vec<ArrayElement>,
    },
    Object {
        span: SourceSpan,
        properties: Vec<ObjectProperty>,
    },
    Function(Box<FunctionExpr>),
    Unary {
        span: SourceSpan,
        operator: UnaryOp,
        argument: Box<Expr>,
    },
    Binary {
        span: SourceSpan,
        operator: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Sequence {
        span: SourceSpan,
        expressions: Vec<Expr>,
    },
    Logical {
        span: SourceSpan,
        operator: LogicalOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Conditional {
        span: SourceSpan,
        test: Box<Expr>,
        consequent: Box<Expr>,
        alternate: Box<Expr>,
    },
    Assignment {
        span: SourceSpan,
        target: Box<AssignTarget>,
        operator: AssignOp,
        value: Box<Expr>,
    },
    Update {
        span: SourceSpan,
        target: Box<AssignTarget>,
        operator: UpdateOp,
        prefix: bool,
    },
    Member {
        span: SourceSpan,
        object: Box<Expr>,
        property: MemberProperty,
        optional: bool,
    },
    Call {
        span: SourceSpan,
        callee: Box<Expr>,
        arguments: Vec<CallArgument>,
        optional: bool,
    },
    New {
        span: SourceSpan,
        callee: Box<Expr>,
        arguments: Vec<CallArgument>,
    },
    Template {
        span: SourceSpan,
        quasis: Vec<String>,
        expressions: Vec<Expr>,
    },
    Await {
        span: SourceSpan,
        value: Box<Expr>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArrayElement {
    Value(Expr),
    Hole { span: SourceSpan },
    Spread { span: SourceSpan, value: Expr },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectProperty {
    Property {
        span: SourceSpan,
        key: ObjectPropertyKey,
        value: Expr,
    },
    Spread {
        span: SourceSpan,
        value: Expr,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectPropertyKey {
    Static(PropertyName),
    Computed(Box<Expr>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberProperty {
    Static(PropertyName),
    Computed(Box<Expr>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallArgument {
    Value(Expr),
    Spread { span: SourceSpan, value: Expr },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssignTarget {
    Identifier {
        span: SourceSpan,
        name: String,
    },
    Member {
        span: SourceSpan,
        object: Box<Expr>,
        property: MemberProperty,
        optional: bool,
    },
    Array {
        span: SourceSpan,
        elements: Vec<Option<AssignTarget>>,
        rest: Option<Box<AssignTarget>>,
    },
    Object {
        span: SourceSpan,
        properties: Vec<AssignTargetProperty>,
        rest: Option<Box<AssignTarget>>,
    },
    Default {
        span: SourceSpan,
        target: Box<AssignTarget>,
        default_value: Box<Expr>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignTargetProperty {
    pub span: SourceSpan,
    pub key: PropertyName,
    pub value: AssignTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateOp {
    Increment,
    Decrement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
    Typeof,
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    In,
    Instanceof,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    LessThan,
    LessThanEq,
    GreaterThan,
    GreaterThanEq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicalOp {
    And,
    Or,
    NullishCoalesce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    PowAssign,
    OrAssign,
    AndAssign,
    NullishAssign,
}
