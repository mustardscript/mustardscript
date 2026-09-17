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
        span: crate::span::SourceSpan,
    ) -> MustardResult<()> {
        let saved_completion = if context.completion_binding.is_some() && finally.is_some() {
            // An unconditional private scope avoids conditional declarations
            // changing the slot layout of the surrounding guest environment.
            self.enter_env_scope(context);
            let name = self.fresh_internal_name(context, "finally_result");
            self.emit_declare_name(context, name.clone(), true);
            context.code.push(Instruction::PushUndefined);
            context.code.push(Instruction::InitializePattern(
                crate::ir::Pattern::Identifier {
                    span,
                    name: name.clone(),
                },
            ));
            Some(name)
        } else {
            None
        };
        self.reset_statement_completion(context);
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

        context.active_handlers.push(ActiveHandlerContext {});
        self.compile_stmt(context, body)?;
        context.active_handlers.pop();
        context.code.push(Instruction::PopHandler);

        let mut skip_catch_jump = None;
        let mut after_finally_patches = Vec::new();
        let outer_handler_depth = context.active_handlers.len();

        if let Some(region) = finally_region {
            let patch = context.code.len();
            context.code.push(Instruction::PushCompletionJump {
                target: usize::MAX,
                target_finally_depth: context.active_finally.len(),
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
            self.reset_statement_completion(context);

            if let Some(region) = finally_region {
                let catch_handler_site = context.code.len();
                context.code.push(Instruction::PushHandler {
                    catch: None,
                    finally: Some(usize::MAX),
                });
                context.finally_regions[region]
                    .handler_sites
                    .push(catch_handler_site);
                context.active_handlers.push(ActiveHandlerContext {});
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
                self.record_pattern_collection_kind(context, parameter, None);
            } else {
                context.code.push(Instruction::Pop);
            }
            self.compile_stmt(context, catch_clause.body.as_ref())?;
            self.exit_env_scope(context);

            if let Some(region) = finally_region {
                context.active_handlers.pop();
                context.code.push(Instruction::PopHandler);
                let patch = context.code.len();
                context.code.push(Instruction::PushCompletionJump {
                    target: usize::MAX,
                    target_finally_depth: context.active_finally.len(),
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
            if let Some(saved) = &saved_completion {
                let result = context
                    .completion_binding
                    .clone()
                    .expect("script completion binding");
                self.emit_load_name(context, &result);
                self.emit_store_name_discard(context, saved);
                self.reset_statement_completion(context);
            }
            let enter_finally = context.code.len();
            context
                .code
                .push(Instruction::EnterFinally { exit: usize::MAX });
            context.active_finally.push(ActiveFinallyContext {
                exit_patch_site: enter_finally,
            });
            self.compile_stmt(context, finally_stmt)?;
            // Only normal cleanup restores the try/catch value. Abrupt cleanup
            // skips this and supplies its own completion to the existing unwinder.
            if let Some(saved) = &saved_completion {
                let result = context
                    .completion_binding
                    .clone()
                    .expect("script completion binding");
                self.emit_load_name(context, saved);
                self.emit_store_name_discard(context, &result);
            }
            let continue_ip = context.code.len();
            let active_finally = context
                .active_finally
                .pop()
                .expect("finally context should exist");
            self.patch_finally_exit(context, active_finally, continue_ip);
            context.code.push(Instruction::ContinuePendingRegion {
                handler_depth: context.active_handlers.len(),
                scope_depth: context.scope_depth,
            });
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

        if saved_completion.is_some() {
            self.exit_env_scope(context);
        }

        Ok(())
    }

    pub(super) fn emit_return(&self, context: &mut CompileContext) {
        context.code.push(
            if context.active_handlers.is_empty() && context.active_finally.is_empty() {
                Instruction::Return
            } else {
                Instruction::AbruptReturn
            },
        );
    }

    pub(super) fn emit_jump_transfer(
        &self,
        context: &mut CompileContext,
        target_handler_depth: usize,
        target_scope_depth: usize,
        target_finally_depth: usize,
    ) -> ControlTransferPatch {
        let patch = context.code.len();
        context.code.push(Instruction::AbruptJump {
            target: usize::MAX,
            target_handler_depth,
            target_scope_depth,
            target_finally_depth,
        });
        ControlTransferPatch::AbruptJump(patch)
    }

    pub(super) fn emit_jump_to_finally(&self, context: &mut CompileContext, region: usize) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context.finally_regions[region].jump_sites.push(jump_site);
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
    }

    pub(super) fn patch_pending_jump(
        &self,
        context: &mut CompileContext,
        index: usize,
        target: usize,
    ) {
        if let Instruction::PushCompletionJump { target: jump, .. } = &mut context.code[index] {
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
            ControlTransferPatch::AbruptJump(index) => {
                if let Instruction::AbruptJump { target: jump, .. } = &mut context.code[index] {
                    *jump = target;
                }
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
