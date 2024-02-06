# GitHub

This crate is responsible for getting a Runner token from the provided
GitHub Personal Access Token (PAT), it is used to authenticate the Runner
with GitHub.

We pass this token through the `kernel boot args` in the Manager, and pick it up
inside the VM with our custom entrypoint.
