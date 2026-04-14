'use strict';

export const benchmarkData = {
  "sourceArtifact": "2026-04-14T01-36-43-009Z-workloads.json",
  "benchmarkKind": "ptc_website_demo_small",
  "machine": {
    "cpuModel": "Apple M4",
    "nodeVersion": "v24.12.0",
    "platform": "darwin 25.2.0"
  },
  "addon": {
    "medianMs": 0.155,
    "p95Ms": 0.164
  },
  "note": "Representative 4-tool orchestration workflow derived from the audited programmatic tool-call gallery."
} as const;
