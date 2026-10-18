/** Error body the server sends: `{ error, field?, retry_after? }`. */
export type ErrorBody = { error?: string; field?: string; retry_after?: number } | null

export class ApiError extends Error {
  constructor(
    public status: number,
    public body: ErrorBody,
  ) {
    super(`HTTP ${status}`)
  }
}

/** GET `/api/<path>`, or send `body` as JSON with `method` (POST unless given). */
export function api<T = unknown>(path: string, body?: unknown, method = 'POST'): Promise<T> {
  const init: RequestInit =
    body === undefined
      ? {}
      : { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }
  return request<T>(path, init)
}

/** POSTs a file as the raw request body (card images). */
export function upload<T = unknown>(path: string, file: Blob): Promise<T> {
  return request<T>(path, { method: 'POST', body: file })
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
  const res = await fetch(`/api/${path}`, init)
  const data = res.headers.get('content-type')?.includes('json') ? await res.json() : null
  // The server ended the parent session (10 idle minutes): back to the PIN screen.
  if (res.status === 401 && path.startsWith('parent/')) dispatchEvent(new Event('wistjet:locked'))
  if (!res.ok) throw new ApiError(res.status, data)
  return data as T
}
