use super::super::bytecode::Instruction;
use super::{
    Compiler,
    context::{
        ActiveFinallyContext, ActiveHandlerContext, CompileContext, ControlTransferPatch,
        FinallyRegionContext,
    },
    pattern_bindings,
};
use crate::{diagnostic::MustardResult, ir::Stmt};

impl Compiler {
    pub(super) fn compile_try(
        &mut self,
        context: &mut CompileContext,
        body: &Stmt,
        catch: Option<&crate::ir::CatchClause>,
        finally: Option<&Stmt>,
    ) -> MustardResult<()> {
        let finally_region = finally.map(|_| {
            context
                .finally_regions
                .push(FinallyRegionContext::default());
            context.finally_regions.len() - 1
        });

        let try_handler_site = context.code.len();
        context.code.push(Instruction::PushHandler {
            catch: catch.map(|_| usize::MAX),
            finally: finally_region.map(|_| usize::MAX),
        });
        if let Some(region) = finally_region {
            context.finally_regions[region]
                .handler_sites
                .push(try_handler_site);
        }

        context.active_handlers.push(ActiveHandlerContext {
            finally_region,
            scope_depth: context.scope_depth,
        });
        self.compile_stmt(context, body)?;
        context.active_handlers.pop();
        context.code.push(Instruction::PopHandler);

        let mut skip_catch_jump = None;
        let mut after_finally_patches = Vec::new();
        let outer_handler_depth = context.active_handlers.len();

        if let Some(region) = finally_region {
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth: outer_handler_depth,
                target_scope_depth: context.scope_depth,
            });
            after_finally_patches.push(patch);
            self.emit_jump_to_finally(context, region);
        } else if catch.is_some() {
            skip_catch_jump = Some(self.emit_jump(context, Instruction::Jump(usize::MAX)));
        }

        if let Some(catch_clause) = catch {
            self.patch_handler_catch(context, try_handler_site, context.code.len());

            if let Some(region) = finally_region {
                let catch_handler_site = context.code.len();
                context.code.push(Instruction::PushHandler {
                    catch: None,
                    finally: Some(usize::MAX),
                });
                context.finally_regions[region]
                    .handler_sites
                    .push(catch_handler_site);
                context.active_handlers.push(ActiveHandlerContext {
                    finally_region: Some(region),
                    scope_depth: context.scope_depth,
                });
            }

            self.enter_env_scope(context);
            if let Some(parameter) = &catch_clause.parameter {
                for (name, mutable) in pattern_bindings(parameter) {
                    self.emit_declare_name(context, name, mutable);
                }
            }
            context.code.push(Instruction::BeginCatch);
            if let Some(parameter) = &catch_clause.parameter {
                self.compile_pattern_binding(context, parameter)?;
            } else {
                context.code.push(Instruction::Pop);
            }
            self.compile_stmt(context, catch_clause.body.as_ref())?;
            self.exit_env_scope(context);

            if let Some(region) = finally_region {
                context.active_handlers.pop();
                context.code.push(Instruction::PopHandler);
                let patch = context.code.len();
                context.code.push(Instruction::PushPendingJump {
                    target: usize::MAX,
                    target_handler_depth: outer_handler_depth,
                    target_scope_depth: context.scope_depth,
                });
                after_finally_patches.push(patch);
                self.emit_jump_to_finally(context, region);
            }
        }

        if let Some(finally_stmt) = finally {
            let finally_ip = context.code.len();
            self.patch_finally_region(
                context,
                finally_region.expect("finally region should exist"),
                finally_ip,
            );
            let enter_finally = context.code.len();
            context
                .code
                .push(Instruction::EnterFinally { exit: usize::MAX });
            context.active_finally.push(ActiveFinallyContext {
                exit_patch_site: enter_finally,
                jump_sites: Vec::new(),
                scope_depth: context.scope_depth,
            });
            self.compile_stmt(context, finally_stmt)?;
            let continue_ip = context.code.len();
            let active_finally = context
                .active_finally
                .pop()
                .expect("finally context should exist");
            self.patch_finally_exit(context, active_finally, continue_ip);
            context.code.push(Instruction::ContinuePending);
            let after_finally = context.code.len();
            for patch in after_finally_patches {
                self.patch_pending_jump(context, patch, after_finally);
            }
            if let Some(skip_catch_jump) = skip_catch_jump {
                self.patch_jump(context, skip_catch_jump, after_finally);
            }
        } else if let Some(skip_catch_jump) = skip_catch_jump {
            let after_catch = context.code.len();
            self.patch_jump(context, skip_catch_jump, after_catch);
        }

        Ok(())
    }

    pub(super) fn emit_return(&self, context: &mut CompileContext) {
        if let Some(active_finally) = context.active_finally.last() {
            self.emit_scope_cleanup(context, active_finally.scope_depth);
            context.code.push(Instruction::PushPendingReturn);
            self.emit_jump_to_active_finally_exit(context);
            return;
        }
        if let Some((handler_depth, region)) = self.nearest_finally_region(context, 0) {
            self.emit_scope_cleanup(context, context.active_handlers[handler_depth].scope_depth);
            self.emit_handler_cleanup(context, handler_depth);
            context.code.push(Instruction::PushPendingReturn);
            self.emit_jump_to_finally(context, region);
        } else {
            context.code.push(Instruction::Return);
        }
    }

    pub(super) fn emit_jump_transfer(
        &self,
        context: &mut CompileContext,
        target_handler_depth: usize,
        target_scope_depth: usize,
    ) -> ControlTransferPatch {
        if let Some(active_finally) = context.active_finally.last() {
            self.emit_scope_cleanup(context, active_finally.scope_depth);
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth,
                target_scope_depth,
            });
            self.emit_jump_to_active_finally_exit(context);
            return ControlTransferPatch::PendingJump(patch);
        }
        if let Some((handler_depth, region)) =
            self.nearest_finally_region(context, target_handler_depth)
        {
            self.emit_scope_cleanup(context, context.active_handlers[handler_depth].scope_depth);
            self.emit_handler_cleanup(context, handler_depth);
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth,
                target_scope_depth,
            });
            self.emit_jump_to_finally(context, region);
            ControlTransferPatch::PendingJump(patch)
        } else {
            self.emit_scope_cleanup(context, target_scope_depth);
            self.emit_handler_cleanup(context, target_handler_depth);
            ControlTransferPatch::DirectJump(self.emit_jump(context, Instruction::Jump(usize::MAX)))
        }
    }

    pub(super) fn emit_scope_cleanup(
        &self,
        context: &mut CompileContext,
        target_scope_depth: usize,
    ) {
        for _ in target_scope_depth..context.scope_depth {
            context.code.push(Instruction::PopEnv);
        }
    }

    pub(super) fn emit_handler_cleanup(
        &self,
        context: &mut CompileContext,
        target_handler_depth: usize,
    ) {
        for _ in target_handler_depth..context.active_handlers.len() {
            context.code.push(Instruction::PopHandler);
        }
    }

    pub(super) fn nearest_finally_region(
        &self,
        context: &CompileContext,
        target_handler_depth: usize,
    ) -> Option<(usize, usize)> {
        for handler_depth in (target_handler_depth..context.active_handlers.len()).rev() {
            if let Some(region) = context.active_handlers[handler_depth].finally_region {
                return Some((handler_depth, region));
            }
        }
        None
    }

    pub(super) fn emit_jump_to_finally(&self, context: &mut CompileContext, region: usize) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context.finally_regions[region].jump_sites.push(jump_site);
    }

    pub(super) fn emit_jump_to_active_finally_exit(&self, context: &mut CompileContext) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context
            .active_finally
            .last_mut()
            .expect("finally context should exist")
            .jump_sites
            .push(jump_site);
    }

    pub(super) fn patch_handler_catch(
        &self,
        context: &mut CompileContext,
        index: usize,
        target: usize,
    ) {
        if let Instruction::PushHandler { catch, .. } = &mut context.code[index] {
            *catch = Some(target);
        }
    }

    pub(super) fn patch_finally_region(
        &self,
        context: &mut CompileContext,
        region: usize,
        target: usize,
    ) {
        let handler_sites = context.finally_regions[region].handler_sites.clone();
        let jump_sites = context.finally_regions[region].jump_sites.clone();
        for site in handler_sites {
            if let Instruction::PushHandler { finally, .. } = &mut context.code[site] {
                *finally = Some(target);
            }
        }
        for site in jump_sites {
            self.patch_jump(context, site, target);
        }
    }

    pub(super) fn patch_finally_exit(
        &self,
        context: &mut CompileContext,
        finally: ActiveFinallyContext,
        target: usize,
    ) {
        if let Instruction::EnterFinally { exit } = &mut context.code[finally.exit_patch_site] {
            *exit = target;
        }
        for jump_site in finally.jump_sites {
            self.patch_jump(context, jump_site, target);
        }
    }

    pub(super) fn patch_pending_jump(
        &self,
        context: &mut CompileContext,
        index: usize,
        target: usize,
    ) {
        if let Instruction::PushPendingJump { target: jump, .. } = &mut context.code[index] {
            *jump = target;
        }
    }

    pub(super) fn patch_control_transfer(
        &self,
        context: &mut CompileContext,
        patch: ControlTransferPatch,
        target: usize,
    ) {
        match patch {
            ControlTransferPatch::DirectJump(index) => self.patch_jump(context, index, target),
            ControlTransferPatch::PendingJump(index) => {
                self.patch_pending_jump(context, index, target)
            }
        }
    }

    pub(super) fn emit_jump(
        &self,
        context: &mut CompileContext,
        instruction: Instruction,
    ) -> usize {
        let index = context.code.len();
        context.code.push(instruction);
        index
    }

    pub(super) fn patch_jump(&self, context: &mut CompileContext, index: usize, target: usize) {
        match &mut context.code[index] {
            Instruction::Jump(address)
            | Instruction::JumpIfFalse(address)
            | Instruction::JumpIfTrue(address)
            | Instruction::JumpIfNullish(address) => *address = target,
            _ => {}
        }
    }
}
