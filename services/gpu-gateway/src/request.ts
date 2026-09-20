export async function readJson(request: Request, maximumBytes: number): Promise<unknown> {
  const declaredLength = Number(request.headers.get('content-length') ?? 0);
  if (Number.isFinite(declaredLength) && declaredLength > maximumBytes) {
    throw new Error('Request body is too large.');
  }
  const body = await request.text();
  if (Buffer.byteLength(body, 'utf8') > maximumBytes) throw new Error('Request body is too large.');
  try {
    return JSON.parse(body);
  } catch {
    throw new Error('Request body must be valid JSON.');
  }
}
