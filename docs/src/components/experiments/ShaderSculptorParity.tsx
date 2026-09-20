import type { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { CodeBlock, Metric, Outcome, Section, canvasPalette } from './IslandChrome';

/**
 * The portable event shape emitted by Shader Sculptor. The GPU gateway keeps
 * these events intentionally opaque, so this component only relies on the
 * fields the original browser demo used.
 */
export type ShaderSculptorEvent = Record<string, unknown>;

export type ShaderSculptorRecording = {
  events?: ShaderSculptorEvent[];
  images?: Record<string, string>;
};

export type ShaderSculptorImageInput = {
  /** RGB pixels encoded as base64, not an image file or a browser URL. */
  rgb: string;
  width: number;
  height: number;
};

export type ShaderSculptorRunInput = {
  preset: 'planet' | 'bloom' | 'city';
  resolution: number;
  budgetMs: number;
  seed: number;
  /** Present only for a user-selected image. Presets are regenerated server-side. */
  image?: ShaderSculptorImageInput;
};

export type ShaderSculptorParityProps = {
  /** Use this for a fully controlled island backed by the GPU job stream. */
  events?: ShaderSculptorEvent[];
  current?: ShaderSculptorEvent;
  recording?: ShaderSculptorRecording;
  running?: boolean;
  error?: string;
  gatewayReady?: boolean;
  humanCheck?: ComponentChildren;
  onRun?: (input: ShaderSculptorRunInput) => void | Promise<void>;
  onReplay?: () => void;
  onStop?: () => void;
};

type Target = {
  packed: Uint32Array;
  rgb?: Uint8Array;
  size: number;
  label: string;
  uploaded: boolean;
};

type LoadedImage = {
  source: CanvasImageSource;
  dispose?: () => void;
};

const SAMPLE_PATH = '/experiments-data/shader-sculptor/sample.json';
const MAX_UPLOAD_BYTES = 12 * 1024 * 1024;
const BACKGROUND = 0x15212a;

const pack = (red: number, green: number, blue: number) => (red << 16) | (green << 8) | blue;
const asNumber = (value: unknown) => typeof value === 'number' && Number.isFinite(value) ? value : undefined;
const asString = (value: unknown) => typeof value === 'string' ? value : undefined;
const asRecord = (value: unknown) => value && typeof value === 'object' ? value as ShaderSculptorEvent : undefined;

function last<T>(items: T[], predicate: (item: T) => boolean) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (predicate(items[index])) return items[index];
  }
  return undefined;
}

function formatNumber(value: number | undefined) {
  return typeof value === 'number' ? new Intl.NumberFormat('en-US').format(value) : '—';
}

