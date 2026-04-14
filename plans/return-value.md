# MustardScript Return-Value Design Plan

## Objective

Allow snippet-style MustardScript source to return a value explicitly, without
breaking current script execution semantics or pushing guest-language behavior
into the Node wrapper.

## Audited Current State

Verified current behavior before proposing any change:

- Whole-program execution already returns a value today.
  - `Mustard.run()` resolves to the structured root result.
  - `Mustard.start()` returns either the final structured value or `Progress`.
- The Rust compiler already treats the last top-level expression as the root
  result.
  - `crates/mustard/src/runtime/compiler/mod.rs` special-cases the final
    `Stmt::Expression` in `compile_root(...)`.
  - If no final expression produces a result, the root returns `undefined`.
- Root async results already work through the existing runtime pipeline.
  - If the root result is a guest promise, the runtime waits for settlement and
    exports the fulfilled structured value.
- Top-level `return` is currently rejected.
  - Public API check:
    `new Mustard('return 1;')` raises `MustardParseError` with
    `A 'return' statement can only be used within a function body.`
- The structured host boundary still constrains what can be returned.
  - Results must remain valid `StructuredValue`s, so this feature does not
    change `Map`/`Set`/`BigInt`/cycle/export rules.

That means the real gap is not “Mustard cannot return values.” The gap is:

- snippet authors cannot use explicit root-level `return`
- snippet ergonomics depend on “make the last expression be the answer”
- there is no first-class distinction between full scripts and snippet bodies

## Constraints

- Keep guest/runtime semantics in Rust.
- Keep the Node wrapper thin.
- Preserve current script-mode behavior by default.
- Fail closed on unsupported syntax and unsupported return payloads.
- Keep diagnostics explicit and source spans understandable.

## Option 1: Standardize Existing Final-Expression Semantics

### Summary

Do not change runtime behavior. Document “the snippet result is the last
top-level expression” as the supported pattern.

Example:

```js
const subtotal = lineItems[0].amount + lineItems[1].amount;
subtotal * taxRate;
```

### Pros

- Zero runtime risk.
- No parser or bytecode changes.
- Already works in Rust core, Node, and sidecar.
- Keeps JavaScript surface closest to current implementation.

### Cons

- Not explicit.
- Easy to lose the result by ending with a declaration, `if`, loop, or helper
  call that evaluates to `undefined`.
- No early-return ergonomics for snippet bodies.
- Leaves the user-facing problem mostly unsolved.

### When To Choose It

- If the goal is only to clarify existing behavior, not add new snippet syntax.

## Option 2: Add An Explicit Snippet Mode That Allows Root-Level `return`

### Summary

Add a new compile/source mode for snippet bodies. In snippet mode,
top-level `return` is allowed, while ordinary script mode keeps rejecting it.

Example:

```js
const subtotal = lineItems[0].amount + lineItems[1].amount;
if (subtotal < 0) {
  return 0;
}
return subtotal * taxRate;
```

### API Shape

Rust core:

```rust
enum SourceMode {
    Script,
    Snippet,
}

fn compile_with_mode(source: &str, mode: SourceMode) -> MustardResult<CompiledProgram>;
```

Node wrapper:

```ts
new Mustard(source, {
  inputs: [...],
  sourceMode: 'script' | 'snippet',
})
```

Default remains `script`.

### Semantics

- `script` mode:
  - current behavior unchanged
  - final top-level expression is the result
  - top-level `return` stays a parse/validation error
- `snippet` mode:
  - top-level `return` is allowed
  - existing “final top-level expression returns a value” behavior should still
    work when no explicit `return` is used
  - falling off the end without a result yields `undefined`
  - top-level `await` remains unchanged unless intentionally designed later

### Preferred Implementation Direction

Prefer a Rust-core parsing/lowering path that enables root-level `return` only
for snippet mode, so Mustard keeps:

- one authoritative semantic implementation
- current root last-expression behavior
- clean parity across addon and sidecar

If the upstream parser cannot support that cleanly, the fallback is a Rust-side
synthetic wrapper with source-span remapping. Avoid doing this in the Node
wrapper because that would violate the “core owns semantics” rule.

### Pros

- Solves the actual ergonomics problem.
- Preserves backward compatibility.
- Makes script-vs-snippet intent explicit.
- Leaves room for future snippet-specific features without changing the default
  language contract.

### Cons

- Adds compile-mode API surface.
- Needs careful diagnostic/span handling.
- Requires new tests across Rust, Node, snapshots, and resume paths.

## Option 3: Allow Top-Level `return` In Ordinary Script Mode

### Summary

