# Runner

Responsible for running the firecracker instances.

## Usage

```bash
actions-runner run --config /path/to/config.toml
```

## Debug a role

`--debug_role` automatically sets the log level to `debug`.

```bash
actions-runner run --config /path/to/config.toml --debug_role <role>
```
