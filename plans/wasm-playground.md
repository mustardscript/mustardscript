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
2. Vanilla JavaScript runs client-side in a sandboxed `iframe`, not in the
   main page execution context.
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

### 3. Execute Vanilla JS In A Sandboxed Iframe

Do not run arbitrary user-edited vanilla JS directly in the page thread.

Use a sandboxed `iframe` with explicit `sandbox` attributes.

Iframe responsibilities:

- receive scenario id, source code, and inputs
- expose only scenario-approved helpers
- execute the code
- return result, thrown error, and elapsed time

Required guardrails:

- hard wall-clock timeout and iframe reset/recreation on timeout or crash
- no access to parent React state except message passing
- scenario helper surface frozen before execution
- result serialization constrained to plain JSON-compatible values
- tight `sandbox` policy with the minimum capability set needed for execution

Recommended messaging model:

- parent page sends `postMessage` with a request envelope
- iframe boot script validates the envelope and runs the selected scenario
- iframe returns a normalized response envelope
- parent rejects messages from unexpected origin/source pairs

### 4. Website Integration

Add new website components:

- `website/src/components/PlaygroundSection.tsx`
- `website/src/components/PlaygroundEditor.tsx`
- `website/src/components/PlaygroundOutput.tsx`
- `website/src/components/PlaygroundScenarioTabs.tsx`
- `website/src/lib/playground/scenarios.ts`
- `website/src/lib/playground/mustard-wasm.ts`
- `website/src/lib/playground/vanilla-iframe.ts`
- `website/src/lib/playground/iframe-protocol.ts`
- `website/public/playground-iframe.html`
- `website/public/playground-iframe.js`

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

Action items:

- [x] confirm `crates/mustard` can compile for the chosen WASM target
- [x] identify incompatible dependencies or code paths
- [x] document required feature gates and polyfills
- [x] produce a minimal browser call that parses and executes `const x = 2 + 2; x;`

Implementation notes:

- prefer `wasm32-unknown-unknown`
- use `wasm-bindgen` if the exported JS interop becomes materially simpler
- if allocator or time APIs are platform-sensitive, gate them explicitly

Exit criteria:

- a browser page can load the module and run one pure compute program

## Milestone 1: Core Browser Runner

Action items:

- [x] add `crates/mustard-wasm`
- [x] expose a browser-safe run API
- [x] add typed error mapping for browser consumers
- [x] wire limit configuration from website code
- [x] add trace collection for capability calls
- [x] add Rust tests for exported request validation and error conversion
- [x] add a browser-facing smoke test for pure compute success/failure

Exit criteria:

- website can run pure Mustard guest code in-browser

## Milestone 2: Scenario Capability Bridge

Action items:

- [x] add a fixed scenario registry
- [x] implement the explicit capability bridge from JS to the WASM runtime
- [x] preserve structured-value validation parity with current host-boundary expectations
- [x] add fail-closed errors for unsupported scenario helper use
- [x] add Rust tests for structured boundary rejection
- [x] add website/browser tests for capability trace rendering
- [ ] add negative tests for unsupported values and missing capabilities

Exit criteria:

- one full scenario works end to end in browser Mustard execution

## Milestone 3: Vanilla JS Sandboxed Iframe Runner

Action items:

- [x] add an iframe-based vanilla JS execution path
- [x] add timeout and iframe reset controls
- [x] share the scenario helper registry with the iframe runner
- [x] normalize the result/error envelope to match the Mustard pane
- [ ] add browser tests for iframe timeout and serialization failures
- [x] add comparison tests ensuring the same scenario helpers feed both runtimes

Exit criteria:

- same scenario runs in Mustard WASM and vanilla JS iframe with comparable UI

## Milestone 4: Website Playground UX

Action items:

- [x] add the full `PlaygroundSection`
- [x] add side-by-side editors
- [x] add run/reset controls
- [x] preload scenarios
- [x] render output, errors, timings, and capability traces
- [x] support mobile layout and load-state handling
- [x] add a Playwright smoke test for page load
- [x] add a Playwright smoke test for scenario switch
- [x] add a Playwright smoke test for running both panes
- [x] add a Playwright smoke test for success rendering
- [x] add a Playwright smoke test for failure rendering

Exit criteria:

- live comparison embedded in the website and shippable as a docs/demo feature

## Milestone 5: Hardening And Documentation

Action items:

- [x] update README/docs to describe browser playground as a demo target
- [x] add an explicit note that browser playground does not replace Node/sidecar deployment
- [x] document unsupported features and safety caveats
- [x] document performance notes and a bundle-size budget
- [x] run `website` build and lint
- [x] run targeted repo verification for touched crates and browser assets

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
- iframe timeout or iframe crash
- user vanilla JS returning non-serializable values

Error envelopes should be normalized so the UI can show:

- runtime name
- message
- optional source span or scenario label
- whether the error came from `mustard`, `vanilla`, `wasm-loader`, or `iframe`

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
5. Vanilla JS safety is only acceptable if code runs in a sandboxed iframe with
   explicit timeouts and envelope validation.

## Recommended Order Of Execution

1. Complete Milestone 0 as a real code spike in a feature branch.
2. If the spike passes, land `crates/mustard-wasm` with pure compute support.
3. Add one fixed scenario with Mustard-only execution in the website.
4. Add the sandboxed iframe vanilla JS runner.
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
