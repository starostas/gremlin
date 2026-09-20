import { createHash, createHmac, randomBytes, timingSafeEqual } from 'node:crypto';

const encoder = new TextEncoder();

function callbackSecret() {
  const secret = process.env.JOB_CALLBACK_SIGNING_SECRET;
  if (!secret || secret.length < 32) {
    throw new Error('JOB_CALLBACK_SIGNING_SECRET must be set to at least 32 characters.');
  }
  return secret;
}

export function assertCallbackSigningSecret() {
  callbackSecret();
}

export function createCapability() {
  return randomBytes(32).toString('base64url');
}

export function hashCapability(capability: string) {
  return createHash('sha256').update(capability).digest('hex');
}

export function hasCapability(expectedHash: string, candidate: string | null) {
  if (!candidate) return false;
  const expected = Buffer.from(expectedHash, 'hex');
  const actual = Buffer.from(hashCapability(candidate), 'hex');
  return expected.length === actual.length && timingSafeEqual(expected, actual);
}

type CallbackPayload = { jobId: string; exp: number };

const encodePayload = (payload: CallbackPayload) => Buffer.from(JSON.stringify(payload)).toString('base64url');

const signature = (payload: string) => createHmac('sha256', callbackSecret()).update(payload).digest('base64url');

export function createCallbackTicket(jobId: string, lifetimeSeconds = 60 * 60 * 2) {
  const payload = encodePayload({ jobId, exp: Math.floor(Date.now() / 1000) + lifetimeSeconds });
  return `${payload}.${signature(payload)}`;
}

export function verifiesCallbackTicket(ticket: string | null, jobId: string) {
  if (!ticket) return false;
  const parts = ticket.split('.');
  if (parts.length !== 2) return false;
  const [encoded, provided] = parts;
  if (!encoded || !provided) return false;
  const expected = signature(encoded);
  const expectedBytes = encoder.encode(expected);
  const providedBytes = encoder.encode(provided);
  if (expectedBytes.length !== providedBytes.length || !timingSafeEqual(expectedBytes, providedBytes)) return false;
  try {
    const payload = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8')) as CallbackPayload;
    return payload.jobId === jobId && Number.isInteger(payload.exp) && payload.exp >= Math.floor(Date.now() / 1000);
  } catch {
    return false;
  }
}
