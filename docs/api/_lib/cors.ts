/**
 * The islands and these routes are served from one origin, so a job request is
 * same-origin and needs no CORS grant at all. Nothing here emits
 * Access-Control-Allow-Origin: a cross-site page cannot read these responses,
 * and no allowlist has to be kept in sync with the deployed hostname.
 */

/**
 * Rejects the cross-site form submissions and fetches a browser labels for us.
 * `Sec-Fetch-Site` is set by the browser and cannot be overridden by page
 * script; a non-browser client can of course send anything, which is why this
 * is a hardening measure and not the admission control.
 */
export function hasAllowedOrigin(request: Request) {
  const site = request.headers.get('sec-fetch-site');
  if (site) return site === 'same-origin' || site === 'none';

  // Older browsers omit Sec-Fetch-Site. Fall back to comparing Origin with the
  // host actually serving the request.
  const origin = request.headers.get('origin');
  if (!origin) return true;
  try {
    return new URL(origin).host === request.headers.get('host');
  } catch {
    return false;
  }
}

export function json(_request: Request, body: unknown, status = 200) {
  return Response.json(body, { status });
}

export function privateJson(_request: Request, body: unknown, status = 200) {
  return Response.json(body, {
    status,
    headers: { 'Cache-Control': 'no-store', Vary: 'X-Experiment-Capability' }
  });
}

export function options(_request: Request) {
  return new Response(null, { status: 204 });
}
