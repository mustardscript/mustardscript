use serde::{Deserialize, Serialize};

use crate::{
    ir::{BinaryOp, Pattern, PropertyName, UnaryOp, UpdateOp},
    span::SourceSpan,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeProgram {
    pub functions: Vec<FunctionPrototype>,
    pub root: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionPrototype {
    pub name: Option<String>,
    pub length: usize,
    pub display_source: String,
    pub params: Vec<Pattern>,
    #[serde(default)]
    pub param_binding_names: Vec<Vec<String>>,
    pub rest: Option<Pattern>,
    #[serde(default)]
    pub rest_binding_names: Vec<String>,
    pub code: Vec<Instruction>,
    pub is_async: bool,
    pub is_arrow: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    PushUndefined,
    PushNull,
    PushBool(bool),
    PushNumber(f64),
    PushString(String),
    PushRegExp {
        pattern: String,
        flags: String,
    },
    LoadSlot {
        depth: usize,
        slot: usize,
    },
    LoadSlotGetPropStatic {
        depth: usize,
        slot: usize,
        name: String,
        optional: bool,
    },
    LoadSlotDupGetPropStatic {
        depth: usize,
        slot: usize,
        name: String,
        optional: bool,
    },
    LoadName(String),
    LoadGlobal(String),
    LoadGlobalObject,
    StoreSlot {
        depth: usize,
        slot: usize,
    },
    StoreName(String),
    StoreGlobal(String),
    StoreSlotDiscard {
        depth: usize,
        slot: usize,
    },
    StoreNameDiscard(String),
    StoreGlobalDiscard(String),
    InitializePattern(Pattern),
    PushEnv,
    PopEnv,
    DeclareName {
        name: String,
        mutable: bool,
    },
    MakeClosure {
        function_id: usize,
    },
    MakeArray {
        count: usize,
    },
    ArrayPush,
    ArrayPushHole,
    ArrayExtend,
    MakeObject {
        keys: Vec<PropertyName>,
    },
    CopyDataProperties,
    CreateIterator,
    IteratorNext,
    GetPropStatic {
        name: String,
        optional: bool,
    },
    GetPropComputed {
        optional: bool,
    },
    SetPropStatic {
        name: String,
    },
    SetPropComputed,
    SetPropStaticDiscard {
        name: String,
    },
    SetPropComputedDiscard,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Update(UpdateOp),
    PatternArrayIndex(usize),
    PatternArrayRest(usize),
    PatternObjectRest(Vec<String>),
    Pop,
    Dup,
    DupGetPropStatic {
        name: String,
        optional: bool,
    },
    Dup2,
    PushHandler {
        catch: Option<usize>,
        finally: Option<usize>,
    },
    PopHandler,
    EnterFinally {
        exit: usize,
    },
    BeginCatch,
    Throw {
        span: SourceSpan,
    },
    PushPendingJump {
        target: usize,
        target_handler_depth: usize,
        target_scope_depth: usize,
    },
    PushPendingReturn,
    PushPendingThrow,
    ContinuePending,
    Jump(usize),
    JumpIfFalse(usize),
    JumpIfTrue(usize),
    JumpIfNullish(usize),
    Call {
        argc: usize,
        with_this: bool,
        optional: bool,
        span: SourceSpan,
    },
    MapSetCounter {
        span: SourceSpan,
    },
    CallWithArray {
        with_this: bool,
        optional: bool,
        span: SourceSpan,
    },
    Await,
    Construct {
        argc: usize,
    },
    ConstructWithArray,
    Return,
    PushBigInt(String),
}
