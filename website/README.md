# MustardScript Website

This directory contains the Vite/React marketing site plus the experimental
browser playground.

## Scripts

- `npm run dev`
  Builds the browser `.wasm` artifact in debug mode, copies it into
  `public/mustard-playground.wasm`, and starts Vite.
- `npm run build`
  Builds the browser `.wasm` artifact in release mode and then produces the
  static site bundle.
- `npm run lint`
  Runs ESLint for the website sources.
- `npm run test:playwright`
  Launches the site locally and runs the browser smoke test for the playground.

## Playground Architecture

The playground compares two execution paths for the same scenario:

1. `MustardScript`
   Runs through `crates/mustard-wasm`, a raw `wasm32-unknown-unknown` wrapper
   around the Rust runtime core.
2. Vanilla JavaScript
   Runs inside `public/playground-iframe.html`, which exposes only the
   scenario-defined helper surface over `postMessage`.

Scenario definitions live in
[`src/lib/playground/scenarios.ts`](src/lib/playground/scenarios.ts). The same
helper source strings are reused by both the Mustard WASM wrapper and the
iframe runner so the comparison stays aligned.

## Limitations

- The iframe path is isolated from the main app UI with `sandbox="allow-scripts"`
  and message passing, but it is not a hardened security boundary.
- The timeout/reset logic for vanilla JavaScript is cooperative. A synchronous
  infinite loop can still monopolize the browser event loop before the timeout
  fires.
- The current release `.wasm` artifact is large, roughly 71 MB before HTTP
  compression, so the playground remains an experimental demo target rather
  than a production browser embedding story.
