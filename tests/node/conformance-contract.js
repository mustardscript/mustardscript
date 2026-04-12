'use strict';

const OUTCOME = Object.freeze({
  NODE_PARITY: 'node_parity',
  VALIDATION_REJECT: 'validation_reject',
  RUNTIME_REJECT: 'runtime_reject',
  KNOWN_DIVERGENCE: 'known_divergence',
});

const COVERAGE = Object.freeze({
  GENERATED_AST: 'generated_ast',
  EXHAUSTIVE_AST: 'exhaustive_ast',
  TRACE_DIFFERENTIAL: 'trace_differential',
  METAMORPHIC: 'metamorphic',
  PROPERTY_NEGATIVE: 'property_negative',
  TEST262_UNSUPPORTED: 'test262_unsupported',
  EXISTING: 'existing',
  AUDIT: 'audit',
});

const REJECT_PHASE = Object.freeze({
  CONSTRUCTOR: 'constructor',
  RUNTIME: 'runtime',
});

const DIAGNOSTIC_CATEGORY = Object.freeze({
  AMBIENT_GLOBAL: 'ambient_global',
  UNSUPPORTED_SYNTAX: 'unsupported_syntax',
  UNSUPPORTED_BINDING: 'unsupported_binding',
  UNSUPPORTED_OPERATOR: 'unsupported_operator',
  UNSUPPORTED_RUNTIME_SURFACE: 'unsupported_runtime_surface',
  UNSUPPORTED_GLOBAL_BUILTIN: 'unsupported_global_builtin',
});

const FORBIDDEN_AMBIENT_GLOBALS = Object.freeze([
  'process',
  'module',
  'exports',
  'global',
  'require',
  'setTimeout',
  'setInterval',
  'queueMicrotask',
  'fetch',
]);

