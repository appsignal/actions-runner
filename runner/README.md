# Runner

This crate is responsible for running the GitHub Actions Runner.

It's copied by the initialiser into `/sbin/actions-run` and started as a systemd service.

It relies on a number of environment variables, set by the initialiser, to authenticate with GitHub and start the runner.
