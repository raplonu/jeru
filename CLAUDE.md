# jeru

## Versioning

Every commit must bump the `version` field in `Cargo.toml`. jeru is pre-1.0 (major stays at `0`), so:

- **patch** (`0.y.Z`): bug fixes, new features, and other backwards-compatible changes
- **minor** (`0.Y.0`): breaking changes

`jeru --version` reports this version (via clap's `version` attribute, sourced from `Cargo.toml`).

## Testing

Write unit tests for new code whenever practical. Prefer tests close to the code (inline `#[cfg(test)]` modules).

## Finishing a task

Always end a task by running:

```
cargo test
cargo clippy
```

Fix any warnings or errors before reporting the task as done.
