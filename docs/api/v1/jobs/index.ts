import { createCapability, hashCapability } from '../../_lib/auth.js';
import { admitsJobRequest } from '../../_lib/admission.js';
import { hasAllowedOrigin, json, options } from '../../_lib/cors.js';
import type { JobState } from '../../_lib/contracts.js';
import { parseCreateJob } from '../../_lib/contracts.js';
import { createJob, releaseJobSlot, reserveJobSlot, updateJob } from '../../_lib/job-store.js';
import { gpuTopic, queue } from '../../_lib/queue.js';
import { readJson } from '../../_lib/request.js';
import { assertWorkerConfiguration } from '../../_lib/worker.js';

async function handler(request: Request) {
  if (request.method === 'OPTIONS') return options(request);
  if (request.method !== 'POST') return json(request, { error: 'Method not allowed.' }, 405);
  if (!hasAllowedOrigin(request) || !(await admitsJobRequest(request))) {
    return json(request, { error: 'Live runs are unavailable.' }, 403);
  }
  try {
    assertWorkerConfiguration();
  } catch {
    return json(request, { error: 'Live runs are unavailable.' }, 503);
  }

  let parsed;
  try {
    parsed = parseCreateJob(await readJson(request, 8 * 1024));
  } catch (error) {
    return json(request, { error: error instanceof Error ? error.message : 'Invalid request.' }, 400);
  }
  if (!parsed) return json(request, { error: 'Unsupported demo input.' }, 400);

  const id = crypto.randomUUID();
  const capability = createCapability();
  const now = new Date().toISOString();
  const state: JobState = {
    version: 1,
    id,
    demoId: parsed.demoId,
    input: parsed.input,
    capabilityHash: hashCapability(capability),
    status: 'queued',
    createdAt: now,
    updatedAt: now,
    events: []
  };

  try {
    if (!(await reserveJobSlot(id))) {
      return json(request, { error: 'GPU capacity is full. Try again shortly.' }, 429);
    }
    await createJob(state);
    await queue.send(gpuTopic, { jobId: id }, { idempotencyKey: id });
  } catch (error) {
    await updateJob(id, (job) => ({
      ...job,
      status: 'failed',
      error: 'The job could not be queued.',
      finishedAt: new Date().toISOString()
    })).catch(() => undefined);
    await releaseJobSlot(id).catch(() => undefined);
    return json(request, { error: 'Unable to queue the GPU job.' }, 503);
  }

  return json(request, { id, capability }, 202);
}

// This runtime dispatches on named method exports; a default export is
// invoked with the Node (req, res) signature and its returned Response is
// discarded.
export const POST = handler;
export const OPTIONS = handler;
