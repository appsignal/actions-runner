# Actions Runner — Refactoring Spec

This document captures all architectural decisions made during the design review. Use it to implement the refactored runner from scratch in a new session.

---

## What this binary does

Manages a fleet of ephemeral Firecracker VMs on a single Linux host. Each VM runs one GitHub Actions job and is then discarded. The manager keeps N slots per role continuously cycling: snapshot base image → inject config → boot VM → wait for job to finish → repeat.

---

## What is being removed

### Crates deleted entirely

| Crate | Reason |
|---|---|
| `builder/` | Image building moves to Ansible (separate repo). The crate is orchestrating `docker`, `qemu-img`, `mkfs.ext4` — shell work, not Rust. |
| `initialiser/` | VM first-boot setup moves into the image itself (cache mount service) and into the manager (config injection before boot). |
| `runner/` | Just two shell script invocations (`config.sh`, `run.sh`). Becomes a shell script baked into the rootfs image by Ansible. |

### Binary modes removed

The argv-based dispatch in `bin/src/main.rs` (`actions-init`, `actions-run`) is removed entirely. The binary has one job: `run --config <file>`.

### Patterns removed

- `copy_sparse` for rootfs — replaced by LVM thin snapshots
- `Disk::setup_ext4()` (`dd` + `mkfs.ext4`) — replaced by LVM thin snapshots of a pre-formatted base LV
- `init=/sbin/actions-init` from boot args — standard systemd init is used
- `lazy_static!` — `Mutex::new()` is `const` since Rust 1.63, use `pub static MTX: Mutex<()> = Mutex::new(());`
- `cfg-if` macros — replace with plain `#[cfg(...)]` attribute blocks
- `signal-hook` in `manager/Cargo.toml` — replace with `nix` signals (add `signal` feature) or handle via the per-slot process model

---

## What stays, what changes

### `util/` — cleanup only

- Remove `lazy_static` dependency and `lazy_static!` block; replace with `pub static MTX: Mutex<()> = Mutex::new(());`
- Remove `cfg-if` dependency; replace both `cfg_if!` blocks in `lib.rs` with `#[cfg(...)]` attribute blocks
- `mount.rs` — keep as-is for now (used for config injection before boot, see below); optionally replace `mount`/`umount` subprocess calls with `nix::mount::mount()` and `nix::mount::umount2()` since the `mount` feature is already enabled on `nix`
- All other logic unchanged

### `github/` — unchanged

### `config/` — updated

- Remove `DEFAULT_BOOT_ARGS` constant or update it: drop `init=/sbin/actions-init`, keep `random.trust_cpu=on reboot=k panic=1 pci=off`
- Add `LvmConfig` struct (see config format below)
- `Role` struct: remove `overlay_size` field (unused with LVM); keep all others
- `ManagerConfig`: add `lvm: LvmConfig` field

### `manager/` — heavily refactored

See per-section detail below.

### `bin/` — simplified

`main.rs` becomes ~40 lines: one `run` subcommand, no argv dispatch, no `init`/`run` functions.

---

## New config format

```toml
network_interface = "eth0"          # host interface used for NAT/forwarding
run_path = "/srv/runners"           # working dir for per-slot state (config.json, pid file)
github_org = "your-org"
github_pat = "ghp_xxxxxxxxxxxx"     # manage_runners:org scope

[lvm]
volume_group = "runners"            # must match the VG Ansible created
thin_pool = "pool"                  # must match the thin pool LV name

[[roles]]
name = "default"
rootfs_image = "/var/lib/runners/images/ubuntu-22.04-runner.img"
kernel_image = "/var/lib/runners/kernels/vmlinux-5.10"
kernel_cmdline = ""                 # optional extra kernel args
cpus = 2
memory_size = 4                     # GiB
cache_size = 20                     # GiB
instance_count = 4
cache_paths = ["cargo:/home/runner/.cargo/registry"]
max_cache_pct = 90
labels = ["ubuntu-22.04"]
```

Corresponding Rust structs in `config/src/manager.rs`:

```rust
pub struct LvmConfig {
    pub volume_group: String,
    pub thin_pool: String,
}

pub struct ManagerConfig {
    pub network_interface: String,
    pub run_path: Utf8PathBuf,
    pub github_org: String,
    pub github_pat: String,
    pub lvm: LvmConfig,
    pub roles: Vec<Role>,
}

pub struct Role {
    pub name: String,
    pub rootfs_image: Utf8PathBuf,
    pub kernel_image: Utf8PathBuf,
    pub kernel_cmdline: Option<String>,
    pub cpus: u32,
    pub memory_size: u32,   // GiB
    pub cache_size: u32,    // GiB
    pub instance_count: u8,
    pub cache_paths: Vec<String>,   // "label:guest_path" pairs
    pub max_cache_pct: u8,          // default 90
    pub labels: Vec<String>,
}
```