const FEATURE_CONTRACT = Object.freeze([
  {
    id: 'language.literals',
    title: 'primitive literals',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.identifiers',
    title: 'lexical identifier reads',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.unary-operators',
    title: 'supported unary operators',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.binary-operators',
    title: 'supported binary operators',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.logical-operators',
    title: 'supported logical operators',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.array-literals',
    title: 'array literals',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.array-holes',
    title: 'sparse array holes across the documented helper and JSON surface',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.array-spread',
    title: 'array spread over the documented iterable surface',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.object-literals',
    title: 'plain object literals with static keys',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.object-literal-computed-keys',
    title: 'object literals with computed keys',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.object-literal-methods',
    title: 'object literal method shorthand',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.object-literal-spread',
    title: 'object spread for plain objects and arrays',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.member-access',
    title: 'static and computed member access',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.optional-chaining',
    title: 'optional chaining on documented supported cases',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.function-calls',
    title: 'function and member calls',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.EXHAUSTIVE_AST, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.spread-arguments',
    title: 'spread arguments over the documented iterable surface',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.sequence-expressions',
    title: 'sequence expressions',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'language.variable-declarations',
    title: 'let and const declarations',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.TRACE_DIFFERENTIAL, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.if-statements',
    title: 'if/else control flow',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.GENERATED_AST, COVERAGE.TRACE_DIFFERENTIAL, COVERAGE.METAMORPHIC],
  },
  {
    id: 'language.for-loops',
    title: 'for loops',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'host.console-callbacks',
    title: 'deterministic console callbacks',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.TRACE_DIFFERENTIAL, COVERAGE.METAMORPHIC],
  },
  {
    id: 'host.sync-capabilities',
    title: 'synchronous host capability calls',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.TRACE_DIFFERENTIAL, COVERAGE.METAMORPHIC],
  },
  {
    id: 'metamorphic.alpha-renaming',
    title: 'alpha-renaming preserves meaning',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.METAMORPHIC],
  },
  {
    id: 'metamorphic.parenthesized-rendering',
    title: 'extra parenthesization preserves meaning',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.METAMORPHIC],
  },
  {
    id: 'metamorphic.dead-branch-insertion',
    title: 'dead branch insertion preserves meaning',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.METAMORPHIC],
  },
  {
    id: 'validation.default-parameters',
    title: 'default parameters are validation rejects',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'function wrap(value = 1) { return value; }',
    messageIncludes: 'default parameters are not supported in v1',
  },
  {
    id: 'validation.default-destructuring',
    title: 'default destructuring is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'const { value = 1 } = {};',
    messageIncludes: 'default destructuring is not supported in v1',
  },
  {
    id: 'validation.free-arguments',
    title: 'free arguments is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'function wrap() { return arguments[0]; }',
    messageIncludes: 'forbidden ambient global `arguments`',
  },
  {
    id: 'validation.free-eval',
    title: 'free eval is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'eval("1");',
    messageIncludes: 'forbidden ambient global `eval`',
  },
  {
    id: 'validation.free-function-constructor',
    title: 'free Function is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'Function("return 1");',
    messageIncludes: 'forbidden ambient global `Function`',
  },
  {
    id: 'validation.module-syntax',
    title: 'module syntax is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'export const value = 1;',
    messageIncludes: 'module syntax is not supported',
  },
  {
    id: 'validation.dynamic-import',
    title: 'dynamic import is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'import("pkg");',
    messageIncludes: 'dynamic import() is not supported',
  },
  {
    id: 'validation.delete',
    title: 'delete is a validation reject until object and array deletion semantics exist',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'delete value.prop;',
    messageIncludes: 'delete is not supported in v1',
  },
  {
    id: 'validation.delete-array-element',
    title: 'delete stays rejected for array element removal semantics',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'delete values[0];',
    messageIncludes: 'delete is not supported in v1',
  },
  {
    id: 'validation.for-in',
    title: 'for...in rejects unsupported right-hand sides at runtime',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'for (const key in "hi") { key; }',
    messageIncludes: 'Object helpers currently only support plain objects and arrays',
  },
  {
    id: 'language.for-await-of',
    title: 'for await...of matches the documented async iteration subset',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'validation.classes',
    title: 'classes are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'class Example {}',
    messageIncludes: 'classes are not supported in v1',
  },
  {
    id: 'validation.generators',
    title: 'generators are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'function* make() { yield 1; }',
    messageIncludes: 'generators are not supported in v1',
  },
  {
    id: 'validation.with',
    title: 'with is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'with ({ alpha: 1 }) { alpha; }',
    messageIncludes: 'with is not supported',
  },
  {
    id: 'validation.array-spread-surface',
    title: 'array spread rejects unsupported iterable inputs at runtime',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'const value = {}; [...value];',
    messageIncludes: 'value is not iterable in the supported surface',
  },
  {
    id: 'validation.debugger',
    title: 'debugger is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'debugger;',
    messageIncludes: 'debugger statements are not supported',
  },
  {
    id: 'validation.labeled-statements',
    title: 'labeled statements are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'label: 1;',
    messageIncludes: 'labeled statements are not supported in v1',
  },
  {
    id: 'validation.private-fields',
    title: 'private fields are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'value.#secret;',
    messageIncludes: 'private fields are not supported in v1',
  },
  {
    id: 'validation.meta-properties',
    title: 'meta properties are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'new.target;',
    messageIncludes: 'meta properties are not supported',
  },
  {
    id: 'validation.super',
    title: 'super is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'super.value;',
    messageIncludes: 'super is not supported in v1',
  },
  {
    id: 'validation.update-expressions',
    title: 'update expressions are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'let value = 1; value++;',
    messageIncludes: 'update expressions are not supported in v1',
  },
  {
    id: 'validation.logical-assignment-or',
    title: 'logical assignment ||= matches Node short-circuit assignment semantics',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'validation.logical-assignment-and',
    title: 'logical assignment &&= matches Node short-circuit assignment semantics',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING],
  },
  {
    id: 'validation.tagged-templates',
    title: 'tagged templates are a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'tag`value`;',
    messageIncludes: 'tagged templates are not supported in v1',
  },
  {
    id: 'validation.spread-arguments-surface',
    title: 'spread arguments reject unsupported iterable inputs at runtime',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'function run() {} const values = {}; run(...values);',
    messageIncludes: 'value is not iterable in the supported surface',
  },
  {
    id: 'validation.destructuring-assignment',
    title: 'destructuring assignment is a validation reject',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: '[value] = [1];',
    messageIncludes: 'destructuring assignment is not supported in v1',
  },
  {
    id: 'validation.unsupported-unary',
    title: 'unsupported unary operators are validation rejects',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: '~1;',
    messageIncludes: 'unsupported unary operator in v1',
  },
  {
    id: 'validation.unsupported-binary',
    title: 'instanceof stays rejected until the prototype model is explicit',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: '1 instanceof Number;',
    messageIncludes: 'unsupported binary operator in v1',
  },
  {
    id: 'validation.instanceof-guest-function',
    title: 'instanceof stays rejected even with a guest function constructor',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'function Box() {} const value = {}; value instanceof Box;',
    messageIncludes: 'unsupported binary operator in v1',
  },
  {
    id: 'validation.unsupported-assignment',
    title: 'unsupported assignment operators are validation rejects',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'let value = 1; value **= 2;',
    messageIncludes: 'unsupported assignment operator in v1',
  },
  {
    id: 'validation.object-accessors',
    title: 'object literal accessors are validation rejects',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: '({ get value() { return 1; } });',
    messageIncludes: 'object literal accessors are not supported in v1',
  },
  {
    id: 'validation.var',
    title: 'var is a validation reject because v1 keeps lexical bindings only',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'var value = 1;',
    messageIncludes: 'only let and const are supported',
  },
  {
    id: 'validation.var-function-scope',
    title: 'var stays rejected inside function scope as well as top level',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'function wrap() { var value = 1; return value; }',
    messageIncludes: 'only let and const are supported',
  },
  {
    id: 'validation.ambient-globals',
    title: 'documented ambient host globals are validation rejects',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.TEST262_UNSUPPORTED],
    source: 'process;',
    messageIncludes: 'forbidden ambient global `process`',
  },
  {
    id: 'validation.using-declarations',
    title: 'using and await using declarations stay rejected with lexical-only bindings',
    outcome: OUTCOME.VALIDATION_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'using value = resource;',
    messageIncludes: 'only let and const are supported',
  },
  {
    id: 'runtime.symbol',
    title: 'Symbol remains unavailable in the supported guest surface',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'Symbol("x");',
    messageIncludes: 'ReferenceError: `Symbol` is not defined',
  },
  {
    id: 'runtime.typed-arrays',
    title: 'typed array constructors remain unavailable in the supported guest surface',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'new Uint8Array(2);',
    messageIncludes: 'ReferenceError: `Uint8Array` is not defined',
  },
  {
    id: 'runtime.intl',
    title: 'Intl remains unavailable in the supported guest surface',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'Intl.DateTimeFormat();',
    messageIncludes: 'ReferenceError: `Intl` is not defined',
  },
  {
    id: 'runtime.proxy',
    title: 'Proxy remains unavailable in the supported guest surface',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE],
    source: 'new Proxy({}, {});',
    messageIncludes: 'ReferenceError: `Proxy` is not defined',
  },
  {
    id: 'runtime.object-create',
    title: 'Object.create remains unavailable while prototype semantics stay deferred',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.EXISTING],
    source: 'Object.create(null);',
    messageIncludes: 'Object.create is unsupported because prototype semantics are deferred',
  },
  {
    id: 'runtime.object-freeze',
    title: 'Object.freeze remains unavailable while property descriptor semantics stay deferred',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.EXISTING],
    source: 'Object.freeze({ alpha: 1 });',
    messageIncludes: 'Object.freeze is unsupported because property descriptor semantics are deferred',
  },
  {
    id: 'runtime.object-seal',
    title: 'Object.seal remains unavailable while property descriptor semantics stay deferred',
    outcome: OUTCOME.RUNTIME_REJECT,
    coverage: [COVERAGE.PROPERTY_NEGATIVE, COVERAGE.EXISTING],
    source: 'Object.seal({ alpha: 1 });',
    messageIncludes: 'Object.seal is unsupported because property descriptor semantics are deferred',
  },
  {
    id: 'observable.sorted-object-enumeration',
    title: 'plain object key enumeration matches JavaScript own-property order',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING, COVERAGE.AUDIT],
  },
  {
    id: 'observable.sorted-json-stringify',
    title: 'JSON.stringify preserves JavaScript ordering and rendering semantics',
    outcome: OUTCOME.NODE_PARITY,
    coverage: [COVERAGE.EXISTING, COVERAGE.AUDIT],
  },
]);

