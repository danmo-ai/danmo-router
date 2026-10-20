# 系统架构

danmo-router 是一个多协议 LLM 中转站。客户端按路径选择协议；每个模型在每个协议上有一个原厂方言；每条渠道的每个模型有一个上游方言。两端协议和方言相同就透传原字节，否则经中间表示转换。用量按响应字段特征解析后计费。

和其他开源中转站 / 网关（One API、New API、LiteLLM、GPT-Load、Sub2API 等）的定位与能力对照见 [comparison.md](comparison.md)。部署、云平台方案、升级、备份与排障见 **[operations.md](operations.md)**。

## 进程

三个二进制共享同一个 `DATABASE_URL` 和 `ENCRYPTION_KEY`，可以分开部署：

| 二进制 | 默认端口 | 职责 |
|--------|----------|------|
| `danmo-relay` | 3000 | 鉴权、路由、转发、计费 |
| `danmo-admin` | 3001 | 渠道、用户、价格、日志 |
| `danmo-user` | 3002 | 注册（自托管图形验证码）、令牌、自己的日志 |
| `danmo-router` | 3000 | 本地一体进程 |

```text
Client ──► LB ──► danmo-relay × N ──► upstream LLM
Admin UI ──────► danmo-admin
User UI  ──────► danmo-user
                      │
                      └── SQLite WAL（单机）/ MySQL（多实例）
```

代码分五层：`danmo-core`（模型、方言注册表、路由、计费、迁移）、`danmo-relay`（协议编解码与转发）、`danmo-admin-api`、`danmo-user-api`、`danmo-server`（二进制）。前端在 `apps/admin` 和 `apps/user`。

## 协议

入站协议只由路径决定，不看请求内容。

| 路径 | 协议 |
|------|------|
| `POST /v1/chat/completions` | `openai_chat` |
| `POST /v1/responses` | `openai_responses` |
| `POST /v1/messages` | `anthropic_messages` |
| `POST /v1/embeddings` | `embeddings` |
| `POST /v1beta/models/{model}:generateContent` | `gemini` |
| `POST /v1beta/models/{model}:streamGenerateContent` | `gemini`（流式） |
| `GET /v1/models` | 令牌可见的模型列表 |

渠道的 `upstream_protocol` 是上游实际提供的协议。同一厂商的不同协议使用不同 base URL，因此是不同渠道。`relay_mode` 决定渠道能否服务其他协议：`passthrough` 只服务同协议，`convert` 还可以在四种对话协议之间转换。Embeddings 只能透传。同协议内的方言转换不受 `relay_mode` 限制。

## 方言

方言是一份厂商规范的完整实现，复用所属协议的基础编解码，只覆盖本规范自己的字段。注册表在 `danmo-core::dialect`，编解码在 `danmo-relay::dialect`。

| 方言 | 规范 |
|------|------|
| `openai_chat/standard`、`openai_chat/opaque` | OpenAI Chat |
| `openai_chat/thinking` | DeepSeek、方舟、智谱：`thinking.type` + `reasoning_effort` |
| `openai_chat/enable_thinking` | 百炼：`enable_thinking` + `thinking_budget` |
| `openai_chat/kimi` | Kimi：`thinking.type` + `reasoning_effort` |
| `openai_chat/qianfan` | 千帆上的 DeepSeek、GLM、Kimi：`thinking.type` + `thinking_budget` |
| `openai_chat/qianfan_ernie` | 千帆上的文心、Qwen3：`enable_thinking` + `thinking_strategy` |
| `openai_chat/hunyuan` | 混元：`enable_enhancement`、`search_info`、`citation`，无思考开关 |
| `openai_chat/minimax` | MiniMax：`thinking` 为 `adaptive` 或 `disabled`，`reasoning_split`，`base_resp` 软错误 |
| `openai_responses/standard`、`openai_responses/opaque` | OpenAI Responses |
| `openai_responses/thinking` | 百炼、方舟：`thinking.type` + `reasoning.effort`；关闭时去掉 `reasoning` |
| `anthropic_messages/standard`、`anthropic_messages/opaque` | Anthropic Messages |
| `anthropic_messages/effort` | MiniMax Anthropic：`output_config.effort`，无预算时用 adaptive thinking |
| `gemini/standard` | Gemini `thinkingConfig` |
| `embeddings/standard` | OpenAI Embeddings |