---

## LVM management

### What Ansible does (once, never again)

```sh
pvcreate /dev/sdb /dev/sdc ...     # all non-OS disks
vgcreate runners /dev/sdb /dev/sdc ...
lvcreate -L 200G --thinpool pool runners
```

The VG name and pool name must match `config.toml`. That is the only coupling between Ansible and this binary.

### What the binary manages

The binary owns all LVs inside the pool. On startup it reconciles desired state (config) against actual state (existing LVs).

**Named LVs the binary manages:**

| LV name | Purpose | Created when |
|---|---|---|
| `base-{role-slug}` | Rootfs base for a role. Image provisioned onto it. | Role first seen, or image checksum changed |
| `cache-empty` | Pre-formatted empty ext4 LV. Never written to. | Once, on first run |
| `rootfs-{role}-{idx}` | Thin snapshot of `base-{role}` per slot. | Each boot cycle |
| `cache-{role}-{idx}` | Thin snapshot of `cache-empty` per slot. | Each boot cycle; also on cache clear |

**LVM tagging for change detection:**

When provisioning a base LV, tag it with the image checksum:

```sh
lvchange --addtag checksum=<sha256_of_image_file> runners/base-default
```

On startup, read tags via `lvs -o lv_name,lv_tags runners` and compare against the checksum of the current image file. If different, reprovision and recreate all slot LVs for that role.

**LVM operations the binary runs:**

```sh
# Create thin LV (for base-* and cache-empty)
lvcreate -V {size}G --thin -n {name} {vg}/{pool}

# Tag an LV
lvchange --addtag checksum={sha256} {vg}/{lv}

# Provision image onto base LV
dd if={image_path} of=/dev/{vg}/{lv} bs=4M

# Format cache-empty
mkfs.ext4 /dev/{vg}/cache-empty

# Create thin snapshot (per boot cycle)
lvcreate -s {vg}/base-{role} -n rootfs-{role}-{idx}
lvcreate -s {vg}/cache-empty -n cache-{role}-{idx}

# Remove LV
lvremove -y {vg}/{lv}

# List all LVs with tags
lvs --noheadings -o lv_name,lv_tags {vg}
```

**Reconciliation on startup:**

```
for each role in config:
    if base-{role} LV does not exist:
        create thin LV of size role.rootfs_image file size + 20%
        dd image file onto it
        tag with checksum
    else if checksum tag != sha256(role.rootfs_image):
        lvremove base-{role}
        recreate and reprovision as above
        lvremove all rootfs-{role}-* LVs (slots will recreate on next boot)

if cache-empty LV does not exist:
    create thin LV of 1G
    mkfs.ext4 it

for each role in config:
    desired slot LVs: rootfs-{role}-0 through rootfs-{role}-{N-1}
    existing slot LVs: query lvs
    create missing ones, remove extras

for each role NOT in config that has base-{role} LV:
    lvremove base-{role}
    lvremove all rootfs-{role}-* and cache-{role}-* LVs
```

---

## Config injection before boot

Before each Firecracker boot, the manager mounts the slot's rootfs LV, writes two files into it, and unmounts. This replaces `actions-init`.

The mount point is a temporary directory under `run_path`.

**File 1: `/etc/systemd/system/runner.service`**

```ini
[Unit]
Description=Actions Runner
After=network.target cache-mount.service

[Service]
ExecStart=/bin/bash -c '/home/runner/config.sh \
  --url https://github.com/{github_org} \
  --token {github_token} \
  --unattended --ephemeral \
  --name {runner_name} \
  --labels {labels} \
  && /home/runner/run.sh'
KillMode=control-group
KillSignal=SIGTERM
TimeoutStopSec=5min
WorkingDirectory=/home/runner
User=runner
Restart=never
ExecStopPost=+/usr/sbin/reboot
```

Values substituted at injection time:
- `{github_org}` — from config
- `{github_token}` — fresh registration token from GitHub API (`github.registration_token()`)
- `{runner_name}` — `{role}-{idx}-{4 random alphanumeric chars}`
- `{labels}` — role labels joined with commas, plus role name

**File 2: `/etc/systemd/network/eth.network`**

```ini
[Match]
MACAddress={guest_mac}

[Network]
Address={client_ip}/30
Gateway={host_ip}
```

Values from `NetworkAllocation` (already computed before boot):
- `{guest_mac}` — derived from client IP via `ip_to_mac()`
- `{client_ip}` — `172.16.{idx}.2`
- `{host_ip}` — `172.16.{idx}.1`

**Also create the symlink** to enable the service:

```
{mount}/etc/systemd/system/multi-user.target.wants/runner.service
  → /etc/systemd/system/runner.service
```

