import { BlobPreconditionFailedError, get, put } from '@vercel/blob';
import { isTerminal, type JobState, type JsonObject } from './contracts.js';

const contentType = 'application/json; charset=utf-8';
const retryLimit = 5;

const pathname = (jobId: string) => `jobs/${jobId}/state.json`;
const pendingPathname = 'admission/pending-jobs.json';
const retainedEventLimit = 128;
const retainedEventBytes = 3 * 1024 * 1024;
const maximumEventBytes = 1_750_000;
const activeJobTimeoutMs = 15 * 60 * 1000;

function eventKind(event: JsonObject) {
  return typeof event.kind === 'string' ? event.kind : undefined;
}

function jsonBytes(value: unknown) {
  return Buffer.byteLength(JSON.stringify(value), 'utf8');
}

function retainEvents(events: JsonObject[], event: JsonObject) {
  const pinnedKinds = new Set(['input', 'start']);
  const isPinned = (item: JsonObject) => pinnedKinds.has(eventKind(item) ?? '');
  const combined = isPinned(event)
    ? [...events.filter((item) => eventKind(item) !== eventKind(event)), event]
    : [...events, event];
  const pinned = ['input', 'start']
    .map((kind) => [...combined].reverse().find((item) => eventKind(item) === kind))
    .filter((item): item is JsonObject => Boolean(item));
  const history = combined.filter((item) => !isPinned(item));
  const retained = [...pinned, ...history.slice(-(retainedEventLimit - pinned.length))];
  while (retained.length > pinned.length && jsonBytes(retained) > retainedEventBytes) {
    retained.splice(pinned.length, 1);
  }
  return retained;
}

export type StoredJob = { state: JobState; etag: string };
type PendingSlots = { version: 1; jobs: Record<string, string> };
type StoredSlots = { slots: PendingSlots; etag: string };

function isStoredJob(value: unknown): value is JobState {
  if (!value || typeof value !== 'object') return false;
  const job = value as Partial<JobState>;
  return job.version === 1 && typeof job.id === 'string' && typeof job.demoId === 'string' && typeof job.status === 'string' && Array.isArray(job.events);
}

function isPendingSlots(value: unknown): value is PendingSlots {
  if (!value || typeof value !== 'object') return false;
  const slots = value as Partial<PendingSlots>;
  return slots.version === 1 && typeof slots.jobs === 'object' && slots.jobs !== null && !Array.isArray(slots.jobs);
}

function pendingJobLimit() {
  const value = Number(process.env.MAX_PENDING_JOBS ?? 6);
  if (!Number.isInteger(value) || value < 1 || value > 20) {
    throw new Error('MAX_PENDING_JOBS must be an integer from 1 to 20.');
  }
  return value;
}

async function readSlots(): Promise<StoredSlots | undefined> {
  const result = await get(pendingPathname, { access: 'private', useCache: false });
  if (!result || result.statusCode !== 200) return undefined;
  const value = JSON.parse(await new Response(result.stream).text()) as unknown;
  if (!isPendingSlots(value)) throw new Error('Pending-job state is invalid.');
  return { slots: value, etag: result.blob.etag };
}

function activeSlots(slots: PendingSlots) {
  const now = Date.now();
  return Object.fromEntries(
    Object.entries(slots.jobs).filter(([, expiresAt]) => Number.isFinite(Date.parse(expiresAt)) && Date.parse(expiresAt) > now)
  );
}

async function writeSlots(slots: PendingSlots, etag?: string) {
  await put(pendingPathname, JSON.stringify(slots), {
    access: 'private',
    addRandomSuffix: false,
    ...(etag ? { allowOverwrite: true, ifMatch: etag } : { ifNoneMatch: '*' }),
    contentType,
    cacheControlMaxAge: 0
  });
}

/** Reserve a short-lived global queue slot so public verification cannot build an unlimited backlog. */
export async function reserveJobSlot(jobId: string) {
  const expiresAt = new Date(Date.now() + 20 * 60 * 1000).toISOString();
  for (let attempt = 0; attempt < retryLimit; attempt += 1) {
    const stored = await readSlots();
    const jobs = activeSlots(stored?.slots ?? { version: 1, jobs: {} });
    if (jobs[jobId]) return true;
    if (Object.keys(jobs).length >= pendingJobLimit()) return false;
    try {
      await writeSlots({ version: 1, jobs: { ...jobs, [jobId]: expiresAt } }, stored?.etag);
      return true;
    } catch (error) {
      if (!(error instanceof BlobPreconditionFailedError) || attempt === retryLimit - 1) throw error;
    }
  }
  throw new Error('Unable to reserve GPU capacity.');
}

