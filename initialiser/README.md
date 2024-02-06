# Initialiser

This crate is responsible for initialising a newly booted VM.

It:

- Sets up the network interface, so the VM can communicate with the outside world.
- Sets up the persisted Cache disk, so we can persist packages/docker images between runs.
- Sets up a runner systemd service, so we can run the GitHub Action runner after the boot process is complete.

We don't start the runner from this init script, but we set it as a new (one-shot) service. This becase we want to only start the GitHub Action runner after other processes have finished booting (e.g. Docker).

The service has a `ExecStopPost` command that will reboot the VM after the runner has finished running the GitHub Action. (this signals to Firecracker to stop the VM and boot a fresh one).

You might notice we only have one binary file and it performs several jobs.
The user-facing jobs are separated into different subcommands, (`actions-runner build`, `actions-runner run`), but the internal jobs rely on the name of the binary.

To start this initialiser, make sure it's renamed (or copied) from `actions-runner` to `actions-init`.

The same goes for the GitHub Actions runner process. This initialiser copies itself into `/sbin/actions-run`.