/** The original three target generators, retained byte-for-byte in behaviour. */
function presetPixels(name: ShaderSculptorRunInput['preset'], size: number) {
  const output = new Uint32Array(size * size);
  let pixel = 0;
  for (let iy = 0; iy < size; iy += 1) {
    for (let ix = 0; ix < size; ix += 1) {
      const x = (ix * 128) / size;
      const y = (iy * 128) / size;
      let red: number;
      let green: number;
      let blue: number;

      if (name === 'planet') {
        red = 18 + Math.trunc(y * 0.25);
        green = 14 + Math.trunc(y * 0.1);
        blue = 45 + Math.trunc(y * 0.3);
        const distance = Math.hypot(x - 76, y - 48);
        if (distance < 29) {
          red = 240 - Math.trunc(distance * 1.8);
          green = 115 + Math.trunc(y * 0.6);
          blue = 80 + Math.trunc(distance * 2);
        }
        const ring = ((x - 73) + (y - 50) * 1.9) ** 2 / 58 ** 2 + ((y - 50) - (x - 73) * 0.12) ** 2 / 9 ** 2;
        if (ring > 0.8 && ring < 1.2 && (y > 47 || distance > 29)) {
          red = 100;
          green = 235;
          blue = 221;
        }
        const horizon = 93 + 8 * Math.sin(x * 0.09) + 9 * Math.sin(x * 0.23);
        if (y > horizon) {
          red = 20;
          green = 43 + Math.trunc((y - 90) * 0.5);
          blue = 62;
        }
        if (y > horizon + 12) {
          red = 9;
          green = 23;
          blue = 35;
        }
      } else if (name === 'bloom') {
        const dx = x - 64;
        const dy = y - 64;
        const radius = Math.hypot(dx, dy);
        const angle = Math.atan2(dy, dx);
        const edge = 31 + 15 * Math.cos(angle * 7);
        red = 15 + Math.trunc(Math.max(0, 42 - radius) * 0.6);
        green = 12;
        blue = 35 + Math.trunc(Math.max(0, 60 - radius) * 0.8);
        if (radius < edge) {
          red = 150 + Math.trunc(90 * (1 - radius / 48));
          green = 55 + Math.trunc(95 * (1 - radius / 48));
          blue = 170 + Math.trunc(65 * radius / 48);
        }
        if (radius < edge && radius > edge - 3) {
          red = 255;
          green = 157;
          blue = 218;
        }
        if (radius < 10) {
          red = 255;
          green = 212;
          blue = 118;
        }
        if (radius > 53 && radius < 55) {
          red = 80;
          green = 183;
          blue = 190;
        }
      } else {
        red = 35 + Math.trunc(y * 0.28);
        green = 18 + Math.trunc(y * 0.08);
        blue = 68 + Math.trunc(y * 0.23);
        if (Math.hypot(x - 91, y - 28) < 17) {
          red = 246;
          green = 176;
          blue = 104;
        }
        const heights = [85, 65, 93, 42, 78, 54, 88, 69, 94, 49, 81];
        const building = Math.min(10, Math.floor(x / 12));
        if (y > heights[building]) {
          red = 12;
          green = 29;
          blue = 46;
          if (x % 12 === 1 || x % 12 === 2) {
            red = 75;
            green = 179;
            blue = 184;
          }
          if (x % 12 > 4 && x % 12 < 8 && y % 10 > 2 && y % 10 < 5) {
            red = 237;
            green = 124;
            blue = 163;
          }
        }
        if (y > 112) {
          red = 21;
          green = 34;
          blue = 48;
        }
      }
      output[pixel] = pack(red, green, blue);
      pixel += 1;
    }
  }
  return output;
}

function makePresetTarget(name: ShaderSculptorRunInput['preset'], size: number): Target {
  return {
    packed: presetPixels(name, size),
    size,
    label: name === 'planet' ? 'Orbital sunset' : name === 'bloom' ? 'Neon bloom' : 'Midnight signal',
    uploaded: false
  };
}

function fillCanvas(canvas: HTMLCanvasElement | null, color: number, size: number) {
  if (!canvas) return;
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d');
  if (!context) return;
  context.fillStyle = `#${(color >>> 0).toString(16).padStart(6, '0')}`;
  context.fillRect(0, 0, size, size);
}

function drawPacked(canvas: HTMLCanvasElement | null, values: Uint32Array | number[], size: number) {
  if (!canvas) return;
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d');
  if (!context) return;
  const image = context.createImageData(size, size);
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index] >>> 0;
    image.data[index * 4] = value >>> 16;
    image.data[index * 4 + 1] = (value >>> 8) & 255;
    image.data[index * 4 + 2] = value & 255;
    image.data[index * 4 + 3] = 255;
  }
  context.putImageData(image, 0, 0);
}

function decodeRgb(base64: string) {
  const raw = atob(base64);
  const packed = new Uint32Array(Math.floor(raw.length / 3));
  for (let index = 0; index < packed.length; index += 1) {
    packed[index] = pack(raw.charCodeAt(index * 3), raw.charCodeAt(index * 3 + 1), raw.charCodeAt(index * 3 + 2));
  }
  return packed;
}

function imageText(recording: ShaderSculptorRecording | undefined, image: unknown) {
  const key = asString(image);
  if (!key) return undefined;
  return recording?.images?.[key] ?? key;
}

