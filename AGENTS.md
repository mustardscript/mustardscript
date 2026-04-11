# Repository Instructions

Read `IMPLEMENT_PROMPT.md` before doing roadmap work in this repository. That file is the authoritative execution brief for completing `TODOS.md`. This `AGENTS.md` is a durable summary of the rules that must remain true across future runs.

## Core Objective

- Fully implement `TODOS.md`.
- Update `TODOS.md` only when an item is verifiably complete.
- Treat the work as execution, not planning.
- Keep the repository buildable and the verification commands passing as you go.

## Required Reading

Before making changes, read:

- `IMPLEMENT_PROMPT.md`
- `TODOS.md`
- `README.md`
- `Cargo.toml`
- `package.json`
- the existing implementation in the affected crates, bindings, docs, and tests

## Non-Negotiable Rules

- Honor locked decisions and non-negotiable rules in `TODOS.md`.
- Do not assume an unchecked item is unfinished. Audit first.
- Do not mark a checkbox done without implementation, tests, docs when behavior changed, and explicit failure behavior.
- Keep guest/runtime semantics in Rust. Keep the Node wrapper thin.
- Fail closed on unsupported features.
- Never revert unrelated user changes.

## Execution Workflow

1. Audit the current repo against `TODOS.md`.
2. Mark already-satisfied items done only after verifying code, tests, and docs.
3. Identify the earliest incomplete dependency chain and implement it end to end.
4. For each work chunk:
   - implement the functionality
   - update or add tests
   - update docs and ADRs when needed
   - run relevant verification
   - update `TODOS.md` only for items now verifiably complete
5. After each substantial, verified milestone, create a focused git commit.
6. Continue until every feasible item is complete.

## Stop Criteria

- Large remaining work is not a blocker.
- Do not stop merely because the remaining work is subsystem-scale.
- If one path is blocked, document the blocker precisely and continue on every independent path that remains feasible.
- Stop only when either:
  - everything in `TODOS.md` is complete and verified, or
  - every remaining unchecked item is blocked by external verification, missing platform access, or a design decision that genuinely cannot be inferred from the repo
- Before stopping, classify every unchecked `TODOS.md` item as either:
  - externally blocked, with the exact reason and missing prerequisite
  - still feasible, in which case implementation must continue

## Verification And Commits

- Primary verification commands:
  - `cargo test --workspace`
  - `npm test`
  - `npm run lint`
- If lint fails because Rust toolchain components are missing, install the missing components and rerun lint.
- Make multiple logical commits, not one giant final commit.
- Only commit substantial, verified progress.
- Avoid committing unrelated dirty work already in the tree.

## Done Means Done

Do not check a box unless all of the following are true:

- implementation exists
- tests exist and pass
- docs are updated if behavior or design changed
- diagnostics or explicit failure behavior are covered
- the checkbox was updated only after verification

## Communication

- Provide short progress updates while working.
- Mention which `TODOS.md` items were completed.
- Mention each commit created.
- Surface blockers immediately with exact files and reasons.
- When blocked on one path, state which alternative path is being taken next.
