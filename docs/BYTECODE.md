---
title: "Bytecode VM Model"
description: "Stack-based bytecode VM architecture and instruction set"
category: "Language & Runtime"
order: 3
slug: "bytecode"
lastUpdated: "2026-04-14"
---

# Bytecode VM Model

`mustard` currently uses a private stack-based bytecode.

## Program Shape

- A `BytecodeProgram` is a function table plus a `root` function id.
- Each `FunctionPrototype` stores parameter patterns, bytecode instructions, an
  `is_async` flag, and the guest source span that produced the function.
- The root function is executed first and represents the top-level script.

## Operand Model

- Each frame owns an operand stack.
- Literal loads, name loads, and closure creation push values.
- Arithmetic and comparison operations pop their operands and push one result.
- `StoreName` and property-set instructions push the assigned value back so
  assignment expressions still produce a result.
- `JumpIfFalse`, `JumpIfTrue`, and `JumpIfNullish` inspect the top-of-stack
  value without popping it.
- The optimizer may emit private combined handlers such as
  `LoadSlotGetPropStatic`, `DupGetPropStatic`, and
  `LoadSlotDupGetPropStatic` when the same semantics can be preserved with
  fewer dispatches.
- `Await` pops one value, coerces it to an internal guest promise, and
  suspends the current async continuation until a later microtask checkpoint.
- `Return` completes the current frame and returns the top value, defaulting to
  `undefined` if the stack is empty.

## Optimizer Boundaries

- Post-lowering bytecode optimization stays inside block-local regions.
- Within each region, the optimizer tracks abstract stack-top equivalence for
  recent literal and binding loads so redundant reloads can collapse into
  `Dup` before later stack-noop and superinstruction cleanup.
- Jump targets always start a fresh optimization block.
- The optimizer flushes at handler and pending-completion edges, control-flow
  transfers, `await`, calls, construction, `return`, and `throw`.
- There is no bytecode-level source-position marker today, so no additional
  source-position flush boundary is currently encoded.

## Frame Layout

Each frame currently tracks:

- `function_id`: which `FunctionPrototype` is executing
- `ip`: the next instruction index to execute
- `env`: the current lexical environment
- `scope_stack`: the nested lexical environments introduced by `PushEnv`
- `stack`: the operand stack for the frame
- `async_promise`: the backing promise for an async function frame when present

`this` is stored in the frame's lexical environment as a normal binding.

## Validation Rules

Bytecode validation currently checks:

- the root function id exists
- closure targets and jump targets stay in range
- every function ends in `Return`
- stack depth stays valid across every reachable control-flow edge
- lexical scope depth stays valid across `PushEnv` and `PopEnv`
- snapshots reference existing functions, environments, cells, objects, arrays,
  closures, and promises

Malformed bytecode and malformed snapshots fail validation before execution or
restore.