**Injection in `setup_run()`:**

```rust
fn setup_run(&mut self) -> Result<()> {
    // 1. Create/refresh slot LVM snapshots
    lvm_snapshot("base-{role}", "rootfs-{role}-{idx}")?;
    // cache snapshot created during setup(), recreated on clear

    // 2. Mount rootfs LV
    let mnt = self.work_dir.join("mnt");
    fs::mkdir_p(&mnt)?;
    mount::mount_image(format!("/dev/{vg}/rootfs-{role}-{idx}"), &mnt)?;

    // 3. Inject runner.service
    let token = self.github.registration_token()?;
    let runner_name = self.name();
    write_runner_service(&mnt, &github_org, &token, &runner_name, &self.labels())?;

    // 4. Inject networkd config
    write_network_config(&mnt, &self.network_allocation)?;

    // 5. Symlink to enable service
    enable_runner_service(&mnt)?;

    // 6. Unmount
    mount::unmount(&mnt)?;
    fs::rm_rf(&mnt)?;

    // 7. Write Firecracker config.json
    fs::write(self.work_dir.join("config.json"), serde_json::to_string(&self.config()?)?)?;

    Ok(())
}
```

**Firecracker drive config changes:**

`path_on_host` for rootfs is now the LV device path:
```
/dev/{volume_group}/rootfs-{role}-{idx}
```

For cache:
```
/dev/{volume_group}/cache-{role}-{idx}
```

**Boot args:** remove `init=/sbin/actions-init`. Updated constant:

```rust
pub const DEFAULT_BOOT_ARGS: &str =
    "random.trust_cpu=on reboot=k panic=1 pci=off";
```

---

## Per-slot process model

Replace the single polling loop in `Manager::run()` with one independent process per slot.

### Manager becomes a thin coordinator

`Manager::run()` after setup:
1. Set up IP forwarding once (existing `Forwarding::setup()`)
2. Reconcile LVM state (see above)
3. For each slot: spawn a child process running `actions-runner slot --config ... --role ... --idx ...`
4. Wait for SIGINT, forward to children, wait for them to finish

Each slot process is its own OS process. A manager restart does not kill slot processes (see graceful restart below).

### Slot subcommand

New subcommand: `slot --config <file> --role <name> --idx <n>`

The slot loop:

```rust
fn run_slot(config, role, idx) -> Result<()> {
    let mut instance = Instance::new(...);
    instance.setup()?;  // create tap device, create cache LV snapshot once

    loop {
        // Graceful restart: check if Firecracker is already running for this slot
        let pid_file = instance.work_dir.join("firecracker.pid");
        if let Some(pid) = read_pid_file(&pid_file) {
            if is_process_alive(pid) {
                info!("slot {idx}: Firecracker PID {pid} already running, reattaching");
                wait_for_pid(pid);  // poll until dead
                remove_pid_file(&pid_file);
                // fall through to next cycle
                continue;
            }
        }
        remove_pid_file(&pid_file);  // clean up stale file if present

        // Normal boot cycle
        match instance.start() {  // setup_run + spawn firecracker
            Ok(child) => {
                write_pid_file(&pid_file, child.id().unwrap())?;
                child.wait()?;
                remove_pid_file(&pid_file);
            }
            Err(e) => {
                error!("slot {idx}: failed to start: {e}");
                std::thread::sleep(Duration::from_secs(20));
                instance.reset();
            }
        }

        // Cache clear check before next cycle
        instance.try_clear_cache()?;
    }
}
```

### `instance.start()` changes

```rust
pub fn start(&mut self) -> Result<std::process::Child> {
    self.setup_run()?;

    let child = unsafe {
        Command::new("firecracker")
            .args(["--config-file", "config.json", "--no-api"])
            .current_dir(&self.work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false)       // Do not kill when Child is dropped
            .pre_exec(|| {
                // Detach from manager's process group so SIGTERM on deploy
                // does not propagate to Firecracker
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            })
            .spawn()?
    };

    Ok(child)
}
```

`kill_on_drop(false)` and `setsid()` together ensure Firecracker survives a manager or slot process restart.

---

## Graceful restart (reconciliation)

On slot process startup, before starting a new boot cycle, check if a Firecracker is already running for this slot.

### PID file

Written to `{run_path}/{role}/{idx}/firecracker.pid` immediately after successful `spawn()`. Deleted after `child.wait()` returns.

### Process liveness check

```rust
fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) does not send a signal; returns 0 if process exists
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
```

### Waiting on a non-child process (after restart)

After a manager/slot restart, Firecracker has been reparented to PID 1. `child.wait()` is not available. Poll instead:

```rust
fn wait_for_pid(pid: u32) {
    loop {
        std::thread::sleep(Duration::from_secs(2));
        if !is_process_alive(pid) {
            break;
        }
    }
}
```

