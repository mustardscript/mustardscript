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

Report vulnerabilities through GitHub Security Advisories:

- https://github.com/keppoai/jslite/security/advisories/new

Do not open public GitHub issues for unpatched security reports.

## Supported Versions

- The latest published npm release
- `main` before the next release is cut

## Response Expectations

- Initial triage acknowledgment target: 5 business days
- Coordinated disclosure and fix timing will be handled through the advisory
  thread
