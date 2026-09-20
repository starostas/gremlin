import { createRemoteJWKSet, jwtVerify } from 'jose';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { AccuracyFailure, benchmark } from './benchmark.mjs';
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync
} from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';

const jobIdPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
// A 2,048² normalized RGB input is 12 MiB and becomes 16 MiB as base64 while
// travelling over the private gateway-to-worker channel. Public browser
// requests never use this limit; they use a short-lived private Blob upload.
const maximumRequestBytes = 18 * 1024 * 1024;
const maximumEventBytes = 1_750_000;
// Shader Sculptor's terminal event carries a full-resolution raster: 16 MiB of
// base64 at 2,048². That line is read in full and then reduced to a bounded
// preview, so the read limit and the delivered-event limit are not the same.
const maximumEngineLineBytes = 24 * 1024 * 1024;
const callbackAttempts = 4;
// Every callback costs the gateway one job-state write. Shader Detective alone
// emits ~150 engine events per run, so progress is batched: the browser still
// receives every event, but a run costs a bounded number of writes.
const flushIntervalMs = 1_000;
// One callback must stay inside the gateway function's request-body limit, so a
// batch is closed before it would cross this, never after.
const maximumBatchBytes = 2_500_000;
const immediateKinds = new Set(['start', 'input', 'stage', 'comparison', 'done', 'error', 'diagnostic']);
const terminalRecordLifetimeMs = 2 * 60 * 60 * 1000;
const validDemoIds = new Set([
  'shader-detective',
  'landing-lab',
  'tiny-robot',
  'shader-sculptor',
  'orbit-forge'
]);

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required.`);
  return value;
}

function canonicalOrigin(name) {
  const url = new URL(required(name));
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

function canonicalIssuer() {
  const url = new URL(required('VERCEL_OIDC_ISSUER'));
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash) {
    throw new Error('VERCEL_OIDC_ISSUER must be an HTTPS issuer URL.');
  }
  return url.toString().replace(/\/$/, '');
}

function boundedInteger(name, fallback, minimum, maximum) {
  const value = process.env[name] === undefined ? fallback : Number(process.env[name]);
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be an integer from ${minimum} to ${maximum}.`);
  }
  return value;
}

// The Vercel gateway is the only client. Every one of these is mandatory, so a
// misconfigured worker refuses to start rather than listening unauthenticated.
const gatewayOrigin = canonicalOrigin('GATEWAY_ORIGIN');
const oidcIssuer = canonicalIssuer();
const oidcAudience = required('GPU_WORKER_OIDC_AUDIENCE');
const oidcSubject = required('VERCEL_OIDC_SUBJECT');
const jwks = createRemoteJWKSet(new URL(`${oidcIssuer}/.well-known/jwks`));
const repositoryRoot = realpathSync(required('GREMLIN_REPOSITORY'));
const stateDirectory = required('GPU_WORKER_STATE_DIR');
const jobTimeoutMs = boundedInteger('MAX_JOB_SECONDS', 240, 30, 600) * 1000;
const port = boundedInteger('PORT', 8080, 1, 65535);

if (!isAbsolute(stateDirectory)) throw new Error('GPU_WORKER_STATE_DIR must be absolute.');
mkdirSync(stateDirectory, { recursive: true, mode: 0o700 });

function isObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value, keys) {
  return Object.keys(value).length === keys.length && Object.keys(value).every((key) => keys.includes(key));
}

function isOneOf(value, values) {
  return values.includes(value);
}

function expectedRgbBytes(width) {
  return width * width * 3;
}

function encodedLength(byteLength) {
  return Math.ceil(byteLength / 3) * 4;
}

function isCanonicalRgb(value, width) {
  const byteLength = expectedRgbBytes(width);
  return typeof value === 'string' &&
    value.length === encodedLength(byteLength) &&
    /^[A-Za-z0-9+/]*={0,2}$/.test(value);
}

