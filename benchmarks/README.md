# Smoke Benchmarks

These benchmarks are intentionally small and repeatable. They are meant to give
maintainers a quick signal on regressions in:

- startup and compile-and-run latency
- steady-state synchronous execution cost
- suspension and resume overhead
- retained Node heap after repeated guest runs

Run them with:

```sh
npm run bench:smoke
```

The thresholds live in `budgets.json` and are intentionally broad enough for a
source-build development workflow while still catching major regressions.
