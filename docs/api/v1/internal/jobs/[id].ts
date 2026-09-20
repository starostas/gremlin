import { verifiesCallbackTicket } from '../../../_lib/auth.js';
import { isJobId, isObjectValue, type JobStatus, type JsonObject } from '../../../_lib/contracts.js';
import { appendWorkerUpdate } from '../../../_lib/job-store.js';
import { readJson } from '../../../_lib/request.js';

const callbackStatuses = new Set<JobStatus>(['running', 'succeeded', 'failed', 'cancelled']);

/**
 * This runtime hands the handler a path-only `request.url`, so it is resolved
 * against a placeholder base before reading the path or query.
 */
function requestUrl(request: Request) {
  return new URL(request.url, 'http://request.invalid');
}

function idFromRequest(request: Request) {
  const id = requestUrl(request).pathname.split('/').at(-1) ?? '';
  return decodeURIComponent(id);
}

function bearerToken(request: Request) {
  const authorization = request.headers.get('authorization');
  return authorization?.startsWith('Bearer ') ? authorization.slice('Bearer '.length) : null;
}

const isEvent = (value: unknown) =>
  isObjectValue(value) && typeof value.kind === 'string' && value.kind.length <= 64;

/**
 * The worker batches progress into one callback so a run costs a bounded number
 * of job-state writes. A single `event` is still accepted for a worker that has
 * not been updated yet.
 */
function parseUpdate(value: unknown) {
  if (!isObjectValue(value) || !callbackStatuses.has(value.status as JobStatus)) return undefined;
  if (!Object.keys(value).every((key) => ['status', 'sequence', 'event', 'events', 'error'].includes(key))) return undefined;
  if (value.event !== undefined && value.events !== undefined) return undefined;
  if (!Number.isInteger(value.sequence) || (value.sequence as number) < 0 || (value.sequence as number) > 512) return undefined;
  if (value.event !== undefined && !isEvent(value.event)) return undefined;
  if (
    value.events !== undefined &&
    (!Array.isArray(value.events) || value.events.length > 256 || !value.events.every(isEvent))
  ) return undefined;
  if (value.error !== undefined && (typeof value.error !== 'string' || value.error.length > 1_000)) return undefined;

  const events = value.events !== undefined
    ? (value.events as JsonObject[])
    : value.event !== undefined ? [value.event as JsonObject] : [];
  return {
    status: value.status as JobStatus,
    sequence: value.sequence as number,
    events,
    error: value.error as string | undefined
  };
}

export default async function handler(request: Request) {
  if (request.method !== 'POST') return Response.json({ error: 'Method not allowed.' }, { status: 405 });
  const id = idFromRequest(request);
  if (!isJobId(id) || !verifiesCallbackTicket(bearerToken(request), id)) {
    return Response.json({ error: 'Unauthorized.' }, { status: 401 });
  }

  let update;
  try {
    update = parseUpdate(await readJson(request, 3 * 1024 * 1024));
  } catch (error) {
    return Response.json({ error: error instanceof Error ? error.message : 'Invalid update.' }, { status: 400 });
  }
  if (!update) return Response.json({ error: 'Unsupported worker update.' }, { status: 400 });

  const state = await appendWorkerUpdate(id, update);
  if (!state) return Response.json({ error: 'Job not found.' }, { status: 404 });
  return Response.json({ status: state.status });
}