function validInput(demoId, input) {
  if (!isObject(input)) return false;
  if (demoId === 'shader-detective') {
    return hasExactKeys(input, ['mode', 'preset', 'cases', 'seed']) &&
      isOneOf(input.mode, ['cpu', 'gpu', 'both']) &&
      isOneOf(input.preset, ['afterglow', 'aurora']) &&
      isOneOf(input.cases, [8192, 32768]) &&
      isOneOf(input.seed, [1, 2, 3]);
  }
  if (demoId === 'landing-lab') {
    return hasExactKeys(input, ['mode', 'preset', 'cases', 'seed']) &&
      isOneOf(input.mode, ['cpu', 'gpu', 'both']) && input.preset === 'lander' &&
      isOneOf(input.cases, [2048, 8192]) && isOneOf(input.seed, [1, 2, 3]);
  }
  if (demoId === 'tiny-robot') {
    return hasExactKeys(input, ['mode', 'preset', 'cases', 'seed']) &&
      input.mode === 'gpu' && input.preset === 'robot' &&
      isOneOf(input.cases, [128, 512]) && isOneOf(input.seed, [1, 2, 3]);
  }
  if (demoId === 'shader-sculptor') {
    const common = isOneOf(input.budget_ms, [1500, 3000, 12000, 24000, 48000]) && isOneOf(input.seed, [1, 2, 3]);
    return common && (
      (hasExactKeys(input, ['targetPreset', 'resolution', 'budget_ms', 'seed']) &&
        isOneOf(input.targetPreset, ['planet', 'bloom', 'city']) &&
        isOneOf(input.resolution, [128, 256, 512, 1024, 2048])) ||
      (hasExactKeys(input, ['targetRgb', 'width', 'height', 'budget_ms', 'seed']) &&
        isOneOf(input.width, [128, 256, 512, 1024, 2048]) &&
        input.height === input.width && isCanonicalRgb(input.targetRgb, input.width))
    );
  }
  return hasExactKeys(input, ['tolerance', 'seed']) &&
    isOneOf(input.tolerance, [0.001, 0.0001, 0.00001]) && isOneOf(input.seed, [1, 2, 3]);
}

function validRequest(value) {
  if (!isObject(value)) return false;
  if (!hasExactKeys(value, ['jobId', 'demoId', 'input', 'callbackUrl', 'callbackTicket'])) return false;
  if (typeof value.jobId !== 'string' || !jobIdPattern.test(value.jobId)) return false;
  if (typeof value.demoId !== 'string' || !validDemoIds.has(value.demoId)) return false;
  if (typeof value.callbackUrl !== 'string' || typeof value.callbackTicket !== 'string' || value.callbackTicket.length < 16 || value.callbackTicket.length > 512) return false;
  try {
    const callbackUrl = new URL(value.callbackUrl);
    if (
      callbackUrl.origin !== gatewayOrigin ||
      callbackUrl.pathname !== `/v1/internal/jobs/${value.jobId}` ||
      callbackUrl.search ||
      callbackUrl.hash
    ) return false;
  } catch {
    return false;
  }
  return validInput(value.demoId, value.input);
}

function jobRecordPath(jobId) {
  return join(stateDirectory, `${jobId}.json`);
}

function readRecord(jobId) {
  try {
    const value = JSON.parse(readFileSync(jobRecordPath(jobId), 'utf8'));
    if (
      isObject(value) && value.version === 1 && value.jobId === jobId &&
      isOneOf(value.status, ['running', 'succeeded', 'failed', 'cancelled']) &&
      typeof value.updatedAt === 'string'
    ) return value;
  } catch {
    return undefined;
  }
  return undefined;
}

function writeRecord(jobId, status, error, sequence) {
  const path = jobRecordPath(jobId);
  const record = { version: 1, jobId, status, updatedAt: new Date().toISOString(), ...(error ? { error } : {}), ...(Number.isInteger(sequence) ? { sequence } : {}) };
  const temporary = `${path}.${process.pid}.tmp`;
  writeFileSync(temporary, JSON.stringify(record), { encoding: 'utf8', mode: 0o600 });
  renameSync(temporary, path);
  return record;
}

