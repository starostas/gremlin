# Gremlin GPU gateway

This is a separate Vercel project for live experiment runs. The Astro site stays static; this service creates short job records and dispatches a bounded request to the GPU worker. GPU code never runs in a Vercel Function, and the browser never reaches the GPU host.

```text
docs island → gateway → private Blob + Queue → GPU worker → signed callback → island
```

Three trust boundaries, each one-way:

- **Browser → gateway.** A request must come from an allowed docs origin and carry a fresh Turnstile proof. It receives one capability per job, which reads or cancels only that job. It never learns the worker's address.
- **Gateway → worker.** Authenticated with a short-lived Vercel OIDC token for one exact production subject. The worker holds no Blob, Queue, or Vercel credential.
- **Worker → gateway.** A per-job callback ticket, scoped to one job id and expiring. The worker never follows redirects and posts only to the configured origin.

See [DEPLOYMENT.md](./DEPLOYMENT.md) for the full setup, the isolation checks to run afterwards, and the Hobby-tier budget.

## Vercel setup

The short version; the runbook has the ordered detail.

1. Create a second Vercel project with `services/gpu-gateway` as its root directory.
2. Attach a private Blob store and enable the `gremlin-gpu-jobs` Queue trigger in the same region as the gateway.
3. Set the variables in [`.env.example`](./.env.example). `GPU_WORKER_URL` and `GATEWAY_ORIGIN` are canonical HTTPS origins only.
4. Create a Turnstile widget for the docs hostname. Put its secret and exact hostname only in this Vercel project; put its public site key beside `PUBLIC_EXPERIMENT_API_URL` in the docs project.
5. Deploy the worker in [`worker/`](./worker/) behind a Cloudflare Tunnel, then set the worker's OIDC issuer, audience, and exact production subject. The worker validates those claims before accepting either start or stop requests.
6. Finally, set the two public docs variables and deploy the static site. Leaving either unset keeps every page replay-only.

The gateway is intentionally fail-closed: production job creation needs an allowed docs origin, a verified Turnstile proof, a valid worker configuration, and a free global pending-job slot. Blob-backed slots cap the backlog; the worker permits one GPU process at a time. Browser capabilities can read or cancel only the one job that returned them, while callback tickets are per job and expire.

## Staying inside the Hobby plan

Job state lives in Blob, and Hobby allows 2,000 advanced operations a month. Two things keep a run's cost bounded:

- The worker batches progress into at most one callback per second, so a Shader Detective run costs about 18 job-state writes rather than 148.
- `GET /v1/jobs/:id` takes an `after` cursor and returns only new events, so a browser polling a one-minute run does not re-download its previews each time.

A chosen Shader Sculptor image is uploaded straight to private Blob under a short-lived, digest-bound ticket rather than posted with the job, because a 2,048² raster is far larger than a function's 4.5 MB body limit. That upload is deleted as soon as the job is dispatched.

## Local checks

Use pnpm only:

```sh
pnpm run check
pnpm exec vercel dev
```

For a strictly local smoke test, `ALLOW_LOCAL_UNPROTECTED_JOBS=true` is allowed only outside production. It must never be set in Vercel. The worker still needs its own OIDC, TLS, and fixed-path configuration before it will start.
