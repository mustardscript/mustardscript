'use strict';

const assert = require('node:assert/strict');

const { averageMetric } = require('./ptc-portfolio.ts');

function wrapIife(body) {
  return `(() => {\n${body}\n})()`;
}

function makeOperations(operationCount) {
  return Array.from({ length: operationCount }, (_, index) => ({
    path: `/v1/${index % 4 === 0 ? 'accounts' : 'users'}/${index}/actions/${index % 7}`,
    method: index % 2 === 0 ? 'GET' : 'POST',
    tagA: index % 2 === 0 ? 'billing' : 'identity',
    tagB: index % 3 === 0 ? 'search' : 'mutate',
    tagC: index % 5 === 0 ? 'enterprise' : 'self-serve',
    schemaWeight: (index % 11) + 1,
  }));
}

function createCodeModeSearchScenario(variantId, operationCount, returnStructured) {
  const operations = makeOperations(operationCount);

  return {
    familyId: 'code_mode_search',
    variantId,
    metricName: `sentinel_code_mode_search_${variantId}`,
    source: wrapIife(`
      const operations = ${JSON.stringify(operations)};
      const matches = [];
      let schemaTotal = 0;
      for (let index = 0; index < operations.length; index += 1) {
        const operation = operations[index];
        const isBilling = operation.tagA === 'billing';
        const isAccountPath = operation.path.indexOf('/accounts/') !== -1;
        const supportsSearch = operation.tagB === 'search';
        if (isBilling && isAccountPath && supportsSearch) {
          matches.push(operation);
          schemaTotal += operation.schemaWeight;
        }
      }
      if (${returnStructured ? 'true' : 'false'}) {
        return {
          count: matches.length,
          schemaTotal,
          topMatches: matches.slice(0, 12),
        };
      }
      return {
        count: matches.length,
        schemaTotal,
        top: matches.slice(0, 8).map((entry) => entry.method + ':' + entry.path),
      };
    `),
    inputs: {},
    shape: {
      toolFamilyCount: 0,
      logicalPeakFanout: 0,
      compactionExpectation: returnStructured ? 'lower_compaction' : 'moderate_compaction',
    },
    createCapabilities() {
      return {};
    },
    assertResult(result) {
      assert.equal(typeof result.count, 'number');
      assert.equal(typeof result.schemaTotal, 'number');
      if (returnStructured) {
        assert.ok(Array.isArray(result.topMatches));
        assert.equal(typeof result.topMatches[0]?.path, 'string');
        return;
      }
      assert.ok(Array.isArray(result.top));
      assert.equal(typeof result.top[0], 'string');
    },
  };
}

function createResultMaterializationScenario(variantId, projectionKind) {
  const records = Array.from({ length: 48 }, (_, index) => ({
    id: `row_${index}`,
    accountId: `acct_${index % 12}`,
    score: 100 - index,
    status: index % 4 === 0 ? 'watch' : 'clear',
    summary: `row ${index} summary`,
    tags: [`t${index % 3}`, `t${(index + 1) % 5}`],
  }));

  return {
    familyId: 'result_materialization',
    variantId,
    metricName: `sentinel_result_materialization_${variantId}`,
    source: wrapIife(`
      const rows = load_materialization_rows();
      let watched = 0;
      let scoreTotal = 0;
      for (let index = 0; index < rows.length; index += 1) {
        const row = rows[index];
        scoreTotal += row.score;
        if (row.status === 'watch') {
          watched += 1;
        }
      }
      if (${JSON.stringify(projectionKind)} === 'summary') {
        return {
          rowCount: rows.length,
          watched,
          scoreTotal,
        };
      }
      if (${JSON.stringify(projectionKind)} === 'structured') {
        return {
          rowCount: rows.length,
          watched,
          scoreTotal,
          topRows: rows.slice(0, 12),
        };
      }
      return {
        rowCount: rows.length,
        watched,
        scoreTotal,
        allRows: rows,
      };
    `),
    inputs: {},
    shape: {
      toolFamilyCount: 1,
      logicalPeakFanout: 1,
      compactionExpectation:
        projectionKind === 'summary' ? 'moderate_compaction' : 'lower_compaction',
    },
    createCapabilities() {
      return {
        load_materialization_rows() {
          return records;
        },
      };
    },
    assertResult(result) {
      assert.equal(result.rowCount, records.length);
      assert.equal(typeof result.scoreTotal, 'number');
      if (projectionKind === 'summary') {
        assert.equal(Object.prototype.hasOwnProperty.call(result, 'allRows'), false);
        return;
      }
      if (projectionKind === 'structured') {
        assert.equal(result.topRows.length, 12);
        return;
      }
      assert.equal(result.allRows.length, records.length);
    },
  };
}

