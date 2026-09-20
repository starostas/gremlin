export const demoIds = [
  'shader-detective',
  'landing-lab',
  'tiny-robot',
  'shader-sculptor',
  'orbit-forge'
] as const;

export type DemoId = (typeof demoIds)[number];
export type JsonObject = Record<string, unknown>;

export type DemoInput =
  | { mode: 'cpu' | 'gpu' | 'both'; preset: 'afterglow' | 'aurora'; cases: 8192 | 32768; seed: 1 | 2 | 3 }
  | { mode: 'cpu' | 'gpu' | 'both'; cases: 2048 | 8192; seed: 1 | 2 | 3 }
  | { cases: 128 | 512; seed: 1 | 2 | 3 }
  | { preset: 'planet' | 'bloom' | 'city'; resolution: 128 | 256 | 512 | 1024 | 2048; budgetMs: 1500 | 3000 | 12000 | 24000 | 48000; seed: 1 | 2 | 3 }
  | { imageAsset: { id: string; ticket: string }; resolution: 128 | 256 | 512 | 1024 | 2048; budgetMs: 1500 | 3000 | 12000 | 24000 | 48000; seed: 1 | 2 | 3 }
  | { tolerance: 0.001 | 0.0001 | 0.00001; seed: 1 | 2 | 3 };

export type JobStatus =
  | 'queued'
  | 'dispatching'
  | 'running'
  | 'cancel_requested'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface JobState {
  version: 1;
  id: string;
  demoId: DemoId;
  input: DemoInput;
  capabilityHash: string;
  status: JobStatus;
  createdAt: string;
  updatedAt: string;
  finishedAt?: string;
  error?: string;
  events: JsonObject[];
  lastWorkerSequence?: number;
}

const allowedInputs: Record<DemoId, ReadonlySet<string>> = {
  'shader-detective': new Set(['mode', 'preset', 'cases', 'seed']),
  'landing-lab': new Set(['mode', 'cases', 'seed']),
  'tiny-robot': new Set(['cases', 'seed']),
  'shader-sculptor': new Set(['preset', 'imageAsset', 'resolution', 'budgetMs', 'seed']),
  'orbit-forge': new Set(['tolerance', 'seed'])
};

const isObject = (value: unknown): value is JsonObject =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const isOneOf = <T extends string | number>(value: unknown, options: readonly T[]): value is T =>
  options.includes(value as T);

const hasOnlyKeys = (input: JsonObject, allowed: ReadonlySet<string>) =>
  Object.keys(input).every((key) => allowed.has(key));

const hasExactKeys = (input: JsonObject, keys: readonly string[]) =>
  Object.keys(input).length === keys.length && keys.every((key) => Object.hasOwn(input, key));

const isAssetReference = (value: unknown): value is { id: string; ticket: string } =>
  isObject(value) &&
  hasExactKeys(value, ['id', 'ticket']) &&
  typeof value.id === 'string' &&
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value.id) &&
  typeof value.ticket === 'string' &&
  value.ticket.length >= 32 && value.ticket.length <= 1_000;

export function parseCreateJob(value: unknown): { demoId: DemoId; input: DemoInput } | undefined {
  if (!isObject(value) || !isOneOf(value.demoId, demoIds) || !isObject(value.input)) return undefined;
  if (!hasOnlyKeys(value, new Set(['demoId', 'input']))) return undefined;
  const { demoId, input } = value;
  if (!hasOnlyKeys(input, allowedInputs[demoId])) return undefined;

  if (
    demoId === 'shader-detective' &&
    isOneOf(input.mode, ['cpu', 'gpu', 'both'] as const) &&
    isOneOf(input.preset, ['afterglow', 'aurora'] as const) &&
    isOneOf(input.cases, [8192, 32768] as const) &&
    isOneOf(input.seed, [1, 2, 3] as const)
  ) {
    return { demoId, input: { mode: input.mode, preset: input.preset, cases: input.cases, seed: input.seed } };
  }

  if (
    demoId === 'landing-lab' &&
    isOneOf(input.mode, ['cpu', 'gpu', 'both'] as const) &&
    isOneOf(input.cases, [2048, 8192] as const) &&
    isOneOf(input.seed, [1, 2, 3] as const)
  ) {
    return { demoId, input: { mode: input.mode, cases: input.cases, seed: input.seed } };
  }

  if (
    demoId === 'tiny-robot' &&
    isOneOf(input.cases, [128, 512] as const) &&
    isOneOf(input.seed, [1, 2, 3] as const)
  ) {
    return { demoId, input: { cases: input.cases, seed: input.seed } };
  }

  if (
    demoId === 'shader-sculptor' &&
    isOneOf(input.resolution, [128, 256, 512, 1024, 2048] as const) &&
    isOneOf(input.budgetMs, [1500, 3000, 12000, 24000, 48000] as const) &&
    isOneOf(input.seed, [1, 2, 3] as const)
  ) {
    if (hasExactKeys(input, ['preset', 'resolution', 'budgetMs', 'seed']) && isOneOf(input.preset, ['planet', 'bloom', 'city'] as const)) {
      return { demoId, input: { preset: input.preset, resolution: input.resolution, budgetMs: input.budgetMs, seed: input.seed } };
    }
    if (hasExactKeys(input, ['imageAsset', 'resolution', 'budgetMs', 'seed']) && isAssetReference(input.imageAsset)) {
      return { demoId, input: { imageAsset: input.imageAsset, resolution: input.resolution, budgetMs: input.budgetMs, seed: input.seed } };
    }
  }

  if (
    demoId === 'orbit-forge' &&
    isOneOf(input.tolerance, [0.001, 0.0001, 0.00001] as const) &&
    isOneOf(input.seed, [1, 2, 3] as const)
  ) {
    return { demoId, input: { tolerance: input.tolerance, seed: input.seed } };
  }

  return undefined;
}

export const isJobId = (value: string) => /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);

export const isTerminal = (status: JobStatus) =>
  status === 'succeeded' || status === 'failed' || status === 'cancelled';

export const isObjectValue = (value: unknown): value is JsonObject => isObject(value);