### Startup flow with reconciliation

```
read firecracker.pid
if file exists:
    parse pid
    if is_process_alive(pid):
        // Job is in progress — do not interrupt
        wait_for_pid(pid)
        delete pid file
        continue (next iteration of the loop, try_clear_cache, then new boot cycle)
    else:
        // Stale file (process died cleanly and file was not cleaned up, or crash)
        delete pid file
// Fall through to normal boot cycle
```

---

## Dependency changes

### `Cargo.toml` (workspace)

- **Remove:** `lazy_static`, `cfg-if`
- **Add:** `libc = "*"` (for `setsid()` and `kill()`)
- **Update `nix` features:** add `signal` → `nix = { version = "*", features = ["fs", "mount", "signal"] }`
- **Pin all versions** from current `Cargo.lock` (replace `*` with locked versions: `anyhow = "1"`, `clap = "4"`, etc.)

### `manager/Cargo.toml`

- **Remove:** `signal-hook = "*"`
- Signal handling is now via per-slot processes receiving SIGTERM; no shared `AtomicBool` needed

### Crates removed from workspace

Remove from workspace `members`:
- `builder`
- `initialiser`
- `runner`

---

## What the VM image must contain (for reference — Ansible's job)

See `docs/building-root-image.md` for full details. Summary:

- Minimal Ubuntu 22.04, systemd init
- `runner` user at uid/gid 1001
- GitHub Actions runner at `/home/runner` (unconfigured — no `config.sh` run at image build)
- `/cache` directory exists
- `cache-mount.service` oneshot unit, enabled, mounts `/dev/vdb` at `/cache`:

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

- Static `/etc/resolv.conf`: `nameserver 1.1.1.1\noptions use-vc\n`
- systemd-networkd enabled (for injected `.network` file to take effect)
- No SSH, no cloud-init, no `actions-init` binary

---

## File structure after refactor

```
actions-runner/
├── Cargo.toml          workspace: bin, manager, github, config, util
├── bin/
│   └── src/main.rs     ~40 lines: clap, two subcommands (run, slot)
├── manager/
│   └── src/
│       ├── lib.rs       Manager: setup forwarding, LVM reconcile, spawn slot processes
│       ├── instance.rs  Instance: setup_run with LVM + config injection, start with setsid
│       ├── lvm.rs       NEW: LVM operations (lvcreate, lvremove, lvs, dd, checksum)
│       ├── slot.rs      NEW: per-slot loop with graceful restart logic
│       ├── inject.rs    NEW: write runner.service and networkd config into mounted rootfs
│       ├── disk.rs      UPDATED: cache LV lifecycle (snapshot, clear = lvremove+snapshot)
│       └── network/     unchanged
├── github/             unchanged
├── config/
│   └── src/
│       ├── lib.rs       updated boot args constant, LvmConfig struct
│       ├── manager.rs   updated ManagerConfig + Role structs
│       └── firecracker.rs unchanged
├── util/
│   └── src/
│       ├── lib.rs       lazy_static removed, cfg-if removed
│       ├── fs.rs        unchanged (copy_sparse kept for other uses; dd may be removed)
│       ├── mount.rs     unchanged or replace with nix::mount calls
│       └── network.rs   unchanged
└── docs/
    ├── building-root-image.md
    └── provisioning-runner-machine.md
```

---

## Summary of all decisions

| Decision | Choice | Reason |
|---|---|---|
| Image building | Ansible, separate repo | It's shell work: docker, qemu-img, mkfs |
| VM first-boot setup | Config injected by manager before each boot | Removes custom PID-1 binary; host already mounts rootfs for LVM reasons |
| Runner launch in VM | Shell script in image (`config.sh && run.sh`) | `runner/` crate was 2 shell invocations |
| Rootfs per-boot copy | LVM thin snapshot (instant, ~0 space) | `cp --sparse` copies all used blocks every boot |
| Cache disk | LVM thin snapshot of pre-formatted base LV | Replaces `dd + mkfs.ext4` per clear cycle |
| Multiple disks | LVM VG spans all disks; binary never thinks about layout | Ansible creates VG, binary creates LVs |
| Config changes | Binary reconciles on startup; restart to apply | No Ansible re-runs for operational changes |
| Slot architecture | One process per slot | Isolates failures; simplifies lifecycle to a plain loop |
| Graceful restart | `setsid()` + `kill_on_drop(false)` + PID file | Firecracker survives manager restarts; in-progress jobs complete |
| `lazy_static` | Removed | `Mutex::new()` is const since Rust 1.63 |
| `cfg-if` | Removed | Plain `#[cfg(...)]` attributes are sufficient |
| `signal-hook` | Removed | Per-slot processes handle their own lifecycle |