function claimRecord(jobId) {
  const path = jobRecordPath(jobId);
  try {
    const fd = openSync(path, 'wx', 0o600);
    const record = { version: 1, jobId, status: 'running', updatedAt: new Date().toISOString() };
    writeFileSync(fd, JSON.stringify(record), 'utf8');
    closeSync(fd);
    return { claimed: true, record };
  } catch (error) {
    if (error && typeof error === 'object' && error.code === 'EEXIST') {
      return { claimed: false, record: readRecord(jobId) };
    }
    throw error;
  }
}

function clearStaleRecords() {
  for (const name of readdirSync(stateDirectory)) {
    const match = /^([0-9a-f-]{36})\.json$/i.exec(name);
    if (!match || !jobIdPattern.test(match[1])) continue;
    const record = readRecord(match[1]);
    const expired = !record || Date.now() - Date.parse(record.updatedAt) >= terminalRecordLifetimeMs;
    if (record?.status === 'running' || expired) rmSync(jobRecordPath(match[1]), { force: true });
  }
}

clearStaleRecords();

function binary(...parts) {
  const file = resolve(repositoryRoot, ...parts);
  if (relative(repositoryRoot, file).startsWith('..')) throw new Error('Resolved engine escaped the repository.');
  const info = statSync(file);
  if (!info.isFile()) throw new Error('Configured engine binary is unavailable.');
  return file;
}

function pack(red, green, blue) {
  return (((red & 255) << 16) | ((green & 255) << 8) | (blue & 255)) >>> 0;
}

function sculptorPreset(name, size = 128) {
  const output = [];
  for (let iy = 0; iy < size; iy += 1) {
    for (let ix = 0; ix < size; ix += 1) {
      const x = ix * 128 / size;
      const y = iy * 128 / size;
      let red;
      let green;
      let blue;
      if (name === 'planet') {
        red = 18 + Math.trunc(y * 0.25); green = 14 + Math.trunc(y * 0.1); blue = 45 + Math.trunc(y * 0.3);
        const distance = Math.hypot(x - 76, y - 48);
        if (distance < 29) { red = 240 - Math.trunc(distance * 1.8); green = 115 + Math.trunc(y * 0.6); blue = 80 + Math.trunc(distance * 2); }
        const ring = ((x - 73) + (y - 50) * 1.9) ** 2 / 58 ** 2 + ((y - 50) - (x - 73) * 0.12) ** 2 / 9 ** 2;
        if (ring > 0.8 && ring < 1.2 && (y > 47 || distance > 29)) { red = 100; green = 235; blue = 221; }
        const horizon = 93 + 8 * Math.sin(x * 0.09) + 9 * Math.sin(x * 0.23);
        if (y > horizon) { red = 20; green = 43 + Math.trunc((y - 90) * 0.5); blue = 62; }
        if (y > horizon + 12) { red = 9; green = 23; blue = 35; }
      } else if (name === 'bloom') {
        const dx = x - 64; const dy = y - 64; const radius = Math.hypot(dx, dy); const angle = Math.atan2(dy, dx); const edge = 31 + 15 * Math.cos(angle * 7);
        red = 15 + Math.trunc(Math.max(0, 42 - radius) * 0.6); green = 12; blue = 35 + Math.trunc(Math.max(0, 60 - radius) * 0.8);
        if (radius < edge) { red = 150 + Math.trunc(90 * (1 - radius / 48)); green = 55 + Math.trunc(95 * (1 - radius / 48)); blue = 170 + Math.trunc(65 * radius / 48); }
        if (radius < edge && radius > edge - 3) { red = 255; green = 157; blue = 218; }
        if (radius < 10) { red = 255; green = 212; blue = 118; }
        if (radius > 53 && radius < 55) { red = 80; green = 183; blue = 190; }
      } else {
        red = 35 + Math.trunc(y * 0.28); green = 18 + Math.trunc(y * 0.08); blue = 68 + Math.trunc(y * 0.23);
        if (Math.hypot(x - 91, y - 28) < 17) { red = 246; green = 176; blue = 104; }
        const heights = [85, 65, 93, 42, 78, 54, 88, 69, 94, 49, 81]; const index = Math.min(10, Math.floor(x / 12));
        if (y > heights[index]) { red = 12; green = 29; blue = 46; if (x % 12 === 1 || x % 12 === 2) { red = 75; green = 179; blue = 184; } if (x % 12 > 4 && x % 12 < 8 && y % 10 > 2 && y % 10 < 5) { red = 237; green = 124; blue = 163; } }
        if (y > 112) { red = 21; green = 34; blue = 48; }
      }
      output.push(pack(red, green, blue));
    }
  }
  return output;
}

