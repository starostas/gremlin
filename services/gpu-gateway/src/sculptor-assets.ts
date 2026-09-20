import { createHash, createHmac, randomUUID, timingSafeEqual } from 'node:crypto';
import { del, get } from '@vercel/blob';

const resolutions = [128, 256, 512, 1024, 2048] as const;
const assetLifetimeMs = 15 * 60 * 1000;

export type SculptorResolution = (typeof resolutions)[number];
export type SculptorAssetReference = { id: string; ticket: string };

type AssetClaims = {
  version: 1;
  id: string;
  resolution: SculptorResolution;
  byteLength: number;
  sha256: string;
  exp: number;
};

const isResolution = (value: unknown): value is SculptorResolution =>
  typeof value === 'number' && resolutions.includes(value as SculptorResolution);

export function expectedRasterBytes(resolution: SculptorResolution) {
  return resolution * resolution * 3;
}

function assetSigningSecret() {
  const secret = process.env.SCULPTOR_ASSET_SIGNING_SECRET;
  if (!secret || secret.length < 32) {
    throw new Error('SCULPTOR_ASSET_SIGNING_SECRET must be set to at least 32 characters.');
  }
  return secret;
}

function sign(encoded: string) {
  return createHmac('sha256', assetSigningSecret()).update(encoded).digest('base64url');
}

function validId(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function validDigest(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value);
}

function decodeClaims(ticket: string): AssetClaims | undefined {
  if (typeof ticket !== 'string' || ticket.length > 1_000) return undefined;
  const [encoded, supplied] = ticket.split('.');
  if (!encoded || !supplied || ticket.split('.').length !== 2) return undefined;
  const expected = Buffer.from(sign(encoded));
  const actual = Buffer.from(supplied);
  if (expected.length !== actual.length || !timingSafeEqual(expected, actual)) return undefined;
  try {
    const claims = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8')) as Partial<AssetClaims>;
    if (
      claims.version !== 1 ||
      !validId(claims.id) ||
      !isResolution(claims.resolution) ||
      !Number.isInteger(claims.byteLength) ||
      claims.byteLength !== expectedRasterBytes(claims.resolution) ||
      !validDigest(claims.sha256) ||
      typeof claims.exp !== 'number' ||
      !Number.isInteger(claims.exp) ||
      claims.exp < Date.now()
    ) return undefined;
    return claims as AssetClaims;
  } catch {
    return undefined;
  }
}

export function sculptorAssetPath(id: string) {
  return `uploads/sculptor/${id}.rgb`;
}

/** Creates a one-time, path-bound reference for a normalized browser raster. */
export function createSculptorAsset(resolution: SculptorResolution, byteLength: number, sha256: string) {
  if (byteLength !== expectedRasterBytes(resolution) || !validDigest(sha256)) {
    throw new Error('Unsupported image raster.');
  }
  const claims: AssetClaims = {
    version: 1,
    id: randomUUID(),
    resolution,
    byteLength,
    sha256: sha256.toLowerCase(),
    exp: Date.now() + assetLifetimeMs
  };
  const encoded = Buffer.from(JSON.stringify(claims)).toString('base64url');
  return { id: claims.id, pathname: sculptorAssetPath(claims.id), ticket: `${encoded}.${sign(encoded)}` };
}

export function parseSculptorAsset(reference: SculptorAssetReference) {
  if (!reference || !validId(reference.id)) return undefined;
  const claims = decodeClaims(reference.ticket);
  return claims?.id === reference.id ? claims : undefined;
}

/** Reads only an asset that was authorized for this exact job request. */
export async function readSculptorAsset(reference: SculptorAssetReference) {
  const claims = parseSculptorAsset(reference);
  if (!claims) throw new Error('The image upload has expired or is invalid.');
  const result = await get(sculptorAssetPath(claims.id), { access: 'private', useCache: false });
  if (!result || result.statusCode !== 200 || result.blob.size !== claims.byteLength) {
    throw new Error('The uploaded image is unavailable.');
  }
  const bytes = Buffer.from(await new Response(result.stream).arrayBuffer());
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (bytes.byteLength !== claims.byteLength || actual !== claims.sha256) {
    throw new Error('The uploaded image did not pass verification.');
  }
  return { ...claims, bytes };
}

export async function deleteSculptorAsset(reference: SculptorAssetReference) {
  const claims = parseSculptorAsset(reference);
  if (claims) await del(sculptorAssetPath(claims.id));
}
