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

/** GET `/api/<path>`, or POST it as JSON when `body` is given. */
export async function api<T = unknown>(path: string, body?: unknown): Promise<T> {
  const init: RequestInit =
    body === undefined
      ? {}
      : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }
  const res = await fetch(`/api/${path}`, init)
  const data = res.headers.get('content-type')?.includes('json') ? await res.json() : null
  if (!res.ok) throw new ApiError(res.status, data)
  return data as T
}
