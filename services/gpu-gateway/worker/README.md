# Gremlin GPU worker

This is the only component that runs GPU binaries. It is deliberately separate from the public docs site and Vercel gateway:

```text
Astro island → Vercel gateway → Vercel Queue → this HTTPS worker → signed gateway callback
```

The browser never reaches this service. The worker accepts only Vercel-issued OIDC tokens for one exact production subject, accepts only five fixed demo schemas, derives all executable paths itself, and runs a single job at a time. It never receives an SSH key, Blob credential, Vercel Queue token, or browser request.

## Provisioning

Do this only after independently verifying the GPU host’s SSH fingerprint. Create an unprivileged `gremlin-gpu` account, put the checked-out repository at the fixed `GREMLIN_REPOSITORY` path, build the approved engine binaries there, and create a state directory owned by that account.

Build each engine with its CUDA feature, from its own directory:

```sh
CUDA_HOME=/usr/local/cuda cargo build --release --features cuda
```

Install Node 20 or newer and the runtime dependency:

```sh
pnpm install --frozen-lockfile --prod
cp .env.example /etc/gremlin-gpu-worker.env
```

Fill in the exact issuer, audience, and production OIDC subject from the Vercel gateway project. `GATEWAY_ORIGIN` must be the gateway’s canonical HTTPS origin. Keep the listener on loopback.

### Reaching the worker over HTTPS

A [named Cloudflare Tunnel](./cloudflared-config.example.yml) is the supported path: it dials out, so no inbound port is opened and the GPU host’s address stays unpublished. Its hostname becomes the gateway’s `GPU_WORKER_URL`. On a host with its own public name and certificate, the [Caddy example](./Caddyfile.example) works instead. Do not expose the Node listener or a demo engine directly.

Install the systemd unit after checking its paths, then enable it. On a rented GPU container without systemd, use [`run-container.sh`](./run-container.sh), which starts the tunnel and the worker together. The account needs access to the NVIDIA device nodes through the host’s normal GPU group configuration. Do not run the service as root.

### Optional: Orbit Forge native evidence

Orbit Forge compiles the winning program’s LLVM and times it against a reference solver, the same check the recorded run reports. Install `clang-18` for that; without it the run still succeeds and reports the comparison as unavailable.

## Operating boundaries

- One process runs at a time; a busy worker returns `429`, so Queue delivery retries instead of starting concurrent GPU work.
- Each job has a fixed timeout (30–600 seconds, default 240), fixed binary path, fixed arguments, and fixed input sizes.
- Job IDs are written before launch and terminal IDs are retained for two hours, making queue redelivery idempotent across transient gateway timeouts.
- Progress is batched into at most one callback per second, and a batch is closed before it would exceed the gateway function’s request-body limit. Every engine event is still delivered.
- Shader Sculptor searches at full resolution but sends bounded previews: 256 a side while running, and up to 512 for the result, reduced further if the exported program would otherwise not fit in one event. The full-resolution raster is not archived.
- The job callback uses a per-job ticket and only targets the configured gateway origin. The worker never follows redirects.
- Health checks use `GET /healthz`; every job endpoint requires Vercel OIDC.
