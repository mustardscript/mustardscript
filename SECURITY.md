# Security Policy

## Scope

Security issues include:

- Guest escapes from the intended language boundary
- Host value validation failures that permit unsafe object transfer
- Snapshot or sidecar deserialization bugs
- Limit enforcement bugs that break documented containment guarantees

## Important Notes

- Addon mode is not a hard isolation boundary.
- Sidecar mode is stronger, but hostile deployments still require OS-level controls.

## Reporting

Report security issues privately to the maintainers before public disclosure.
