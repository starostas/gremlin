import { handleUpload, type HandleUploadBody } from '@vercel/blob/client';
import { hasAllowedOrigin, json, options } from '../../../src/cors.js';
import { expectedRasterBytes, parseSculptorAsset, sculptorAssetPath } from '../../../src/sculptor-assets.js';
import { readJson } from '../../../src/request.js';

function isTokenRequest(body: HandleUploadBody) {
  return body.type === 'blob.generate-client-token';
}

/**
 * Vercel Blob invokes this endpoint to mint a client upload token. The browser
 * can upload only a private, one-time raster whose path, exact size, digest,
 * and expiry were signed by /authorize. Blob completion callbacks are checked
 * by the SDK and do not require a browser Origin header.
 */
export default async function handler(request: Request) {
  if (request.method === 'OPTIONS') return options(request);
  if (request.method !== 'POST') return json(request, { error: 'Method not allowed.' }, 405);

  try {
    const body = await readJson(request, 8 * 1024) as HandleUploadBody;
    if (isTokenRequest(body) && !hasAllowedOrigin(request)) {
      return json(request, { error: 'Image uploads are unavailable.' }, 403);
    }
    const response = await handleUpload({
      body,
      request,
      onBeforeGenerateToken: async (pathname, clientPayload) => {
        const reference = clientPayload ? (() => {
          try {
            return JSON.parse(clientPayload) as { id?: unknown; ticket?: unknown };
          } catch {
            return undefined;
          }
        })() : undefined;
        const asset = reference && typeof reference.id === 'string' && typeof reference.ticket === 'string'
          ? parseSculptorAsset({ id: reference.id, ticket: reference.ticket })
          : undefined;
        if (!asset || pathname !== sculptorAssetPath(asset.id)) {
          throw new Error('Unsupported image upload.');
        }
        return {
          allowedContentTypes: ['application/octet-stream'],
          maximumSizeInBytes: expectedRasterBytes(asset.resolution),
          validUntil: asset.exp,
          addRandomSuffix: false,
          allowOverwrite: false,
          cacheControlMaxAge: 60
        };
      }
    });
    return json(request, response);
  } catch {
    return json(request, { error: 'Image uploads are unavailable.' }, 400);
  }
}
