// Channel presets only pre-fill the form. The relay never reads them: forwarding depends on
// the saved upstream_protocol / auth_style / base_url alone.

export type ChannelPreset = {
  id: string
  label: string
  group: string
  type: string
  base_url: string
  upstream_protocol: string
  auth_style: string
  models: string
}

const chat = 'openai_chat'
const responses = 'openai_responses'
const messages = 'anthropic_messages'

export const CHANNEL_PRESETS: ChannelPreset[] = [
  { id: 'deepseek-chat', group: '国产原厂', label: 'DeepSeek · OpenAI Chat', type: 'deepseek', base_url: 'https://api.deepseek.com', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek-chat,deepseek-reasoner' },
  { id: 'deepseek-messages', group: '国产原厂', label: 'DeepSeek · Anthropic Messages', type: 'deepseek', base_url: 'https://api.deepseek.com/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'deepseek-chat,deepseek-reasoner' },
  { id: 'kimi-chat', group: '国产原厂', label: 'Kimi · OpenAI Chat', type: 'moonshot', base_url: 'https://api.moonshot.cn/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'kimi-k2-0905-preview,kimi-k2-turbo-preview' },
  { id: 'kimi-messages', group: '国产原厂', label: 'Kimi · Anthropic Messages', type: 'moonshot', base_url: 'https://api.moonshot.cn/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'kimi-k2-0905-preview,kimi-k2-turbo-preview' },
  { id: 'qwen-chat', group: '国产原厂', label: 'Qwen 百炼 · OpenAI Chat', type: 'dashscope', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'qwen-plus,qwen3-max,qwen3-coder-plus' },
  { id: 'qwen-responses', group: '国产原厂', label: 'Qwen 百炼 · OpenAI Responses', type: 'dashscope', base_url: 'https://dashscope.aliyuncs.com/compatible-mode/v1', upstream_protocol: responses, auth_style: 'bearer', models: 'qwen-plus,qwen3-max' },
  { id: 'qwen-messages', group: '国产原厂', label: 'Qwen 百炼 · Anthropic Messages', type: 'dashscope', base_url: 'https://dashscope.aliyuncs.com/apps/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'qwen3-coder-plus,qwen3-max' },
  { id: 'glm-chat', group: '国产原厂', label: '智谱 GLM · OpenAI Chat', type: 'zhipu', base_url: 'https://open.bigmodel.cn/api/paas/v4', upstream_protocol: chat, auth_style: 'bearer', models: 'glm-4.6,glm-4.5-air' },
  { id: 'glm-coding', group: '国产原厂', label: '智谱 GLM Coding Plan · OpenAI Chat', type: 'zhipu', base_url: 'https://open.bigmodel.cn/api/coding/paas/v4', upstream_protocol: chat, auth_style: 'bearer', models: 'glm-4.6' },
  { id: 'glm-messages', group: '国产原厂', label: '智谱 GLM · Anthropic Messages', type: 'zhipu', base_url: 'https://open.bigmodel.cn/api/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'glm-4.6,glm-4.5-air' },
  { id: 'ark-chat', group: '国产原厂', label: '豆包 火山方舟 · OpenAI Chat', type: 'volcengine', base_url: 'https://ark.cn-beijing.volces.com/api/v3', upstream_protocol: chat, auth_style: 'bearer', models: 'doubao-seed-1-6-250615' },
  { id: 'ark-responses', group: '国产原厂', label: '豆包 火山方舟 · OpenAI Responses', type: 'volcengine', base_url: 'https://ark.cn-beijing.volces.com/api/v3', upstream_protocol: responses, auth_style: 'bearer', models: 'doubao-seed-1-6-250615' },
  { id: 'minimax-chat', group: '国产原厂', label: 'MiniMax · OpenAI Chat', type: 'minimax', base_url: 'https://api.minimaxi.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'MiniMax-M2' },
  { id: 'minimax-messages', group: '国产原厂', label: 'MiniMax · Anthropic Messages', type: 'minimax', base_url: 'https://api.minimaxi.com/anthropic', upstream_protocol: messages, auth_style: 'x_api_key', models: 'MiniMax-M2' },
  { id: 'qianfan-chat', group: '国产原厂', label: '百度千帆 · OpenAI Chat', type: 'qianfan', base_url: 'https://qianfan.baidubce.com/v2', upstream_protocol: chat, auth_style: 'bearer', models: 'ernie-4.5-turbo-128k,ernie-x1-turbo-32k' },
  { id: 'siliconflow', group: '三方', label: '硅基流动 · OpenAI Chat', type: 'siliconflow', base_url: 'https://api.siliconflow.cn/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek-ai/DeepSeek-V3.2-Exp' },
  { id: 'openrouter', group: '三方', label: 'OpenRouter · OpenAI Chat', type: 'openrouter', base_url: 'https://openrouter.ai/api/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'deepseek/deepseek-chat-v3.1' },
  { id: 'relay-chat', group: '三方', label: 'new-api / one-api 中转 · OpenAI Chat', type: 'relay', base_url: 'https://relay.example.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: '' },
  { id: 'relay-messages', group: '三方', label: 'new-api / one-api 中转 · Anthropic Messages', type: 'relay', base_url: 'https://relay.example.com', upstream_protocol: messages, auth_style: 'x_api_key', models: '' },
  { id: 'openai', group: '海外原厂', label: 'OpenAI · Chat', type: 'openai', base_url: 'https://api.openai.com/v1', upstream_protocol: chat, auth_style: 'bearer', models: 'gpt-4o-mini' },
  { id: 'openai-responses', group: '海外原厂', label: 'OpenAI · Responses', type: 'openai', base_url: 'https://api.openai.com/v1', upstream_protocol: responses, auth_style: 'bearer', models: 'gpt-5' },
  { id: 'anthropic', group: '海外原厂', label: 'Anthropic · Messages', type: 'anthropic', base_url: 'https://api.anthropic.com', upstream_protocol: messages, auth_style: 'x_api_key', models: 'claude-sonnet-4-5' },
  { id: 'gemini', group: '海外原厂', label: 'Google Gemini', type: 'gemini', base_url: 'https://generativelanguage.googleapis.com/v1beta', upstream_protocol: 'gemini', auth_style: 'x_goog_api_key', models: 'gemini-2.5-pro,gemini-2.5-flash' },
]

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
