'use strict';

function clampOffset(source, offset) {
  if (!Number.isFinite(offset)) {
    return 0;
  }
  return Math.max(0, Math.min(source.length, offset));
}

function offsetToLineColumn(source, offset) {
  const clamped = clampOffset(source, offset);
  let line = 1;
  let column = 1;
  for (let index = 0; index < clamped; index += 1) {
    if (source.charCodeAt(index) === 10) {
      line += 1;
      column = 1;
    } else {
      column += 1;
    }
  }
  return { line, column };
}

function totalCollectionCalls(site) {
  return (
    (site.map_get_calls ?? 0) +
    (site.map_set_calls ?? 0) +
    (site.set_add_calls ?? 0) +
    (site.set_has_calls ?? 0)
  );
}

function sourceSnippetForSpan(source, span) {
  if (!span || typeof span.start !== 'number' || typeof span.end !== 'number') {
    return '';
  }
  const start = clampOffset(source, span.start);
  const end = clampOffset(source, span.end);
  const lineStart = source.lastIndexOf('\n', Math.max(0, start - 1)) + 1;
  const nextNewline = source.indexOf('\n', end);
  const lineEnd = nextNewline === -1 ? source.length : nextNewline;
  return source.slice(lineStart, lineEnd).trim();
}

function compareHotspots(left, right) {
  return (
    totalCollectionCalls(right) - totalCollectionCalls(left) ||
    ((right.map_get_calls ?? 0) - (left.map_get_calls ?? 0)) ||
    ((right.map_set_calls ?? 0) - (left.map_set_calls ?? 0)) ||
    ((right.set_add_calls ?? 0) - (left.set_add_calls ?? 0)) ||
    ((right.set_has_calls ?? 0) - (left.set_has_calls ?? 0)) ||
    ((left.span?.start ?? 0) - (right.span?.start ?? 0)) ||
    ((left.instruction_offset ?? 0) - (right.instruction_offset ?? 0))
  );
}

function annotateCollectionCallSites(metrics, scenario, limit = 8) {
  if (
    !metrics ||
    !Array.isArray(metrics.collection_call_sites) ||
    metrics.collection_call_sites.length === 0
  ) {
    return metrics;
  }

  const source = typeof scenario?.source === 'string' ? scenario.source : '';
  const sourceFile = scenario?.sourceFile ?? null;
  const collection_hotspots = metrics.collection_call_sites
    .map((site) => {
      const start = offsetToLineColumn(source, site.span?.start ?? 0);
      const end = offsetToLineColumn(source, site.span?.end ?? 0);
      return {
        ...site,
        total_calls: totalCollectionCalls(site),
        source_file: sourceFile,
        start_line: start.line,
        start_column: start.column,
        end_line: end.line,
        end_column: end.column,
        snippet: sourceSnippetForSpan(source, site.span),
      };
    })
    .filter((site) => site.total_calls > 0)
    .sort(compareHotspots)
    .slice(0, limit);

  return {
    ...metrics,
    collection_hotspots,
  };
}

module.exports = {
  annotateCollectionCallSites,
  offsetToLineColumn,
  totalCollectionCalls,
};
