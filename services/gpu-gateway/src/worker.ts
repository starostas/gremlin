import { getVercelOidcToken } from '@vercel/oidc';
import { assertCallbackSigningSecret } from './auth.js';
import type { DemoInput, JobState } from './contracts.js';
import { deleteSculptorAsset, readSculptorAsset } from './sculptor-assets.js';

function requiredUrl(name: 'GPU_WORKER_URL' | 'GATEWAY_ORIGIN') {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not configured.`);
  const url = new URL(value);
  if (
    url.protocol !== 'https:' ||
    url.username ||
    url.password ||
    url.pathname !== '/' ||
    url.search ||
    url.hash
  ) {
    throw new Error(`${name} must be a canonical HTTPS origin.`);
  }
  return url.origin;
}

async function workerInput(demoId: JobState['demoId'], input: DemoInput) {
  if (demoId === 'shader-detective') {
    const value = input as Extract<DemoInput, { preset: 'afterglow' | 'aurora' }>;
    return { mode: value.mode, preset: value.preset, cases: value.cases, seed: value.seed };
  }
  if (demoId === 'landing-lab') {
    const value = input as Extract<DemoInput, { cases: 2048 | 8192 }>;
    return { mode: value.mode, preset: 'lander', cases: value.cases, seed: value.seed };
  }
  if (demoId === 'tiny-robot') {
    const value = input as Extract<DemoInput, { cases: 128 | 512 }>;
    return { mode: 'gpu', preset: 'robot', cases: value.cases, seed: value.seed };
  }
  if (demoId === 'shader-sculptor') {
    const value = input as Extract<DemoInput, { resolution: 128 | 256 | 512 | 1024 | 2048; budgetMs: number }>;
    if ('imageAsset' in value) {
      const asset = await readSculptorAsset(value.imageAsset);
      if (asset.resolution !== value.resolution) throw new Error('The uploaded image resolution is invalid.');
      return {
        targetRgb: asset.bytes.toString('base64'),
        width: asset.resolution,
        height: asset.resolution,
        budget_ms: value.budgetMs,
        seed: value.seed
      };
    }
    return { targetPreset: value.preset, resolution: value.resolution, budget_ms: value.budgetMs, seed: value.seed };
  }
  return input;
}

async function workerToken() {
  const audience = process.env.GPU_WORKER_OIDC_AUDIENCE;
  if (!audience) throw new Error('GPU_WORKER_OIDC_AUDIENCE is not configured.');
  return getVercelOidcToken({ audience, skipCache: true });
}

export function assertWorkerConfiguration() {
  requiredUrl('GPU_WORKER_URL');
  requiredUrl('GATEWAY_ORIGIN');
  if (!process.env.GPU_WORKER_OIDC_AUDIENCE) {
    throw new Error('GPU_WORKER_OIDC_AUDIENCE is not configured.');
  }
  assertCallbackSigningSecret();
}

export async function dispatchToWorker(state: JobState, callbackTicket: string) {
  const workerUrl = requiredUrl('GPU_WORKER_URL');
  const gatewayUrl = requiredUrl('GATEWAY_ORIGIN');
  const input = await workerInput(state.demoId, state.input);
  const response = await fetch(new URL('/v1/jobs', workerUrl), {
    method: 'POST',
    headers: {
      authorization: `Bearer ${await workerToken()}`,
      'content-type': 'application/json',
      'idempotency-key': state.id
    },
    body: JSON.stringify({
      jobId: state.id,
      demoId: state.demoId,
      input,
      callbackUrl: new URL(`/v1/internal/jobs/${state.id}`, gatewayUrl).toString(),
      callbackTicket
    }),
    redirect: 'error',
    signal: AbortSignal.timeout(8_000)
  });
  if (!response.ok) throw new Error(`GPU worker rejected the job with status ${response.status}.`);
  if (state.demoId === 'shader-sculptor' && 'imageAsset' in state.input) {
    await deleteSculptorAsset(state.input.imageAsset).catch(() => undefined);
  }
}

export async function requestWorkerCancellation(jobId: string) {
  const workerUrl = requiredUrl('GPU_WORKER_URL');
  const response = await fetch(new URL(`/v1/jobs/${jobId}`, workerUrl), {
    method: 'DELETE',
    headers: { authorization: `Bearer ${await workerToken()}` },
    redirect: 'error',
    signal: AbortSignal.timeout(8_000)
  });
  if (!response.ok && response.status !== 404) {
    throw new Error(`GPU worker could not accept cancellation (${response.status}).`);
  }
}
