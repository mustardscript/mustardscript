'use strict';

const fc = require('fast-check');

const { COVERAGE, featuresForCoverage } = require('./conformance-contract.js');

const AST_PROPERTY_RUNS = process.env.CI ? 80 : 40;

const DEFAULT_BINDINGS = Object.freeze({
  alpha: 'alpha',
  beta: 'beta',
  fallback: 'fallback',
  maybeObject: 'maybeObject',
  text: 'text',
  values: 'values',
  box: 'box',
  toolbox: 'toolbox',
  doubleFn: 'double',
  probe: 'probe',
});

const RENAMED_BINDINGS = Object.freeze({
  alpha: 'theta',
  beta: 'omega',
  fallback: 'delta',
  maybeObject: 'nullable',
  text: 'letters',
  values: 'collection',
  box: 'record',
  toolbox: 'kit',
  doubleFn: 'mirror',
  probe: 'probe',
});

function literal(value) {
  return { type: 'literal', value };
}

function ref(name) {
  return { type: 'ref', name };
}

function unary(op, argument) {
  return { type: 'unary', op, argument };
}

function binary(op, left, right) {
  return { type: 'binary', op, left, right };
}

function logical(op, left, right) {
  return { type: 'logical', op, left, right };
}

function arrayLiteral(elements) {
  return { type: 'array', elements };
}

function objectLiteral(properties) {
  return { type: 'object', properties };
}

function member(object, property, computed = false) {
  return { type: 'member', object, property, computed };
}

function optionalMember(object, property, computed = false) {
  return { type: 'optional-member', object, property, computed };
}

function call(callee, args) {
  return { type: 'call', callee, args };
}

function pureProgram(result) {
  return { kind: 'pure', result };
}

function consoleTraceProgram(first, second, result) {
  return { kind: 'console-trace', first, second, result };
}

function capabilityTraceProgram(first, second, result) {
  return { kind: 'capability-trace', first, second, result };
}

function branchTraceProgram(test, consequent, alternate, result) {
  return { kind: 'branch-trace', test, consequent, alternate, result };
}

function renderNumberLiteral(value) {
  if (Number.isNaN(value)) {
    return '(0 / 0)';
  }
  if (Object.is(value, -0)) {
    return '(-0)';
  }
  if (value === Infinity) {
    return '(1 / 0)';
  }
  if (value === -Infinity) {
    return '(-1 / 0)';
  }
  return String(value);
}

function renderLiteral(value) {
  if (value === undefined) {
    return 'undefined';
  }
  if (value === null) {
    return 'null';
  }
  if (typeof value === 'boolean') {
    return value ? 'true' : 'false';
  }
  if (typeof value === 'number') {
    return renderNumberLiteral(value);
  }
  if (typeof value === 'string') {
    return JSON.stringify(value);
  }
  throw new TypeError(`Unsupported AST literal value: ${String(value)}`);
}

function maybeExtraParens(rendered, expr, extraParens) {
  if (!extraParens) {
    return rendered;
  }
  if (expr.type === 'literal' || expr.type === 'ref') {
    return rendered;
  }
  return `(${rendered})`;
}

function renderProperty(property, bindings, options) {
  if (typeof property === 'string') {
    return property;
  }
  return renderExpression(property, bindings, options);
}

function renderCallCallee(callee, bindings, options) {
  if (callee.type === 'member' || callee.type === 'optional-member') {
    return renderExpression(callee, bindings, options);
  }
  const rendered = renderExpression(callee, bindings, options);
  return callee.type === 'ref' ? rendered : `(${rendered})`;
}

function renderExpression(expr, bindings = DEFAULT_BINDINGS, options = {}) {
  const { extraParens = false } = options;
  let rendered;

  switch (expr.type) {
    case 'literal':
      rendered = renderLiteral(expr.value);
      break;
    case 'ref':
      rendered = bindings[expr.name] ?? expr.name;
      break;
    case 'unary': {
      const argument = renderExpression(expr.argument, bindings, options);
      rendered = expr.op === 'typeof' ? `(typeof (${argument}))` : `(${expr.op}(${argument}))`;
      break;
    }
    case 'binary': {
      const left = renderExpression(expr.left, bindings, options);
      const right = renderExpression(expr.right, bindings, options);
      rendered = `(${left} ${expr.op} ${right})`;
      break;
    }
    case 'logical': {
      const left = renderExpression(expr.left, bindings, options);
      const right = renderExpression(expr.right, bindings, options);
      rendered = `(${left} ${expr.op} ${right})`;
      break;
    }
    case 'array':
      rendered = `[${expr.elements.map((element) => renderExpression(element, bindings, options)).join(', ')}]`;
      break;
    case 'object':
      rendered = `({ ${expr.properties
        .map(({ key, value }) => `${key}: ${renderExpression(value, bindings, options)}`)
        .join(', ')} })`;
      break;
    case 'member': {
      const object = renderExpression(expr.object, bindings, options);
      if (expr.computed) {
        rendered = `(${object})[${renderProperty(expr.property, bindings, options)}]`;
      } else {
        rendered = `(${object}).${expr.property}`;
      }
      break;
    }
    case 'optional-member': {
      const object = renderExpression(expr.object, bindings, options);
      if (expr.computed) {
        rendered = `(${object})?.[${renderProperty(expr.property, bindings, options)}]`;
      } else {
        rendered = `(${object})?.${expr.property}`;
      }
      break;
    }
    case 'call': {
      const callee = renderCallCallee(expr.callee, bindings, options);
      const args = expr.args.map((arg) => renderExpression(arg, bindings, options)).join(', ');
      rendered = `${callee}(${args})`;
      break;
    }
    default:
      throw new TypeError(`Unsupported AST expression type: ${expr.type}`);
  }

  return maybeExtraParens(rendered, expr, extraParens);
}

