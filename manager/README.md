# Manager

This crate is responsible for managing the VM lifecycle, and it's what gets called when you run `actions-runner run`.

It sets up the required networking on the host machine, to allow internet
connectivity inside the VM. It also sets up the Cache disk, so we can persist data between runs.

Finally, it starts the Firecracker VM, and waits for it to finish.

There's also a debug feature that starts a Firecracker VM with stdin/out/err connected to the host machine, so you can see the output of the VM, and maniulate it.

We pass variables from outside the VM to inside the VM through the kernel boot args, and pick them up with our custom entrypoint.