function decodeSculptorTarget(input) {
  const byteLength = expectedRgbBytes(input.width);
  const raw = Buffer.from(input.targetRgb, 'base64');
  if (raw.byteLength !== byteLength || raw.toString('base64') !== input.targetRgb) {
    throw new Error('The custom image target is invalid.');
  }
  const target = new Array(input.width * input.height);
  for (let index = 0; index < target.length; index += 1) {
    target[index] = pack(raw[index * 3], raw[index * 3 + 1], raw[index * 3 + 2]);
  }
  return target;
}

function sculptorPreview(raw, width, side) {
  if (width <= side) return raw.toString('base64');
  const preview = Buffer.alloc(side * side * 3);
  for (let index = 0; index < side * side; index += 1) {
    const x = (index % side) * width / side | 0;
    const y = (index / side | 0) * width / side | 0;
    const source = (y * width + x) * 3;
    const destination = index * 3;
    preview[destination] = raw[source];
    preview[destination + 1] = raw[source + 1];
    preview[destination + 2] = raw[source + 2];
  }
  return preview.toString('base64');
}

/**
 * A full-resolution drawing is far larger than one job event may carry, so the
 * browser receives a bounded preview and the reported dimensions. Nothing reads
 * the untouched raster, so it is not archived. The terminal event also carries
 * the exported program and its shape list, so the preview is shrunk until the
 * whole event fits rather than failing an otherwise successful search.
 */
/**
 * The engine already caps in-progress previews, but at 512 a side they still
 * dominate a run's callback volume and the bytes the browser downloads. The
 * preview's own side comes from its raster length: a progress event reports the
 * search resolution in `width`, not the size of the image it carries.
 */
function withSculptorProgress(job, event) {
  if (job.request.demoId !== 'shader-sculptor' || event.kind !== 'progress') return event;
  if (typeof event.image !== 'string') return event;
  const raw = Buffer.from(event.image, 'base64');
  const side = Math.sqrt(raw.byteLength / 3);
  if (!Number.isInteger(side) || side <= 256) return event;
  return { ...event, image: sculptorPreview(raw, side, 256) };
}

function withSculptorResult(job, event) {
  if (job.request.demoId !== 'shader-sculptor' || event.kind !== 'done') return event;
  const width = Number(event.width);
  const height = Number(event.height);
  if (!isOneOf(width, [128, 256, 512, 1024, 2048]) || height !== width || typeof event.image !== 'string') {
    throw new Error('The GPU worker returned an invalid image result.');
  }
  const raw = Buffer.from(event.image, 'base64');
  if (raw.byteLength !== expectedRgbBytes(width) || raw.toString('base64') !== event.image) {
    throw new Error('The GPU worker returned a malformed image result.');
  }
  const { image, ...rest } = event;
  const overhead = Buffer.byteLength(JSON.stringify({ ...rest, image: '' }), 'utf8');
  for (const side of [512, 384, 256, 128]) {
    if (side > width) continue;
    const preview = sculptorPreview(raw, width, side);
    if (overhead + preview.length <= maximumEventBytes) {
      return { ...rest, image: preview, imagePreview: { width: Math.min(width, side) } };
    }
  }
  throw new Error('The GPU worker returned a result that cannot be delivered.');
}

