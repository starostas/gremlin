import { useEffect, useRef, useState } from 'preact/hooks';
import ShaderSculptorParity, { type ShaderSculptorRunInput } from './ShaderSculptorParity';
import TinyRobotParity from './TinyRobotParity';
import { CodeBlock, Metric, Outcome, Section, canvasPalette, useTheme } from './IslandChrome';

export type DemoId =
  | 'shader-detective'
  | 'landing-lab'
  | 'tiny-robot'
  | 'shader-sculptor'
  | 'orbit-forge';

type Json = Record<string, any>;

type TurnstileApi = {
  render: (element: HTMLElement, options: Record<string, unknown>) => string;
  execute: (widgetId: string) => void;
  remove: (widgetId: string) => void;
};

declare global {
  interface Window {
    turnstile?: TurnstileApi;
  }
}

/** The job routes deploy with this site, so they are always same-origin. */
const API_BASE = '/api';

/** Live runs stay off unless the deployment opts in, keeping forks replay-only. */
const liveRunsEnabled = import.meta.env.PUBLIC_EXPERIMENT_LIVE_RUNS === 'true';

let turnstileLoader: Promise<TurnstileApi> | undefined;

function loadTurnstile() {
  if (window.turnstile) return Promise.resolve(window.turnstile);
  if (turnstileLoader) return turnstileLoader;
  turnstileLoader = new Promise<TurnstileApi>((resolve, reject) => {
    const complete = () => window.turnstile ? resolve(window.turnstile) : reject(new Error('Human verification did not load.'));
    const existing = document.querySelector<HTMLScriptElement>('script[data-gremlin-turnstile]');
    if (existing) {
      existing.addEventListener('load', complete, { once: true });
      existing.addEventListener('error', () => reject(new Error('Human verification did not load.')), { once: true });
      return;
    }
    const script = document.createElement('script');
    script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';
    script.async = true;
    script.defer = true;
    script.dataset.gremlinTurnstile = 'true';
    script.addEventListener('load', complete, { once: true });
    script.addEventListener('error', () => reject(new Error('Human verification did not load.')), { once: true });
    document.head.append(script);
  }).catch((error) => {
    turnstileLoader = undefined;
    throw error;
  });
  return turnstileLoader;
}

function useHumanCheck() {
  const container = useRef<HTMLDivElement>(null);
  const siteKey = import.meta.env.PUBLIC_EXPERIMENT_TURNSTILE_SITE_KEY;

  const requestToken = async () => {
    if (!siteKey) return undefined;
    const target = container.current;
    if (!target) throw new Error('Human verification is not ready.');
    const turnstile = await loadTurnstile();
    return new Promise<string>((resolve, reject) => {
      let widgetId = '';
      let timeout = 0;
      const finish = (callback: () => void) => {
        window.clearTimeout(timeout);
        if (widgetId) turnstile.remove(widgetId);
        callback();
      };
      timeout = window.setTimeout(() => finish(() => reject(new Error('Human verification timed out.'))), 20_000);
      widgetId = turnstile.render(target, {
        sitekey: siteKey,
        size: 'invisible',
        action: 'gremlin_gpu_job',
        callback: (token: string) => finish(() => resolve(token)),
        'error-callback': () => finish(() => reject(new Error('Human verification failed.'))),
        'expired-callback': () => finish(() => reject(new Error('Human verification expired.')))
      });
      turnstile.execute(widgetId);
    });
  };

  return { container, requestToken };
}

const samplePath = (demo: DemoId) => `/experiments-data/${demo}/sample.json`;

const formatNumber = (value: number | undefined) =>
  typeof value === 'number' ? new Intl.NumberFormat('en-US').format(value) : '—';

const last = <T,>(items: T[], predicate: (item: T) => boolean) => {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (predicate(items[index])) return items[index];
  }
  return undefined;
};

function downloadText(filename: string, text?: string) {
  if (!text) return;
  const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function imageBytes(data: Json | undefined, image: unknown) {
  const key = typeof image === 'string' ? image : (image as Json | undefined)?.image;
  if (!key) return undefined;

  try {
    return atob(data?.images?.[key] ?? key);
  } catch {
    return undefined;
  }
}

function drawRgb(canvas: HTMLCanvasElement | null, data: Json | undefined, image: unknown, width?: number, height?: number) {
  if (!canvas || !width || !height) return;
  const raw = imageBytes(data, image);
  if (!raw) return;

  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) return;
  const pixels = context.createImageData(width, height);
  for (let index = 0; index < width * height; index += 1) {
    pixels.data[index * 4] = raw.charCodeAt(index * 3);
    pixels.data[index * 4 + 1] = raw.charCodeAt(index * 3 + 1);
    pixels.data[index * 4 + 2] = raw.charCodeAt(index * 3 + 2);
    pixels.data[index * 4 + 3] = 255;
  }
  context.putImageData(pixels, 0, 0);
}

function drawPacked(canvas: HTMLCanvasElement | null, values: unknown, width?: number, height?: number) {
  if (!canvas || !width || !height || !Array.isArray(values) || values.length !== width * height) return;
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) return;
  const pixels = context.createImageData(width, height);
  values.forEach((value, index) => {
    if (typeof value !== 'number' || !Number.isInteger(value)) return;
    const packed = value >>> 0;
    pixels.data[index * 4] = packed >>> 16;
    pixels.data[index * 4 + 1] = (packed >>> 8) & 255;
    pixels.data[index * 4 + 2] = packed & 255;
    pixels.data[index * 4 + 3] = 255;
  });
  context.putImageData(pixels, 0, 0);
}

function drawPixels(canvas: HTMLCanvasElement | null, data: Json | undefined, value: unknown, width?: number, height?: number) {
  if (Array.isArray(value)) drawPacked(canvas, value, width, height);
  else drawRgb(canvas, data, value, width, height);
}

