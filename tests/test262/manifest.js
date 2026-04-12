'use strict';

module.exports = {
  pass: [
    {
      id: 'language/expressions/coalesce/basic.js',
      file: 'cases/pass/language/expressions/coalesce/basic.js',
      expected: [3, 7, 9],
    },
    {
      id: 'language/expressions/optional-chaining/member-chain.js',
      file: 'cases/pass/language/expressions/optional-chaining/member-chain.js',
      expected: [5, undefined, 4],
    },
    {
      id: 'language/expressions/optional-chaining/call.js',
      file: 'cases/pass/language/expressions/optional-chaining/call.js',
      expected: [3, undefined],
    },
    {
      id: 'language/expressions/template-literal/basic-substitution.js',
      file: 'cases/pass/language/expressions/template-literal/basic-substitution.js',
      expected: 'value=7',
    },
    {
      id: 'language/expressions/sequence/basic.js',
      file: 'cases/pass/language/expressions/sequence/basic.js',
      expected: [3, 3],
    },
    {
      id: 'language/expressions/exponentiation/basic.js',
      file: 'cases/pass/language/expressions/exponentiation/basic.js',
      expected: [8, 512, -8],
    },
    {
      id: 'language/statements/variable/dstr/array-object-basic.js',
      file: 'cases/pass/language/statements/variable/dstr/array-object-basic.js',
      expected: 14,
    },
    {
      id: 'language/expressions/member/computed-set-get.js',
      file: 'cases/pass/language/expressions/member/computed-set-get.js',
      expected: { total: 7, key: 3 },
    },
    {
      id: 'language/expressions/array/index-growth.js',
      file: 'cases/pass/language/expressions/array/index-growth.js',
      expected: { first: 1, third: 4, size: 3 },
    },
    {
      id: 'language/statements/try/try-catch-finally-primitive.js',
      file: 'cases/pass/language/statements/try/try-catch-finally-primitive.js',
      expected: ['caught', 'ok', ['body', 'boom', 'finally', 'body', 'finally']],
    },
    {
      id: 'built-ins/Error/TypeError/name-message.js',
      file: 'cases/pass/built-ins/Error/TypeError/name-message.js',
      expected: { name: 'TypeError', message: 'boom' },
    },
    {
      id: 'built-ins/Math/max-abs-composition.js',
      file: 'cases/pass/built-ins/Math/max-abs-composition.js',
      expected: 7,
    },
    {
      id: 'built-ins/Array/isArray/basic.js',
      file: 'cases/pass/built-ins/Array/isArray/basic.js',
      expected: [true, false, -1],
    },
    {
      id: 'language/statements/function/recursion-factorial.js',
      file: 'cases/pass/language/statements/function/recursion-factorial.js',
      expected: 120,
    },
    {
      id: 'language/statements/for-of/array-basic.js',
      file: 'cases/pass/language/statements/for-of/array-basic.js',
      expected: [1, 2, 3],
    },
  ],
  unsupported: [
    {
      id: 'language/module-code/import-declaration/basic.js',
      file: 'cases/unsupported/language/module-code/import-declaration/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'module syntax is not supported',
      reason: 'Module syntax is outside the script-only v1 contract.',
    },
    {
      id: 'language/expressions/dynamic-import/basic.js',
      file: 'cases/unsupported/language/expressions/dynamic-import/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'dynamic import() is not supported',
      reason: 'Dynamic import is explicitly rejected by validation.',
    },
    {
      id: 'language/expressions/delete/basic.js',
      file: 'cases/unsupported/language/expressions/delete/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'delete is not supported in v1',
      reason: 'Property deletion is intentionally not part of the v1 surface.',
    },
    {
      id: 'language/statements/with/basic.js',
      file: 'cases/unsupported/language/statements/with/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'with is not supported',
      reason: 'The runtime only supports strict-mode execution.',
    },
    {
      id: 'language/statements/class/basic.js',
      file: 'cases/unsupported/language/statements/class/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'classes are not supported in v1',
      reason: 'Class semantics are outside the documented subset.',
    },
    {
      id: 'language/statements/generators/basic.js',
      file: 'cases/unsupported/language/statements/generators/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'generators are not supported in v1',
      reason: 'Generators and iterator protocol support are deferred.',
    },
    {
      id: 'language/statements/for-of/assignment-target.js',
      file: 'cases/unsupported/language/statements/for-of/assignment-target.js',
      errorKind: 'Validation',
      messageIncludes: 'for...of currently requires a let or const binding declaration',
      reason: 'The first iteration surface only supports let/const loop bindings.',
    },
    {
      id: 'language/statements/debugger/basic.js',
      file: 'cases/unsupported/language/statements/debugger/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'debugger statements are not supported',
      reason: 'Debugger hooks are excluded from the guest surface.',
    },
    {
      id: 'language/expressions/object/spread/basic.js',
      file: 'cases/unsupported/language/expressions/object/spread/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'object spread is not supported in v1',
      reason: 'Object spread remains outside the supported subset.',
    },
    {
      id: 'language/expressions/array/spread/basic.js',
      file: 'cases/unsupported/language/expressions/array/spread/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'array spread is not supported in v1',
      reason: 'Array spread remains outside the supported subset.',
    },
    {
      id: 'language/expressions/assignment/exponentiation/basic.js',
      file: 'cases/unsupported/language/expressions/assignment/exponentiation/basic.js',
      errorKind: 'Validation',
      messageIncludes: 'unsupported assignment operator in v1',
      reason: 'Exponent assignment remains outside the supported assignment surface.',
    },
    {
      id: 'language/expressions/object/method-basic.js',
      file: 'cases/unsupported/language/expressions/object/method-basic.js',
      errorKind: 'Validation',
      messageIncludes: 'object literal methods are not supported in v1',
      reason: 'Object literal methods are part of the documented exclusions.',
    },
  ],
};