function commandFor(request) {
  const { demoId, input } = request;
  if (demoId === 'shader-detective') {
    return { executable: binary('apps', 'shader-detective', 'engine', 'target', 'release', 'shader-detective'), args: [input.mode, input.preset, String(input.cases), String(input.seed)] };
  }
  if (demoId === 'landing-lab') {
    return { executable: binary('apps', 'landing-lab', 'engine', 'target', 'release', 'landing-lab'), args: [input.mode, input.preset, String(input.cases), String(input.seed)] };
  }
  if (demoId === 'tiny-robot') {
    return { executable: binary('apps', 'tiny-robot', 'engine', 'target', 'release', 'tiny-robot'), args: [input.mode, input.preset, String(input.cases), String(input.seed)] };
  }
  if (demoId === 'shader-sculptor') {
    const target = input.targetRgb ? decodeSculptorTarget(input) : sculptorPreset(input.targetPreset, input.resolution);
    return {
      executable: binary('apps', 'shader-sculptor', 'engine', 'target', 'release', 'shader-sculptor'),
      args: [],
      stdin: JSON.stringify({ target, seed: input.seed, budget_ms: input.budget_ms })
    };
  }
  return {
    executable: binary('apps', 'orbit-forge', 'engine', 'target', 'release', 'orbit-forge'),
    args: [],
    stdin: JSON.stringify(input)
  };
}

async function authenticate(request) {
  const header = request.headers.authorization;
  if (!header?.startsWith('Bearer ')) throw new Error('Missing bearer token.');
  const { payload } = await jwtVerify(header.slice('Bearer '.length), jwks, {
    issuer: oidcIssuer,
    audience: oidcAudience
  });
  if (payload.sub !== oidcSubject) throw new Error('Unexpected OIDC subject.');
}

async function readJson(request, maximumBytes) {
  const type = request.headers['content-type']?.split(';', 1)[0];
  if (type !== 'application/json') throw new Error('JSON is required.');
  const declaredLength = Number(request.headers['content-length'] ?? 0);
  if (!Number.isFinite(declaredLength) || declaredLength < 1 || declaredLength > maximumBytes) throw new Error('Invalid body size.');
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > maximumBytes) throw new Error('Request body is too large.');
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}

function send(response, status, body) {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    'cache-control': 'no-store',
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(payload),
    'x-content-type-options': 'nosniff'
  });
  response.end(payload);
}

const activeJobs = new Map();

function sleep(milliseconds) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));
}

/** Holds an event until a flush boundary so one callback can carry several. */
async function collect(job, event) {
  const bytes = Buffer.byteLength(JSON.stringify(event), 'utf8');
  if (job.pending.length && job.pendingBytes + bytes > maximumBatchBytes) await flush(job);
  job.pending.push(event);
  job.pendingBytes += bytes;
  if (immediateKinds.has(event.kind) || Date.now() - job.lastFlush >= flushIntervalMs) {
    await flush(job);
  }
}

async function flush(job) {
  if (!job.pending.length) return;
  const events = job.pending;
  job.pending = [];
  job.pendingBytes = 0;
  job.lastFlush = Date.now();
  await report(job, { status: 'running', events });
}

async function report(job, update) {
  const sequence = job.sequence;
  job.sequence += 1;
  const payload = JSON.stringify({ sequence, ...update });
  let failure;
  for (let attempt = 0; attempt < callbackAttempts; attempt += 1) {
    try {
      const response = await fetch(job.request.callbackUrl, {
        method: 'POST',
        headers: {
          authorization: `Bearer ${job.request.callbackTicket}`,
          'content-type': 'application/json'
        },
        body: payload,
        redirect: 'error',
        signal: AbortSignal.timeout(10_000)
      });
      if (response.ok) return;
      failure = new Error(`Gateway callback failed with ${response.status}.`);
    } catch (error) {
      failure = error;
    }
    await sleep(250 * (attempt + 1));
  }
  throw failure ?? new Error('Gateway callback failed.');
}

function terminate(job) {
  const child = job.child;
  if (!child || child.exitCode !== null || child.killed) return;
  try { process.kill(-child.pid, 'SIGTERM'); } catch { child.kill('SIGTERM'); }
  const escalation = setTimeout(() => {
    if (child.exitCode === null) {
      try { process.kill(-child.pid, 'SIGKILL'); } catch { child.kill('SIGKILL'); }
    }
  }, 5_000);
  escalation.unref();
}

