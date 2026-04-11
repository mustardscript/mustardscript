You are working in `/Users/mini/jslite`.

Your job is to fully implement everything in `TODOS.md`, update `TODOS.md` as items become verifiably complete, and create git commits after each substantial, verified milestone. Treat this as an execution task, not a planning exercise.

Repository context you must account for:
- The repo already contains a Rust workspace with `crates/jslite`, `crates/jslite-node`, and `crates/jslite-sidecar`.
- There is already meaningful implementation in the parser, IR, runtime, Node addon, JS wrapper, docs, and tests.
- `README.md` and `TODOS.md` currently have local uncommitted edits. Do not discard, overwrite, or revert unrelated user changes. Work with the existing state carefully.
- Current verification commands:
  - `cargo test --workspace`
  - `npm test`
  - `npm run lint`
- If `npm run lint` fails because Rust toolchain components are missing, install the missing components first, then rerun lint.

Hard rules:
- Read `TODOS.md`, `README.md`, `Cargo.toml`, `package.json`, and the existing implementation before making changes.
- Honor the non-negotiable rules and locked decisions in `TODOS.md`.
- Do not assume unchecked boxes are truly undone. Audit the repo first and check off items only when code, tests, and docs actually satisfy them.
- No placeholder completions. A completed item must have implementation, tests, docs where applicable, and explicit failure behavior.
- Keep the Node wrapper thin and keep guest/runtime semantics in Rust.
- Fail closed on unsupported features.
- Never revert changes you did not make.
- Do not stop at partial progress unless you have exhausted all safe, independent work.

Required workflow:
1. Audit the current repo against `TODOS.md`.
2. Mark any already-satisfied checklist items as done, but only after verifying them in code, tests, and docs.
3. Identify the earliest incomplete dependency chain and implement it end to end.
4. For each work chunk:
   - implement the missing functionality
   - add or update tests
   - add or update docs and ADRs when needed
   - run relevant verification commands
   - update `TODOS.md` checkboxes only for items that are now verifiably complete
5. After each substantial, verified milestone, create a focused git commit.
6. Continue until every feasible item in `TODOS.md` is complete.
7. If one item is blocked, do not stop overall:
   - document the blocker precisely
   - update any related docs or TODO notes to reflect the real state
   - skip to other independent TODO items or later phases that can be completed safely
   - keep making as much verified progress as possible
8. Only stop when one of these is true:
   - everything in `TODOS.md` is complete, verified, and checked off
   - or no further safe progress is possible without external input or a decision that cannot be inferred from the repo
9. If you end with remaining blockers, leave the repo in a verified state and provide a precise summary of:
   - what was completed
   - what remains blocked
   - why it is blocked
   - what exact next action would unblock it

Commit policy:
- Make multiple logical commits, not one giant final commit.
- Only commit substantial, verified progress.
- Avoid accidentally committing unrelated dirty work already in the tree.
- Use clear commit messages tied to the milestone completed.

Definition of done for any checked box:
- implementation exists
- tests exist and pass
- docs are updated if behavior or design changed
- diagnostics or failure behavior are covered
- the checkbox was updated only after verification

Execution expectations:
- Prefer making reasonable, evidence-based decisions instead of stopping to ask questions.
- If a blocker affects only one slice of work, continue on every other non-blocked slice.
- If a later phase has independent work that can be implemented safely without violating earlier design constraints, do that work instead of waiting.
- Maintain momentum across docs, core runtime, sidecar, Node binding, tests, CI, and packaging work whenever they can proceed independently.
- Keep the repo buildable and the test suite passing as you go.

While working:
- Provide short progress updates.
- Mention which `TODOS.md` items were completed.
- Mention each commit you created.
- Surface blockers immediately with exact files and reasons.
- When blocked on one path, explicitly state what alternative path you are taking next.

Start by auditing the current implementation and updating `TODOS.md` for anything already verifiably complete. Then implement the next incomplete phase and keep going until the roadmap is finished or no further safe progress is possible.