function useDemoRun(demo: DemoId) {
  const [data, setData] = useState<Json>();
  const [events, setEvents] = useState<Json[]>([]);
  const [cursor, setCursor] = useState(-1);
  const [playing, setPlaying] = useState(false);
  const [job, setJob] = useState<{ id: string; capability?: string }>();
  const [error, setError] = useState<string>();
  const requestEpoch = useRef(0);
  const requestController = useRef<AbortController>();
  const pollTimer = useRef<number>();
  /** Events already delivered for the active run, so polls can ask only for new ones. */
  const liveEvents = useRef<Json[]>([]);
  const humanCheck = useHumanCheck();

  useEffect(() => {
    let mounted = true;
    fetch(samplePath(demo))
      .then((response) => {
        if (!response.ok) throw new Error('The recorded run is unavailable.');
        return response.json();
      })
      .then((recording) => {
        if (!mounted) return;
        setData(recording);
        setEvents(recording.events ?? []);
        const completed = (recording.events ?? []).map((event: Json) => event.kind).lastIndexOf('done');
        setCursor(completed >= 0 ? completed : Math.max(0, (recording.events?.length ?? 1) - 1));
      })
      .catch((reason) => mounted && setError(reason.message));
    return () => {
      mounted = false;
    };
  }, [demo]);

  useEffect(() => () => {
    requestEpoch.current += 1;
    requestController.current?.abort();
    if (pollTimer.current) window.clearTimeout(pollTimer.current);
  }, []);

  useEffect(() => {
    if (!playing || cursor < 0) return;
    if (cursor >= events.length - 1) {
      setPlaying(false);
      return;
    }
    const timeout = window.setTimeout(() => setCursor((value) => value + 1), 110);
    return () => window.clearTimeout(timeout);
  }, [cursor, events.length, playing]);

  const replay = () => {
    if (!events.length) return;
    requestEpoch.current += 1;
    requestController.current?.abort();
    if (pollTimer.current) window.clearTimeout(pollTimer.current);
    setError(undefined);
    setJob(undefined);
    setCursor(0);
    setPlaying(true);
  };

  const cancel = async () => {
    if (!job) return;
    const api = API_BASE;
    const activeJob = job;
    requestEpoch.current += 1;
    requestController.current?.abort();
    if (pollTimer.current) window.clearTimeout(pollTimer.current);
    try {
      const response = await fetch(`${api}/v1/jobs/${activeJob.id}`, {
        method: 'DELETE',
        headers: activeJob.capability ? { 'X-Experiment-Capability': activeJob.capability } : undefined
      });
      const body = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(body.error ?? 'Unable to cancel the GPU job.');
      setJob(undefined);
      setError('Cancellation requested.');
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Unable to cancel the GPU job.');
    }
  };

  const runGpu = async (input: Json) => {
    const api = API_BASE;

    const epoch = requestEpoch.current + 1;
    requestEpoch.current = epoch;
    requestController.current?.abort();
    if (pollTimer.current) window.clearTimeout(pollTimer.current);
    const controller = new AbortController();
    requestController.current = controller;

    setError(undefined);
    setPlaying(false);
    setEvents([]);
    setCursor(-1);
    liveEvents.current = [];

    try {
      const humanToken = await humanCheck.requestToken();
      if (epoch !== requestEpoch.current) return;
      const response = await fetch(`${api}/v1/jobs`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(humanToken ? { 'X-Turnstile-Token': humanToken } : {})
        },
        body: JSON.stringify({ demoId: demo, input }),
        signal: controller.signal
      });
      const created = await response.json().catch(() => ({}));
      if (epoch !== requestEpoch.current) return;
      if (!response.ok) throw new Error(created.error ?? 'Unable to queue the GPU run.');
      if (!created.id) throw new Error('The GPU gateway returned an invalid job id.');

      const activeJob = { id: created.id as string, capability: created.capability as string | undefined };
      setJob(activeJob);

      let delay = 500;
      let cursor = 0;
      const poll = async (): Promise<void> => {
        if (epoch !== requestEpoch.current) return;
        try {
          const statusResponse = await fetch(`${api}/v1/jobs/${activeJob.id}?after=${cursor}`, {
            headers: activeJob.capability
              ? { 'X-Experiment-Capability': activeJob.capability }
              : undefined,
            signal: controller.signal
          });
          const status = await statusResponse.json().catch(() => ({}));
          if (epoch !== requestEpoch.current) return;
          if (!statusResponse.ok) throw new Error(status.error ?? 'Unable to read GPU job status.');

          if (Array.isArray(status.events) && status.events.length) {
            liveEvents.current = [...liveEvents.current, ...status.events];
            const delivered = liveEvents.current;
            setData((previous) => ({ ...previous, events: delivered }));
            setEvents(delivered);
            setCursor(delivered.length - 1);
          }
          if (Number.isInteger(status.cursor)) cursor = status.cursor;

          if (status.status === 'failed' || status.status === 'cancelled') {
            setJob(undefined);
            setError(status.error ?? (status.status === 'cancelled' ? 'The GPU run was cancelled.' : 'The GPU run failed.'));
            return;
          }
          if (status.status === 'succeeded') {
            setJob(undefined);
            return;
          }

          delay = Math.min(2500, Math.round(delay * 1.25));
          pollTimer.current = window.setTimeout(() => void poll(), delay);
        } catch (reason) {
          if (epoch !== requestEpoch.current || controller.signal.aborted) return;
          setJob(undefined);
          setError(reason instanceof Error ? reason.message : 'Unable to read GPU job status.');
        }
      };
      void poll();
    } catch (reason) {
      if (epoch !== requestEpoch.current || controller.signal.aborted) return;
      setJob(undefined);
      setError(reason instanceof Error ? reason.message : 'Unable to queue the GPU run.');
    }
  };

  const current =
    cursor >= 0
      ? last(events.slice(0, cursor + 1), (event) => ['start', 'progress', 'done'].includes(event.kind))
      : undefined;

  return {
    data,
    events,
    current,
    playing,
    job,
    error,
    gatewayReady: liveRunsEnabled,
    requestHumanToken: humanCheck.requestToken,
    humanCheck: <div ref={humanCheck.container} class="experiment-human-check" aria-hidden="true" />,
    replay,
    cancel,
    runGpu
  };
}

function IslandToolbar({
  children,
  humanCheck,
  replay,
  runGpu,
  cancel,
  job,
  gatewayReady
}: {
  children?: any;
  humanCheck?: any;
  replay: () => void;
  runGpu: () => void;
  cancel: () => void;
  job?: { id: string };
  gatewayReady: boolean;
}) {
  return (
    <>
      {gatewayReady && humanCheck}
      <Section title="Setup">
      <div class="experiment-toolbar">
        {children && <div class="experiment-fields">{children}</div>}
        <div class="experiment-actions">
          <button type="button" class="experiment-button" onClick={replay}>
            Replay
          </button>
          {gatewayReady && job ? (
            <button type="button" class="experiment-button" onClick={cancel}>
              Stop
            </button>
          ) : gatewayReady ? (
            <button
              type="button"
              class="experiment-button experiment-button-primary"
              onClick={runGpu}
            >
              Run
            </button>
          ) : null}
        </div>
      </div>
      </Section>
    </>
  );
}

