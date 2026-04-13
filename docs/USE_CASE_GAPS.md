---
title: "Use Case Gaps"
description: "Audit status of the realistic programmatic tool-call use case gallery"
category: "Development"
order: 4
slug: "use-case-gaps"
lastUpdated: "2026-04-13"
---

# Use Case Gaps

This file tracks the audit status of the realistic programmatic tool-call
gallery under `examples/programmatic-tool-calls/`.

Audit command:

```sh
node scripts/audit-use-cases.js --json
```

Current audited summary on April 11, 2026:

- total cataloged use cases: `24`
- passing: `24`
- failing: `0`

## Current Status

There are no active audited gaps in the current gallery.

The gallery remains intentionally realistic. If future realistic tool-calling
examples expose missing runtime support or runtime correctness bugs, keep the
examples realistic and record the failure here instead of simplifying the use
case to fit the runtime.

## Recently Closed Gaps

The current all-pass audit came from adding proper runtime support rather than
papering over failures with broader rejections. The support added in this
tranche includes:

- `Array.from` on the supported iterable surface
- `Object.fromEntries` on the supported iterable surface
- comparator-based `Array.prototype.sort`
- `String.prototype.matchAll`
- conservative `Date` support for the documented surface:
  `Date.now()`, `new Date(value)`, and `Date.prototype.getTime()`
- async callback/runtime behavior needed for realistic
  `Promise.all(items.map(...))` guest fan-out
- mixed resume payload handling in the use-case audit harness for both bare
  resume values and `{ capability, value }` entries

## Passing Gallery Areas

- analytics use cases
- operations use cases
- workflows use cases

If this file ever becomes non-empty again, list each failing use case with:

- file path
- current failure
- the real runtime gap or correctness bug
- whether the next action is support, correctness work, or a deliberate
  fail-closed diagnostic
