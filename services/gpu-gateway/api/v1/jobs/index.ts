import { createCapability, hashCapability } from '../../../src/auth.js';
import { admitsJobRequest } from '../../../src/admission.js';
import { hasAllowedOrigin, json, options } from '../../../src/cors.js';
import type { JobState } from '../../../src/contracts.js';
import { parseCreateJob } from '../../../src/contracts.js';
import { createJob, releaseJobSlot, reserveJobSlot, updateJob } from '../../../src/job-store.js';
import { gpuTopic, queue } from '../../../src/queue.js';
import { readJson } from '../../../src/request.js';
import { assertWorkerConfiguration } from '../../../src/worker.js';

export default async function handler(request: Request) {
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