function IslandStatus({ error, job, current }: { error?: string; job?: { id: string }; current?: Json }) {
  const message = error ?? (job ? 'Running…' : current?.kind === 'progress' ? 'Replaying…' : undefined);
  return message ? <p class={`experiment-status${error ? ' experiment-status-error' : ''}`} aria-live="polite">{message}</p> : null;
}

function drawErrorChart(canvas: HTMLCanvasElement | null, values: Array<[number, number]>, logarithmic = false) {
  if (!canvas) return;
  const width = 420;
  const height = 96;
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) return;
  const palette = canvasPalette(canvas);
  context.fillStyle = palette.ground;
  context.fillRect(0, 0, width, height);
  context.strokeStyle = palette.grid;
  context.lineWidth = 1;
  for (let y = 20; y < height; y += 24) {
    context.beginPath();
    context.moveTo(0, y);
    context.lineTo(width, y);
    context.stroke();
  }
  if (!values.length) return;
  // A solver's error falls by orders of magnitude, so plot it on a log scale;
  // a bit-error rate is a proportion and reads better linearly.
  const scale = (value: number) => (logarithmic ? Math.log10(Math.max(value, 1e-12)) : value);
  const scaled = values.map(([generation, value]) => [generation, scale(value)] as [number, number]);
  const highest = Math.max(...scaled.map(([, value]) => value));
  const lowest = logarithmic ? Math.min(...scaled.map(([, value]) => value)) : 0;
  const span = Math.max(highest - lowest, logarithmic ? 1 : 0.001);
  context.strokeStyle = palette.ink;
  context.lineWidth = 1.5;
  context.beginPath();
  scaled.forEach(([generation, value], index) => {
    const x = 6 + ((generation - 1) / Math.max(1, scaled.at(-1)?.[0] ?? 1)) * (width - 12);
    const y = height - 8 - ((value - lowest) / span) * (height - 22);
    if (index === 0) context.moveTo(x, y);
    else context.lineTo(x, y);
  });
  context.stroke();
}