function renderPrelude(bindings = DEFAULT_BINDINGS) {
  return [
    `const ${bindings.alpha} = 2;`,
    `const ${bindings.beta} = 3;`,
    `const ${bindings.fallback} = 5;`,
    `const ${bindings.maybeObject} = null;`,
    `const ${bindings.text} = "az";`,
    `const ${bindings.values} = [${bindings.alpha}, ${bindings.beta}, 4];`,
    `const ${bindings.box} = { value: ${bindings.beta}, nested: { leaf: 7 } };`,
    `function ${bindings.doubleFn}(value) {`,
    '  return value + value;',
    '}',
    `const ${bindings.toolbox} = {`,
    `  base: ${bindings.alpha},`,
    '  add: function (step) {',
    '    return this.base + step;',
    '  },',
    '};',
  ].join('\n');
}

function renderProgram(program, options = {}) {
  const bindings = options.bindings ?? DEFAULT_BINDINGS;
  const expressionOptions = {
    extraParens: options.extraParens ?? false,
  };
  const deadBranch = options.deadBranch
    ? [
        'if (false) {',
        '  console.log("dead");',
        '  probe(0);',
        '}',
      ].join('\n')
    : '';

  let body;
  switch (program.kind) {
    case 'pure':
      body = `${renderExpression(program.result, bindings, expressionOptions)};`;
      break;
    case 'console-trace':
      body = [
        `console.log(${renderExpression(program.first, bindings, expressionOptions)});`,
        `console.warn(${renderExpression(program.second, bindings, expressionOptions)});`,
        `${renderExpression(program.result, bindings, expressionOptions)};`,
      ].join('\n');
      break;
    case 'capability-trace':
      body = [
        `const first = probe(${renderExpression(program.first, bindings, expressionOptions)});`,
        `const second = probe(${renderExpression(program.second, bindings, expressionOptions)});`,
        `[first, second, ${renderExpression(program.result, bindings, expressionOptions)}];`,
      ].join('\n');
      break;
    case 'branch-trace':
      body = [
        'let branch;',
        `if (${renderExpression(program.test, bindings, expressionOptions)}) {`,
        `  const seen = probe(${renderExpression(program.consequent, bindings, expressionOptions)});`,
        '  console.log(seen);',
        '  branch = seen;',
        '} else {',
        `  const seen = ${renderExpression(program.alternate, bindings, expressionOptions)};`,
        '  console.error(seen);',
        '  branch = seen;',
        '}',
        `[branch, ${renderExpression(program.result, bindings, expressionOptions)}];`,
      ].join('\n');
      break;
    default:
      throw new TypeError(`Unsupported AST program kind: ${program.kind}`);
  }

  return ['"use strict";', renderPrelude(bindings), deadBranch, body]
    .filter(Boolean)
    .join('\n\n');
}

function collectExpressionFeatures(expr, features = new Set()) {
  switch (expr.type) {
    case 'literal':
      features.add('language.literals');
      break;
    case 'ref':
      features.add('language.identifiers');
      break;
    case 'unary':
      features.add('language.unary-operators');
      collectExpressionFeatures(expr.argument, features);
      break;
    case 'binary':
      features.add('language.binary-operators');
      collectExpressionFeatures(expr.left, features);
      collectExpressionFeatures(expr.right, features);
      break;
    case 'logical':
      features.add('language.logical-operators');
      collectExpressionFeatures(expr.left, features);
      collectExpressionFeatures(expr.right, features);
      break;
    case 'array':
      features.add('language.array-literals');
      expr.elements.forEach((element) => collectExpressionFeatures(element, features));
      break;
    case 'object':
      features.add('language.object-literals');
      expr.properties.forEach(({ value }) => collectExpressionFeatures(value, features));
      break;
    case 'member':
      features.add('language.member-access');
      collectExpressionFeatures(expr.object, features);
      if (expr.computed && typeof expr.property !== 'string') {
        collectExpressionFeatures(expr.property, features);
      }
      break;
    case 'optional-member':
      features.add('language.member-access');
      features.add('language.optional-chaining');
      collectExpressionFeatures(expr.object, features);
      if (expr.computed && typeof expr.property !== 'string') {
        collectExpressionFeatures(expr.property, features);
      }
      break;
    case 'call':
      features.add('language.function-calls');
      collectExpressionFeatures(expr.callee, features);
      expr.args.forEach((arg) => collectExpressionFeatures(arg, features));
      break;
    default:
      throw new TypeError(`Unsupported AST expression type: ${expr.type}`);
  }

  return features;
}