async function* lines(stream) {
  let buffered = '';
  for await (const chunk of stream) {
    buffered += chunk.toString('utf8');
    if (Buffer.byteLength(buffered, 'utf8') > maximumEngineLineBytes + 1) throw new Error('Engine output exceeds the read limit.');
    let boundary;
    while ((boundary = buffered.indexOf('\n')) >= 0) {
      const line = buffered.slice(0, boundary);
      buffered = buffered.slice(boundary + 1);
      if (Buffer.byteLength(line, 'utf8') > maximumEngineLineBytes) throw new Error('Engine event exceeds the read limit.');
      if (line.trim()) yield line;
    }
  }
  if (buffered.trim()) yield buffered;
}

async function finish(job, status, error) {
  await flush(job).catch(() => undefined);
  writeRecord(job.request.jobId, status, error, job.sequence);
  try {
    await report(job, { status, ...(error ? { error } : {}) });
  } catch {
    // The durable terminal marker prevents a duplicate queue delivery from launching the job.
  }
}

async function run(job) {
  const timeout = setTimeout(() => {
    job.cancelled = true;
    job.timedOut = true;
    terminate(job);
  }, jobTimeoutMs);
  timeout.unref();
  try {
    if (job.cancelled) {
      await finish(job, 'cancelled', 'The GPU run was cancelled.');
      return;
    }
    const command = commandFor(job.request);
    await report(job, { status: 'running' });

    const child = spawn(command.executable, command.args, {
      cwd: repositoryRoot,
      detached: true,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { PATH: process.env.PATH ?? '/usr/bin:/bin' }
    });
    job.child = child;
    let spawnFailure;
    child.once('error', (error) => { spawnFailure = error; });
    let diagnostic = '';
    child.stderr.on('data', (chunk) => {
      if (diagnostic.length < 4096) diagnostic += chunk.toString('utf8');
    });
    if (command.stdin) child.stdin.end(command.stdin);
    else child.stdin.end();

    const closed = new Promise((resolveClose) => child.once('close', resolveClose));
    const emit = (value) => collect(job, value);
    let sawDone = false;
    for await (const line of lines(child.stdout)) {
      let event;
      try {
        event = JSON.parse(line);
      } catch {
        // Engines report setup failures as plain text. Surfacing them as a
        // diagnostic keeps an infrastructure fault from looking like a result.
        const message = line.trim().slice(0, 500);
        if (message) await emit({ kind: 'diagnostic', message });
        continue;
      }
      if (!isObject(event) || typeof event.kind !== 'string' || event.kind.length > 64) continue;
      event = await withOrbitForgeBenchmark(job, event, emit);
      event = withSculptorProgress(job, event);
      event = withSculptorResult(job, event);
      if (Buffer.byteLength(JSON.stringify(event), 'utf8') > maximumEventBytes) throw new Error('Engine event exceeds the event limit.');
      if (event.kind === 'done') sawDone = true;
      await collect(job, event);
    }
    await closed;
    if (spawnFailure) throw spawnFailure;
    if (job.cancelled) {
      await finish(job, 'cancelled', job.timedOut ? 'The GPU run timed out.' : 'The GPU run was cancelled.');
    } else if (child.exitCode === 0 && sawDone) {
      await finish(job, 'succeeded');
    } else {
      const message = diagnostic.trim().split('\n').at(-1)?.slice(0, 300);
      if (message) await emit({ kind: 'diagnostic', message });
      await finish(job, 'failed', 'The GPU run did not complete.');
    }
  } catch (error) {
    terminate(job);
    await finish(job, job.cancelled ? 'cancelled' : 'failed', job.cancelled ? 'The GPU run was cancelled.' : 'The GPU run did not complete.');
  } finally {
    clearTimeout(timeout);
    activeJobs.delete(job.request.jobId);
  }
}

const orbitForgeSource = () => resolve(repositoryRoot, 'apps', 'orbit-forge');