function ShaderDetective() {
  const demo = useDemoRun('shader-detective');
  const theme = useTheme();
  const inputCanvas = useRef<HTMLCanvasElement>(null);
  const targetCanvas = useRef<HTMLCanvasElement>(null);
  const previewCanvas = useRef<HTMLCanvasElement>(null);
  const chartCanvas = useRef<HTMLCanvasElement>(null);
  const [preset, setPreset] = useState('afterglow');
  const [mode, setMode] = useState<'cpu' | 'gpu' | 'both'>('both');
  const [cases, setCases] = useState(32768);
  const [seed, setSeed] = useState(1);

  const current = demo.current ?? last(demo.events, (event) => event.kind === 'done');
  const currentIndex = Math.max(0, demo.events.indexOf(current));
  const visible = demo.events.slice(0, currentIndex + 1);
  const start = last(visible, (event) => event.kind === 'start');
  const width = start?.width ?? 128;
  const height = start?.height ?? 88;
  const preview = last(visible, (event) => Boolean(event.preview))?.preview;
  const program = last(visible, (event) => typeof event.program === 'string')?.program;
  // Reported only on the terminal event: the search itself never receives it.
  const targetSource = last(visible, (event) => typeof event.target_source === 'string')?.target_source;
  const result = last(visible, (event) => event.kind === 'done');
  // Shader Detective reports one `done` per engine rather than a `comparison`
  // event, so the CPU/GPU comparison is derived the way the original demo did:
  // the two runs are comparable only when their search states agree.
  const comparison = (() => {
    const cpu = last(visible, (event) => event.kind === 'done' && event.mode === 'cpu');
    const gpu = last(visible, (event) => event.kind === 'done' && event.mode === 'gpu');
    if (!cpu || !gpu || !cpu.search_seconds || !gpu.search_seconds) return undefined;
    return {
      matching: cpu.search_state_hash === gpu.search_state_hash,
      cpu_seconds: cpu.search_seconds,
      gpu_seconds: gpu.search_seconds,
      speedup: cpu.search_seconds / gpu.search_seconds
    };
  })();
  const startIndex = Math.max(0, visible.lastIndexOf(start));
  const curve = visible.slice(startIndex).flatMap((event) =>
    typeof event.generation === 'number' && typeof event.bit_errors === 'number' && typeof event.cases === 'number'
      ? [[event.generation, event.bit_errors / (event.cases * 32)] as [number, number]]
      : []
  );

  useEffect(() => {
    drawPixels(inputCanvas.current, demo.data, start?.input, width, height);
    drawPixels(targetCanvas.current, demo.data, start?.target, width, height);
    drawPixels(previewCanvas.current, demo.data, preview, width, height);
  }, [demo.data, height, preview, start, width]);
  useEffect(() => drawErrorChart(chartCanvas.current, curve), [curve, theme]);

  const mismatches = current?.mismatches;
  const accuracy =
    typeof mismatches === 'number' && current?.cases
      ? `${((1 - mismatches / current.cases) * 100).toFixed(1)}%`
      : current?.success
        ? '100%'
        : undefined;

  return (
    <section class="experiment-island" aria-label="Shader Detective interactive demo">
      <IslandToolbar
        humanCheck={demo.humanCheck}
        replay={demo.replay}
        runGpu={() => demo.runGpu({ mode, preset, cases, seed })}
        cancel={demo.cancel}
        job={demo.job}
        gatewayReady={demo.gatewayReady}
      >
        <label>
          Engine
          <select value={mode} onInput={(event) => setMode((event.currentTarget as HTMLSelectElement).value as 'cpu' | 'gpu' | 'both')}>
            <option value="both">CPU + GPU</option>
            <option value="gpu">GPU</option>
            <option value="cpu">CPU</option>
          </select>
        </label>
        <label>
          Transform
          <select value={preset} onInput={(event) => setPreset((event.currentTarget as HTMLSelectElement).value)}>
            <option value="afterglow">Afterglow</option>
            <option value="aurora">Aurora</option>
          </select>
        </label>
        <label>
          Cases
          <select value={cases} onInput={(event) => setCases(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={32768}>32,768</option>
            <option value={8192}>8,192</option>
          </select>
        </label>
        <label>
          Seed
          <select value={seed} onInput={(event) => setSeed(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={1}>1</option>
            <option value={2}>2</option>
            <option value={3}>3</option>
          </select>
        </label>
      </IslandToolbar>
      <IslandStatus error={demo.error} job={demo.job} current={current} />
      <Section title="View">
        <div class="experiment-canvas-grid experiment-canvas-grid-three">
          <figure><canvas ref={inputCanvas} aria-label="Input image" /><figcaption>Input</figcaption></figure>
          <figure><canvas ref={targetCanvas} aria-label="Target image" /><figcaption>Target</figcaption></figure>
          <figure><canvas ref={previewCanvas} aria-label="Candidate image" /><figcaption>Candidate</figcaption></figure>
        </div>
        <div class="experiment-metrics">
          <Metric label="generation" value={current?.generation} />
          <Metric label="match" value={accuracy} better="higher" />
          <Metric label="evaluations" value={formatNumber(current?.evaluations)} />
          <Metric better="lower" label="time" value={typeof result?.search_seconds === 'number' ? `${result.search_seconds.toFixed(2)}s` : typeof current?.elapsed_seconds === 'number' ? `${current.elapsed_seconds.toFixed(2)}s` : undefined} />
        </div>
        <figure class="experiment-chart"><canvas ref={chartCanvas} aria-label="Bit error rate by generation" /><figcaption>Bit error rate</figcaption></figure>
      </Section>
      <Outcome
        targetLabel="Hidden target"
        target={targetSource
          ? <CodeBlock text={targetSource} />
          : <p class="experiment-objective">A packed-colour transform built from rotate, XOR and add. Its arrangement is revealed once a run finishes.</p>}
        resultLabel="Discovered program"
        result={program ? <CodeBlock text={program} /> : undefined}
        note={targetSource && program
          ? 'The search was given the operators and constants but never this arrangement. A match in behaviour does not require a match in structure.'
          : undefined}
      />
      {(result || comparison) && (
        <details class="experiment-details">
          <summary>Evidence</summary>
          {result && <p>{result.success ? 'Validated.' : 'Not validated.'} {result.holdout_mismatches ?? '—'} mismatches across {formatNumber(result.holdout_cases)} fresh colors; preview {result.image_matches ? 'matches' : 'differs'}.</p>}
          {comparison && (
            <p>
              {comparison.matching
                ? `Identical search · CPU ${comparison.cpu_seconds.toFixed(2)}s / GPU ${comparison.gpu_seconds.toFixed(2)}s · ${comparison.speedup.toFixed(2)}× GPU speedup.`
                : 'CPU and GPU search states differ; the comparison is not validated.'}
            </p>
          )}
        </details>
      )}
    </section>
  );
}

function landingTraces(data: Json | undefined, event: Json | undefined, key: string) {
  const value = event?.[key];
  if (Array.isArray(value)) return value;
  const traceKey = value?.trace;
  return traceKey ? data?.traces?.[traceKey] ?? [] : [];
}

function drawLanding(canvas: HTMLCanvasElement | null, traces: Json[], phase: number) {
  if (!canvas) return;
  const width = 720;
  const height = 350;
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) return;
  const palette = canvasPalette(canvas);
  context.fillStyle = palette.ground;
  context.fillRect(0, 0, width, height);
  context.strokeStyle = palette.grid;
  context.lineWidth = 1;
  context.beginPath();
  context.moveTo(0, height - 34);
  context.lineTo(width, height - 34);
  context.stroke();
  context.fillStyle = palette.muted;
  context.fillRect(width / 2 - 22, height - 37, 44, 3);
  const x = (value: number) => width / 2 + (value * width) / 1900;
  const y = (value: number) => height - 36 - (Math.max(0, value) * (height - 64)) / 7000;
  const frame = Math.max(0, Math.floor(phase * 255));
  for (const trace of traces) {
    if (!trace.frames?.length) continue;
    const frames = trace.frames as number[][];
    context.beginPath();
    frames.forEach((point, index) => {
      if (index === 0) context.moveTo(x(point[0]), y(point[1]));
      else context.lineTo(x(point[0]), y(point[1]));
    });
    context.globalAlpha = trace.safe ? 0.45 : 0.28;
    context.strokeStyle = trace.safe ? palette.muted : palette.faint;
    context.stroke();
    context.globalAlpha = 1;
    const point = frames[Math.min(frame, frames.length - 1)];
    context.fillStyle = trace.safe ? palette.ink : palette.faint;
    context.fillRect(x(point[0]) - 2, y(point[1]) - 2, 4, 4);
  }
}

function LandingLab() {
  const demo = useDemoRun('landing-lab');
  const theme = useTheme();
  const baselineCanvas = useRef<HTMLCanvasElement>(null);
  const bestCanvas = useRef<HTMLCanvasElement>(null);
  const [mode, setMode] = useState<'cpu' | 'gpu' | 'both'>('both');
  const [cases, setCases] = useState(8192);
  const [seed, setSeed] = useState(1);

  const current = demo.current ?? last(demo.events, (event) => event.kind === 'done');
  const prior = demo.events.slice(0, Math.max(0, demo.events.indexOf(current) + 1));
  const baselineEvent = last(prior, (event) => Boolean(event.baseline));
  const traceEvent = last(prior, (event) => Boolean(event.traces)) ?? current;
  const baseline = landingTraces(demo.data, baselineEvent, 'baseline');
  const best = landingTraces(demo.data, traceEvent, 'traces');
  const result = last(prior, (event) => event.kind === 'done');
  const comparison = last(prior, (event) => event.kind === 'comparison');
  const controller = last(prior, (event) => typeof event.source === 'string');

  useEffect(() => {
    let frame = 0;
    let phase = 0;
    const animate = () => {
      phase = (phase + 0.003) % 1;
      drawLanding(baselineCanvas.current, baseline, phase);
      drawLanding(bestCanvas.current, best, phase);
      frame = requestAnimationFrame(animate);
    };
    animate();
    return () => cancelAnimationFrame(frame);
  }, [baseline, best, theme]);

  return (
    <section class="experiment-island" aria-label="Landing Lab interactive demo">
      <IslandToolbar
        humanCheck={demo.humanCheck}
        replay={demo.replay}
        runGpu={() => demo.runGpu({ mode, cases, seed })}
        cancel={demo.cancel}
        job={demo.job}
        gatewayReady={demo.gatewayReady}
      >
        <label>
          Engine
          <select value={mode} onInput={(event) => setMode((event.currentTarget as HTMLSelectElement).value as 'cpu' | 'gpu' | 'both')}>
            <option value="both">CPU + GPU</option>
            <option value="gpu">GPU</option>
            <option value="cpu">CPU</option>
          </select>
        </label>
        <label>
          Cases
          <select value={cases} onInput={(event) => setCases(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={8192}>8,192</option>
            <option value={2048}>2,048</option>
          </select>
        </label>
        <label>
          Seed
          <select value={seed} onInput={(event) => setSeed(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={1}>1</option>
            <option value={2}>2</option>
            <option value={3}>3</option>
          </select>
        </label>
      </IslandToolbar>
      <IslandStatus error={demo.error} job={demo.job} current={current} />
      <Section title="View">
      <div class="experiment-canvas-grid">
        <figure><canvas ref={baselineCanvas} aria-label="Baseline landing trajectories" /><figcaption>Baseline</figcaption></figure>
        <figure><canvas ref={bestCanvas} aria-label="Selected-controller landing trajectories" /><figcaption>Selected controller</figcaption></figure>
      </div>
      <div class="experiment-metrics">
        <Metric label="tested" value={typeof current?.tested === 'number' ? `${current.tested} / 128` : result?.programs ? `${result.programs} / 128` : undefined} />
        <Metric better="higher" label="safe flights" value={typeof current?.safe === 'number' ? `${formatNumber(current.safe)} / ${formatNumber(current.cases)}` : undefined} />
        <Metric better="lower" label="search" value={typeof current?.seconds === 'number' ? `${current.seconds.toFixed(2)}s` : undefined} />
        <Metric better="higher" label="speedup" value={typeof comparison?.speedup === 'number' ? `${comparison.speedup.toFixed(1)}×` : undefined} />
      </div>
      </Section>
      <Outcome
        targetLabel="Objective"
        target={<p class="experiment-objective">Land every flight safely: touch down on the pad below the impact speed, against delayed thrusters, wind and a fixed fuel budget. No reference controller is supplied — the baseline above is what the untuned starting point achieves.</p>}
        resultLabel="Selected controller"
        result={controller?.source
          ? (
            <>
              {controller.description && <p class="experiment-objective">{controller.description}</p>}
              <CodeBlock text={controller.source} />
            </>
          )
          : undefined}
      />
      {(result || comparison) && (
        <details class="experiment-details">
          <summary>Evidence</summary>
          {result && <p>Holdout: {formatNumber(result.holdout_safe)} / {formatNumber(result.holdout_cases)} safe. {formatNumber(result.executed_steps)} interpreted instructions; {typeof result.total_seconds === 'number' ? `${result.total_seconds.toFixed(2)}s including validation.` : ''}</p>}
          {comparison && <p>{comparison.matching ? `Exact CPU/GPU outcome and step-count parity · ${Number(comparison.total_speedup).toFixed(1)}× including setup and holdout.` : 'CPU/GPU parity did not validate.'}</p>}
        </details>
      )}
    </section>
  );
}

function TinyRobot() {
  const demo = useDemoRun('tiny-robot');
  const [cases, setCases] = useState(128);
  const [seed, setSeed] = useState(1);
  // The episode program is the fixed task every candidate is scored against;
  // only the brain is searched for.
  const episodeSource = last(demo.events, (event) => typeof event.episode_source === 'string')?.episode_source;
  const brainSource = last(demo.events, (event) => typeof event.brain_source === 'string')?.brain_source;

  return (
    <section class="experiment-island" aria-label="Tiny Robot interactive demo">
      <IslandToolbar
        humanCheck={demo.humanCheck}
        replay={demo.replay}
        runGpu={() => demo.runGpu({ cases, seed })}
        cancel={demo.cancel}
        job={demo.job}
        gatewayReady={demo.gatewayReady}
      >
        <label>
          Cases
          <select value={cases} onInput={(event) => setCases(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={128}>128</option>
            <option value={512}>512</option>
          </select>
        </label>
        <label>
          Seed
          <select value={seed} onInput={(event) => setSeed(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={1}>1</option>
            <option value={2}>2</option>
            <option value={3}>3</option>
          </select>
        </label>
      </IslandToolbar>
      <IslandStatus error={demo.error} job={demo.job} current={demo.current} />
      <Section title="View">
        <TinyRobotParity recording={demo.data} events={demo.events} current={demo.current} />
      </Section>
      <Outcome
        targetLabel="The task it runs inside"
        target={episodeSource
          ? <CodeBlock text={episodeSource} />
          : <p class="experiment-objective">Reach the exit carrying the key, in rooms the robot has never seen, using three wall sensors and a single bit of memory.</p>}
        resultLabel="Discovered brain"
        result={brainSource ? <CodeBlock text={brainSource} /> : undefined}
        note={episodeSource && brainSource
          ? 'The task program on the left is fixed and scores every candidate. Only the brain on the right was searched for.'
          : undefined}
      />
    </section>
  );
}

function base64ToBytes(value: string) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

async function sha256Hex(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', bytes as unknown as ArrayBuffer);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function ShaderSculptor() {
  const demo = useDemoRun('shader-sculptor');

  /**
   * A 2,048² raster is far larger than a Vercel Function body may carry, so a
   * chosen image never travels through the job request. The gateway issues a
   * short-lived, digest-bound ticket for one private upload and the job then
   * references that asset; the browser never reaches the GPU host itself.
   */
  const uploadImage = async (api: string, image: NonNullable<ShaderSculptorRunInput['image']>) => {
    const bytes = base64ToBytes(image.rgb);
    const humanToken = await demo.requestHumanToken();
    const authorized = await fetch(`${api}/v1/uploads/authorize`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(humanToken ? { 'X-Turnstile-Token': humanToken } : {})
      },
      body: JSON.stringify({
        resolution: image.width,
        byteLength: bytes.byteLength,
        sha256: await sha256Hex(bytes)
      })
    });
    const asset = await authorized.json().catch(() => ({}));
    if (!authorized.ok) throw new Error(asset.error ?? 'Unable to prepare the image upload.');

    const { upload } = await import('@vercel/blob/client');
    await upload(asset.pathname, new Blob([bytes as unknown as BlobPart]), {
      access: 'private',
      handleUploadUrl: `${api}/v1/uploads`,
      contentType: 'application/octet-stream',
      clientPayload: JSON.stringify({ id: asset.id, ticket: asset.ticket }),
      multipart: bytes.byteLength > 4 * 1024 * 1024
    });
    return { id: asset.id as string, ticket: asset.ticket as string };
  };

  const run = async (input: ShaderSculptorRunInput) => {
    const api = API_BASE;
    const imageAsset = input.image ? await uploadImage(api, input.image) : undefined;
    await demo.runGpu(
      imageAsset
        ? { imageAsset, resolution: input.resolution, budgetMs: input.budgetMs, seed: input.seed }
        : { preset: input.preset, resolution: input.resolution, budgetMs: input.budgetMs, seed: input.seed }
    );
  };

  return (
    <ShaderSculptorParity
      recording={demo.data}
      events={demo.events}
      current={demo.current}
      running={Boolean(demo.job)}
      error={demo.error}
      gatewayReady={demo.gatewayReady}
      humanCheck={demo.humanCheck}
      onRun={run}
      onReplay={demo.replay}
      onStop={demo.cancel}
    />
  );
}

function orbitalTruth(mean: number, eccentricity: number) {
  let low = 0;
  let high = Math.PI;
  for (let index = 0; index < 54; index += 1) {
    const value = (low + high) / 2;
    if (value - eccentricity * Math.sin(value) > mean) high = value;
    else low = value;
  }
  return (low + high) / 2;
}

const Q = 268435456n;
const P = 843314857n;
const sine = [268435456n, -44739243n, 2236962n, -53261n, 740n, -7n];
const cosine = [268435456n, -134217728n, 11184811n, -372827n, 6658n, -74n];
const clamp = (value: bigint, low: bigint, high: bigint) => (value < low ? low : value > high ? high : value);
const fixedMultiply = (left: bigint, right: bigint) => (left * right) >> 28n;

function solveOrbit(genome: Json, mean: number, eccentricity: number) {
  const m = BigInt(mean);
  const e = BigInt(eccentricity);
  const initial = [() => m, () => P / 2n, () => m + e / 2n, () => m + e, () => (m < Q ? m + e : m + e / 4n)][genome.seed] ?? (() => m);
  let estimate = clamp(initial(), 0n, P);
  let low = 0n;
  let high = P;

  for (let tick = 0; tick < genome.iterations; tick += 1) {
    const folded = estimate > P / 2n ? P - estimate : estimate;
    const square = fixedMultiply(folded, folded);
    const polynomial = (coefficients: bigint[]) => {
      let value = coefficients.at(-1) as bigint;
      for (let index = coefficients.length - 2; index >= 0; index -= 1) value = coefficients[index] + fixedMultiply(value, square);
      return value;
    };
    const sin = fixedMultiply(folded, polynomial(sine));
    const cos = polynomial(cosine) * (estimate > P / 2n ? -1n : 1n);
    const eccentricSin = fixedMultiply(e, sin);
    const residual = estimate - m - eccentricSin;
    const threshold = [0n, 16n, 256n, 4096n][genome.stop] ?? 0n;
    if ((residual < 0n ? -residual : residual) < threshold) break;
    if (residual > 0n) high = estimate;
    else low = estimate;
    const derivative = Q - fixedMultiply(e, cos);
    const operation = tick < genome.switch ? genome.first : genome.later;
    let step: bigint;
    if (operation === 0) step = (residual * Q) / derivative;
    else if (operation === 1) {
      const newton = (residual * Q) / derivative;
      const halley = derivative - fixedMultiply(eccentricSin, newton) / 2n;
      step = halley > 26843n ? (residual * Q) / halley : newton;
    } else if (operation === 2) step = ((residual * Q) / derivative) / 2n;
    else step = residual;
    if (genome.cap) {
      const cap = genome.cap === 1 ? Q : Q / 2n;
      step = clamp(step, -cap, cap);
    }
    const next = estimate - step;
    estimate = genome.guard && (next < low || next > high) ? (low + high) / 2n : clamp(next, 0n, P);
  }
  return Number(estimate);
}

function orbitMeasurement(genome: Json | undefined, eccentricity: number, phase: number) {
  const sign = phase > Math.PI ? -1 : 1;
  const mean = sign < 0 ? Math.PI * 2 - phase : phase;
  const meanFixed = Math.round(mean * Number(Q));
  const eccentricityFixed = Math.round(eccentricity * Number(Q));
  const reference = orbitalTruth(mean, eccentricity);
  const actual = genome ? solveOrbit(genome, meanFixed, eccentricityFixed) / Number(Q) : reference;
  return { actual, error: Math.abs(actual - reference), sign, eccentricityFixed };
}

function drawOrbit(canvas: HTMLCanvasElement | null, genome: Json | undefined, eccentricity: number, phase: number) {
  if (!canvas) return;
  const width = 720;
  const height = 360;
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) return;
  const palette = canvasPalette(canvas);
  context.fillStyle = palette.ground;
  context.fillRect(0, 0, width, height);
  const scale = 108;
  const centerX = width / 2 + eccentricity * scale * 0.45;
  const centerY = height / 2;
  const point = (angle: number) => [
    centerX + scale * (Math.cos(angle) - eccentricity),
    centerY + scale * Math.sqrt(1 - eccentricity * eccentricity) * Math.sin(angle)
  ];
  context.strokeStyle = palette.faint;
  context.beginPath();
  for (let degree = 0; degree <= 360; degree += 1) {
    const [x, y] = point((degree * Math.PI) / 180);
    if (!degree) context.moveTo(x, y);
    else context.lineTo(x, y);
  }
  context.stroke();
  context.fillStyle = palette.ink;
  context.beginPath();
  context.arc(centerX, centerY, 5, 0, Math.PI * 2);
  context.fill();
  const measurement = orbitMeasurement(genome, eccentricity, phase);
  const { actual, sign, eccentricityFixed } = measurement;
  const mean = sign < 0 ? Math.PI * 2 - phase : phase;
  const reference = orbitalTruth(mean, eccentricity);
  for (let index = 24; index > 0; index -= 1) {
    const trailPhase = (phase - index * 0.025 + Math.PI * 2) % (Math.PI * 2);
    const trailSign = trailPhase > Math.PI ? -1 : 1;
    const trailMean = trailSign < 0 ? Math.PI * 2 - trailPhase : trailPhase;
    const trail = genome
      ? solveOrbit(genome, Math.round(trailMean * Number(Q)), eccentricityFixed) / Number(Q)
      : orbitalTruth(trailMean, eccentricity);
    const [trailX, trailY] = point(trailSign * trail);
    context.globalAlpha = ((25 - index) / 25) * 0.28;
    context.fillStyle = palette.muted;
    context.fillRect(trailX - 1, trailY - 1, 2, 2);
  }
  context.globalAlpha = 1;
  const [referenceX, referenceY] = point(sign * reference);
  context.strokeStyle = palette.ink;
  context.lineWidth = 1.5;
  context.beginPath();
  context.arc(referenceX, referenceY, 9, 0, Math.PI * 2);
  context.stroke();
  const [x, y] = point(sign * actual);
  context.fillStyle = palette.muted;
  context.beginPath();
  context.arc(x, y, 6, 0, Math.PI * 2);
  context.fill();
  return measurement;
}

function drawHeatmap(canvas: HTMLCanvasElement | null, grid: number[] | undefined, tolerance: number | undefined) {
  if (!canvas || !grid?.length) return;
  canvas.width = 256;
  canvas.height = 256;
  const context = canvas.getContext('2d');
  if (!context) return;
  const image = context.createImageData(256, 256);
  // Ramp between the theme's own ground and ink rather than a fixed pair of
  // greys, so the map stays legible in either theme.
  const palette = canvasPalette(canvas);
  const channels = (colour: string) => {
    context.fillStyle = colour;
    const parsed = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(context.fillStyle);
    return parsed ? [1, 2, 3].map((i) => Number.parseInt(parsed[i], 16)) : [0, 0, 0];
  };
  const low = channels(palette.grid);
  const high = channels(palette.strong);
  for (let y = 0; y < 256; y += 1) {
    for (let x = 0; x < 256; x += 1) {
      const value = grid[y * 256 + x];
      const ratio = Math.min(1, Math.max(0, Math.log10(Math.max(value, 1e-12) / (tolerance ?? 0.0001)) / 5 + 1));
      const index = ((255 - y) * 256 + x) * 4;
      for (let channel = 0; channel < 3; channel += 1) {
        image.data[index + channel] = Math.round(low[channel] + ratio * (high[channel] - low[channel]));
      }
      image.data[index + 3] = 255;
    }
  }
  context.putImageData(image, 0, 0);
}

function OrbitForge() {
  const demo = useDemoRun('orbit-forge');
  const theme = useTheme();
  const orbitCanvas = useRef<HTMLCanvasElement>(null);
  const heatCanvas = useRef<HTMLCanvasElement>(null);
  const errorCanvas = useRef<HTMLCanvasElement>(null);
  const [tolerance, setTolerance] = useState(0.0001);
  const [seed, setSeed] = useState(1);
  const [eccentricity, setEccentricity] = useState(0.7);
  const [phase, setPhase] = useState(1.3);
  const [animating, setAnimating] = useState(true);
  const current = demo.current ?? last(demo.events, (event) => event.kind === 'done');
  const done = last(demo.events, (event) => event.kind === 'done');
  const genome = current?.genome ?? done?.genome;
  const stage = last(demo.events.slice(0, Math.max(0, demo.events.indexOf(current) + 1)), (event) => event.kind === 'stage');
  const point = orbitMeasurement(genome, eccentricity, phase);
  // While a run is live the curve comes from the progress events; once it has
  // finished, the terminal event carries the whole history, so a replayed run
  // shows the same shape.
  // Plot the quantity the search minimises. It is not the error: a solver only
  // has to stay inside the tolerance budget, and once it does, accuracy beyond
  // that is spent on speed. Charting error therefore showed a rise at the
  // moment the search first traded surplus accuracy for a shorter solve.
  const progressCurve = demo.events
    .filter((event) => event.kind === 'progress' && typeof event.mean_steps === 'number')
    .map((event) => [event.generation as number, event.mean_steps as number] as [number, number]);
  const historyCurve = Array.isArray(done?.history)
    ? done.history
        .filter((entry: Json) => typeof entry?.mean_steps === 'number')
        .map((entry: Json) => [entry.generation as number, entry.mean_steps as number] as [number, number])
    : [];
  const curve = progressCurve.length >= historyCurve.length ? progressCurve : historyCurve;
  const benchmark = done?.benchmark;
  const ratios: number[] = Array.isArray(benchmark?.summaries)
    ? benchmark.summaries.map((entry: Json) => Number(entry.ratio)).filter((value: number) => Number.isFinite(value))
    : [];
  // A speedup is only claimed when every trial won; otherwise the measured
  // range is reported so the number is not read as a result it did not earn.
  const speedup = benchmark?.consistent_win && typeof benchmark.speedup === 'number'
    ? `${benchmark.speedup.toFixed(2)}×`
    : ratios.length
      ? `${Math.min(...ratios).toFixed(2)}–${Math.max(...ratios).toFixed(2)}× (mixed)`
      : undefined;
  const totalGenerations = last(demo.events, (event) => typeof event.generations === 'number')?.generations;

  useEffect(() => drawOrbit(orbitCanvas.current, genome, eccentricity, phase), [genome, eccentricity, phase, theme]);
  useEffect(() => drawHeatmap(heatCanvas.current, done?.grid, done?.tolerance), [done, theme]);
  useEffect(() => drawErrorChart(errorCanvas.current, curve), [curve, theme]);
  useEffect(() => {
    if (!animating) return;
    let frame = 0;
    let previous = performance.now();
    const advance = (now: number) => {
      setPhase((value) => (value + Math.min(0.1, (now - previous) / 1000) * 0.45) % (Math.PI * 2));
      previous = now;
      frame = requestAnimationFrame(advance);
    };
    frame = requestAnimationFrame(advance);
    return () => cancelAnimationFrame(frame);
  }, [animating]);

  const selectHeatPoint = (event: MouseEvent) => {
    const canvas = heatCanvas.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
    setEccentricity((1 - y) * 0.95);
    setPhase(x * Math.PI);
    setAnimating(false);
  };

  const recipe = (value: Json | undefined) => {
    if (!value) return undefined;
    const names = ['Newton', 'Halley', 'damped Newton', 'fixed-point'];
    const initial = ['E = M', 'E = π/2', 'E = M + e/2', 'E = M + e', 'if M < 1: M + e; otherwise M + e/4'];
    const method = value.switch === 0 || value.first === value.later ? names[value.later] : `${names[value.first]} for ${value.switch} iterations, then ${names[value.later]}`;
    return `Start  ${initial[value.seed]}\nLoop   at most ${value.iterations} iterations\nStep   ${method}\nStop   |residual| < ${[0, 16, 256, 4096][value.stop]} / 2²⁸\nGuard  ${value.guard ? 'bisect outside the bracket' : 'clamp angle to [0, π]'}\nCap    ${['no step cap', '1 radian', '0.5 radians'][value.cap]}`;
  };

  return (
    <section class="experiment-island" aria-label="Orbit Forge interactive demo">
      <IslandToolbar
        humanCheck={demo.humanCheck}
        replay={demo.replay}
        runGpu={() => demo.runGpu({ tolerance, seed })}
        cancel={demo.cancel}
        job={demo.job}
        gatewayReady={demo.gatewayReady}
      >
        <label>
          Tolerance
          <select value={tolerance} onInput={(event) => setTolerance(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={0.001}>1e−3 · easiest</option>
            <option value={0.0001}>1e−4 · default</option>
            <option value={0.00001}>1e−5 · hardest</option>
          </select>
        </label>
        <label>
          Seed
          <select value={seed} onInput={(event) => setSeed(Number((event.currentTarget as HTMLSelectElement).value))}>
            <option value={1}>1</option>
            <option value={2}>2</option>
            <option value={3}>3</option>
          </select>
        </label>
      </IslandToolbar>
      <IslandStatus error={demo.error} job={demo.job} current={current} />
      <Section title="View">
      <div class="experiment-canvas-grid">
        <figure>
          <canvas ref={orbitCanvas} aria-label="Orbit solver visualization" />
          <figcaption>Orbit</figcaption>
        </figure>
        <figure><canvas ref={heatCanvas} onClick={selectHeatPoint} aria-label="Solver error map; click to inspect a point" /><figcaption>Error map · click to inspect</figcaption></figure>
      </div>
      <div class="experiment-viewer-controls">
        <button type="button" class="experiment-button" onClick={() => setAnimating((value) => !value)}>{animating ? 'Pause' : 'Play'}</button>
        <label class="experiment-range">Eccentricity <input type="range" min="0" max="0.95" step="0.01" value={eccentricity} onInput={(event) => { setEccentricity(Number((event.currentTarget as HTMLInputElement).value)); setAnimating(false); }} /></label>
        <label class="experiment-range">Phase <input type="range" min="0" max="6.28" step="0.01" value={phase} onInput={(event) => { setPhase(Number((event.currentTarget as HTMLInputElement).value)); setAnimating(false); }} /></label>
        <span class="experiment-readout">E = {point.actual.toFixed(6)} rad · error {point.error.toExponential(2)} rad</span>
      </div>
      <div class="experiment-metrics">
        <Metric
          label="generation"
          value={typeof current?.generation === 'number'
            ? (totalGenerations ? `${current.generation} / ${totalGenerations}` : current.generation)
            : done?.checked !== undefined ? `${formatNumber(done.checked)} checked` : undefined}
        />
        <Metric
          label="max error"
          value={typeof current?.max_error === 'number'
            ? `${current.max_error.toExponential(2)} rad${done?.tolerance ? ` / ${Number(done.tolerance).toExponential(0)}` : ''}`
            : undefined}
        />
        <Metric
          better="lower"
          label="iterations"
          value={typeof current?.mean_steps === 'number' ? current.mean_steps.toFixed(1) : undefined}
        />
        <Metric better="higher" label="native speedup" value={speedup} />
        <Metric better="lower" label="time" value={typeof current?.seconds === 'number' ? `${current.seconds.toFixed(2)}s` : undefined} />
      </div>
      <figure class="experiment-chart">
        <canvas ref={errorCanvas} aria-label="Average solver iterations by generation" />
        <figcaption>Average iterations per solve, by generation · lower is better</figcaption>
      </figure>
      </Section>
      <Outcome
        targetLabel="Objective"
        target={<p class="experiment-objective">Solve Kepler's equation E − e·sin(E) = M for the eccentric anomaly E, within the chosen tolerance, across the whole domain. There is no reference program to copy: accuracy is judged against double-precision bisection.</p>}
        resultLabel="Discovered solver"
        result={done?.source
          ? (
            <>
              {recipe(genome) && <pre class="experiment-code-block"><code>{recipe(genome)}</code></pre>}
              <CodeBlock text={done.source} />
            </>
          )
          : undefined}
      />
      {done?.source && (
        <Section title="Export">
          <div class="experiment-exports">
            <button type="button" class="experiment-button" onClick={() => downloadText('discovered-orbit-solver.gremlin', done.source)}>
              Download .gremlin
            </button>
          </div>
        </Section>
      )}
      {(done || stage) && (
        <details class="experiment-details">
          <summary>Evidence</summary>
          {stage?.message && <p>{stage.message}</p>}
          {done && <p>{done.passed ? 'All checked cases pass.' : `${done.failures} checked cases fail.`} {done.counterexamples ?? 0} counterexamples fed back.</p>}
          {done?.benchmark?.available && (
            <p>
              Compiled program matches the interpreter on {formatNumber(done.benchmark.compiled_grid_checked)} grid inputs.{' '}
              {done.benchmark.consistent_win
                ? `Every trial beat the reference solver, so the speedup shown is the slowest of them: ≥ ${Number(done.benchmark.speedup).toFixed(2)}×.`
                : 'The trials did not all beat the reference solver, so no single speedup is claimed; the measured range is shown instead.'}
              {done.benchmark.compiler ? ` Built with ${String(done.benchmark.compiler).split(' version ')[0]}.` : ''}
            </p>
          )}
        </details>
      )}
    </section>
  );
}

export default function ExperimentIsland({ demo }: { demo: DemoId }) {
  if (demo === 'shader-detective') return <ShaderDetective />;
  if (demo === 'landing-lab') return <LandingLab />;
  if (demo === 'tiny-robot') return <TinyRobot />;
  if (demo === 'shader-sculptor') return <ShaderSculptor />;
  return <OrbitForge />;
}