function collectProgramFeatures(program) {
  const features = new Set(['language.variable-declarations']);

  switch (program.kind) {
    case 'pure':
      collectExpressionFeatures(program.result, features);
      break;
    case 'console-trace':
      features.add('host.console-callbacks');
      collectExpressionFeatures(program.first, features);
      collectExpressionFeatures(program.second, features);
      collectExpressionFeatures(program.result, features);
      break;
    case 'capability-trace':
      features.add('host.sync-capabilities');
      collectExpressionFeatures(program.first, features);
      collectExpressionFeatures(program.second, features);
      collectExpressionFeatures(program.result, features);
      break;
    case 'branch-trace':
      features.add('language.if-statements');
      features.add('host.console-callbacks');
      features.add('host.sync-capabilities');
      collectExpressionFeatures(program.test, features);
      collectExpressionFeatures(program.consequent, features);
      collectExpressionFeatures(program.alternate, features);
      collectExpressionFeatures(program.result, features);
      break;
    default:
      throw new TypeError(`Unsupported AST program kind: ${program.kind}`);
  }

  return features;
}

function leafExpressions() {
  return [
    literal(undefined),
    literal(null),
    literal(false),
    literal(true),
    literal(-1),
    literal(0),
    literal(1),
    literal('a'),
    ref('alpha'),
    ref('beta'),
    ref('fallback'),
    ref('text'),
    ref('maybeObject'),
    member(ref('box'), 'value'),
    member(member(ref('box'), 'nested'), 'leaf'),
    member(ref('values'), literal(1), true),
    member(ref('values'), 'length'),
    optionalMember(ref('maybeObject'), 'value'),
    call(ref('doubleFn'), [ref('alpha')]),
    call(member(ref('toolbox'), 'add'), [ref('beta')]),
  ];
}

function leafExpressionArbitrary() {
  return fc.constantFrom(...leafExpressions());
}

function scalarExpressionArbitrary(maxDepth) {
  const leaf = leafExpressionArbitrary();

  if (maxDepth <= 0) {
    return leaf;
  }

  const next = scalarExpressionArbitrary(maxDepth - 1);
  return fc.oneof(
    leaf,
    fc.tuple(fc.constantFrom('!', '-', 'typeof'), next).map(([op, argument]) => unary(op, argument)),
    fc
      .tuple(next, fc.constantFrom('+', '-', '*', '/', '%', '===', '!==', '<', '<=', '>', '>='), next)
      .map(([left, op, right]) => binary(op, left, right)),
    fc.tuple(next, fc.constantFrom('&&', '||', '??'), next).map(([left, op, right]) => logical(op, left, right)),
  );
}

function expressionArbitrary(maxDepth, options = {}) {
  const { structured = true } = options;
  const scalar = scalarExpressionArbitrary(maxDepth);

  if (!structured) {
    return scalar;
  }

  if (maxDepth <= 0) {
    return scalar;
  }

  const next = expressionArbitrary(maxDepth - 1, options);
  return fc.oneof(
    scalar,
    fc.oneof(
      fc.tuple(next, next).map(([left, right]) => arrayLiteral([left, right])),
      fc
        .tuple(next, next)
        .map(([left, right]) => objectLiteral([{ key: 'value', value: left }, { key: 'other', value: right }])),
    ),
  );
}

function simpleExpressionArbitrary(maxDepth) {
  return scalarExpressionArbitrary(maxDepth);
}

function astProgramArbitrary() {
  return fc.oneof(
    expressionArbitrary(2).map((result) => pureProgram(result)),
    fc
      .tuple(simpleExpressionArbitrary(1), simpleExpressionArbitrary(1), expressionArbitrary(2))
      .map(([first, second, result]) => consoleTraceProgram(first, second, result)),
    fc
      .tuple(simpleExpressionArbitrary(1), simpleExpressionArbitrary(1), simpleExpressionArbitrary(1))
      .map(([first, second, result]) => capabilityTraceProgram(first, second, result)),
    fc
      .tuple(simpleExpressionArbitrary(1), simpleExpressionArbitrary(1), simpleExpressionArbitrary(1), expressionArbitrary(2))
      .map(([test, consequent, alternate, result]) => branchTraceProgram(test, consequent, alternate, result)),
  );
}

