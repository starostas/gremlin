# Deploying the experiments backend

Two pieces: one Vercel project that serves the site and its job routes, and the GPU host.

```text
island ──same-origin──> /api/v1/jobs ──Queue──> GPU worker (your host)
                        server-only secrets      OIDC-only, no inbound port
```

The browser never learns the GPU host's address and never holds a credential for it. It receives one capability per job, which reads or cancels only that job. Because the routes ship with the site they are same-origin, so there is no CORS grant and no origin allowlist to keep in sync.

Everything below fits the **Vercel Hobby** plan. Read [Hobby limits](#hobby-limits) before opening this to the public.

## 1. GPU host

1. Build the five engines, each from its own directory under `apps/`:

   ```sh
   CUDA_HOME=/usr/local/cuda cargo build --release --features cuda
   ```

2. Install Node 20+, then the worker's one runtime dependency:

   ```sh
   cd services/gpu-worker
   pnpm install --frozen-lockfile --prod
   ```

3. Optional: install `clang-18` so Orbit Forge can report its compiled-runtime comparison. Without it the run still succeeds and marks that evidence unavailable.

4. Copy `.env.example` to `/etc/gremlin-gpu-worker.env`. Leave the OIDC values blank for now; you fill them in at step 3.

Do **not** start the worker yet — it refuses to start until its OIDC configuration is complete, which is deliberate.

## 2. Tailscale Funnel

Funnel dials out, so you open no inbound port and never publish the host's address, and the `*.ts.net` hostname is stable across restarts.

```sh
# Containers without /dev/net/tun need userspace networking.
tailscaled --tun=userspace-networking --state=/var/lib/tailscale/tailscaled.state &
tailscale up --hostname=gremlin-gpu
tailscale funnel --bg 8080
```

Two tailnet-wide settings gate this, and neither has a CLI: **HTTPS certificates** must be enabled, and **Funnel** must be allowed in the tailnet policy. The first `tailscale funnel` prints a one-time link that enables the latter. After that, `tailscale funnel status` shows the public hostname — that is your `GPU_WORKER_URL`.

## 3. The Vercel project

1. Create one project with **`docs` as its root directory**. It builds the Astro site and the `api/` routes together.

2. Attach a **private** Blob store. The `gremlin-gpu-jobs` Queue topic needs no setup: the trigger in `docs/vercel.json` registers the consumer on deploy.

3. Under **Settings → Security**, confirm OIDC federation is in **Team** issuer mode. Note the issuer URL (`https://oidc.vercel.com/<team-slug>`).

4. Set the server-only variables from [`docs/.env.example`](../../docs/.env.example):

   | Variable | Value |
   | --- | --- |
   | `GPU_WORKER_URL` | your Funnel hostname, origin only |
   | `GATEWAY_ORIGIN` | this project's canonical production URL |
   | `GPU_WORKER_OIDC_AUDIENCE` | `gremlin-gpu-worker-production` |
   | `JOB_CALLBACK_SIGNING_SECRET` | 32+ random characters |
   | `SCULPTOR_ASSET_SIGNING_SECRET` | 32+ random characters, different |
   | `QUEUE_REGION`, `MAX_PENDING_JOBS` | defaults are fine |

   Generate each secret with `openssl rand -base64 48`. Neither ever leaves Vercel, and neither goes to the GPU host.

5. Decide how job requests are admitted, then set **one** of:

   - `TURNSTILE_SECRET_KEY` and `TURNSTILE_EXPECTED_HOSTNAME`, plus `PUBLIC_EXPERIMENT_TURNSTILE_SITE_KEY` — every job and image upload must carry a fresh human-verification proof.
   - `ALLOW_UNVERIFIED_JOBS=true` — no human check. The only limits on GPU use are then `MAX_PENDING_JOBS` and the worker running one job at a time, so anyone who finds the endpoint can keep the GPU busy.

   With neither set the routes stay closed, which is the intended default.

6. Set `PUBLIC_EXPERIMENT_LIVE_RUNS=true` to show the Run button. Leaving it unset keeps the site replay-only, which is a valid deployment: the recorded runs still play.

7. Deploy, then finish the worker's environment file on the GPU host:

   ```sh
   VERCEL_OIDC_ISSUER=https://oidc.vercel.com/<team-slug>
   VERCEL_OIDC_SUBJECT=owner:<team-slug>:project:<project>:environment:production
   GPU_WORKER_OIDC_AUDIENCE=gremlin-gpu-worker-production
   GATEWAY_ORIGIN=https://<project>.vercel.app
   ```

   The subject must name the **production** environment. A preview or development subject must not be accepted.

8. Start the worker — `systemctl enable --now gremlin-gpu-worker` on a VM, or `run-container.sh` on a host without systemd.

## Verifying the isolation

- `POST https://<funnel-hostname>/v1/jobs` with no bearer token → `401`. That is the most a browser could ever do.
- `GET /healthz` on the same host → `200`, confirming you are reaching the worker and not something else.
- `GET /api/v1/jobs/<id>` without the capability header returned at creation → `404`, including for a job that exists.
- The GPU host holds no Blob token, Queue token, or Vercel API token. Its only credential is Tailscale's, and its only trust is one OIDC subject.

## Hobby limits

| Resource | Hobby allowance | What it costs here |
| --- | --- | --- |
| Blob advanced operations | 2,000 / month | One per job-state write. A run costs roughly 8–20; a long 2,048² Shader Sculptor run costs about 60. |
| Blob storage | 1 GB | Job state is small and short-lived. Uploaded images are deleted at dispatch. |
| Queue operations | 1,000,000 / month | One send and one delivery per run. Not a constraint. |
| Function duration | 300 s | Every function here is short; the GPU run itself is not a function. |
| Function request/response body | 4.5 MB | Why a chosen image is uploaded to Blob rather than posted with the job, and why polling is incremental. |

The worker batches progress into at most one callback per second specifically to keep the Blob operation count low; without it a single Shader Detective run would cost about 150 operations instead of 19. If you exceed the Blob allowance the store stops serving until the month rolls over, so watch that number first.

To raise the ceiling materially, move job state off Blob to a marketplace Redis rather than upgrading the plan: it is the only per-event write in the system.
