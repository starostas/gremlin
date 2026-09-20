#!/bin/sh
# Runs the worker on a host without systemd, such as a rented GPU container.
# On a normal VM use gremlin-gpu-worker.service instead.
#
# Tailscale must already be up with Funnel pointing at this worker's port:
#
#   tailscaled --tun=userspace-networking &   # containers without /dev/net/tun
#   tailscale up --hostname=gremlin-gpu
#   tailscale funnel --bg 8080
#
# Usage: GPU_WORKER_ENV=/etc/gremlin-gpu-worker.env ./run-container.sh
set -eu

env_file="${GPU_WORKER_ENV:-/etc/gremlin-gpu-worker.env}"
[ -r "$env_file" ] || { echo "Missing environment file: $env_file" >&2; exit 1; }

worker_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
log_directory="${GPU_WORKER_LOG_DIR:-/var/log/gremlin-gpu}"
mkdir -p "$log_directory"

set -a
. "$env_file"
set +a

cd "$worker_directory"
exec node server.mjs >> "$log_directory/worker.log" 2>&1