const REJECT_EXPECTATIONS = Object.freeze({
  'validation.default-parameters': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.default-destructuring': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.free-arguments': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
  },
  'validation.free-eval': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
  },
  'validation.free-function-constructor': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
  },
  'validation.module-syntax': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.dynamic-import': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.delete': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
  },
  'validation.delete-array-element': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
  },
  'validation.for-in': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
  },
  'validation.classes': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.generators': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.with': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.array-spread-surface': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
  },
  'validation.debugger': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.labeled-statements': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.private-fields': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.meta-properties': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.super': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.update-expressions': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.tagged-templates': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.spread-arguments-surface': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
  },
  'validation.destructuring-assignment': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.unsupported-unary': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
  },
  'validation.unsupported-binary': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
  },
  'validation.instanceof-guest-function': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
  },
  'validation.unsupported-assignment': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
  },
  'validation.object-accessors': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
  },
  'validation.var': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING,
  },
  'validation.var-function-scope': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING,
  },
  'validation.ambient-globals': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
  },
  'validation.using-declarations': {
    phase: REJECT_PHASE.CONSTRUCTOR,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING,
  },
  'runtime.symbol': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
  },
  'runtime.typed-arrays': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
  },
  'runtime.intl': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
  },
  'runtime.proxy': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
  },
  'runtime.object-create': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
  },
  'runtime.object-freeze': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
  },
  'runtime.object-seal': {
    phase: REJECT_PHASE.RUNTIME,
    category: DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
  },
});