Change the default language contract so all Mustard scripts may use top-level
`return`.

Example:

```js
const subtotal = lineItems[0].amount + lineItems[1].amount;
return subtotal * taxRate;
```

### Pros

- Minimal user-facing syntax.
- No new public compile mode.
- Likely the simplest mental model for snippet-heavy usage.

### Cons

- Changes existing script semantics globally.
- Diverges from normal JavaScript script parsing.
- Makes the language contract less explicit about what is “script” versus
  “Mustard-specific snippet body” behavior.
- Harder to reverse later if modules or other source modes arrive.

### When To Choose It

- Only if Mustard wants to treat “top-level return is allowed” as a deliberate
  always-on language extension, not a snippet-specific affordance.

## Option 4: Add A Separate Expression-Oriented API

### Summary

Expose a dedicated expression/snippet entrypoint instead of widening general
script parsing.

Possible forms:

- `Mustard.evaluateExpression(source, options)`
- `new MustardExpression(source)`
- `sourceMode: 'expression'`

Example:

```js
price * quantity + shipping
```

### Pros

- Very clear contract.
- Smallest parser surface if limited to a single expression.
- Good fit for tiny inline snippets or config formulas.

### Cons

- Does not solve multi-statement snippet bodies by itself.
- Likely needs to coexist with another option for block snippets.
- Can become redundant if snippet mode already exists and keeps final-expression
  semantics.

### When To Choose It

- As a complement to Option 2, not as the only solution, if Mustard wants a
  very constrained expression-only surface.

## Recommendation

Choose **Option 2**: add an explicit `snippet` source mode in the Rust core and
keep `script` as the default.

Why this is the best fit:

- It solves the user problem directly.
- It preserves current behavior for existing programs.
- It respects the repo’s “Rust core owns semantics / Node wrapper stays thin”
  rule.
- It avoids making a Mustard-only top-level `return` extension universal when
  the need is specifically snippet ergonomics.
- It still lets Mustard keep today’s useful final-expression result behavior.

## Proposed Scope For The First Iteration

### In Scope

- Add `script` vs `snippet` compile mode.
- Allow root-level `return` only in `snippet` mode.
- Preserve current final-expression result behavior in both modes.
- Keep structured-boundary export rules unchanged.
- Document the distinction clearly in `README.md` and `docs/LANGUAGE.md`.

### Explicitly Out Of Scope

- Top-level `await`
- General module support
- Host-boundary result type expansion
- JS-wrapper-only source rewriting

## Implementation Plan

### Phase 1: Core API And Parsing

- Add a Rust-core `SourceMode` enum and a `compile_with_mode(...)` entrypoint.
- Thread source mode through parser/lowering.
- Keep `compile(...)` as a convenience wrapper for `SourceMode::Script`.
- In `snippet` mode, allow root-level `return`.
- In `script` mode, preserve the current parse error.

### Phase 2: Runtime And Public API

- Expose `sourceMode` from the Node API.
- Keep `run()` / `start()` / `resume()` behavior unchanged after compilation.
- Ensure dumped program identity and snapshot flows preserve source-mode
  semantics.

### Phase 3: Tests

Rust tests:

- snippet mode accepts root `return`
- script mode still rejects root `return`
- final-expression root result still works
- snippet `return` interacts correctly with `if`, `try`/`finally`, and early
  exit
- unsupported result values still fail closed at the structured boundary

Node tests:

- `run()` returns snippet `return` values
- `start()` returns final values or `Progress` equivalently in snippet mode
- snapshot dump/load preserves snippet-mode resumability
- error messages clearly distinguish script mode vs snippet mode failures

### Phase 4: Docs

- Update `README.md` examples to show both:
  - current final-expression pattern
  - explicit snippet-mode `return`
- Update `docs/LANGUAGE.md` with the exact mode split.
- Update `docs/HOST_API.md` only if public API options change materially.

## Failure Behavior

These should remain explicit and covered:

- `script` mode + top-level `return`:
  reject with a parse or validation error
- `snippet` mode + unsupported syntax:
  reject exactly as current Mustard syntax validation does
- any mode + unsupported structured result:
  fail closed during result export with the existing typed error path

## Acceptance Criteria

- Existing `script` users see no behavior change.
- Snippet authors can write root-level `return` when `sourceMode: 'snippet'`.
- The last top-level expression still works as a result in existing code.
- Rust, Node, and snapshot/resume paths behave consistently.
- Docs explain the difference without ambiguity.

## Decision

Recommended next implementation target:

- add `sourceMode: 'snippet'`
- keep `script` default
- allow root-level `return` only in snippet mode
- preserve existing final-expression result semantics