`opaque` 表示规范未知（聚合渠道）。它只接受同协议请求并透传；需要转换时路由跳过该行。

入站方言不从请求猜测。模型 M 走协议 P 时，入站方言是 `model_prices.native_dialects[P]`，未配置为 `standard`。上游方言是该渠道下这个模型那一行的 `upstream_dialect`。

## 转发

```text
请求路径 ──► 协议 P
模型价格 ──► 入站方言（未配置则 standard）
路由     ──► 渠道 + channel_models 行（上游模型名、上游方言）
                 │
                 ├─ 协议相同且（方言相同 或 行是 opaque）──► 透传原字节
                 └─ 其他 ──► 入站方言解码到 IR ──► 上游方言编码
响应 ──► 按字段特征解析 usage ──► 结算（预扣已在转发前完成）
       └─ 透传：原厂方言已配置且渲染格式不同时，重写 usage
       └─ 转换：上游方言解码响应，入站方言编码
```

透传仍允许两处与方言无关的改写：把 `model` 换成行上的上游模型名；Chat 流在客户端未要求时补 `stream_options.include_usage`，并吞掉因此多出来的用量块。

选路按分组、模型、优先级和权重。候选必须 `serves(入站协议)`，且 `opaque` 行在协议不同时被排除。每次重试前在进程内应用渠道健康（熔断、429 降权）；上游返回 429 或 5xx 时换下一个渠道，次数由 `RELAY_MAX_RETRIES` 限制。候选或次数用尽时，客户端收到最后一次上游的状态码（如 429、503、529）；最后一次失败是连接错误或本地转换失败时为 502。客户端断开后继续读完上游，以便拿到 usage。

