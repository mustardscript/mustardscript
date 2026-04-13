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
    pub(super) binding_scopes: Vec<BindingScope>,
}

#[derive(Debug, Default, Clone)]
pub(super) struct BindingScope {
    pub(super) bindings: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedBinding {
    pub(super) depth: usize,
    pub(super) slot: usize,
}

impl CompileContext {
    pub(super) fn with_inherited_bindings(binding_scopes: &[BindingScope]) -> Self {
        Self {
            binding_scopes: binding_scopes.to_vec(),
            ..Self::default()
        }
    }

    pub(super) fn push_binding_scope(&mut self) {
        self.binding_scopes.push(BindingScope::default());
    }

    pub(super) fn pop_binding_scope(&mut self) {
        self.binding_scopes
            .pop()
            .expect("binding scope should exist before scope exit");
    }

    pub(super) fn declare_binding(&mut self, name: String) {
        self.binding_scopes
            .last_mut()
            .expect("binding scope should exist before declarations")
            .bindings
            .push(name);
    }

    pub(super) fn resolve_binding(&self, name: &str) -> Option<ResolvedBinding> {
        self.binding_scopes
            .iter()
            .rev()
            .enumerate()
            .find_map(|(depth, scope)| {
                scope
                    .bindings
                    .iter()
                    .position(|binding| binding == name)
                    .map(|slot| ResolvedBinding { depth, slot })
            })
    }
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
