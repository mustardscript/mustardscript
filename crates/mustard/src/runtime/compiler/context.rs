use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnownCollectionKind {
    Map,
    Set,
}

#[derive(Debug, Default, Clone)]
pub(super) struct BindingScope {
    pub(super) bindings: Vec<String>,
    pub(super) immutable_bindings: HashSet<String>,
    pub(super) known_collections: HashMap<String, KnownCollectionKind>,
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

    pub(super) fn declare_binding(&mut self, name: String, mutable: bool) {
        let scope = self
            .binding_scopes
            .last_mut()
            .expect("binding scope should exist before declarations");
        if !mutable {
            scope.immutable_bindings.insert(name.clone());
        }
        scope.bindings.push(name);
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

    pub(super) fn known_collection_kind(&self, name: &str) -> Option<KnownCollectionKind> {
        self.binding_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.known_collections.get(name).copied())
    }

    pub(super) fn record_known_collection(&mut self, name: &str, kind: KnownCollectionKind) {
        let Some(scope) = self
            .binding_scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.bindings.iter().any(|binding| binding == name))
        else {
            return;
        };
        if scope.immutable_bindings.contains(name) {
            scope.known_collections.insert(name.to_string(), kind);
        }
    }

    pub(super) fn clear_known_collection(&mut self, name: &str) {
        if let Some(scope) = self
            .binding_scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.bindings.iter().any(|binding| binding == name))
        {
            scope.known_collections.remove(name);
        }
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