for (const entry of FEATURE_CONTRACT) {
  const expectation = REJECT_EXPECTATIONS[entry.id];
  if (expectation !== undefined) {
    entry.phase = expectation.phase;
    entry.category = expectation.category;
  }
}

const VALIDATION_REJECT_CASES = Object.freeze(
  FEATURE_CONTRACT.filter((entry) => entry.outcome === OUTCOME.VALIDATION_REJECT)
    .map(({ id, source, messageIncludes, phase, category }) => ({
      id,
      source,
      messageIncludes,
      phase,
      category,
    })),
);

const RUNTIME_REJECT_CASES = Object.freeze(
  FEATURE_CONTRACT.filter((entry) => entry.outcome === OUTCOME.RUNTIME_REJECT)
    .map(({ id, source, messageIncludes, phase, category }) => ({
      id,
      source,
      messageIncludes,
      phase,
      category,
    })),
);

const CURATED_REJECTION_REGRESSION_IDS = Object.freeze([
  'validation.module-syntax',
  'validation.free-eval',
  'validation.var',
  'validation.delete',
  'validation.object-accessors',
  'validation.instanceof-guest-function',
  'runtime.symbol',
  'validation.array-spread-surface',
  'runtime.object-create',
  'runtime.object-freeze',
  'runtime.object-seal',
]);

const CURATED_REJECTION_REGRESSION_CASES = Object.freeze(
  CURATED_REJECTION_REGRESSION_IDS.map((id) => {
    const entry = FEATURE_CONTRACT.find((feature) => feature.id === id);
    if (entry === undefined) {
      throw new Error(`missing curated rejection contract entry: ${id}`);
    }
    const { source, messageIncludes, phase, category } = entry;
    return { id, source, messageIncludes, phase, category };
  }),
);

function featuresForCoverage(coverage) {
  return FEATURE_CONTRACT.filter((entry) => entry.coverage.includes(coverage));
}

module.exports = {
  COVERAGE,
  CURATED_REJECTION_REGRESSION_CASES,
  DIAGNOSTIC_CATEGORY,
  FEATURE_CONTRACT,
  FORBIDDEN_AMBIENT_GLOBALS,
  OUTCOME,
  REJECT_PHASE,
  RUNTIME_REJECT_CASES,
  VALIDATION_REJECT_CASES,
  featuresForCoverage,
};
