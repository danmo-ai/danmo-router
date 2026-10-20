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

## 与常见开源中转站对比

以下基于各项目**公开文档与源码**（截至 2026-10），仅对比**渠道/上游可用性**相关能力，不涉及计费、UI、协议转换等。产品迭代快，部署前请以你使用的版本为准。

| 维度 | danmo-router | [One API](https://github.com/songquanpeng/one-api) | [New API](https://github.com/QuantumNous/new-api)（new-api 系） | [LiteLLM Proxy](https://docs.litellm.ai/docs/proxy/load_balancing) |
|------|--------------|---------------------------------------------------|----------------------------------------------------------------|----------------------------------------------------------------------|
| **定位** | 多协议 + 方言透传/转换的 Rust relay | Go 单进程，OpenAI 兼容为主 | One API 分支，功能更全的运营面 | Python 网关 + Router，deployment 级负载均衡 |
| **选路** | 优先级分层 + 同层权重随机 | 首次取最高优先级随机；重试时策略随版本/缓存变化（曾有无 Redis 时只在最高优先级重试的问题，见 [issue #1197](https://github.com/songquanpeng/one-api/issues/1197)） | 首次仅 **最高 priority 档**；同档内权重随机；`RetryTimes=0` 时不换渠道 | `model_list` 多 deployment；支持 `order` 逐级、`weight`、RPM/并发等 |
| **单请求失败切换** | 失败渠道进 `exclude`，同优先级可换其他渠道（受 `RELAY_MAX_RETRIES` 限制） | 429/5xx 可重试；重试选路见上 | 需 `RetryTimes>0`；重试常按 **下一 priority 档** 选路，同档兄弟渠道在一次请求内可能用不上（社区文章与 FAQ 均有说明） | `num_retries` + deployment 切换；另有 model 级 `fallbacks` |
| **坏渠道隔离** | 内存熔断 + 半开探测；**不**改 DB `status` | `monitor.ShouldDisableChannel` → 常 **持久 auto-disable**（如 401）；文档/Pro 描述 cooldown、RPM 等 | **自动禁用**写库（状态码/关键词/`ChannelError` 可配）；可选连续失败阈值（社区 PR 方向） | **Cooldown**：429 可立即 cooldown；也可按分钟失败率；时长 `cooldown_time` |
| **429 处理** | 权重减半 + 时间恢复；`Retry-After` 踢出 | 429 参与重试；是否 disable 看 monitor 规则 | 默认 **不**因 429 自动禁用；429 在默认可重试列表；常需状态码映射或运营技巧 | 429 **立即** cooldown（默认约 5s，可配） |
| **恢复** | 半开 + 成功即恢复；全熔断兜底 | 手动启用 / 自动启用策略 / 测试任务 | 多依赖 **手动或定时渠道测试**；[issue #5420](https://github.com/QuantumNous/new-api/issues/5420) 提议半开，主线实现以版本为准 | cooldown 到期后再参与选路 |
| **状态存储** | 每 relay 进程内存 | DB + 进程内 monitor | DB + 进程内计数（如连续失败） | 常配合 **Redis** 存 deployment 健康（多实例） |
| **粒度** | **(渠道, 模型)** | 渠道（能力表按 model 关联） | 渠道 + multi-key 时按 key | **deployment**（同一 `model_name` 多条配置） |

### 差异摘要（选型时）

1. **持久禁用 vs 临时踢出**  
   One API / New API 习惯把「命中规则的错误」写成 **`auto_disabled`**，全集群可见，但恢复往往要人工或定时测试。danmo-router 选择 **仅内存踢出 + 半开**，避免 SQLite/多写和误伤后难恢复；代价是副本间状态短暂不一致。

2. **429：禁用、冷却、还是降权**  
   - New API：429 默认可重试且**不**禁用，容易造成同档免费渠道反复被抽到（需 `RetryTimes`、降档或状态码映射等运营手段）。  
   - LiteLLM：429 **立刻 cooldown**，偏「硬隔离、短冷却」。  
   - danmo-router：**软降权**（AIMD 式减半 + 线性恢复），必要时配合 `Retry-After` 硬踢出；不修改管理员配置的静态 `weight`。

3. **重试与优先级**  
   danmo-router 重试在同一请求内 **exclude 已失败渠道**，仍优先高优先级层，适合「同档多活 + 少量重试」。New API 重试若按 **priority 下标降档**，同档多渠道兜底能力弱，需把备用渠道放到**更低 priority**（文档与社区实践常见做法）。

4. **与 danmo 架构的耦合**  
   健康逻辑只在 **`danmo-relay`**，与 admin/user 解耦；适合水平扩 relay。LiteLLM/New API 多为单类 API 进程 + 可选 Redis；One API 经典部署为单 Go 进程。

### 参考链接

- One API 重试与 disable：[`controller/relay.go`](https://github.com/songquanpeng/one-api/blob/master/controller/relay.go)、[`model/ability.go`](https://github.com/songquanpeng/one-api/blob/master/model/ability.go)  
- New API 自动禁用：[`service/channel.go`](https://github.com/QuantumNous/new-api/blob/main/service/channel.go)、运营 FAQ / [分流重试文章](https://tendcode.com/subject/article/newapi-routing-retry-failover/)  
- LiteLLM： [Load balancing / cooldowns](https://docs.litellm.ai/docs/proxy/load_balancing)、[`cooldown_handlers.py`](https://github.com/BerriAI/litellm/blob/main/litellm/router_utils/cooldown_handlers.py)  
- danmo-router：[`docs/architecture.md`](architecture.md) 转发与数据模型章节
