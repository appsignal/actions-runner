# Building the root image

The root image is a raw ext4 filesystem image that serves as the base for every VM that the runner manager boots. It is provisioned once onto an LVM thin logical volume and then snapshotted per slot at boot time — it is never written to directly after provisioning.

Image building is handled entirely by Ansible and lives outside this repository. This document describes what the image must contain, what it must not contain, and how to produce and deploy it.

## What the image must contain

### Base system

A minimal Linux installation. Ubuntu 22.04 LTS is the recommended base because the GitHub Actions hosted runner tooling targets it. Strip out anything not required to run a CI job: documentation, manpages, recommended packages, and optional locale data add size without value.

The image must use systemd as its init system. The runner manager injects a `runner.service` unit into each VM's rootfs copy before boot and relies on systemd to start it.

### The `runner` user

The GitHub Actions runner process runs as an unprivileged `runner` user. Create it consistently:

```
useradd --uid 1001 --gid 1001 --home /home/runner --shell /bin/bash runner
```

The UID and GID must be fixed. The runner binary and its working directory must be owned by this user.

### GitHub Actions runner

Install the GitHub Actions runner at `/home/runner`. The runner version should match or exceed the minimum version required by your organisation's workflows. The installation must be done as the `runner` user or with correct ownership set afterwards.

Do not run `./config.sh` during image build. Configuration (URL, token, name, labels) is injected at boot time via the `runner.service` unit written by the runner manager.

### Cache mount service

The VM's cache disk appears as `/dev/vdb` at boot. A oneshot systemd service must mount it before the runner starts:

```ini
[Unit]
Description=Mount cache disk
Before=runner.service
DefaultDependencies=no

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/bin/mount -t ext4 /dev/vdb /cache
ExecStartPost=/bin/chmod 777 /cache

[Install]
WantedBy=multi-user.target
```

Enable this service in the image. The `/cache` directory must exist.

### DNS resolution

Write a static `/etc/resolv.conf`. The VMs do not use DHCP or systemd-resolved:

```
nameserver 1.1.1.1
options use-vc
```

### Reboot on runner exit

The `runner.service` unit is written at boot time by the manager, but the `ExecStopPost` line that triggers a reboot when the job finishes must be understood when authoring that unit. The reboot causes Firecracker to exit, which the manager detects as the signal to prepare and boot a fresh slot. This is not part of the image itself but is worth noting here as context for how the lifecycle works.

## What the image must NOT contain

- **SSH daemon** — VMs are ephemeral and not accessed interactively. Including sshd is unnecessary attack surface.
- **Cloud-init** — adds boot time and complexity. All configuration is injected by the manager before the VM starts.
- **Swap** — Firecracker VMs have a fixed memory allocation. Swap on the rootfs is not useful.
- **The runner manager binary** — the `actions-init` / `actions-run` binary used in the previous architecture is no longer needed. The image should not contain it.
- **Preconfigured runner.service** — the service unit is written fresh by the manager on every boot cycle with a new registration token and runner name. A stale unit in the image would be overwritten anyway, but do not include one.

## Filesystem requirements

The image must be a raw ext4 filesystem image — not qcow2, not a partition inside a disk image. Firecracker takes a path to a file or block device and expects to find a filesystem directly.

Size: aim for the smallest image that can hold the runner and its dependencies. A typical Ubuntu 22.04 base plus the Actions runner lands around 3-4 GB of used space. The image file itself can be larger (allocated but unused space is sparse), but the used extent is what gets copy-on-write snapshotted and that size determines how much each running VM diverges from the base.

## Building the image with Ansible

The recommended approach is to use Ansible to build a Docker container with all required software, export its filesystem, and write it into a raw ext4 image. The broad steps are:

1. Build a Docker image from a Dockerfile that installs the base system, runner user, Actions runner, and systemd units.
2. Create a container from the image (`docker create`), export its filesystem (`docker export`), and pipe it into the raw image via a loop mount.
3. Run `mkfs.ext4` on the raw file, mount it, extract the tarball into it, unmount.

Alternatively, use `debootstrap` directly in the Ansible playbook against a chroot for a cleaner result without requiring Docker on the build machine.

## Deploying the image to an LVM base volume

Once built, the image is written to the LVM base logical volume for its role. The runner manager reads the path from the config and handles provisioning (writing the image to the LV and snapshotting it per slot), but the image file must be placed where the config points before starting the manager.

Write it with `dd` for a straightforward raw copy:

```sh
dd if=ubuntu-22.04-runner.img of=/dev/runners/base-default bs=4M status=progress
```

If the image is updated (new runner version, updated packages), replace the file and restart the manager. On startup the manager checksums the image against what is stored as an LVM tag on the base LV and reprovisions if they differ.

## Verifying the image

Before deploying, test that the image boots correctly in Firecracker with a minimal config. Verify:

- The VM reaches the systemd multi-user target without errors.
- The `cache-mount.service` starts without a `/dev/vdb` present (it should fail gracefully or be guarded with `ConditionPathExists`).
- The `/home/runner` directory is owned by the `runner` user.
- There are no units that stall boot waiting for network or hardware that is not present in a Firecracker VM (e.g. cloud-init, `systemd-udevd` waiting for devices).
