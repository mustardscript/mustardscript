# MustardScript Browser Playground Plan

## Goal

Embed a live playground into the website that:

- executes `MustardScript` in the browser via a dedicated WASM target
- executes comparison "vanilla JavaScript" on the client side
- shows the same scenario, inputs, outputs, timings, and failure modes for both
- preserves the repository's existing constraints:
  - guest/runtime semantics stay in Rust
  - unsupported features fail closed
  - the existing Node wrapper remains thin and unchanged in role

This plan is intentionally scoped to a website demo target first. It is not a
commitment to full browser-runtime parity with the Node addon.

## Current Constraints Confirmed In Repo

- The current public runtime paths are Node addon and sidecar, not browser:
  [README.md](/Users/mini/mustardscript-wasm-plan/README.md) and
  [docs/HOST_API.md](/Users/mini/mustardscript-wasm-plan/docs/HOST_API.md).
- The website is a standalone Vite/React app in
  [website/](/Users/mini/mustardscript-wasm-plan/website).
- The site composition point for a new section is
  [website/src/App.tsx](/Users/mini/mustardscript-wasm-plan/website/src/App.tsx).
- Existing site code examples already present the right narrative surface for a
  live comparison section:
  [website/src/components/CodeStorytelling.tsx](/Users/mini/mustardscript-wasm-plan/website/src/components/CodeStorytelling.tsx)
  and
  [website/src/components/ApiDocs.tsx](/Users/mini/mustardscript-wasm-plan/website/src/components/ApiDocs.tsx).

## Product Shape

Build a scenario-based playground, not a general unrestricted browser sandbox.

The first shipped UX should be:

- left pane: editable MustardScript guest code
- right pane: editable or reference vanilla JS implementation
- shared scenario selector
- shared fixed input payload
- shared fixed host helper surface
- output panel:
  - result
  - error
  - elapsed time
  - capability trace

This keeps the comparison honest and avoids turning the website into a generic
unsafe JavaScript executor.

## Locked Decisions

1. `MustardScript` runs in browser through a new Rust-to-WASM target.
2. Vanilla JavaScript runs client-side, but not in the main page execution
   context when user-editable code is enabled.
3. The first milestone supports synchronous and promise-free demo scenarios
   unless the existing async/runtime model ports cleanly.
4. Browser playground capabilities are explicit and fixed per scenario.
5. Unsupported features in the WASM/browser path fail closed with clear errors.
6. Browser support is a website/demo target first, not an alternative to the
   existing Node or sidecar embedding story.

## Recommended Architecture

### 1. Add a Browser-Facing WASM Crate

Add a new workspace member:

- `crates/mustard-wasm`

Responsibilities:

- expose a narrow JS-facing API for browser execution
- call into the existing Rust core
- keep all guest semantics in `crates/mustard`
- translate structured values and typed failures into browser-safe JS values

Recommended export surface:

```ts
type WasmRunRequest = {
  code: string
  inputs?: Record<string, unknown>
  limits?: Partial<{
    instructionBudget: number
    heapLimitBytes: number
    allocationBudget: number
    callDepthLimit: number
    maxOutstandingHostCalls: number
  }>
  scenario?: string
}

type WasmRunResponse = {
  ok: true
  result: unknown
  elapsedMs: number
  trace: Array<{ capability: string; args: unknown[] }>
} | {
  ok: false
  error: {
    name: string
    message: string
    span?: { start: number; end: number }
  }
  elapsedMs: number
  trace: Array<{ capability: string; args: unknown[] }>
}
```

Exports:

- `init()` or generated WASM module bootstrap
- `runScenario(request)`
- optionally later: `validateProgram(code)`

Do not start with browser snapshot/resume support unless needed for the demo.

### 2. Keep Host Semantics In Rust, Bridge Capabilities Through JS

The browser wrapper should only provide explicit capability callbacks for the
selected scenario. The Rust core should still own:

- parsing
- validation
- lowering
- bytecode execution
- limits
- diagnostics
- structured value acceptance/rejection

Browser capability model for v1:

- scenario registers a fixed set of host functions
- each function receives structured values only
- each invocation is logged into a trace buffer
- any unsupported return shape fails closed

First-pass scenarios should prefer synchronous capabilities. If async
capabilities are needed, port them only after the base synchronous bridge is
working and testable.

### 3. Execute Vanilla JS In An Isolated Browser Context

Do not run arbitrary user-edited vanilla JS directly in the page thread.

Use one of these:

- preferred v1: dedicated `Web Worker`
- stricter alternative: sandboxed `iframe` with `sandbox` attributes

Worker responsibilities:

- receive scenario id, source code, and inputs
- expose only scenario-approved helpers
- execute the code
- return result, thrown error, and elapsed time

Required guardrails:

- hard wall-clock timeout with `worker.terminate()`
- no access to parent React state except message passing
- scenario helper surface frozen before execution
- result serialization constrained to plain JSON-compatible values

### 4. Website Integration

Add new website components:

- `website/src/components/PlaygroundSection.tsx`
- `website/src/components/PlaygroundEditor.tsx`
- `website/src/components/PlaygroundOutput.tsx`
- `website/src/components/PlaygroundScenarioTabs.tsx`
- `website/src/lib/playground/scenarios.ts`
- `website/src/lib/playground/mustard-wasm.ts`
- `website/src/lib/playground/vanilla-worker.ts`
- `website/src/workers/vanillaRunner.ts`

Insert the section in:

- [website/src/App.tsx](/Users/mini/mustardscript-wasm-plan/website/src/App.tsx)

Recommended placement:

- after `CodeStorytelling`
- before `SpeedSection` or `ApiDocs`

This keeps the playground close to the existing code-story narrative while
still early enough in the page to matter.

### 5. Scenario Model

Define scenarios centrally and keep both runtimes on the same rails.

Suggested shape:

```ts
type PlaygroundScenario = {
  id: string
  label: string
  description: string
  inputs: Record<string, unknown>
  helpers: Record<string, (...args: unknown[]) => unknown>
  mustardTemplate: string
  vanillaTemplate: string
  expectedResult?: unknown
}
```

Initial scenarios:

1. `quote-builder`
   mirrors the current marketing story around capabilities.
2. `search-reducer`
   compares tool fan-out and reduction.
3. `policy-check`
   shows straightforward data transformation with one or two host calls.

The first shipped scenario should be `quote-builder`, because the current site
already teaches that model visually.

## Phased Milestones

## Milestone 0: Feasibility Spike

Deliverables:

- confirm `crates/mustard` can compile for the chosen WASM target
- identify incompatible dependencies or code paths
- document required feature gates and polyfills
- produce a minimal browser call that parses and executes `const x = 2 + 2; x;`

Implementation notes:

- prefer `wasm32-unknown-unknown`
- use `wasm-bindgen` if the exported JS interop becomes materially simpler
- if allocator or time APIs are platform-sensitive, gate them explicitly

Exit criteria:

- a browser page can load the module and run one pure compute program

## Milestone 1: Core Browser Runner

Deliverables:

- `crates/mustard-wasm`
- browser-safe run API
- typed error mapping
- limit configuration from website code
- trace collection for capability calls

Tests:

- Rust tests for exported request validation and error conversion
- browser-facing smoke test for pure compute success/failure

Exit criteria:

- website can run pure Mustard guest code in-browser

## Milestone 2: Scenario Capability Bridge

Deliverables:

- fixed scenario registry
- explicit capability bridge from JS to WASM runtime
- structured-value validation parity with current host boundary expectations
- fail-closed errors for unsupported scenario helper use

Tests:

- Rust tests for structured boundary rejection
- website/browser tests for capability trace rendering
- negative tests for unsupported values and missing capabilities