/**
 * Orbit Forge's native-runtime evidence is produced outside the engine: the
 * winner's Gremlin LLVM is compiled and timed against a reference solver. The
 * original demo server did this, so a live run here must too, or the island's
 * "Solver and validation" panel silently loses the compiled comparison that
 * the recorded run shows.
 */
async function withOrbitForgeBenchmark(job, event, emit) {
  if (job.request.demoId !== 'orbit-forge' || event.kind !== 'done') return event;
  await emit({ kind: 'stage', message: 'Compiling the winner and checking native runtime' });
  const value = { ...event };
  if (value.passed) {
    try {
      value.benchmark = await benchmark(value, orbitForgeSource());
    } catch (error) {
      if (error instanceof AccuracyFailure) {
        value.passed = false;
        value.failures = (value.failures ?? 0) + 1;
        value.checked = (value.checked ?? 0) + 1;
        value.max_error = Math.max(value.max_error ?? 0, error.error);
        value.benchmark = {
          available: false,
          reason: error.message,
          counterexample: { m: error.m, e: error.e, error: error.error }
        };
      } else {
        value.benchmark = { available: false, reason: error?.message ?? 'The native benchmark did not run.' };
      }
    }
  }
  // The browser never reads the LLVM text and it is large; the original server dropped it too.
  delete value.llvm;
  return value;
}

function refreshTerminalCallback(record, request) {
  const job = {
    request,
    sequence: Number.isInteger(record.sequence) ? record.sequence : 0,
    pending: [],
    pendingBytes: 0,
    lastFlush: Date.now()
  };
  void report(job, { status: record.status, ...(record.error ? { error: record.error } : {}) }).catch(() => undefined);
}

async function jobs(request, response) {
  await authenticate(request);
  if (request.method === 'POST') {
    const body = await readJson(request, maximumRequestBytes);
    if (!validRequest(body) || request.headers['idempotency-key'] !== body.jobId) return send(response, 400, { error: 'Unsupported job request.' });

    const existing = activeJobs.get(body.jobId);
    if (existing) {
      existing.request = body;
      return send(response, 202, { accepted: true });
    }
    const record = readRecord(body.jobId);
    if (record?.status === 'running') return send(response, 202, { accepted: true });
    if (record && Date.now() - Date.parse(record.updatedAt) < terminalRecordLifetimeMs) {
      refreshTerminalCallback(record, body);
      return send(response, 202, { accepted: true });
    }
    if (record) rmSync(jobRecordPath(body.jobId), { force: true });
    if (activeJobs.size >= 1) return send(response, 429, { error: 'GPU worker is busy.' });

    const claim = claimRecord(body.jobId);
    if (!claim.claimed) return send(response, 202, { accepted: true });
    const job = {
      request: body,
      child: undefined,
      cancelled: false,
      timedOut: false,
      sequence: 0,
      pending: [],
      pendingBytes: 0,
      lastFlush: Date.now()
    };
    activeJobs.set(body.jobId, job);
    void run(job);
    return send(response, 202, { accepted: true });
  }

  if (request.method === 'DELETE') {
    const jobId = new URL(request.url, 'http://worker.invalid').pathname.split('/').at(-1);
    if (!jobId || !jobIdPattern.test(jobId)) return send(response, 404, { error: 'Job not found.' });
    const job = activeJobs.get(jobId);
    if (!job) return send(response, 404, { error: 'Job not found.' });
    job.cancelled = true;
    terminate(job);
    return send(response, 202, { cancelled: true });
  }

  return send(response, 405, { error: 'Method not allowed.' });
}

const server = createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url, 'http://worker.invalid').pathname;
    if (request.method === 'GET' && pathname === '/healthz') return send(response, 200, { ok: true });
    if (pathname === '/v1/jobs' || pathname.startsWith('/v1/jobs/')) return await jobs(request, response);
    return send(response, 404, { error: 'Not found.' });
  } catch (error) {
    return send(response, 401, { error: 'Unauthorized.' });
  }
});

server.listen(port, '127.0.0.1', () => {
  console.log(`Gremlin GPU worker listening on 127.0.0.1:${port}`);
});
