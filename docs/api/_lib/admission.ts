const verificationUrl = 'https://challenges.cloudflare.com/turnstile/v0/siteverify';
const action = 'gremlin_gpu_job';

type TurnstileResponse = {
  success?: unknown;
  hostname?: unknown;
  action?: unknown;
};

function localBypassEnabled() {
  return process.env.NODE_ENV !== 'production' && process.env.ALLOW_LOCAL_UNPROTECTED_JOBS === 'true';
}

/**
 * Running without a human check is a deliberate choice, not a consequence of
 * forgetting to configure one. Missing Turnstile configuration still fails
 * closed; only this explicit variable opens the routes, and what then limits
 * abuse is the global pending-job cap plus the worker running one job at a
 * time. Anyone who finds the endpoint can keep the GPU busy.
 */
function unverifiedJobsAllowed() {
  return process.env.ALLOW_UNVERIFIED_JOBS === 'true';
}

/**
 * A GPU run is an expensive public action. Production requests must carry a
 * one-time Turnstile proof unless human verification is explicitly disabled.
 */
export async function admitsTurnstileToken(token: string | null) {
  if (localBypassEnabled() || unverifiedJobsAllowed()) return true;

  const secret = process.env.TURNSTILE_SECRET_KEY;
  const hostname = process.env.TURNSTILE_EXPECTED_HOSTNAME;
  if (!secret || !hostname || !token || token.length > 4096) return false;

  const body = new URLSearchParams({ secret, response: token });
  try {
    const response = await fetch(verificationUrl, {
      method: 'POST',
      body,
      redirect: 'error',
      signal: AbortSignal.timeout(5_000)
    });
    if (!response.ok) return false;
    const result = (await response.json()) as TurnstileResponse;
    return result.success === true && result.hostname === hostname && result.action === action;
  } catch {
    return false;
  }
}

export async function admitsJobRequest(request: Request) {
  return admitsTurnstileToken(request.headers.get('x-turnstile-token'));
}
