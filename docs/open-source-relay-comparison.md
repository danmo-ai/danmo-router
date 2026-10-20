# 开源 LLM 中转站：渠道可用性对比

本文只对比**渠道选路、失败重试、限流与上游隔离/恢复**，不涉及计费、UI、多协议转换等。依据各项目公开文档与源码整理（截至 2026-10）；产品迭代快，部署前请以你使用的版本为准。

danmo-router 自身机制见 [渠道健康与选路](channel-health.md)、[系统架构](architecture.md) 转发章节。

## 总览

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

## 差异摘要（选型时）

1. **持久禁用 vs 临时踢出**  
   One API / New API 习惯把「命中规则的错误」写成 **`auto_disabled`**，全集群可见，但恢复往往要人工或定时测试。danmo-router 选择 **仅内存踢出 + 半开**，避免 SQLite/多写和误伤后难恢复；代价是副本间状态短暂不一致。

2. **429：禁用、冷却、还是降权**  
   - New API：429 默认可重试且**不**禁用，容易造成同档免费渠道反复被抽到（需 `RetryTimes`、降档或状态码映射等运营手段）。  
   - LiteLLM：429 **立刻 cooldown**，偏「硬隔离、短冷却**。  
   - danmo-router：**软降权**（AIMD 式减半 + 线性恢复），必要时配合 `Retry-After` 硬踢出；不修改管理员配置的静态 `weight`。

3. **重试与优先级**  
   danmo-router 重试在同一请求内 **exclude 已失败渠道**，仍优先高优先级层，适合「同档多活 + 少量重试」。New API 重试若按 **priority 下标降档**，同档多渠道兜底能力弱，需把备用渠道放到**更低 priority**（文档与社区实践常见做法）。

4. **与 danmo 架构的耦合**  
   健康逻辑只在 **`danmo-relay`**，与 admin/user 解耦；适合水平扩 relay。LiteLLM/New API 多为单类 API 进程 + 可选 Redis；One API 经典部署为单 Go 进程。

## 参考链接

- One API 重试与 disable：[`controller/relay.go`](https://github.com/songquanpeng/one-api/blob/master/controller/relay.go)、[`model/ability.go`](https://github.com/songquanpeng/one-api/blob/master/model/ability.go)  
- New API 自动禁用：[`service/channel.go`](https://github.com/QuantumNous/new-api/blob/main/service/channel.go)、运营 FAQ / [分流重试文章](https://tendcode.com/subject/article/newapi-routing-retry-failover/)  
- LiteLLM： [Load balancing / cooldowns](https://docs.litellm.ai/docs/proxy/load_balancing)、[`cooldown_handlers.py`](https://github.com/BerriAI/litellm/blob/main/litellm/router_utils/cooldown_handlers.py)  
- danmo-router：[渠道健康与选路](channel-health.md)、[系统架构](architecture.md)
