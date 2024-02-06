# Builder

Responsible for converting a Dockerfile to a rootfs image.

## Usage

```bash
actions-runner build /path/to/Dockerfile /path/to/result.img
```

Use `--log-level debug` to see debug information.


## What does it do?

The builder runs the `Docker build` command to generate a container id.
It then uses the container id to export the container's filesystem to a tarball.

We generate a new empty rootfs image with `qemu-img`, format it as ext4, and mount it, so we can add files to the image.

We then extract the Docker tarball into the rootfs image, copy ourselves into it, and set ourselves as the entrypoint.
