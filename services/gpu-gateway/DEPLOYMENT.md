# Deploying the experiments backend

Three pieces, deployed in this order. Each step says who holds which secret, because the isolation between them is the point: the browser can start a bounded run, and nothing more.

```text
docs site (static)  →  gateway (Vercel Functions)  →  Queue  →  GPU worker (your host)
      public              server-only secrets                    OIDC-only, no inbound port
```

The browser never learns the GPU host's address and never holds a credential for it. It receives one capability per job, which reads or cancels only that job.

## Before you start

You need: a Vercel account, a Cloudflare account (for the tunnel and Turnstile), and a GPU host with the repository checked out and the engines built.

Everything below fits the **Vercel Hobby** plan. The relevant ceilings are in [Hobby limits](#hobby-limits) — read that section before opening this to the public.

## 1. GPU host

1. Build the five engines, each from its own directory under `apps/`:

   ```sh
   CUDA_HOME=/usr/local/cuda cargo build --release --features cuda
   ```

2. Install Node 20+, then the worker's one runtime dependency:

   ```sh
   cd services/gpu-gateway/worker
   pnpm install --frozen-lockfile --prod
   ```

3. Optional: install `clang-18` so Orbit Forge can report its compiled-runtime comparison. Without it the run still succeeds and marks that evidence unavailable.

4. Copy `worker/.env.example` to `/etc/gremlin-gpu-worker.env`. Leave the OIDC values blank for now; you fill them in at step 3.

Do **not** start the worker yet — it refuses to start until its OIDC configuration is complete, which is deliberate.

## 2. Cloudflare Tunnel

The tunnel dials out from the GPU host, so you open no inbound port and never publish the host's address.

```sh
cloudflared tunnel login
cloudflared tunnel create gremlin-gpu
cloudflared tunnel route dns gremlin-gpu gpu-worker.example.com
```

Copy `worker/cloudflared-config.example.yml` to `/etc/cloudflared/config.yml`, set your hostname, and put the credentials file from `tunnel create` beside it. The ingress rules end in `http_status:404`, so the tunnel exposes exactly one origin.

`https://gpu-worker.example.com` is the gateway's `GPU_WORKER_URL`.

## 3. Gateway project

1. Create a **second** Vercel project with **`services/gpu-gateway` as its root directory**. It must not be the same project as the docs site; that separation is what keeps the GPU credentials out of anything the browser touches.

2. Attach a **private** Blob store, and enable the **`gremlin-gpu-jobs`** Queue topic in the same region as the gateway. `vercel.json` already declares the queue trigger.

3. Under **Settings → Security → Secure backend access with OIDC federation**, set the issuer mode to **Team**. Note the issuer URL (`https://oidc.vercel.com/<team-slug>`).

4. Set the environment variables from [`.env.example`](./.env.example):

   | Variable | Value |
   | --- | --- |
   | `GPU_WORKER_URL` | your tunnel hostname, origin only |
   | `GATEWAY_ORIGIN` | this project's canonical production URL |
   | `DOCS_ORIGIN` | the docs site's origin, comma-separated if more than one |
   | `GPU_WORKER_OIDC_AUDIENCE` | `gremlin-gpu-worker-production` |
   | `JOB_CALLBACK_SIGNING_SECRET` | 32+ random characters |
   | `SCULPTOR_ASSET_SIGNING_SECRET` | 32+ random characters, different from the above |
   | `TURNSTILE_SECRET_KEY` | from step 4 |
   | `TURNSTILE_EXPECTED_HOSTNAME` | the docs hostname, exactly |
   | `QUEUE_REGION`, `MAX_PENDING_JOBS` | defaults are fine |

   Generate each secret with `openssl rand -base64 48`. Neither ever leaves Vercel.

5. Deploy, then finish the worker's environment file on the GPU host:

   ```sh
   VERCEL_OIDC_ISSUER=https://oidc.vercel.com/<team-slug>
   VERCEL_OIDC_SUBJECT=owner:<team-slug>:project:<gateway-project>:environment:production
   GPU_WORKER_OIDC_AUDIENCE=gremlin-gpu-worker-production
   GATEWAY_ORIGIN=https://<gateway-project>.vercel.app
   ```

   The subject must name the **production** environment. A preview or development subject must not be accepted.

6. Start the worker — `systemctl enable --now gremlin-gpu-worker` on a VM, or `run-container.sh` on a host without systemd — and confirm `https://gpu-worker.example.com/healthz` returns `{"ok":true}` while `POST /v1/jobs` without a token returns `401`.

## 4. Turnstile

Create a Turnstile widget for the **docs** hostname. Its secret and the exact hostname go in the gateway project only (step 3). Its **site key** goes with the docs site, below. A GPU run is an expensive public action, so production job creation and image uploads both require a fresh Turnstile proof.

## 5. Docs site

Set these in the docs project and deploy:

```sh
PUBLIC_EXPERIMENT_API_URL=https://<gateway-project>.vercel.app
PUBLIC_EXPERIMENT_TURNSTILE_SITE_KEY=<site key>
```

Both are public by design — they are a URL and a widget key, not credentials. Leaving either unset keeps every page replay-only, which is a valid deployment: the recorded runs still play.

## Verifying the isolation

After deploying, confirm each of these:

- `POST https://gpu-worker.example.com/v1/jobs` with no bearer token → `401`. The browser can never do more than this.
- A job request from an origin outside `DOCS_ORIGIN` → `403`.
- A job request with no Turnstile token → `403`.
- `GET /v1/jobs/<id>` without the capability header returned at creation → `404`, including for a job that exists.
- The GPU host holds no Blob token, Queue token, or Vercel API token. Its only credential is the tunnel's, and its only trust is one OIDC subject.

## Hobby limits

These are the ceilings that actually bind, all on the free tier:

| Resource | Hobby allowance | What it costs here |
| --- | --- | --- |
| Blob advanced operations | 2,000 / month | One per job-state write. A run costs roughly 8–20; a long 2,048² Shader Sculptor run costs about 60. |
| Blob storage | 1 GB | Job state is small and short-lived. Uploaded images are deleted at dispatch. |
| Queue operations | 1,000,000 / month | One send and one delivery per run. Not a constraint. |
| Function duration | 300 s | Every function here is short; the GPU run itself is not a function. |
| Function request/response body | 4.5 MB | Why a chosen image is uploaded to Blob rather than posted with the job, and why polling is incremental. |

The worker batches progress into at most one callback per second specifically to keep the Blob operation count low; without it a single Shader Detective run would cost about 150 operations instead of 18. If you exceed the Blob allowance the store stops serving until the month rolls over, so watch that number first.

To raise the ceiling materially, move job state off Blob to a marketplace Redis rather than upgrading the plan: it is the only per-event write in the system.
