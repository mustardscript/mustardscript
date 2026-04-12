# Repository Instructions

This file defines how work must be executed in this repository. Treat it as an execution contract, not general guidance.

## Core Objective

- Complete the repository's remaining implementation work end to end.
- Verify existing behavior before assuming something is unfinished.
- Keep the repository buildable and verification passing as work progresses.
- Prefer execution over planning. Do the work unless a real blocker prevents it.

## Required Startup Checklist

Before making changes:

1. Read `IMPLEMENT_PROMPT.md`.
2. Read `README.md`.
3. Read `Cargo.toml`.
4. Read `package.json`.
5. Run `git status` and inspect the current worktree.
6. Inspect the existing implementation in the affected Rust crates, Node bindings, docs, and tests.
7. Audit the current repository state before editing anything.

Do not skip the audit step.

## Audit-First Rule

Before changing code, determine whether the target behavior already exists or is partially implemented.

For each work item you take on, verify:

- which implementation files are relevant
- which tests already cover the behavior
- whether docs already describe the behavior
- whether failure behavior is already defined

Do not assume a missing checkmark, note, or mention means missing implementation. Verify in code and tests first.

## Execution Loop

Work in small, dependency-aware chunks.

For each chunk:

1. Audit the current implementation and identify the next concrete gap.
2. Implement the functionality end to end.
3. Add or update tests.
4. Update docs or ADRs if behavior, guarantees, or design changed.
5. Verify narrow scope first, then broader repo checks.
6. Only after verification, update any progress-tracking docs or checklists.
7. Create a focused commit for substantial verified progress.
8. Move directly to the next feasible chunk.

## Non-Negotiable Rules

- Honor locked decisions and explicit constraints documented in this repository.
- Keep guest/runtime semantics in Rust.
- Keep the Node wrapper thin.
- Fail closed on unsupported features.
- Preserve explicit diagnostics and failure behavior.
- Never revert unrelated user changes.
- Never treat planning artifacts as proof of missing implementation.
- Never mark progress as complete without code, tests, verification, and docs when required.

## Sub-Agents

- Default to `gpt-5.4` for all spawned sub-agents.
- Do not choose a different sub-agent model unless explicitly instructed by the user or by repository-level instructions.
- Treat model selection as fixed by default, not as an optimization target.

## Definition Of Done

A work item is done only when all of the following are true:

- implementation exists
- tests exist or were updated appropriately
- tests pass
- docs were updated if user-visible behavior or design changed
- explicit failure behavior is implemented and covered where applicable
- broader verification was run before claiming completion

If any of these are missing, the item is not done.

## Required Evidence Before Claiming Completion

Before marking any item complete, be able to point to:

- implementation files changed or verified
- test files added, changed, or confirmed sufficient
- docs changed, or a clear reason docs were not needed
- exact verification commands run
- explicit failure-path behavior

## Verification Policy

Primary verification commands:

- `cargo test --workspace`
- `npm test`
- `npm run lint`

During iteration, run the narrowest relevant tests first. Before claiming completion of substantial work, run the broader verification needed to support that claim.

If lint fails because required Rust toolchain components are missing, install the missing components and rerun lint.

If full verification is blocked:

- do not claim completion
- document the exact command that failed
- document whether the failure is caused by environment, platform limits, missing credentials, or unrelated pre-existing breakage
- continue on other feasible independent work

## Commit Policy

- Make multiple logical commits, not one large final commit.
- Commit only substantial, verified progress.
- Keep each commit focused on one milestone or tightly related change set.
- Do not include unrelated dirty work already present in the tree.
- Do not amend commits unless explicitly requested.

Preferred commit style:

- `feat(runtime): implement ...`
- `fix(bindings): handle ...`
- `test(node): cover ...`
- `docs(adr): clarify ...`

## Blockers And Fallbacks

If blocked on one path:

- state the blocker precisely
- name the exact file, command, platform, or missing prerequisite involved
- continue immediately on any other independent feasible path

Acceptable blockers include:

- missing external credentials
- unavailable platform access
- required external system not present
- unresolved design conflict that cannot be inferred from repository context

A large amount of remaining work is not a blocker by itself.

## Stop Criteria

Stop only when one of these is true:

1. All feasible repository work is implemented and verified.
2. Every remaining incomplete item is externally blocked, and the blocker is documented precisely.

Before stopping, classify every remaining incomplete item as either:

- externally blocked, with the exact reason and prerequisite
- still feasible, in which case work should continue

## Communication

Provide short progress updates while working.

Each update should mention:

- what was audited or implemented
- what was verified
- what was completed
- what commit was created, if any
- any blocker, with exact file/command/reason
- what independent path is being taken next if blocked

## Practical Heuristics

For any behavior change, check all relevant layers:

- Rust implementation
- Node bindings/wrapper
- tests
- docs
- diagnostics and failure paths

Do not stop at the first code change if adjacent layers also need updates.

When repository instructions are ambiguous:

- infer from code, tests, docs, and ADRs
- do not silently invent scope
- continue on unambiguous work
- document the ambiguity if it becomes blocking
