# Cairn

Cair is an userspace application packaging and deployment tool
targeting Linux-based systems.

Unlike other similar tools, Cairn is built *around* package delta update
mechanism that uses content-aware hashing.

The project itself consists of several parts:

- `cairn-digest`: see [`cairn-digest` specification](./cairn-digest.md)
- `cairn-pkg`
- TBD

## Setup

Before running `task deps` or other build tasks, ensure [mise](https://mise.jdx.dev/installing-mise.html) is installed and on your PATH.

Building `linux/arm64` images locally via `task docker:build:arm64` requires QEMU user-mode emulation registered with the kernel (`binfmt_misc`). If you hit `exec format error` on a `RUN` step, register it once with:
```
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
```
(substitute `podman run` if using Podman)
