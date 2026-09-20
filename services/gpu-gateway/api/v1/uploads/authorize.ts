import { admitsJobRequest } from '../../../src/admission.js';
import { hasAllowedOrigin, json, options } from '../../../src/cors.js';
import { createSculptorAsset, expectedRasterBytes, type SculptorResolution } from '../../../src/sculptor-assets.js';
import { readJson } from '../../../src/request.js';

const resolutions = [128, 256, 512, 1024, 2048] as const;

function isResolution(value: unknown): value is SculptorResolution {
  return typeof value === 'number' && resolutions.includes(value as SculptorResolution);
}

function parseRequest(value: unknown) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const input = value as Record<string, unknown>;
  if (Object.keys(input).length !== 3 || !Object.hasOwn(input, 'resolution') || !Object.hasOwn(input, 'byteLength') || !Object.hasOwn(input, 'sha256')) return undefined;
  if (!isResolution(input.resolution) || !Number.isInteger(input.byteLength) || input.byteLength !== expectedRasterBytes(input.resolution)) return undefined;
  if (typeof input.sha256 !== 'string' || !/^[0-9a-f]{64}$/i.test(input.sha256)) return undefined;
  return { resolution: input.resolution, byteLength: input.byteLength, sha256: input.sha256 };
}

/**
 * A browser first proves it is human, then gets a short-lived ticket for one
 * normalized RGB raster. The ticket is path- and digest-bound; it is not a
 * Blob credential and cannot be repurposed for arbitrary uploads.
 */
export default async function handler(request: Request) {
  if (request.method === 'OPTIONS') return options(request);
  if (request.method !== 'POST') return json(request, { error: 'Method not allowed.' }, 405);
  if (!hasAllowedOrigin(request) || !(await admitsJobRequest(request))) {
    return json(request, { error: 'Image uploads are unavailable.' }, 403);
  }
  try {
    const parsed = parseRequest(await readJson(request, 2 * 1024));
    if (!parsed) return json(request, { error: 'Unsupported image upload.' }, 400);
    const asset = createSculptorAsset(parsed.resolution, parsed.byteLength, parsed.sha256);
    return json(request, { id: asset.id, pathname: asset.pathname, ticket: asset.ticket });
  } catch {
    return json(request, { error: 'Image uploads are unavailable.' }, 503);
  }
}