const EXHAUSTIVE_LEAVES = Object.freeze([
  literal(0),
  literal(1),
  ref('alpha'),
  ref('beta'),
  member(ref('box'), 'value'),
  optionalMember(ref('maybeObject'), 'value'),
  call(ref('doubleFn'), [literal(1)]),
]);

function dedupeExpressions(expressions) {
  const seen = new Map();
  for (const expression of expressions) {
    const key = renderExpression(expression);
    if (!seen.has(key)) {
      seen.set(key, expression);
    }
  }
  return [...seen.values()];
}

function enumerateExpressions(maxDepth) {
  if (maxDepth <= 0) {
    return EXHAUSTIVE_LEAVES.slice();
  }

  const previous = enumerateExpressions(maxDepth - 1);
  const restricted = previous.slice(0, 4);
  const current = [...previous];

  for (const expression of previous) {
    current.push(unary('!', expression));
    current.push(unary('-', expression));
    current.push(unary('typeof', expression));
  }

  for (const left of restricted) {
    for (const right of restricted) {
      current.push(binary('+', left, right));
      current.push(binary('===', left, right));
      current.push(binary('<', left, right));
      current.push(logical('&&', left, right));
      current.push(logical('??', left, right));
    }
  }

  for (const left of restricted.slice(0, 3)) {
    for (const right of restricted.slice(0, 3)) {
      current.push(arrayLiteral([left, right]));
      current.push(objectLiteral([{ key: 'value', value: left }, { key: 'other', value: right }]));
    }
  }

  return dedupeExpressions(current);
}

function enumerateExhaustivePrograms() {
  const expressions = enumerateExpressions(1);
  const traceExpressions = expressions.slice(0, 6);
  const branchTests = expressions.slice(0, 4);
  const branchValues = expressions.slice(2, 6);
  const programs = [];
  let index = 0;

  for (const expression of expressions) {
    programs.push({ id: `pure-${index++}`, program: pureProgram(expression) });
  }

  for (const first of traceExpressions) {
    for (const second of traceExpressions) {
      programs.push({
        id: `console-${index++}`,
        program: consoleTraceProgram(first, second, binary('+', literal(1), literal(2))),
      });
      programs.push({
        id: `capability-${index++}`,
        program: capabilityTraceProgram(first, second, member(ref('box'), 'value')),
      });
    }
  }

  for (const test of branchTests) {
    for (const consequent of branchValues) {
      for (const alternate of branchValues) {
        programs.push({
          id: `branch-${index++}`,
          program: branchTraceProgram(test, consequent, alternate, call(ref('doubleFn'), [ref('alpha')])),
        });
      }
    }
  }

  const seen = new Map();
  for (const entry of programs) {
    const source = renderProgram(entry.program);
    if (!seen.has(source)) {
      seen.set(source, entry);
    }
  }
  return [...seen.values()];
}

function metamorphicVariants(program) {
  return [
    {
      id: 'alpha-rename',
      featureId: 'metamorphic.alpha-renaming',
      source: renderProgram(program, { bindings: RENAMED_BINDINGS }),
    },
    {
      id: 'extra-parens',
      featureId: 'metamorphic.parenthesized-rendering',
      source: renderProgram(program, { extraParens: true }),
    },
    {
      id: 'dead-branch',
      featureId: 'metamorphic.dead-branch-insertion',
      source: renderProgram(program, { deadBranch: true }),
    },
  ];
}

function coveredFeatureIds() {
  const covered = new Set();

  for (const { program } of enumerateExhaustivePrograms()) {
    for (const featureId of collectProgramFeatures(program)) {
      covered.add(featureId);
    }
    for (const variant of metamorphicVariants(program)) {
      covered.add(variant.featureId);
    }
  }

  return covered;
}

function contractCoverageExpectations() {
  return new Set(
    [
      ...featuresForCoverage(COVERAGE.GENERATED_AST),
      ...featuresForCoverage(COVERAGE.EXHAUSTIVE_AST),
      ...featuresForCoverage(COVERAGE.TRACE_DIFFERENTIAL),
      ...featuresForCoverage(COVERAGE.METAMORPHIC),
    ].map((entry) => entry.id),
  );
}

module.exports = {
  AST_PROPERTY_RUNS,
  astProgramArbitrary,
  collectProgramFeatures,
  contractCoverageExpectations,
  coveredFeatureIds,
  enumerateExhaustivePrograms,
  metamorphicVariants,
  renderProgram,
};
