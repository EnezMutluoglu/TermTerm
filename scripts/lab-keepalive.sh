#!/usr/bin/env bash
# WSL systemd services do not themselves keep a distro alive.
# The Windows lab helper owns this foreground process and stops it explicitly.
trap 'exit 0' TERM INT
while true; do sleep 60 & wait $!; done
