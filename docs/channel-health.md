# 渠道健康与选路

`danmo-relay` 在静态「分组 + 模型 + 优先级 + 权重」之上，增加进程内渠道健康状态，用于自动避开不稳定或被限流的上游。状态**不写数据库**，**admin 界面不可见**；多 relay 副本之间**不共享**（每个副本独立学习，与 Envoy sidecar 式 outlier detection 类似）。

实现见 [`crates/relay/src/health.rs`](../crates/relay/src/health.rs)，在 [`forward.rs`](../crates/relay/src/forward.rs) 的每次重试前调用 `health.pick`，在上游结果返回后上报。

## 与一次请求的关系

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

## 熔断（连续失败）

| 计入失败 | 不计入 |
|----------|--------|
| 连接错误、超时 | 4xx（上游可达，视为成功并清零连续失败） |
| HTTP 5xx | |
| 非流式读 body 失败 | |
| 流式中途断开（`stream_error`） | |

- 连续失败达到 `CB_FAILURE_THRESHOLD`（默认 5）→ 该 **(渠道 id, 模型名)** 被踢出候选，时长 `CB_BASE_EJECT_SECS`（默认 30s）。
- 踢出期间到期 → **半开**：只允许 **一个** 探测请求（`probe_until` 最长 120s，与上游超时一致）；成功则恢复，失败则再踢出，时长 ×2，上限 `CB_MAX_EJECT_SECS`（默认 600s）。
- 熔断已打开后迟到的失败**不会**把踢出时间再拉长（避免拖死半开）。

## 429 与动态权重

- 每次 429：有效权重系数 ×0.5，不低于 `RL_WEIGHT_FLOOR`（默认 0.05）；在 `RL_RECOVER_SECS`（默认 60s）内**线性恢复**到 1（与是否有流量无关）。
- 响应头带秒数格式的 `Retry-After` 时：额外踢出到该时间点（与权重降权叠加）。
- **仅 429 不会单独触发熔断**；若同时满足连续 5xx/连接失败阈值，仍会走熔断。

`effective_weight ≈ channel.weight × 100 × factor`，同优先级内按该值加权随机。

## 兜底（全候选被踢出）

- **高优先级全部被踢出**：正常切到更低优先级渠道（故障转移），**不**触发兜底。
- **所有优先级都没有可 admit 的候选**：忽略熔断，在**当前** `eligible_channels` 列表（已去掉本请求 `exclude`）里再选一次，避免整体不可用。
- **不做**按「不健康比例」的 panic 阈值：单模型候选通常只有 2～5 个，比例阈值容易让熔断失效。

## 环境变量

| 变量 | 默认 | 含义 |
|------|------|------|
| `CB_ENABLED` | `true` | `false` 时关闭健康逻辑，行为与未接入前一致 |
| `CB_FAILURE_THRESHOLD` | `5` | 连续失败次数阈值 |
| `CB_BASE_EJECT_SECS` | `30` | 首次踢出时长（秒） |
| `CB_MAX_EJECT_SECS` | `600` | 踢出时长上限（秒） |
| `RL_WEIGHT_FLOOR` | `0.05` | 429 降权下限 |
| `RL_RECOVER_SECS` | `60` | 权重系数从当前值恢复到 1 的时长（秒） |

与 `RELAY_MAX_RETRIES` 一起配置：重试次数决定**单请求**最多换几个渠道；熔断决定**后续请求**是否还会先打到坏渠道。

## 运维提示

- 观察 relay 日志：`channel ejected`、`probing ejected channel`、`channel rate limited`、`every channel is ejected, ignoring circuit breakers`。
- 多副本时若需「全局一致」的禁用状态，当前方案** intentionally 未做**（无 Redis/写库）；若某 key 长期 401，渠道不会被熔断，需人工禁用渠道。
- 后续可选：只读 HTTP 暴露内存健康表、401 可选立即熔断、长期踢出后写 `auto_disabled`（见架构文档「边界」）。

与 One API、New API、LiteLLM 等在渠道可用性上的客观对比见 **[comparison.md — 渠道可用性与上游隔离](comparison.md#渠道可用性与上游隔离)**。
