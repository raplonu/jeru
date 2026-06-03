# jeru

## Testing

Write unit tests for new code whenever practical. Prefer tests close to the code (inline `#[cfg(test)]` modules).

## Finishing a task

Always end a task by running:

```
cargo test
cargo clippy
```

Fix any warnings or errors before reporting the task as done.