function drawEventImage(
  canvas: HTMLCanvasElement | null,
  recording: ShaderSculptorRecording | undefined,
  image: unknown,
  size: number
) {
  const base64 = imageText(recording, image);
  if (!base64) {
    fillCanvas(canvas, BACKGROUND, size);
    return;
  }
  try {
    const pixels = decodeRgb(base64);
    const previewSize = Math.sqrt(pixels.length);
    drawPacked(canvas, pixels, Number.isInteger(previewSize) ? previewSize : size);
  } catch {
    fillCanvas(canvas, BACKGROUND, size);
  }
}

/** Exact integer geometry and alpha compositing from apps/shader-sculptor/graphics.js. */
function paintLayer(canvas: Uint32Array, shape: unknown) {
  if (!Array.isArray(shape) || shape.length < 8) return;
  const [cx, cy, rx, ry, kind, rotate, color, alpha] = shape.map(Number);
  if (![cx, cy, rx, ry, kind, rotate, color, alpha].every(Number.isFinite)) return;
  const size = Math.sqrt(canvas.length);
  const boundX = rotate ? rx + ry : rx;
  const boundY = rotate ? rx + ry : ry;
  for (let y = Math.max(0, cy - boundY); y <= Math.min(size - 1, cy + boundY); y += 1) {
    for (let x = Math.max(0, cx - boundX); x <= Math.min(size - 1, cx + boundX); x += 1) {
      const index = y * size + x;
      let dx = x - cx;
      let dy = y - cy;
      if (rotate) {
        const old = dx;
        dx += dy;
        dy = old - dy;
      }
      dx = Math.abs(dx);
      dy = Math.abs(dy);
      const hits = kind === 0
        ? dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry
        : kind === 1
          ? dx <= rx && dy <= ry
          : dx * ry + dy * rx <= rx * ry;
      if (!hits) continue;
      let value = 0;
      for (const shift of [0, 8, 16]) {
        value |= Math.floor(((((canvas[index] >>> shift) & 255) * (255 - alpha)) + (((color >>> shift) & 255) * alpha) + 127) / 255) << shift;
      }
      canvas[index] = value;
    }
  }
}

function bytesToBase64(bytes: Uint8Array) {
  const chunks: string[] = [];
  const chunkSize = 0x8000;
  for (let start = 0; start < bytes.length; start += chunkSize) {
    const chunk = bytes.subarray(start, Math.min(bytes.length, start + chunkSize));
    let text = '';
    for (let index = 0; index < chunk.length; index += 1) text += String.fromCharCode(chunk[index]);
    chunks.push(text);
  }
  return btoa(chunks.join(''));
}