渠道健康（配置、状态机、环境变量）见下文 **[渠道健康与选路](#渠道健康与选路)**；与同类项目的选型对比见 **[comparison.md](comparison.md#渠道可用性与上游隔离)**。

## 渠道健康与选路

`danmo-relay` 在静态「分组 + 模型 + 优先级 + 权重」之上，增加进程内渠道健康状态，用于自动避开不稳定或被限流的上游。状态**不写数据库**，**admin 界面不可见**；多 relay 副本之间**不共享**（每个副本独立学习，与 Envoy sidecar 式 outlier detection 类似）。

实现见 [`crates/relay/src/health.rs`](../crates/relay/src/health.rs)，在 [`forward.rs`](../crates/relay/src/forward.rs) 的每次重试前调用 `health.pick`，在上游结果返回后上报。

### 与一次请求的关系

```text
eligible_channels（DB：enabled + 模型 + 协议 + exclude）
        │
        ▼
health.pick（去掉已熔断候选、429 降权、半开探测）
        │
        ▼
weighted_shuffle_by_priority（同优先级内按 effective_weight 随机）
        │
        ▼
上游调用 ──► 成功 / 429 / 5xx / 连接错误 / 4xx / 流错误
        │
        └──► record_success / record_failure / record_rate_limited
```

- 单次请求最多尝试 `RELAY_MAX_RETRIES` 次（默认 2）。每次失败会把该渠道 id 加入 `exclude`，下一次从剩余候选里再 `pick`。
- 选路与熔断只作用于**当前 relay 进程**；扩容后各副本的「黑名单」可能略有差异，通常几次失败内会收敛。

### 熔断（连续失败）

| 计入失败 | 不计入 |
|----------|--------|
| 连接错误、超时 | 4xx（上游可达，视为成功并清零连续失败） |
| HTTP 5xx | |
| 非流式读 body 失败 | |
| 流式中途断开（`stream_error`） | |

- 连续失败达到 `CB_FAILURE_THRESHOLD`（默认 5）→ 该 **(渠道 id, 模型名)** 被踢出候选，时长 `CB_BASE_EJECT_SECS`（默认 30s）。
- 踢出期间到期 → **半开**：只允许 **一个** 探测请求（`probe_until` 最长 120s，与上游超时一致）；成功则恢复，失败则再踢出，时长 ×2，上限 `CB_MAX_EJECT_SECS`（默认 600s）。
- 熔断已打开后迟到的失败**不会**把踢出时间再拉长（避免拖死半开）。

### 429 与动态权重

- 每次 429：有效权重系数 ×0.5，不低于 `RL_WEIGHT_FLOOR`（默认 0.05）；在 `RL_RECOVER_SECS`（默认 60s）内**线性恢复**到 1（与是否有流量无关）。
- 响应头带秒数格式的 `Retry-After` 时：额外踢出到该时间点（与权重降权叠加）。
- **仅 429 不会单独触发熔断**；若同时满足连续 5xx/连接失败阈值，仍会走熔断。

`effective_weight ≈ channel.weight × 100 × factor`，同优先级内按该值加权随机。

### 兜底（全候选被踢出）

- **高优先级全部被踢出**：正常切到更低优先级渠道（故障转移），**不**触发兜底。
- **所有优先级都没有可 admit 的候选**：忽略熔断，在**当前** `eligible_channels` 列表（已去掉本请求 `exclude`）里再选一次，避免整体不可用。
- **不做**按「不健康比例」的 panic 阈值：单模型候选通常只有 2～5 个，比例阈值容易让熔断失效。

### 环境变量

| 变量 | 默认 | 含义 |
|------|------|------|
| `CB_ENABLED` | `true` | `false` 时关闭健康逻辑，行为与未接入前一致 |
| `CB_FAILURE_THRESHOLD` | `5` | 连续失败次数阈值 |
| `CB_BASE_EJECT_SECS` | `30` | 首次踢出时长（秒） |
| `CB_MAX_EJECT_SECS` | `600` | 踢出时长上限（秒） |
| `RL_WEIGHT_FLOOR` | `0.05` | 429 降权下限 |
| `RL_RECOVER_SECS` | `60` | 权重系数从当前值恢复到 1 的时长（秒） |

与 `RELAY_MAX_RETRIES` 一起配置：重试次数决定**单请求**最多换几个渠道；熔断决定**后续请求**是否还会先打到坏渠道。

**运维边界：** 多副本间无全局一致的禁用状态（无 Redis/写库）；上游 Key 长期 **401 不会触发熔断**，需人工禁用渠道。后续可选：只读 HTTP 暴露内存健康表、401 可选立即熔断、长期踢出后写 `auto_disabled`（见下文「边界」）。relay 日志关键字见 [operations.md — 健康检查与观测](operations.md#健康检查与观测)。

与 One API、New API、LiteLLM 等在渠道可用性上的客观对比见 **[comparison.md — 渠道可用性与上游隔离](comparison.md#渠道可用性与上游隔离)**。

## 中间表示

对话协议共用一份 IR：系统提示、消息（文本、图片、推理、工具调用、工具结果）、采样参数、推理开关。推理有三种状态：打开、关闭、交给上游默认。打开时用 `effort` 或 `budget_tokens` 表示程度，方言按自己的字段写回去。

请求、消息、响应各带 `extra`。同协议转换时，基础编解码和源方言都不认识的字段原样写回；跨协议时这些字段对不上，丢弃并记 debug 日志。历史里的空 `reasoning_content` 会保留，思考模式的上游要求把它回传。

## 数据

```text
channels
  base_url, upstream_protocol, relay_mode, auth_style, inject_stream_usage
  default_dialect   新增模型行时的默认值，不参与转发
  vendor            原厂标识，管理端用来建议原厂方言
        │
        └── channel_models
              model, upstream_model, upstream_dialect
              UNIQUE(channel_id, model)
              upstream_model 为空表示与 model 相同
                    │
                    └── abilities（group × model，由模型行重建）

model_prices
  model_ratio, completion_ratio, cache_read_ratio, cache_write_ratio, search_price
  tiers, periods
  native_dialects   JSON，协议 → 方言
```

渠道密钥以 `ENCRYPTION_KEY` 加密后存放。迁移按**数据库引擎**分目录（`crates/core/migrations/sqlite/`、`mysql/`，同号同语义），记在 `schema_migrations`，每个版本只执行一次。`003_channel_models.sql` 把旧的逗号分隔模型列表和 `model_mapping` 拆成行，并把 `native_usage` 改成 `native_dialects`。规则见 [AGENTS.md](../AGENTS.md)。

数据访问不用 ORM：业务代码在 `sqlx::Any` 连接池上写两种引擎通用的 SQL，方言差异集中在 `SqlEngine` trait（`crates/core/src/db/`）。trait 提供默认实现，`Sqlite` / `Mysql` 只覆盖不同的部分：取自增 id（`RETURNING id` vs `last_insert_id`）、`INSERT OR IGNORE` vs `INSERT IGNORE`、upsert 语法、写事务开头（`BEGIN IMMEDIATE`）与行锁（`FOR UPDATE`）、迁移互斥（MySQL `GET_LOCK`）。时间统一由 Rust 生成 UTC 文本绑定，不依赖库函数。

## 用量与计费

Relay 对每个成功转发的请求做 **预扣 → 解析 usage → 结算（多退少补）**。没有「缺 usage 就按预扣当最终账单」的兜底。

### 流程

```text
鉴权、解析 body
    │
    ▼
预扣 pre_consume ──► 用户 used_quota ↑、令牌 remain_quota ↓（无限额令牌只动用户侧）
    │                  金额 = estimate_pre_consume（见下），不足则 402/额度错误，不上游
    ▼
选路、转发上游
    │
    ▼
从上游原始响应提取 usage ──► match_usage（按字段特征选适配器）
    │
    ├─ 成功 ──► settle：按公式算 actual_quota，库内再改 delta = actual − pre_consumed
    │           调用日志 status = success，记录 quota 与 raw_usage
    │
    └─ 失败 ──► refund：退回整笔 pre_consumed
                调用日志 status = billing_failed
                非流式：502，不向客户端交付无法计费的 JSON 体
                流式：客户端可能已收到部分 chunk；后台仍退预扣（relay 会在客户端断开后继续读上游以尽量拿到 usage）
```

**预扣估算**（`crates/core/src/billing.rs`）：用请求体里第一个存在的上限字段——`max_tokens`、`max_completion_tokens`、`max_output_tokens`、`generationConfig.maxOutputTokens`（与入站协议无关，统一从 JSON 指针读取）。**都没传**时按 **1024** 计，并 clamp 在 1～128000。估算假设 prompt ≈ 上限的一半、completion = 上限，再乘该模型的 `model_ratio` 与分组 `group_ratio`，向上取整，至少 1。**不是**按模型 catalog 的最大 context 预扣，也**不等于**最终按 token 计价的金额。

**结算**只认上游返回、且能被适配器解析的用量；结算时再用 `model_prices` 上的倍率、阶梯与时段（见下）。预扣只是占位，最终以 `actual_quota` 为准。

### 用量解析与失败

用量按响应里实际出现的字段匹配适配器，匹配不到则计费失败（见上流程）。上游以 200 返回方言内的错误体（如 MiniMax `base_resp`）时同样记 `billing_failed`，但客户端收到的是该上游错误信息（`upstream_error`）。适配器有 `anthropic_messages`、`openai_responses`、`gemini`、`deepseek_chat`、`qianfan_chat`、`kimi_chat`、`embeddings`、`openai_chat`。

方言决定返回给客户端的形状：原厂方言已配置时，usage 渲染成该方言的字段集；未配置时透传路径保持上游原样。MiniMax 的 `base_resp` 非零状态码是软错误，只在 `minimax` 和 `opaque` 方言上检查。Chat 流式且渠道开启 `inject_stream_usage` 时，客户端未带 `stream_options.include_usage` 也会代注入，以便上游返回可计费用量。

### 额度公式

```text
(未命中缓存的输入
  + 缓存读取 × cache_read_ratio
  + 缓存写入 × cache_write_ratio
  + 输出 × completion_ratio) × model_ratio × 时段倍率
+ 搜索次数 × search_price
```

再乘分组倍率，向上取整。推理 token 已包含在输出里。`tiers` 按输入长度换一组倍率，`periods` 是 UTC 时间窗口的系数。

## 用户注册验证码

自托管图形验证码，**不写数据库**：答案与过期时间用 `ENCRYPTION_KEY`（AES-GCM）封进一次性 token；`GET /api/user/captcha` 下发 SVG，`POST /api/user/register` 校验答案。本进程内存记录已用 nonce 防重放；多副本 `danmo-user` 在 TTL 内可能各用一次同 token，生产用户面通常单实例即可。

## 管理端

渠道表单的模型区是一张表：模型名、上游模型名、方言。预设带 `default_dialect` 和前缀规则，添加模型时按「最长前缀命中，否则默认方言」填好，可以逐行改。千帆的 `ernie-`、`qwen` 填 `qianfan_ernie`；OpenRouter、硅基流动等聚合渠道填 `opaque`。

价格页按协议选择原厂方言。建议先取 `vendor` 与模型家族一致的渠道上的行方言，没有再查内置前缀表（`deepseek-`、`qwen`、`kimi-`、`glm-`、`doubao-`、`MiniMax-`、`hunyuan-`、`ernie-`）。建议带来源，管理员确认后保存。`/api/admin/meta` 返回协议、转发模式、鉴权方式和方言列表。

## 入口限额

限额在单个 `danmo-relay` 进程的内存里，用来挡住死循环、重试风暴和伪造 key。没有跨实例共享计数，也没有按账号哈希分桶。一个账号在每个进程上各有一份额度。

新令牌是 `sk-v1.{base64url(user_id 大端 8 字节与 16 字节随机数)}.{base64url(hmac_16)}`。MAC 密钥是 `HMAC-SHA256(ENCRYPTION_KEY, "danmo-api-key-v1")`，签的是那 24 字节载荷，截断到 16 字节。校验在读 body 和查库之前完成，比较 MAC 用常数时间。

准入只看 `Authorization`、`x-api-key`、`x-goog-api-key` 和 Gemini 的 `?key=`：

- HMAC 通过：占用该账号和本进程已验证流量的并发与 QPS。超限返回 429，不读 body，不查库。
- 旧的 `sk-` 加 48 位十六进制：先占用本进程一小块未验证额度。查库确认账号后释放它，再占用该账号和本进程的已验证额度；账号已满则 429。
- 没有 key、格式不对或 HMAC 失败：401，不占用任何额度。

在途名额从准入一直持有到响应结束。流式响应把名额放在返回给客户端的 body 上，客户端断开或流结束就释放，上游那边可以继续读完以便计费。换渠道重试不另占名额。QPS 是令牌桶，容量等于每秒限额；并发没拿到时不扣 QPS。

`group_name` 仍只负责选路和倍率。禁用、过期、模型白名单和额度仍在限额之后查库。环境变量都是本进程上限：`RELAY_USER_CONCURRENCY`、`RELAY_INSTANCE_CONCURRENCY`、`RELAY_USER_QPS`、`RELAY_INSTANCE_QPS`、`RELAY_LEGACY_CONCURRENCY`、`RELAY_LEGACY_QPS`。

## 边界

- 入站方言来自模型配置。
- 上游方言配在渠道下的每个模型上。
- 模型名后缀不表达推理参数。
- OpenRouter 兼容的入站面不在本版。
- 无内置支付网关、Realtime / 绘图任务面；进程级并发与 QPS 不跨实例共享（见入口限额）。选型时与同类项目的差距见 [comparison.md](comparison.md)。
