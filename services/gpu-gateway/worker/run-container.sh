#!/bin/sh
# Runs the worker and its tunnel on a host without systemd, such as a rented GPU
# container. On a normal VM use gremlin-gpu-worker.service instead.
#
# Usage: GPU_WORKER_ENV=/etc/gremlin-gpu-worker.env ./run-container.sh
set -eu

env_file="${GPU_WORKER_ENV:-/etc/gremlin-gpu-worker.env}"
[ -r "$env_file" ] || { echo "Missing environment file: $env_file" >&2; exit 1; }

worker_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
log_directory="${GPU_WORKER_LOG_DIR:-/var/log/gremlin-gpu}"
mkdir -p "$log_directory"

# The tunnel is the only path in, so start it first and stop it with the worker.
if [ -n "${CLOUDFLARE_TUNNEL_TOKEN:-}" ]; then
  cloudflared tunnel --no-autoupdate run --token "$CLOUDFLARE_TUNNEL_TOKEN" \
    >> "$log_directory/cloudflared.log" 2>&1 &
else
  cloudflared tunnel --no-autoupdate --config /etc/cloudflared/config.yml run \
    >> "$log_directory/cloudflared.log" 2>&1 &
fi
tunnel_pid=$!
trap 'kill "$tunnel_pid" 2>/dev/null || true' EXIT INT TERM

set -a
. "$env_file"
set +a

cd "$worker_directory"
exec node server.mjs >> "$log_directory/worker.log" 2>&1
