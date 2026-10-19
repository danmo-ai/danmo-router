// Channel presets only pre-fill the form. The relay never reads them: forwarding depends on
// the saved upstream_protocol / auth_style / base_url and each model row's dialect alone.

export type DialectRule = [prefix: string, dialect: string]

export type ChannelPreset = {
  id: string
  label: string
  group: string
  type: string
  /** Original vendor of the models; empty for aggregators. */
  vendor: string
  base_url: string
  upstream_protocol: string
  auth_style: string
  models: string
  default_dialect: string
  /** Model-name prefixes whose rows take another dialect on this platform. */
  rules?: DialectRule[]
}

const chat = 'openai_chat'
const responses = 'openai_responses'
const messages = 'anthropic_messages'

export const CHANNEL_PRESETS: ChannelPreset[] = [
  { id: 'deepseek-chat', group: '国产原厂', label: 'DeepSeek · OpenAI Chat', type: 'deepseek', vendor: 'deepseek', base_url: 'https://api.deepseek.com', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek-chat,deepseek-reasoner', default_dialect: 'thinking' },
  { id: 'deepseek-messages', group: '国产原厂', label: 'DeepSeek · Anthropic Messages', type: 'deepseek', vendor: 'deepseek', base_url: 'https://api.deepseek.com/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'deepseek-chat,deepseek-reasoner', default_dialect: 'standard' },
  { id: 'kimi-chat', group: '国产原厂', label: 'Kimi · OpenAI Chat', type: 'moonshot', vendor: 'moonshot', base_url: 'https://api.moonshot.cn/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'kimi-k2-0905-preview,kimi-k2-turbo-preview', default_dialect: 'kimi' },
  { id: 'kimi-messages', group: '国产原厂', label: 'Kimi · Anthropic Messages', type: 'moonshot', vendor: 'moonshot', base_url: 'https://api.moonshot.cn/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'kimi-k2-0905-preview,kimi-k2-turbo-preview', default_dialect: 'standard' },
  { id: 'qwen-chat', group: '国产原厂', label: 'Qwen 百炼 · OpenAI Chat', type: 'dashscope', vendor: 'dashscope', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'qwen-plus,qwen3-max,qwen3-coder-plus', default_dialect: 'enable_thinking' },
  { id: 'qwen-responses', group: '国产原厂', label: 'Qwen 百炼 · OpenAI Responses', type: 'dashscope', vendor: 'dashscope', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1', upstream_protocol: responses, auth_style: 'bearer', models: 'qwen-plus,qwen3-max', default_dialect: 'thinking' },
  { id: 'qwen-messages', group: '国产原厂', label: 'Qwen 百炼 · Anthropic Messages', type: 'dashscope', vendor: 'dashscope', base_url: 'https://dashscope.aliyuncs.com/apps/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'qwen3-coder-plus,qwen3-max', default_dialect: 'standard' },
  { id: 'glm-chat', group: '国产原厂', label: '智谱 GLM · OpenAI Chat', type: 'zhipu', vendor: 'zhipu', base_url: 'https://open.bigmodel.cn/api/paas/v4', upstream_protocol: chat, auth_style: 'bearer', models: 'glm-4.6,glm-4.5-air', default_dialect: 'thinking' },
  { id: 'glm-coding', group: '国产原厂', label: '智谱 GLM Coding Plan · OpenAI Chat', type: 'zhipu', vendor: 'zhipu', base_url: 'https://open.bigmodel.cn/api/coding/paas/v4', upstream_protocol: chat, auth_style: 'bearer', models: 'glm-4.6', default_dialect: 'thinking' },
  { id: 'glm-messages', group: '国产原厂', label: '智谱 GLM · Anthropic Messages', type: 'zhipu', vendor: 'zhipu', base_url: 'https://open.bigmodel.cn/api/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'glm-4.6,glm-4.5-air', default_dialect: 'standard' },
  { id: 'ark-chat', group: '国产原厂', label: '豆包 火山方舟 · OpenAI Chat', type: 'volcengine', vendor: 'volcengine', base_url: 'https://ark.cn-beijing.volces.com/api/v3', upstream_protocol: chat, auth_style: 'bearer', models: 'doubao-seed-1-6-250615', default_dialect: 'thinking' },
  { id: 'ark-responses', group: '国产原厂', label: '豆包 火山方舟 · OpenAI Responses', type: 'volcengine', vendor: 'volcengine', base_url: 'https://ark.cn-beijing.volces.com/api/v3', upstream_protocol: responses, auth_style: 'bearer', models: 'doubao-seed-1-6-250615', default_dialect: 'thinking' },
  { id: 'minimax-chat', group: '国产原厂', label: 'MiniMax · OpenAI Chat', type: 'minimax', vendor: 'minimax', base_url: 'https://api.minimaxi.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'MiniMax-M2', default_dialect: 'minimax' },
  { id: 'minimax-messages', group: '国产原厂', label: 'MiniMax · Anthropic Messages', type: 'minimax', vendor: 'minimax', base_url: 'https://api.minimaxi.com/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'MiniMax-M2', default_dialect: 'effort' },
  { id: 'qianfan-chat', group: '国产原厂', label: '百度千帆 · OpenAI Chat', type: 'qianfan', vendor: 'qianfan', base_url: 'https://qianfan.baidubce.com/v2', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek-v3.2,ernie-4.5-turbo-128k,ernie-x1-turbo-32k', default_dialect: 'qianfan', rules: [['ernie-', 'qianfan_ernie'], ['qwen', 'qianfan_ernie']] },
  { id: 'hunyuan-chat', group: '国产原厂', label: '腾讯混元 · OpenAI Chat', type: 'hunyuan', vendor: 'hunyuan', base_url: 'https://api.hunyuan.cloud.tencent.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'hunyuan-turbos-latest', default_dialect: 'hunyuan' },
  { id: 'siliconflow', group: '三方', label: '硅基流动 · OpenAI Chat', type: 'siliconflow', vendor: '', base_url: 'https://api.siliconflow.cn/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek-ai/DeepSeek-V3.2-Exp', default_dialect: 'opaque' },
  { id: 'openrouter', group: '三方', label: 'OpenRouter · OpenAI Chat', type: 'openrouter', vendor: '', base_url: 'https://openrouter.ai/api/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek/deepseek-chat-v3.1', default_dialect: 'opaque' },
  { id: 'relay-chat', group: '三方', label: 'new-api / one-api 中转 · OpenAI Chat', type: 'relay', vendor: '', base_url: 'https://relay.example.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: '', default_dialect: 'opaque' },
  { id: 'relay-messages', group: '三方', label: 'new-api / one-api 中转 · Anthropic Messages', type: 'relay', vendor: '', base_url: 'https://relay.example.com', upstream_protocol: messages, auth_style: 'x_api_key', models: '', default_dialect: 'opaque' },
  { id: 'openai', group: '海外原厂', label: 'OpenAI · Chat', type: 'openai', vendor: 'openai', base_url: 'https://api.openai.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'gpt-4o-mini', default_dialect: 'standard' },
  { id: 'openai-responses', group: '海外原厂', label: 'OpenAI · Responses', type: 'openai', vendor: 'openai', base_url: 'https://api.openai.com/v1', upstream_protocol: responses, auth_style: 'bearer', models: 'gpt-5', default_dialect: 'standard' },
  { id: 'anthropic', group: '海外原厂', label: 'Anthropic · Messages', type: 'anthropic', vendor: 'anthropic', base_url: 'https://api.anthropic.com', upstream_protocol: messages, auth_style: 'x_api_key', models: 'claude-sonnet-4-5', default_dialect: 'standard' },
  { id: 'gemini', group: '海外原厂', label: 'Google Gemini', type: 'gemini', vendor: 'gemini', base_url: 'https://generativelanguage.googleapis.com/v1beta', upstream_protocol: 'gemini', auth_style: 'x_goog_api_key', models: 'gemini-2.5-pro,gemini-2.5-flash', default_dialect: 'standard' },
]

/** Longest matching prefix rule, else the channel default. */
export function dialectForModel(model: string, defaultDialect: string, rules: DialectRule[] = []): string {
  const lower = model.toLowerCase()
  let best: DialectRule | undefined
  for (const r of rules) {
    if (lower.startsWith(r[0].toLowerCase()) && (!best || r[0].length > best[0].length)) best = r
  }
  return best ? best[1] : defaultDialect
}

type Family = { prefix: string; vendor: string; native: Record<string, string> }

/** Model families with their vendor and the dialects the vendor's own API speaks. */
const FAMILIES: Family[] = [
  { prefix: 'deepseek-', vendor: 'deepseek', native: { openai_chat: 'thinking', anthropic_messages: 'standard' } },
  { prefix: 'qwen', vendor: 'dashscope', native: { openai_chat: 'enable_thinking', openai_responses: 'thinking', anthropic_messages: 'standard' } },
  { prefix: 'kimi-', vendor: 'moonshot', native: { openai_chat: 'kimi', anthropic_messages: 'standard' } },
  { prefix: 'glm-', vendor: 'zhipu', native: { openai_chat: 'thinking', anthropic_messages: 'standard' } },
  { prefix: 'doubao-', vendor: 'volcengine', native: { openai_chat: 'thinking', openai_responses: 'thinking' } },
  { prefix: 'minimax-', vendor: 'minimax', native: { openai_chat: 'minimax', anthropic_messages: 'effort' } },
  { prefix: 'hunyuan-', vendor: 'hunyuan', native: { openai_chat: 'hunyuan' } },
  { prefix: 'ernie-', vendor: 'qianfan', native: { openai_chat: 'qianfan_ernie' } },
]

export type NativeSuggestion = { dialects: Record<string, string>; source: string }

/**
 * Native dialects for a model: rows of channels run by the model's own vendor first, then the
 * built-in family table. Null when neither knows the model.
 */
export function suggestNativeDialects(model: string, channels: any[]): NativeSuggestion | null {
  const lower = model.toLowerCase()
  const family = FAMILIES.find((f) => lower.startsWith(f.prefix))
  if (!family) return null
  const fromChannels: Record<string, string> = {}
  const sources: string[] = []
  for (const c of channels) {
    if (c.vendor !== family.vendor) continue
    const row = (c.models ?? []).find((r: any) => r.model === model)
    if (!row || row.upstream_dialect === 'opaque' || fromChannels[c.upstream_protocol]) continue
    fromChannels[c.upstream_protocol] = row.upstream_dialect
    sources.push(c.name)
  }
  if (sources.length > 0) {
    return { dialects: fromChannels, source: `原厂渠道：${sources.join('、')}` }
  }
  return { dialects: { ...family.native }, source: `内置表：${family.prefix}*` }
}

export const PROTOCOL_LABELS: Record<string, string> = {
  openai_chat: 'OpenAI Chat',
  openai_responses: 'OpenAI Responses',
  anthropic_messages: 'Anthropic Messages',
  gemini: 'Gemini',
  embeddings: 'Embeddings',
}

export const MODE_LABELS: Record<string, string> = {
  passthrough: '透传（同协议）',
  convert: '转换（可服务其他协议）',
}
