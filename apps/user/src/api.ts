export type ApiResponse<T> = {
  success: boolean
  message?: string
  data?: T
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
    ...init,
  })
  const json = (await res.json()) as ApiResponse<T>
  if (!res.ok || !json.success) {
    throw new Error(json.message || `请求失败 (${res.status})`)
  }
  return json.data as T
}

export const api = {
  captcha: () =>
    request<{ token: string; image_svg: string }>('/api/user/captcha'),
  register: (
    username: string,
    password: string,
    captcha_token: string,
    captcha_answer: string,
  ) =>
    request('/api/user/register', {
      method: 'POST',
      body: JSON.stringify({
        username,
        password,
        captcha_token,
        captcha_answer,
      }),
    }),
  login: (username: string, password: string) =>
    request('/api/user/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request('/api/user/logout', { method: 'POST' }),
  me: () => request<any>('/api/user/me'),
  tokens: () => request<any[]>('/api/user/tokens'),
  createToken: (body: any) =>
    request<{ token: any; key: string }>('/api/user/tokens', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  setTokenStatus: (id: number, status: string) =>
    request(`/api/user/tokens/${id}/status`, {
      method: 'POST',
      body: JSON.stringify({ status }),
    }),
  deleteToken: (id: number) =>
    request(`/api/user/tokens/${id}`, { method: 'DELETE' }),
  logs: (q: LogQuery = {}) => request<any[]>(logPath('/api/user/logs', q)),
  usage: (q: LogQuery = {}) => request<UsageReport>(logPath('/api/user/usage', q)),
  updateToken: (id: number, body: any) =>
    request(`/api/user/tokens/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
  redeem: (code: string) =>
    request<any>('/api/user/redeem', {
      method: 'POST',
      body: JSON.stringify({ code }),
    }),
  changePassword: (old_password: string, password: string) =>
    request('/api/user/password', {
      method: 'POST',
      body: JSON.stringify({ old_password, password }),
    }),
  models: () => request<any>('/api/user/models'),
  meta: () => request<{ relay_base_url: string }>('/api/user/meta'),
}

export type UsageBucket = {
  requests: number
  success_requests: number
  prompt_tokens: number
  completion_tokens: number
  cache_read_tokens: number
  cache_write_tokens: number
  reasoning_tokens: number
  quota: number
}

export type UsageReport = {
  totals: UsageBucket
  series: ({ day: string } & UsageBucket)[]
  by_model: {
    model: string
    requests: number
    quota: number
    prompt_tokens: number
    completion_tokens: number
  }[]
  retention_days: number
}

export type LogQuery = {
  limit?: number
  offset?: number
  model?: string
  status?: string
  token_id?: string | number
  start?: string
  end?: string
}

function logPath(base: string, q: LogQuery) {
  const p = new URLSearchParams()
  if (q.limit !== undefined) p.set('limit', String(q.limit))
  if (q.offset !== undefined) p.set('offset', String(q.offset))
  if (q.model) p.set('model', q.model)
  if (q.status) p.set('status', q.status)
  if (q.token_id) p.set('token_id', String(q.token_id))
  if (q.start) p.set('start', q.start)
  if (q.end) p.set('end', q.end)
  const qs = p.toString()
  return qs ? `${base}?${qs}` : base
}