export async function releaseJobSlot(jobId: string) {
  for (let attempt = 0; attempt < retryLimit; attempt += 1) {
    const stored = await readSlots();
    if (!stored || !Object.hasOwn(stored.slots.jobs, jobId)) return;
    const jobs = activeSlots(stored.slots);
    delete jobs[jobId];
    try {
      await writeSlots({ version: 1, jobs }, stored.etag);
      return;
    } catch (error) {
      if (!(error instanceof BlobPreconditionFailedError) || attempt === retryLimit - 1) throw error;
    }
  }
}

/** A worker restart must not leave a browser polling a nonterminal job forever. */
export async function expireStaleActiveJob(jobId: string) {
  const state = await updateJob(jobId, (job) => {
    if (!['dispatching', 'running', 'cancel_requested'].includes(job.status)) return undefined;
    const lastUpdate = Date.parse(job.updatedAt);
    if (!Number.isFinite(lastUpdate) || Date.now() - lastUpdate < activeJobTimeoutMs) return undefined;
    return {
      ...job,
      status: job.status === 'cancel_requested' ? 'cancelled' : 'failed',
      error: job.status === 'cancel_requested' ? 'The GPU run was cancelled.' : 'The GPU worker stopped responding.',
      finishedAt: new Date().toISOString()
    };
  });
  if (state && isTerminal(state.status)) await releaseJobSlot(jobId).catch(() => undefined);
  return state;
}

export async function readJob(jobId: string): Promise<StoredJob | undefined> {
  const result = await get(pathname(jobId), { access: 'private', useCache: false });
  if (!result || result.statusCode !== 200) return undefined;
  const value = JSON.parse(await new Response(result.stream).text()) as unknown;
  if (!isStoredJob(value)) throw new Error('Stored job state is invalid.');
  return { state: value, etag: result.blob.etag };
}

export async function createJob(state: JobState) {
  await put(pathname(state.id), JSON.stringify(state), {
    access: 'private',
    addRandomSuffix: false,
    contentType,
    cacheControlMaxAge: 0
  });
}

export async function updateJob(
  jobId: string,
  mutate: (state: JobState) => JobState | undefined
): Promise<JobState | undefined> {
  for (let attempt = 0; attempt < retryLimit; attempt += 1) {
    const stored = await readJob(jobId);
    if (!stored) return undefined;
    const next = mutate(structuredClone(stored.state));
    if (!next) return stored.state;
    next.updatedAt = new Date().toISOString();
    try {
      await put(pathname(jobId), JSON.stringify(next), {
        access: 'private',
        addRandomSuffix: false,
        allowOverwrite: true,
        ifMatch: stored.etag,
        contentType,
        cacheControlMaxAge: 0
      });
      return next;
    } catch (error) {
      if (!(error instanceof BlobPreconditionFailedError) || attempt === retryLimit - 1) throw error;
      // Back off before re-reading so a racing write has time to settle.
      await new Promise((settle) => setTimeout(settle, 50 * (attempt + 1)));
    }
  }
  throw new Error('Unable to update the job after concurrent changes.');
}

export async function appendWorkerUpdate(
  jobId: string,
  update: { status: JobState['status']; sequence: number; events?: JsonObject[]; error?: string }
) {
  for (const event of update.events ?? []) {
    if (jsonBytes(event) > maximumEventBytes) throw new Error('Worker event exceeds the storage limit.');
  }
  return updateJob(jobId, (state) => {
    if (isTerminal(state.status)) return undefined;
    if (state.status === 'cancel_requested' && update.status !== 'cancelled') {
      return undefined;
    }
    if (update.sequence <= (state.lastWorkerSequence ?? -1)) return undefined;
    state.status = update.status;
    state.lastWorkerSequence = update.sequence;
    for (const event of update.events ?? []) state.events = retainEvents(state.events, event);
    if (update.error) state.error = update.error;
    if (update.status === 'succeeded' || update.status === 'failed' || update.status === 'cancelled') {
      state.finishedAt = new Date().toISOString();
    }
    return state;
  }).then(async (state) => {
    if (state && isTerminal(state.status)) await releaseJobSlot(jobId).catch(() => undefined);
    return state;
  });
}
