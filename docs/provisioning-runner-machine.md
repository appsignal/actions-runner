# Provisioning a runner machine

This document covers everything required to prepare a physical machine to run the actions runner manager. The Ansible playbook that implements these steps should be run once. All subsequent operational changes — adding or removing worker slots, changing CPU or memory allocations, switching to a different root image — are handled by the runner manager binary without re-running Ansible.

## Hardware requirements

- **CPU**: x86_64 with hardware virtualisation (VMX). Verify with `grep -c vmx /proc/cpuinfo`. Firecracker requires it.
- **Memory**: at minimum 512 MB per VM slot plus overhead for the host OS. A machine running 8 slots at 4 GB each needs at least 34 GB.
- **Disks**: one or more block devices dedicated to the LVM volume group. The manager does not care how many disks there are or how they are arranged — LVM handles that. SSDs are strongly recommended; VM boot time is dominated by the rootfs snapshot copy and cache disk setup.

## Operating system

Ubuntu 22.04 LTS or Rocky Linux 8+ on the host. The kernel must be 5.10 or later (required by Firecracker). Verify:

```sh
uname -r
```

The host does not need to be the same distribution as the VM images.

## Required packages

Install via Ansible:

```
lvm2
firecracker
iptables
iproute2
```

The `firecracker` binary must be in `$PATH` as the manager calls it by name. Download the appropriate release from the Firecracker GitHub releases and install it to `/usr/local/bin/firecracker`.

The Linux kernel image for the VMs is a separate artifact from the root filesystem image. Place it at a stable path — the config file references it per role. A typical location is `/var/lib/runners/kernels/vmlinux-5.10`.

## LVM setup

This is the one step Ansible must perform on the machine. The manager takes over from here.

Identify all disks intended for the runner pool. Do not include the OS disk:

```sh
lsblk
```

Create physical volumes from each disk, a single volume group across all of them, and a thin pool logical volume. Adjust the pool size to match available space, leaving some headroom for the OS and any non-pool use:

```sh
pvcreate /dev/sdb /dev/sdc /dev/sdd
vgcreate runners /dev/sdb /dev/sdc /dev/sdd
lvcreate -L 200G --thinpool pool runners
```

The VG name (`runners`) and pool name (`pool`) must match the `[lvm]` section in the manager config file. That is the only connection between Ansible and the manager — the names must agree.

Verify the pool is ready:

```sh
lvs -a runners
```

Do not create any other logical volumes. The manager creates and removes all LVs inside the pool as part of normal operation.

## Network interface

The manager uses NAT to give VMs outbound internet access through one of the host's network interfaces. This should be the interface with the default route — typically the primary ethernet interface.

Find it:

```sh
ip route show default
```

This interface name goes into the `network_interface` field in the config. The manager sets up iptables rules against it at startup. No manual iptables configuration is needed.

The manager creates TAP devices (`tap1`, `tap2`, ...) for each VM slot and assigns them addresses in the `172.16.{slot-index}.0/30` range. These do not need to be pre-created.

Ensure IP forwarding is not disabled by a system policy. The manager enables it at runtime, but if a firewall or security tool resets it the VMs will lose outbound connectivity.

## Config file

The manager reads a single TOML config file. Place it at a stable path, typically `/etc/actions-runner/config.toml`:

```toml
network_interface = "eth0"        # host interface used for NAT
run_path = "/srv/runners"         # working directory for per-slot state files
github_org = "your-org"
github_pat = "ghp_xxxxxxxxxxxx"   # personal access token with manage_runners:org scope

[lvm]
volume_group = "runners"          # must match the vgcreate name
thin_pool = "pool"                # must match the lvcreate --thinpool name

[[roles]]
name = "default"
rootfs_image = "/var/lib/runners/images/ubuntu-22.04-runner.img"
kernel_image = "/var/lib/runners/kernels/vmlinux-5.10"
cpus = 2
memory_size = 4       # GiB per VM
cache_size = 20       # GiB per VM cache disk
instance_count = 4
labels = ["ubuntu-22.04"]

[[roles]]
name = "large"
rootfs_image = "/var/lib/runners/images/ubuntu-22.04-runner.img"
kernel_image = "/var/lib/runners/kernels/vmlinux-5.10"
cpus = 8
memory_size = 16
cache_size = 40
instance_count = 2
labels = ["ubuntu-22.04", "large"]
```

Multiple roles can share the same rootfs image. Each role gets its own LVM base logical volume. `instance_count` controls how many parallel VM slots run for that role.

### Changing the config

The manager reconciles config against running state on startup. To apply a change:

- **Add or remove slots** (`instance_count`): update the value and restart the manager. It creates or removes LVM snapshots to match.
- **Change CPUs or memory**: update the values and restart. No LVM changes needed; these are Firecracker parameters.
- **Switch to a new root image**: update `rootfs_image` and restart. The manager checksums the image against the provisioned base LV and reprovisions if they differ, then recreates all snapshots for that role.
- **Add or remove a role**: add or remove the `[[roles]]` block and restart. The manager creates or removes all associated LVM volumes.

Re-running Ansible is not required for any of these changes.

### GitHub PAT requirements

The PAT must have the `manage_runners:org` scope. The manager calls the GitHub API to obtain a short-lived registration token before each VM boot — the PAT itself is never passed into the VM.

## Running the manager

Run the manager as root (required for TAP device creation, iptables rules, and LVM operations). A systemd service unit:

```ini
[Unit]
Description=Actions Runner Manager
After=network.target lvm2-monitor.service

[Service]
Type=simple
ExecStart=/usr/local/bin/actions-runner run --config /etc/actions-runner/config.toml
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start it:

```sh
systemctl enable --now actions-runner
```

On first start the manager will:

1. Set up iptables forwarding rules on the configured network interface.
2. Create LVM base logical volumes for each role and provision the root images onto them.
3. Create an empty formatted ext4 LV as the cache baseline.
4. Create TAP devices and snapshot LVs for each slot.
5. Begin booting Firecracker VMs.

Subsequent starts skip provisioning for any base LV whose image checksum matches what was previously recorded.

## Directory layout

```
/etc/actions-runner/
    config.toml

/var/lib/runners/
    images/
        ubuntu-22.04-runner.img   # root filesystem images (built by Ansible, separate repo)
    kernels/
        vmlinux-5.10              # Firecracker-compatible kernel

/srv/runners/                     # run_path from config; per-slot working dirs (config.json etc.)
```

The LVM volume group and its logical volumes are managed entirely by the manager at runtime and do not correspond to paths on disk other than their `/dev/runners/` device nodes.
