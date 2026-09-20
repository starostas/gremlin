// Local docs origins are a development convenience. Allowing them in production
// would let any page claiming that Origin reach the job routes, so production
// trusts only what DOCS_ORIGIN names.
const developmentOrigins = ['http://127.0.0.1:4321', 'http://localhost:4321'];

function allowedOrigins() {
  const configured = process.env.DOCS_ORIGIN?.split(',').map((origin) => origin.trim()).filter(Boolean) ?? [];
  const defaults = process.env.NODE_ENV === 'production' ? [] : developmentOrigins;
  return new Set([...defaults, ...configured]);
}

export function corsHeaders(request: Request): Record<string, string> {
  const origin = request.headers.get('origin');
  if (!origin || !allowedOrigins().has(origin)) return {};
  return {
    'Access-Control-Allow-Origin': origin,
    'Access-Control-Allow-Headers': 'content-type, x-experiment-capability, x-turnstile-token',
    'Access-Control-Allow-Methods': 'GET, POST, DELETE, OPTIONS',
    Vary: 'Origin'
  };
}

export function hasAllowedOrigin(request: Request) {
  const origin = request.headers.get('origin');
  return Boolean(origin && allowedOrigins().has(origin));
}

export function json(request: Request, body: unknown, status = 200) {
  return Response.json(body, { status, headers: corsHeaders(request) });
}

export function privateJson(request: Request, body: unknown, status = 200) {
  return Response.json(body, {
    status,
    headers: {
      ...corsHeaders(request),
      'Cache-Control': 'no-store',
      Vary: 'Origin, X-Experiment-Capability'
    }
  });
}

export function options(request: Request) {
  return new Response(null, { status: 204, headers: corsHeaders(request) });
}
