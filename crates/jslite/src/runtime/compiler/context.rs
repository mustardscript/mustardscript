use super::super::bytecode::Instruction;

#[derive(Debug, Default)]
pub(super) struct CompileContext {
    pub(super) code: Vec<Instruction>,
    pub(super) loop_stack: Vec<LoopContext>,
    pub(super) active_handlers: Vec<ActiveHandlerContext>,
    pub(super) active_finally: Vec<ActiveFinallyContext>,
    pub(super) finally_regions: Vec<FinallyRegionContext>,
    pub(super) scope_depth: usize,
    pub(super) internal_name_counter: usize,
}

#[derive(Debug, Default)]
pub(super) struct LoopContext {
    pub(super) break_jumps: Vec<ControlTransferPatch>,
    pub(super) continue_jumps: Vec<ControlTransferPatch>,
    pub(super) continue_target: Option<usize>,
    pub(super) handler_depth: usize,
    pub(super) scope_depth: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ActiveHandlerContext {
    pub(super) finally_region: Option<usize>,
    pub(super) scope_depth: usize,
}

#[derive(Debug, Default)]
pub(super) struct FinallyRegionContext {
    pub(super) handler_sites: Vec<usize>,
    pub(super) jump_sites: Vec<usize>,
}

#[derive(Debug, Default)]
pub(super) struct ActiveFinallyContext {
    pub(super) exit_patch_site: usize,
    pub(super) jump_sites: Vec<usize>,
    pub(super) scope_depth: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ControlTransferPatch {
    DirectJump(usize),
    PendingJump(usize),
}
