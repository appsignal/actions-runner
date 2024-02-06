# Actions Runner

Actions Runner (`actions-runner`) is a tool that helps build and run a (group of) Firecracker VMs that
are to be used with GitHub Actions.

## Requirements

For the builder the following packages are required:

* `qemu`
* `docker`
* A `Dockerfile` to build the rootfs image, this image needs a `runner` user with a home directory at `/home/runner` and a version of the GitHub actions runner installed in `/home/runner/`. If `Docker` is installed _within_ the container, make sure that `docker` is in the `runner` user's group and that the `runner` user has access to the docker socket.

For the runner the following packages are required:

* `firecracker`
* A linux kernel (we use: [5.10]( https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux-5.10.bin))
* A rootfs image, created by the builder
* A configuration file, see below for an example
* A GitHub Personal Access Token with the `repo` scope, so we can add the runner to the organization.


## Building an image

The builder takes a Dockerfile and creates a new rootfs image. It copies
the `actions-runner` binary into the rootfs image and sets it as the entrypoint, so we can control how the server is setup (networking, caching, etc.).

For example:

```bash
./actions-runner build Dockerfile result.img
```

This builds a new rootfs image from the Dockerfile and saves it as `result.img`. The `--debug` flag is optional and will print debug information.


## Running a VM

The runner takes a configuration file and runs one or more groups of VMs. Each group of VMs is defined by a `group` in the configuration file.

The sizes are always in Gigabytes.

```toml
network_interface="enp0s31f6"
run_path="/srv"
github_pat="ghp_1234567890"
github_org="matsimitsu"

[[roles]]
name="your-project"
rootfs_image="/home/runner/containers/your-project-2024-01-20.img"
kernel_image="/home/runner/kernels/vmlinux-5.10.bin"
cpus=4
memory_size=1
cache_size=1
instance_count=4
cache_paths=["docker:/var/lib/docker"]
labels=["your-project-2024-01-20"]
```

You can now run the VMs with the following command:

```bash
./actions-runner run --config config.toml
```


### Debugging a VM

You can run a `debug` instance of a role by setting the `--debug-role` flag.
For example, to start a single vm in debug mode for the config file above,
you can run:

```bash
./actions-runner runner --config config.toml --debug-role your-project --log-level debug
```

This will start a single VM with the `your-project` role, and binds
stdin/stdout to the terminal, meaning you can see and manipulate the VM directly.

You can run this command while the runner is running, and it will start a new, separate VM.


## Contributing

This project was created to run our own GitHub actions, we're not necessarily
looking into expanding this into a full-fledged standalione project. However, we're happy to accept contributions that fit our use case.

We'll most likely not accept contributions that would make this project more
generic, as we're not looking to maintain a generic actions runner.

If you have doubts, please open an issue and we can discuss it!

Please follow our [Contributing guide][contributing-guide] in our
documentation and follow our [Code of Conduct][coc].

Running `cargo fmt` before contributing changes would reduce diffs for future
contributions.

Also, we would be very happy to send you Stroopwafles. Have look at everyone
we send a package to so far on our [Stroopwafles page][waffles-page].

[contributing-guide]: http://docs.appsignal.com/appsignal/contributing.html
[coc]: https://docs.appsignal.com/appsignal/code-of-conduct.html
[waffles-page]: https://appsignal.com/waffles
