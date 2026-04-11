# Limits

## Public Limits

The runtime exposes:

- instruction budget
- heap byte budget
- allocation count budget
- call-depth budget
- maximum outstanding host calls
- cancellation control

## Default Policy

- Limits are enabled by default.
- Cancellation is cooperative and checked at defined execution points.
- Over-budget execution fails with guest-safe runtime errors.