async function loadImage(file: File): Promise<LoadedImage> {
  if ('createImageBitmap' in window) {
    const bitmap = await createImageBitmap(file);
    return { source: bitmap, dispose: () => bitmap.close() };
  }

  const objectUrl = URL.createObjectURL(file);
  try {
    const image = await new Promise<HTMLImageElement>((resolve, reject) => {
      const element = new Image();
      element.onload = () => resolve(element);
      element.onerror = () => reject(new Error('The selected file could not be decoded as an image.'));
      element.src = objectUrl;
    });
    return { source: image };
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}

function rasterizeImage(loaded: LoadedImage, size: number): Target {
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('The browser could not create an image canvas.');
  context.fillStyle = canvasPalette(canvas).ground;
  context.fillRect(0, 0, size, size);

  const image = loaded.source as { width: number; height: number } & CanvasImageSource;
  if (!image.width || !image.height) throw new Error('The selected image has no drawable dimensions.');
  const scale = Math.min(size / image.width, size / image.height);
  const width = image.width * scale;
  const height = image.height * scale;
  context.drawImage(loaded.source, (size - width) / 2, (size - height) / 2, width, height);

  const rgba = context.getImageData(0, 0, size, size).data;
  const packed = new Uint32Array(size * size);
  const rgb = new Uint8Array(size * size * 3);
  for (let index = 0; index < packed.length; index += 1) {
    const red = rgba[index * 4];
    const green = rgba[index * 4 + 1];
    const blue = rgba[index * 4 + 2];
    packed[index] = pack(red, green, blue);
    rgb[index * 3] = red;
    rgb[index * 3 + 1] = green;
    rgb[index * 3 + 2] = blue;
  }
  return { packed, rgb, size, label: 'Your image', uploaded: true };
}

function downloadText(filename: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function downloadCanvas(filename: string, canvas: HTMLCanvasElement | null) {
  if (!canvas) return;
  canvas.toBlob((blob) => {
    if (!blob) return;
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, 'image/png');
}

/**
 * A drop-in Preact island for Shader Sculptor. Without props it loads the
 * recording and offers a faithful local replay. With callbacks it becomes a
 * controlled front end for the queue-backed GPU run without duplicating job
 * state in the island.
 */
export default function ShaderSculptorParity({
  events: controlledEvents,
  current: controlledCurrent,
  recording: suppliedRecording,
  running = false,
  error: externalError,
  gatewayReady = false,
  humanCheck,
  onRun,
  onReplay,
  onStop
}: ShaderSculptorParityProps) {
  const targetCanvas = useRef<HTMLCanvasElement>(null);
  const outputCanvas = useRef<HTMLCanvasElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const uploadedImage = useRef<LoadedImage>();
  const replayFrame = useRef<number>();
  const [loadedRecording, setLoadedRecording] = useState<ShaderSculptorRecording>();
  const [loadError, setLoadError] = useState<string>();
  const [localError, setLocalError] = useState<string>();
  const [localCursor, setLocalCursor] = useState(-1);
  const [localPlaying, setLocalPlaying] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [constructing, setConstructing] = useState(false);
  const [changedTarget, setChangedTarget] = useState(false);
  const [preset, setPreset] = useState<ShaderSculptorRunInput['preset']>('planet');
  const [resolution, setResolution] = useState(128);
  const [budgetMs, setBudgetMs] = useState(3000);
  const [seed, setSeed] = useState(1);
  const [target, setTarget] = useState<Target>(() => makePresetTarget('planet', 128));

  const recording = suppliedRecording ?? loadedRecording;
  const sourceEvents = controlledEvents ?? recording?.events ?? [];
  const hasControlledEvents = controlledEvents !== undefined || suppliedRecording !== undefined;
  const visibleEvents = hasControlledEvents || localCursor < 0 ? sourceEvents : sourceEvents.slice(0, localCursor + 1);
  const start = visibleEvents.find((event) => event.kind === 'start');
  const visibleCurrent = controlledCurrent ?? last(visibleEvents, (event) => ['start', 'progress', 'done'].includes(asString(event.kind) ?? ''));
  const result = changedTarget ? undefined : visibleCurrent;
  const resultSize = asNumber(result?.width) ?? asNumber(start?.width) ?? target.size;
  const outputImage = result?.image;
  const completed = last(visibleEvents, (event) => event.kind === 'done');
  const finished = changedTarget ? undefined : completed;
  const resultProgram = asString(finished?.program);
  const ownReplay = !hasControlledEvents && !onReplay;
  const active = running || submitting || localPlaying || constructing;

  useEffect(() => {
    if (suppliedRecording || controlledEvents) return;
    let mounted = true;
    fetch(SAMPLE_PATH)
      .then((response) => {
        if (!response.ok) throw new Error('The recorded drawing is unavailable.');
        return response.json();
      })
      .then((value) => {
        if (!mounted) return;
        const next = value as ShaderSculptorRecording;
        setLoadedRecording(next);
        const doneIndex = (next.events ?? []).map((event) => event.kind).lastIndexOf('done');
        setLocalCursor(doneIndex >= 0 ? doneIndex : Math.max(0, (next.events?.length ?? 1) - 1));
      })
      .catch((reason) => mounted && setLoadError(reason instanceof Error ? reason.message : 'Unable to load the recording.'));
    return () => {
      mounted = false;
    };
  }, [controlledEvents, suppliedRecording]);

  useEffect(() => {
    if (!ownReplay || !localPlaying || !sourceEvents.length) return;
    if (localCursor >= sourceEvents.length - 1) {
      setLocalPlaying(false);
      return;
    }
    const currentEvent = sourceEvents[Math.max(0, localCursor)];
    const delay = currentEvent?.kind === 'progress' ? 180 : 300;
    const timer = window.setTimeout(() => setLocalCursor((value) => value + 1), delay);
    return () => window.clearTimeout(timer);
  }, [localCursor, localPlaying, ownReplay, sourceEvents]);

  useEffect(() => {
    drawPacked(targetCanvas.current, target.packed, target.size);
  }, [target]);

  useEffect(() => {
    if (constructing) return;
    if (result && outputImage) drawEventImage(outputCanvas.current, recording, outputImage, resultSize);
    else fillCanvas(outputCanvas.current, BACKGROUND, target.size);
  }, [constructing, outputImage, recording, result, resultSize, target.size]);

  useEffect(() => {
    const kind = asString(controlledCurrent?.kind);
    if (kind && ['start', 'input', 'progress', 'done'].includes(kind)) setChangedTarget(false);
  }, [controlledCurrent]);

  useEffect(() => () => {
    uploadedImage.current?.dispose?.();
    if (replayFrame.current) cancelAnimationFrame(replayFrame.current);
  }, []);

  const replacePreset = (nextPreset: ShaderSculptorRunInput['preset'], nextResolution = resolution) => {
    uploadedImage.current?.dispose?.();
    uploadedImage.current = undefined;
    setPreset(nextPreset);
    setTarget(makePresetTarget(nextPreset, nextResolution));
    setChangedTarget(true);
    setLocalError(undefined);
  };

  const replaceResolution = (nextResolution: number) => {
    setResolution(nextResolution);
    if (uploadedImage.current) {
      try {
        setTarget(rasterizeImage(uploadedImage.current, nextResolution));
        setChangedTarget(true);
        setLocalError(undefined);
      } catch (reason) {
        setLocalError(reason instanceof Error ? reason.message : 'Unable to resize the selected image.');
      }
    } else {
      setTarget(makePresetTarget(preset, nextResolution));
      setChangedTarget(true);
    }
  };

  const chooseImage = async (file: File | undefined) => {
    if (!file) return;
    try {
      if (file.type && !file.type.startsWith('image/')) throw new Error('Choose an image file.');
      if (file.size > MAX_UPLOAD_BYTES) throw new Error('Choose an image smaller than 12 MB.');
      const loaded = await loadImage(file);
      uploadedImage.current?.dispose?.();
      uploadedImage.current = loaded;
      setTarget(rasterizeImage(loaded, resolution));
      setChangedTarget(true);
      setLocalError(undefined);
    } catch (reason) {
      setLocalError(reason instanceof Error ? reason.message : 'Unable to use that image.');
    } finally {
      if (fileInput.current) fileInput.current.value = '';
    }
  };

  const resetToRecording = () => {
    uploadedImage.current?.dispose?.();
    uploadedImage.current = undefined;
    setPreset('planet');
    setResolution(128);
    setTarget(makePresetTarget('planet', 128));
    setChangedTarget(false);
    setLocalError(undefined);
  };

  const replay = () => {
    resetToRecording();
    if (onReplay) {
      onReplay();
      return;
    }
    if (!sourceEvents.length) return;
    setLocalCursor(0);
    setLocalPlaying(true);
  };

  const run = async () => {
    if (!onRun || active) return;
    setSubmitting(true);
    setLocalError(undefined);
    try {
      await onRun({
        preset,
        resolution,
        budgetMs,
        seed,
        image: target.uploaded && target.rgb
          ? { width: target.size, height: target.size, rgb: bytesToBase64(target.rgb) }
          : undefined
      });
    } catch (reason) {
      setLocalError(reason instanceof Error ? reason.message : 'Unable to start the GPU search.');
    } finally {
      setSubmitting(false);
    }
  };

  const replayConstruction = () => {
    const shapes = Array.isArray(finished?.shapes) ? finished.shapes : undefined;
    if (!finished || !shapes?.length || active) return;
    const size = asNumber(finished.width) ?? asNumber(start?.width) ?? target.size;
    const background = asNumber(finished.background) ?? BACKGROUND;
    const pixels = new Uint32Array(size * size);
    pixels.fill(background);
    let index = 0;
    setConstructing(true);
    const frame = () => {
      for (let count = 0; count < 8 && index < shapes.length; count += 1) {
        paintLayer(pixels, shapes[index]);
        index += 1;
      }
      drawPacked(outputCanvas.current, pixels, size);
      if (index < shapes.length) replayFrame.current = requestAnimationFrame(frame);
      else setConstructing(false);
    };
    replayFrame.current = requestAnimationFrame(frame);
  };

  const error = externalError ?? localError ?? loadError;
  const layers = asNumber(result?.layers);
  const seconds = asNumber(result?.seconds);
  const initialError = asNumber(result?.initial_error);
  const finalError = asNumber(result?.error);
  const candidates = asNumber(result?.candidates);
  const pixelEvaluations = asNumber(result?.pixel_evaluations) ?? (candidates ? candidates * resultSize * resultSize : undefined);
  const quality = initialError && finalError !== undefined
    ? `${((1 - finalError / initialError) * 100).toFixed(1)}%`
    : finalError === 0 ? 'Exact color' : undefined;
  const canConstruct = Boolean(finished && Array.isArray(finished.shapes) && finished.shapes.length);

  return (
    <section class="experiment-island" aria-label="Shader Sculptor interactive demo">
      {humanCheck}
      <Section title="Setup">
      <div class="experiment-toolbar">
        <div class="experiment-fields">
          <label>
            Target
            <select
              value={preset}
              disabled={active}
              onInput={(event) => replacePreset((event.currentTarget as HTMLSelectElement).value as ShaderSculptorRunInput['preset'])}
            >
              <option value="planet">Orbital sunset</option>
              <option value="bloom">Neon bloom</option>
              <option value="city">Midnight signal</option>
            </select>
          </label>
          <label>
            Resolution
            <select value={resolution} disabled={active} onInput={(event) => replaceResolution(Number((event.currentTarget as HTMLSelectElement).value))}>
              <option value={128}>128 × 128</option>
              <option value={256}>256 × 256</option>
              <option value={512}>512 × 512</option>
              <option value={1024}>1,024 × 1,024</option>
              <option value={2048}>2,048 × 2,048</option>
            </select>
          </label>
          <label>
            Layers
            <select value={budgetMs} disabled={active} onInput={(event) => setBudgetMs(Number((event.currentTarget as HTMLSelectElement).value))}>
              <option value={1500}>Sketch · up to 128</option>
              <option value={3000}>Refined · up to 512</option>
              <option value={12000}>Ultra · up to 2,048</option>
              <option value={24000}>Up to 4,096</option>
              <option value={48000}>Up to 8,192</option>
            </select>
          </label>
          <label>
            Seed
            <select value={seed} disabled={active} onInput={(event) => setSeed(Number((event.currentTarget as HTMLSelectElement).value))}>
              <option value={1}>1</option>
              <option value={2}>2</option>
              <option value={3}>3</option>
            </select>
          </label>
        </div>
        <div class="experiment-actions">
          <button type="button" class="experiment-button" disabled={active} onClick={() => fileInput.current?.click()}>
            Use your image
          </button>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            hidden
            onChange={(event) => void chooseImage((event.currentTarget as HTMLInputElement).files?.[0])}
          />
          <button type="button" class="experiment-button" disabled={active || !sourceEvents.length} onClick={replay}>
            Replay
          </button>
          {active ? (
            <button type="button" class="experiment-button" disabled={!onStop} onClick={onStop}>Stop</button>
          ) : (
            <button
              type="button"
              class="experiment-button experiment-button-primary"
              disabled={!onRun || !gatewayReady}
              onClick={() => void run()}
            >
              Run
            </button>
          )}
        </div>
      </div>
      </Section>

      {error ? <p class="experiment-status experiment-status-error" aria-live="polite">{error}</p> : active ? <p class="experiment-status" aria-live="polite">{constructing ? 'Replaying construction…' : running || submitting ? 'Running…' : 'Replaying…'}</p> : null}

      <Section title="View">
        <div class="experiment-canvas-grid">
          <figure>
            <canvas ref={targetCanvas} aria-label="Target image" />
            <figcaption>{target.label} · {target.size} × {target.size}</figcaption>
          </figure>
          <figure>
            <canvas ref={outputCanvas} aria-label="Discovered drawing output" />
            <figcaption>{constructing ? 'Construction replay' : result ? 'Discovered drawing' : 'Awaiting a search'}</figcaption>
          </figure>
        </div>

        <div class="experiment-viewer-controls">
          <button type="button" class="experiment-button" disabled={!canConstruct || active} onClick={replayConstruction}>
            {constructing ? 'Replaying…' : 'Replay construction'}
          </button>
        </div>

        <div class="experiment-metrics">
          <Metric label="layers" value={formatNumber(layers)} />
          <Metric label="candidate-pixel tests" value={pixelEvaluations ? `${(pixelEvaluations / 1_000_000_000).toFixed(2)} billion` : undefined} />
          <Metric better="lower" label="search wall time" value={seconds !== undefined ? `${seconds.toFixed(2)}s` : undefined} />
          <Metric better="higher" label="color error reduction" value={quality} />
        </div>
      </Section>

      <Outcome
        targetLabel="Objective"
        target={
          <p class="experiment-objective">
            Reproduce the target image on the left using only a bounded list of drawn shapes — ellipses,
            rectangles and diamonds with colour, opacity and order. No reference drawing program exists;
            the target is pixels, and the search must invent a program that paints them.
          </p>
        }
        resultLabel="Discovered drawing program"
        result={resultProgram ? <CodeBlock text={resultProgram} /> : undefined}
      />

      <Section title="Export">
        <div class="experiment-exports">
          <button type="button" class="experiment-button" disabled={!resultProgram} onClick={() => resultProgram && downloadText('discovered-pixel.gremlin', resultProgram)}>Download .gremlin</button>
          <button type="button" class="experiment-button" disabled={!result?.image} onClick={() => downloadCanvas(`drawing-${resultSize}.png`, outputCanvas.current)}>Download PNG</button>
        </div>
      </Section>

      <details class="experiment-details">
        <summary>Evidence</summary>
        <p>{resultSize} × {resultSize} pixels. {seconds !== undefined ? `${seconds.toFixed(2)} s search` : 'No completed search yet'}{asNumber(finished?.total_seconds) !== undefined ? ` / ${asNumber(finished?.total_seconds)?.toFixed(2)} s through source validation.` : '.'}</p>
        {asNumber(finished?.kernel_ms) !== undefined && <p>GPU scoring kernels: {asNumber(finished?.kernel_ms)?.toFixed(0)} ms. Working buffers: {asNumber(finished?.device_bytes) !== undefined ? `${(asNumber(finished?.device_bytes)! / 1024).toFixed(0)} KiB` : 'not reported'}.</p>}
        {asNumber(finished?.native_pixels_checked) !== undefined && <p>Final GPU image: {formatNumber(asNumber(finished?.native_pixels_checked))} checked pixels. Gremlin export: {formatNumber(asNumber(finished?.gremlin_pixels_checked))} sampled pixels.</p>}
        {asNumber(finished?.rmse) !== undefined && <p>Approximation error: {asNumber(finished?.rmse)?.toFixed(1)} / 255 RMS per color channel.</p>}
      </details>
    </section>
  );
}
