# Project Rules

These rules are mandatory for all changes in this repository.

## No Compatibility Shims

- Do not add compatibility shims, alias layers, transitional wrappers, or re-export bridges to preserve old paths.
- When ownership moves, update all call sites to the new location directly.
- Remove the old module/file after references are migrated.

## No Dead Code

- Do not introduce dead code.
- Do not silence dead code with `#[allow(dead_code)]`.
- Remove unused symbols, modules, and stale route files instead of keeping placeholders.
- If a symbol is intentionally feature-gated, ensure it is referenced by the relevant feature path and remains warning-free under enabled feature sets.

## Migration Standard

- Migrations must end in a clean state within the same change set:
  - no dual ownership,
  - no duplicate route registration,
  - no stale modules left behind,
  - and `cargo check --features "api-v2 api-legacy at-interface" --all-targets` must pass.

## Strict Semantics

- Prefer strict typed request and response semantics across service and route boundaries.
- Do not introduce or extend `serde_json::Value`-shaped service APIs when a concrete typed model is known.
- In route handlers, avoid `serde_json::from_value` and `serde_json::to_value` as an adapter between internal code and published API types unless there is no practical typed boundary yet.
- If a route publishes a concrete API schema, prefer a concrete Rust type all the way down to the owning service or add an explicit typed conversion function instead of round-tripping through `Value`.

## OpenAPI Derivation

- OpenAPI must be derived from the same concrete request and response types that implement the runtime contract.
- Do not document endpoints with looser schema placeholders when the route already has a specific semantic type.
- When tightening semantics, update the concrete Rust type first and let OpenAPI derivation follow from that type instead of maintaining a separate documentation shape.