function createLowCompactionFanoutScenario(variantId, detailLevel) {
  const customers = Array.from({ length: 18 }, (_, index) => ({
    id: `cust_${index}`,
    region: index % 2 === 0 ? 'us' : 'eu',
    segment: index % 3 === 0 ? 'enterprise' : 'mid_market',
  }));
  const orders = Array.from({ length: 96 }, (_, index) => ({
    orderId: `ord_${index}`,
    customerId: `cust_${index % customers.length}`,
    amount: 120 + (index % 17) * 13,
    disputed: index % 7 === 0,
    tags: [`tag_${index % 4}`, `tag_${(index + 1) % 6}`],
  }));

  return {
    familyId: 'low_compaction_fanout',
    variantId,
    metricName: `sentinel_low_compaction_fanout_${variantId}`,
    source: wrapIife(`
      async function run() {
        const [customerRows, orderRows] = await Promise.all([
          list_sentinel_customers(),
          list_sentinel_orders(),
        ]);
        const customerById = new Map();
        for (const customer of customerRows) {
          customerById.set(customer.id, customer);
        }
        const summaries = [];
        for (const order of orderRows) {
          const customer = customerById.get(order.customerId);
          summaries.push({
            orderId: order.orderId,
            amount: order.amount,
            disputed: order.disputed,
            region: customer.region,
            segment: customer.segment,
            tags: order.tags,
          });
        }
        if (${JSON.stringify(detailLevel)} === 'high_compaction') {
          return {
            orderCount: summaries.length,
            disputedCount: summaries.filter((entry) => entry.disputed).length,
            topRegions: summaries.slice(0, 8).map((entry) => entry.region),
          };
        }
        if (${JSON.stringify(detailLevel)} === 'moderate_compaction') {
          return {
            orderCount: summaries.length,
            disputedCount: summaries.filter((entry) => entry.disputed).length,
            sample: summaries.slice(0, 18),
          };
        }
        return {
          orderCount: summaries.length,
          disputedCount: summaries.filter((entry) => entry.disputed).length,
          sample: summaries,
        };
      }
      return run();
    `),
    inputs: {},
    shape: {
      toolFamilyCount: 2,
      logicalPeakFanout: 2,
      compactionExpectation:
        detailLevel === 'high_compaction' ? 'moderate_compaction' : 'lower_compaction',
    },
    createCapabilities() {
      return {
        list_sentinel_customers() {
          return customers;
        },
        list_sentinel_orders() {
          return orders;
        },
      };
    },
    assertResult(result) {
      assert.equal(result.orderCount, orders.length);
      assert.equal(typeof result.disputedCount, 'number');
      if (detailLevel === 'high_compaction') {
        assert.equal(result.topRegions.length, 8);
        return;
      }
      if (detailLevel === 'moderate_compaction') {
        assert.equal(result.sample.length, 18);
        return;
      }
      assert.equal(result.sample.length, orders.length);
    },
  };
}

function createSentinelScenarios() {
  return Object.freeze({
    code_mode_search: Object.freeze({
      medium_compact: createCodeModeSearchScenario('medium_compact', 200, false),
      large_compact: createCodeModeSearchScenario('large_compact', 600, false),
      large_structured: createCodeModeSearchScenario('large_structured', 600, true),
    }),
    result_materialization: Object.freeze({
      medium_summary: createResultMaterializationScenario('medium_summary', 'summary'),
      medium_structured: createResultMaterializationScenario('medium_structured', 'structured'),
      medium_expanded: createResultMaterializationScenario('medium_expanded', 'expanded'),
    }),
    low_compaction_fanout: Object.freeze({
      medium_high_compaction: createLowCompactionFanoutScenario(
        'medium_high_compaction',
        'high_compaction',
      ),
      medium_moderate_compaction: createLowCompactionFanoutScenario(
        'medium_moderate_compaction',
        'moderate_compaction',
      ),
      medium_low_compaction: createLowCompactionFanoutScenario(
        'medium_low_compaction',
        'low_compaction',
      ),
    }),
  });
}

function summarizeSentinelFamilyScores(latencyByName, familyScenarios) {
  return Object.fromEntries(
    Object.entries(familyScenarios).map(([familyId, variants]) => [
      familyId,
      averageMetric(
        latencyByName,
        Object.values(variants).map((scenario) => scenario.metricName),
      ),
    ]),
  );
}

module.exports = {
  createSentinelScenarios,
  summarizeSentinelFamilyScores,
};
