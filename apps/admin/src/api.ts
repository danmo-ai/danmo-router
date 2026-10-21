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

export type Dialect = {
  protocol: string
  name: string
  label: string
  opaque: boolean
  usage: string[]
}

export type Meta = {
  protocols: string[]
  relay_modes: string[]
  auth_styles: string[]
  usage_adapters: { name: string; protocol: string }[]
  dialects: Dialect[]
}

export const api = {
  login: (username: string, password: string) =>
    request('/api/admin/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request('/api/admin/logout', { method: 'POST' }),
  me: () => request<any>('/api/admin/me'),
  overview: () => request<any>('/api/admin/overview'),
  meta: () => request<Meta>('/api/admin/meta'),
  channels: () => request<any[]>('/api/admin/channels'),
  createChannel: (body: any) =>
    request('/api/admin/channels', { method: 'POST', body: JSON.stringify(body) }),
  updateChannel: (id: number, body: any) =>
    request(`/api/admin/channels/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
  setChannelStatus: (id: number, status: string) =>
    request(`/api/admin/channels/${id}/status`, {
      method: 'PUT',
      body: JSON.stringify({ status }),
    }),
  deleteChannel: (id: number) =>
    request(`/api/admin/channels/${id}`, { method: 'DELETE' }),
  users: () => request<any[]>('/api/admin/users'),
  createUser: (body: any) =>
    request('/api/admin/users', { method: 'POST', body: JSON.stringify(body) }),
  updateUser: (id: number, body: any) =>
    request(`/api/admin/users/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
  adjustQuota: (id: number, delta: number) =>
    request(`/api/admin/users/${id}/quota`, {
      method: 'POST',
      body: JSON.stringify({ delta }),
    }),
  modelPrices: () => request<any[]>('/api/admin/model-prices'),
  upsertModelPrice: (body: any) =>
    request('/api/admin/model-prices', { method: 'PUT', body: JSON.stringify(body) }),
  groupRatios: () => request<any[]>('/api/admin/group-ratios'),
  upsertGroupRatio: (group_name: string, ratio: number) =>
    request('/api/admin/group-ratios', {
      method: 'PUT',
      body: JSON.stringify({ group_name, ratio }),
    }),
  logs: (q: LogQuery = {}) => request<any[]>(logPath('/api/admin/logs', q)),
  usage: (q: LogQuery = {}) => request<UsageReport>(logPath('/api/admin/usage', q)),
  testChannel: (id: number) =>
    request<any>(`/api/admin/channels/${id}/test`, { method: 'POST' }),
  resetPassword: (id: number, password: string) =>
    request(`/api/admin/users/${id}/password`, {
      method: 'POST',
      body: JSON.stringify({ password }),
    }),
  userTokens: (id: number) => request<any[]>(`/api/admin/users/${id}/tokens`),
  setUserTokenStatus: (userId: number, tokenId: number, status: string) =>
    request(`/api/admin/users/${userId}/tokens/${tokenId}/status`, {
      method: 'POST',
      body: JSON.stringify({ status }),
    }),
  deleteUserToken: (userId: number, tokenId: number) =>
    request(`/api/admin/users/${userId}/tokens/${tokenId}`, { method: 'DELETE' }),
  redemptions: () => request<any[]>('/api/admin/redemptions'),
  createRedemptions: (body: { name: string; quota: number; count: number }) =>
    request<any[]>('/api/admin/redemptions', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  deleteRedemption: (id: number) =>
    request(`/api/admin/redemptions/${id}`, { method: 'DELETE' }),
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
  user_id?: string | number
  model?: string
  status?: string
  channel_id?: string | number
  token_id?: string | number
  start?: string
  end?: string
}

function logPath(base: string, q: LogQuery) {
  const p = new URLSearchParams()
  if (q.limit !== undefined) p.set('limit', String(q.limit))
  if (q.offset !== undefined) p.set('offset', String(q.offset))
  if (q.user_id) p.set('user_id', String(q.user_id))
  if (q.model) p.set('model', q.model)
  if (q.status) p.set('status', q.status)
  if (q.channel_id) p.set('channel_id', String(q.channel_id))
  if (q.token_id) p.set('token_id', String(q.token_id))
  if (q.start) p.set('start', q.start)
  if (q.end) p.set('end', q.end)
  const qs = p.toString()
  return qs ? `${base}?${qs}` : base
}
