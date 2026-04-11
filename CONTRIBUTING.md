# Contributing

`jslite` is a small runtime with an intentionally narrow language contract.

## Development Rules

- Keep semantics in the Rust core.
- Keep the Node wrapper thin.
- Do not widen the language surface accidentally.
- Add tests with each feature.
- Prefer explicit rejections over partial compatibility.

## Local Development

```sh
cargo test --workspace
npm install
npm test
```

## Change Expectations

- Security-sensitive changes need tests.
- Host-boundary changes must update `docs/HOST_API.md`.
- Language-surface changes must update `docs/LANGUAGE.md`.
- Snapshot or sidecar changes must update `docs/SERIALIZATION.md`.