Exit criteria:

- one full scenario works end to end in browser Mustard execution

## Milestone 3: Vanilla JS Client-Side Runner

Deliverables:

- worker-based vanilla JS execution path
- timeout and termination controls
- shared scenario helper registry
- normalized result/error envelope matching the Mustard pane

Tests:

- worker unit tests or browser tests for timeout and serialization failures
- comparison tests ensuring same scenario helpers feed both runtimes

Exit criteria:

- same scenario runs in Mustard WASM and vanilla JS worker with comparable UI

## Milestone 4: Website Playground UX

Deliverables:

- full `PlaygroundSection`
- side-by-side editors
- run/reset controls
- preloaded scenarios
- output, errors, timings, and capability traces
- mobile layout and load-state handling

Tests:

- Playwright smoke test for:
  - page load
  - scenario switch
  - run both panes
  - render success
  - render failure

Exit criteria:

- live comparison embedded in the website and shippable as a docs/demo feature

## Milestone 5: Hardening And Documentation

Deliverables:

- README/docs update describing browser playground as a demo target
- explicit note that browser playground does not replace Node/sidecar deployment
- documented unsupported features and safety caveats
- performance notes and bundle-size budget

Tests:

- `website` build and lint
- targeted repo verification for touched crates and browser assets

Exit criteria:

- browser playground is documented, tested, and clearly scoped

## Failure Behavior Requirements

The browser path must preserve explicit failure behavior.

Must fail closed for:

- unsupported structured host values
- unsupported browser-side helper return values
- missing scenario capability names
- invalid scenario ids
- WASM bootstrap/load failures
- worker timeout or worker crash
- user vanilla JS returning non-serializable values

Error envelopes should be normalized so the UI can show:

- runtime name
- message
- optional source span or scenario label
- whether the error came from `mustard`, `vanilla`, `wasm-loader`, or `worker`

## Verification Plan

During implementation, add targeted commands before broader checks.

Rust/WASM:

- `cargo test -p mustard`
- `cargo test -p mustard-wasm`
- `cargo check -p mustard-wasm --target wasm32-unknown-unknown`

Website:

- `npm --prefix website run lint`
- `npm --prefix website run build`

Browser integration:

- Playwright smoke test for playground happy/failure paths

Broader verification before claiming the feature complete:

- `cargo test --workspace`
- `npm test`
- `npm run lint`
- `npm --prefix website run build`
- `npm --prefix website run lint`

## Risks And Unknowns

1. Some Rust dependencies or runtime assumptions may not compile cleanly to the
   chosen WASM target.
2. Async host capability semantics may be materially harder in browser WASM
   than synchronous demo scenarios.
3. Bundle size may become too large for the website without code splitting or a
   lazy-loaded playground section.
4. Browser timing comparisons are illustrative, not authoritative benchmarks.
5. Vanilla JS safety is only acceptable if code runs in an isolated worker or
   sandboxed frame with explicit timeouts.

## Recommended Order Of Execution

1. Complete Milestone 0 as a real code spike in a feature branch.
2. If the spike passes, land `crates/mustard-wasm` with pure compute support.
3. Add one fixed scenario with Mustard-only execution in the website.
4. Add the isolated vanilla JS runner.
5. Add comparative UI and browser tests.
6. Expand scenarios only after one polished end-to-end slice works.

## Non-Goals For The First Version

- full Node API parity in the browser
- unrestricted arbitrary browser host access
- shipping browser snapshots/resume on day one
- claiming browser execution is a hardened sandbox
- replacing addon or sidecar deployment guidance

## Acceptance Criteria For The First Shippable Version

- website contains an embedded playground section
- Mustard guest code runs in-browser through WASM
- vanilla JS comparison runs client-side in an isolated execution context
- both panes share one scenario definition and one input payload
- both panes show normalized result/error/timing output
- at least one scenario is covered by browser automation
- docs explain scope and limitations clearly
