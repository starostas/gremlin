import { hasCapability } from '../../_lib/auth.js';
import { options, privateJson } from '../../_lib/cors.js';
import { isJobId, isTerminal, type JsonObject } from '../../_lib/contracts.js';
import { expireStaleActiveJob, readJob, updateJob } from '../../_lib/job-store.js';
import { requestWorkerCancellation } from '../../_lib/worker.js';

function idFromRequest(request: Request) {
  const id = new URL(request.url).pathname.split('/').at(-1) ?? '';
  return decodeURIComponent(id);
}

/**
 * Browsers poll this route for the life of a run. Returning the whole event log
 * every time would repeatedly ship megabytes of preview images, so a caller that
 * tracks how many events it already holds asks only for what is new.
 */
function eventsAfter(request: Request, events: JsonObject[]) {
  const raw = new URL(request.url).searchParams.get('after');
  if (raw === null) return { events, nextCursor: events.length };
  const after = Number(raw);
  if (!Number.isInteger(after) || after < 0 || after > events.length) {
    return { events, nextCursor: events.length };
  }
  return { events: events.slice(after), nextCursor: events.length };
}

export default async function handler(request: Request) {
  if (request.method === 'OPTIONS') return options(request);
  const id = idFromRequest(request);
  if (!isJobId(id)) return privateJson(request, { error: 'Job not found.' }, 404);

  const stored = await readJob(id);
  if (!stored) return privateJson(request, { error: 'Job not found.' }, 404);
  if (!hasCapability(stored.state.capabilityHash, request.headers.get('x-experiment-capability'))) {
    return privateJson(request, { error: 'Job not found.' }, 404);
  }

  if (request.method === 'GET') {
    const state = await expireStaleActiveJob(id) ?? stored.state;
    const page = eventsAfter(request, state.events);
    return privateJson(request, {
      id: state.id,
      status: state.status,
      events: page.events,
      cursor: page.nextCursor,
      error: state.error
    });
  }

  if (request.method !== 'DELETE') return privateJson(request, { error: 'Method not allowed.' }, 405);
  if (isTerminal(stored.state.status)) return privateJson(request, { status: stored.state.status });

  const state = await updateJob(id, (job) =>
    isTerminal(job.status) ? undefined : { ...job, status: 'cancel_requested' }
  );
  if (!state) return privateJson(request, { error: 'Job not found.' }, 404);

  // The worker may be unavailable while Vercel Queues retries delivery. The job remains
  // marked for cancellation so a dispatcher will not start it later.
  if (state.status === 'cancel_requested') {
    await requestWorkerCancellation(id).catch(() => undefined);
  }
  return privateJson(request, { status: state.status }, 202);
}
