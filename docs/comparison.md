# 开源 LLM 中转站 / 网关对比

核验日期：2026-10-20。数字（Star、最近发布）会变，选型前请回到各仓库 Releases / LICENSE 再核一次。本文不是排名，而是按**定位、许可证、协议面、计费与分发、运维形态**做客观对照，帮助判断「该不该用」以及「和 danmo-router 差在哪里」。

## 先分清产品类别

名字都叫「中转 / 网关」，实际至少有四类：

| 类别 | 典型代表 | 核心问题 |
|------|----------|----------|
| **分发面板** | One API、New API、VoAPI | 多用户、令牌、渠道、额度、充值；偏「站」 |
| **团队网关** | LiteLLM Proxy | 多厂商统一接入、虚拟 Key、预算、护栏、可观测；偏「平台工程」 |
| **凭据池 / 调度网关** | GPT-Load | 多 Key / 订阅账号进一个池，调度、重试、冷却；偏「资源聚合」 |
| **订阅额度分发** | Sub2API | 把订阅账号额度变成 API Key；合规与上游 ToS 风险最高 |

danmo-router 落在「分发面板 ∩ 多协议转发」：有管理/用户 Portal、额度与兑换码，但协议层强调**原厂方言一致**（客户端看到的字段形状按模型配置的原厂方言，渠道按模型配上游方言），而不是「一切都收成 OpenAI Chat」。

```text
Client SDK / Agent
        │
        ▼
┌───────────────────┐
│  鉴权 · 限流 · 路由 │
│  协议/方言编解码   │
│  用量解析 · 计费   │
└─────────┬─────────┘
          ▼
   Upstream LLM / 聚合商
```

## 一眼对照

Star 约为核验日 GitHub 显示值，仅作生态热度参考，**不等于质量或安全**。

| 项目 | 仓库 | 许可证 | 技术栈 | 大致定位 | Star（约） | 维护态势（核验日） |
|------|------|--------|--------|----------|------------|-------------------|
| **One API** | [songquanpeng/one-api](https://github.com/songquanpeng/one-api) | MIT（README 另要求保留署名与项目链接） | Go + React | 元祖级分发面板；统一成 OpenAI 兼容面 | ~37k | 正式 Release 停在 v0.6.10（2025-02）；仓库未归档，新能力跟进慢 |
| **New API** | [QuantumNous/new-api](https://github.com/QuantumNous/new-api) | **AGPL-3.0** + §7 附加条款（署名与可见原项目链接）；可谈商业豁免 | Go + React | One API 的活跃继任者；多协议转换 + 完整计费分发 | ~50k | 高频发布；生态与中文社区最活跃的「站」类 |
| **LiteLLM** | [BerriAI/litellm](https://github.com/BerriAI/litellm) | 核心 **MIT**；`enterprise/` 等另有商业条款 | Python（网关；另有 SDK） | 国际主流 AI Gateway：100+ 厂商、YAML/代码驱动、护栏与观测 | ~62k | 周更级；企业功能部分收费 |
| **GPT-Load** | [tbphp/gpt-load](https://github.com/tbphp/gpt-load) | MIT | Go 单二进制 + 内嵌管理界面 | 个人/小团队凭据池：Key + 订阅账号统一调度 | ~7k | 2.x 活跃；刻意不做「完整分发站」 |
| **Sub2API** | [Wei-Shaw/sub2api](https://github.com/Wei-Shaw/sub2api) | LGPL-3.0+ | Go + Vue；**需 PostgreSQL + Redis** | 订阅额度 → API Key；内置计费与支付 | 中等 | 活跃；README **明确警告可能违反上游 ToS，未授权商业运营** |
| **VoAPI** | [VoAPI/VoAPI](https://github.com/VoAPI/VoAPI) | 社区版开源 + **Pro 商业功能**（许可证显示为 Other） | Go | 高颜值分发站；支付/工单/实名等多在 Pro | ~1k | 双轨产品；评估时分清社区版边界 |
| **danmo-router** | 本仓库 | **MIT** | Rust（relay/admin/user 拆分）+ 双 Portal | 多协议 + **按模型方言**；额度/令牌/兑换码；进程级并发与 QPS | — | 早期项目；能力面刻意收窄，协议层更细 |

## 许可证：往往比功能更先决定能不能用

| 项目 | 对「改了代码对外提供网络服务」 | 对闭源二次分发 / 去品牌 |
|------|-------------------------------|-------------------------|
| One API | MIT：义务轻；仍需遵守 README 署名约定 | 署名约定不满足时需自行获授权 |
| New API | **AGPL-13**：需向网络用户提供对应源码 | 去品牌 / 规避 AGPL 需商业许可（邮件洽谈，公开价未统一挂出） |
| LiteLLM | 核心 MIT；商业模块另议 | SSO/RBAC/审计等常在企业许可后 |
| GPT-Load | MIT | 宽松 |
| Sub2API | LGPL（动态链接等场景相对宽松，仍建议法务审） | **作者未授权商业运营**；合规风险不在许可证 alone |
| VoAPI | 社区版可用能力有限；支付等在 Pro | 商业功能买断/订阅，非纯开源 |
| danmo-router | MIT | 无 AGPL 网络传染条款 |

**实务建议：** 若业务模型是「改网关源码 + 对外卖额度且不愿开源改动」，优先排除 AGPL（New API）或先谈妥商业许可；内部团队自用、改动不对外提供服务时，AGPL 压力小得多，但仍建议法务过一遍。

## 协议与转换能力

「能不能转」和「转完客户端看到的是否像原厂」是两件事。

| 能力 | One API | New API | LiteLLM | GPT-Load | Sub2API | danmo-router |
|------|---------|---------|---------|----------|---------|--------------|
| OpenAI Chat Completions | ✅ 主路径 | ✅ | ✅ 主路径 | ✅ | ✅（上游相关） | ✅ |
| OpenAI Responses | 弱 / 滞后 | ✅ | ✅（网关能力持续扩） | ✅ | 视上游 | ✅ |
| Anthropic Messages 原生面 | 弱 | ✅ 双向转换重点 | ✅（及 OpenAI 统一面） | ✅ | 常见场景 | ✅ |
| Gemini 原生面 | 弱 | ✅ | ✅ | ✅ | 视上游 | ✅ |
| Embeddings | ✅ | ✅ | ✅ | 视渠道 | 视上游 | ✅ |
| Realtime / Rerank / 绘图任务等 | 少 | ✅ 面较全 | 部分 / 插件生态 | 图片视频渠道有覆盖 | 非重点 | ❌ 本版不做 |
| 厂商「方言」字段（思考开关、缓存 usage 形状等） | 多为兼容层特判 | 渠道适配丰富，粒度偏渠道类型 | Provider 映射 + 参数归一 | 协议级转换 + 参数覆盖 | 偏账号/协议转发 | **一等公民**：`native_dialects` + 每渠道每模型 `upstream_dialect` |
| 同协议同方言 | 透传/重写混用 | 适配器路径 | 归一化为主 | 原生优先或转换 | 依赖账号类型 | **原字节透传**（仅允许换 model 名等少数改写） |
| 跨协议 | 弱（收束 OpenAI） | 强（OpenAI ↔ Claude ↔ Gemini） | 强（收束 OpenAI 格式为主） | 有 | 有限 | 四种对话协议可转；Embeddings 仅透传 |

danmo-router 的差异化不在「渠道数量」，而在：

1. **入站方言不猜请求**：由模型价格上的 `native_dialects[协议]` 决定，未配则 `standard`。
2. **上游方言配在 `channel_models` 每一行**，可服务 DeepSeek / 百炼 / 千帆 / Kimi / 混元 / MiniMax 等不同思考与 usage 字段集。
3. **`opaque`** 专给聚合渠道：只透传同协议，需要转换时路由跳过该行。
4. 用量按**响应字段特征**匹配适配器；匹配失败则退预扣并记 `billing_failed`，避免静默错账。

细节见 [architecture.md](architecture.md)。

## 计费、用户与「能不能当站」

| 能力 | One API | New API | LiteLLM | GPT-Load | Sub2API | danmo-router |
|------|---------|---------|---------|----------|---------|--------------|
| 多用户 / 分组 | ✅ | ✅ 更完善 | 虚拟 Key / 团队预算 | AccessKey + 分组倍率 | ✅ | ✅ |
| 额度 / 预扣 / 结算 | ✅ | ✅（含缓存倍率、订阅计划等） | 花费追踪与预算 | 用量与成本估算 | Token 级计费 | ✅ 预扣 + usage 结算；缓存读/写倍率、阶梯、时段 |
| 在线充值（易支付 / Stripe 等） | 有限 / 插件生态 | ✅ 成熟 | 非「卖额度」主路径 | 非主路径 | ✅ 内置支付 | ❌ 本版无支付网关；有兑换码 |
| 兑换码 | ✅ | ✅ | — | — | 视版本 | ✅ |
| 管理后台 | ✅ | ✅ 多语言、现代 UI | 有 UI，配置偏 YAML/API | 轻量内嵌 UI | ✅ | Admin Portal + User Portal |
| 数据面 / 控制面拆分 | 通常一体 | 一体为主，可集群反代 | Proxy 可水平扩 | 单二进制为主 | 依赖 PG/Redis | **relay / admin / user 三二进制**，共享 DB |

**选站类（对外或多租户卖额度）：** New API 功能最全；One API 适合存量；VoAPI 看社区版是否够用；danmo-router 有额度与 Portal，但**没有内置支付与营销体系**，若要做完整商业站需自建支付或选 New API 一类。

**选内部网关：** LiteLLM 或 GPT-Load 往往更贴切；不必背一整套充值与邀请返佣。

## 部署与运维

| 维度 | One API | New API | LiteLLM | GPT-Load | Sub2API | danmo-router |
|------|---------|---------|---------|----------|---------|--------------|
| 最小依赖 | SQLite 单容器 | SQLite 单容器；生产常用 PG + Redis | Python 运行时；虚拟 Key 需 DB | 单 Go 二进制；SQLite/MySQL/PG | **强制 PG 15+ 与 Redis 7+** | Rust 二进制；SQLite WAL（文档写明后续 Postgres）；三进程共 `DATABASE_URL` |
| 镜像体积 / 冷启动 | 小 | 中 | 偏大（Python 依赖） | 很小 | 中（含依赖栈） | 原生二进制，无 Node 运行时在服务端路径 |
| K8s / Helm / IaC | 社区方案 | Compose / 面板一键为主 | **官方 Helm、Terraform 故事完整** | 二进制/Docker | Compose 脚本 | 官方 Compose + GHCR 镜像 / 云 Serverless 容器（无官方 Helm） |
| 水平扩展 | 注意 SQLite 与会话 | 集群模式 + 外部 LB；状态在 DB/Redis | Proxy 可多副本 | 视部署 | 有状态依赖 | **优先扩 `danmo-relay`**；限额为**进程内存**，多副本不共享计数 |
| 可观测 | 日志 / 仪表盘 | 仪表盘较全 | OTel、回调、护栏生态强 | 健康、路由诊断、用量 | 管理端监控 | 请求日志 + tracing 基础；无 New API/LiteLLM 级仪表盘深度 |

## 渠道可用性与上游隔离

本节只谈**选路、单请求重试、429/5xx 后的隔离与恢复**（核验依据各项目公开文档与源码，2026-10）。GPT-Load / Sub2API 也有 Key 轮换与冷却，但产品重心不在「多租户渠道表」，此处不展开；详见各自仓库。

| 维度 | danmo-router | One API | New API | LiteLLM Proxy |
|------|--------------|---------|---------|---------------|
| **选路** | 优先级分层 + 同层权重随机 | 首次最高优先级随机；重试策略随版本/缓存（曾出现无 Redis 时只在最高优先级重试，见 [issue #1197](https://github.com/songquanpeng/one-api/issues/1197)） | 首次仅最高 priority 档；同档权重随机；`RetryTimes=0` 不换渠道 | 多 deployment；`order` 逐级、`weight`、RPM/并发等 |
| **单请求失败切换** | 失败渠道 `exclude`，同优先级可换兄弟渠道（`RELAY_MAX_RETRIES`） | 429/5xx 可重试；重试选路见上 | 需 `RetryTimes>0`；重试常按**下一 priority 档**，同档兄弟在一次请求内可能用不上 | `num_retries` + deployment 切换；model 级 `fallbacks` |
| **坏渠道隔离** | 内存熔断 + 半开；**不改** DB `status` | 常 **持久 auto-disable**（如 401）；Pro 文档有 cooldown 等 | **自动禁用**写库（状态码/关键词可配）；连续失败阈值（社区方向） | **Cooldown**；429 可立即 cooldown；也可按分钟失败率 |
| **429** | 权重减半 + 时间恢复；秒数 `Retry-After` 可踢出 | 参与重试；是否 disable 看 monitor | 默认**不**因 429 禁用；常需状态码映射等运营手段 | 429 **立即** cooldown（默认约 5s，可配） |
| **恢复** | 半开探测 + 成功即恢复；全熔断兜底 | 手动/自动启用、测试任务 | 多依赖手动或定时测渠道；[半开机制仍在讨论](https://github.com/QuantumNous/new-api/issues/5420) | cooldown 到期后再参与选路 |
| **健康状态存储** | 每 **relay 进程**内存 | DB + monitor | DB + 进程内计数 | 常配 **Redis**（多实例） |
| **粒度** | **(渠道, 模型)** | 渠道 | 渠道；multi-key 时按 key | **deployment** |

**选型时几条硬差异：**

1. **持久禁用 vs 临时踢出**：One API / New API 命中规则后常写 `auto_disabled`，全实例可见，恢复靠人工或定时测试。danmo-router 用**内存踢出 + 半开**（见 [architecture.md — 渠道健康与选路](architecture.md#渠道健康与选路)），避免多 relay 写库，但副本间状态短暂不一致。
2. **429**：New API 默认可重试且不易禁用，同档免费渠道易被反复抽到；LiteLLM 硬 cooldown；danmo-router **软降权**（AIMD 减半 + 线性恢复），必要时 `Retry-After` 硬踢出。
3. **重试与优先级**：danmo 同请求内 exclude 已失败渠道，仍优先高优先级层；New API 重试若按 priority 下标降档，同档兜底弱，备用常放在更低 priority。

源码入口：One API [`controller/relay.go`](https://github.com/songquanpeng/one-api/blob/master/controller/relay.go)；New API [`service/channel.go`](https://github.com/QuantumNous/new-api/blob/main/service/channel.go)；LiteLLM [load balancing / cooldowns](https://docs.litellm.ai/docs/proxy/load_balancing)。

## 安全与合规（选型硬门槛）

客观事实，不是恐吓：

- **自托管 = 你自己负责补丁与暴露面。** 热门项目（含 New API、LiteLLM）历史上都出现过严重 CVE；上线须锁版本、隔离管理面、限制出站、及时升级。
- **Sub2API** 及一切「订阅账号转 API」方案：可能直接违反 Anthropic 等上游服务条款。作者已声明仅供学习研究、未授权商业运营。本文**不推荐**将其用于对外商业分发。
- **转售不明来源「便宜 Key」**：与开源面板无关，但行业实测存在模型身份与输出偏差问题；自建时应用**自己合法取得的上游凭据**。
- **对外提供生成式 AI 服务**：各司法辖区可能有备案、内容安全、实名、日志留存、税务等义务；New API README 对此有明确提示，其它项目同样适用。
- **danmo-router 部署方**：集中说明见 [compliance.md](./compliance.md)（合法授权、禁止规避上游协议的中转/拆分、MIT 责任边界）；与 New API [可接受使用政策](https://docs.newapi.ai/zh/docs/legal/acceptable-use) 同类项目对齐，便于商业/私有化选型时一并审阅。

## 按场景怎么选（决策规则）

```text
要不要「卖额度 / 多租户充值」？
  ├─ 是，且能接受 AGPL 或买商业许可 → New API（功能最完整）
  ├─ 是，但必须 MIT、可接受功能较少 → One API（存量）或自研支付 + danmo-router / 其它 MIT 面板
  └─ 否（内部或个人）
        ├─ Python 团队、要护栏/Fallback/OTel、厂商极多 → LiteLLM
        ├─ 主要痛点是 Key/订阅账号轮换与容错 → GPT-Load
        ├─ 必须坚持原厂 Messages/Gemini/Chat 字段形状，且国产思考方言要配得细
        │     → danmo-router（或 New API + 自行验证方言行为）
        └─ 已有 APISIX / Kong / Envoy Gateway → 优先插件 / Agent Router，而不是再挂一整站
```

### 和 danmo-router 的诚实对照

| 你更在意… | 更可能合适 | 原因 |
|-----------|------------|------|
| 开箱充值、订阅计划、邀请返佣、多语言运营后台 | New API | 分发产品面最完整 |
| 许可证必须宽松、改动可闭源对外 | One API / GPT-Load / LiteLLM 核心 / danmo-router | 避开 AGPL |
| 100+ 国际厂商 + 代码化配置 + 企业治理 | LiteLLM | 网关定位，不是「站」 |
| 订阅号与 API Key 混池调度，界面要轻 | GPT-Load | 明确不做完整分发站 |
| 客户端 Anthropic/Gemini SDK 直连 + 国产模型思考/usage 字段不跑偏 | danmo-router | 方言与透传/转换边界写进架构 |
| 支付闭环、工单、实名、营销 | VoAPI Pro 或 New API | 开源社区版通常不够 |

danmo-router **当前明显短板**（避免误判）：无内置支付；无 Realtime/绘图任务面；无官方 Helm；入口限额与渠道熔断状态均**不跨 relay 进程**；生态与插件远少于 New API / LiteLLM；项目仍早，生产前需自行压测协议与对账。

## 上线前最低核验清单

无论选哪一个，锁定具体版本后至少做：

1. **协议合同测试**：流式、工具调用、取消、错误码、usage 字段；Messages / Gemini 若声称支持，用官方 SDK 打一遍。
2. **计量对账**：预扣与结算、缓存命中、失败退款、重试是否重复计费。
3. **密钥与租户**：管理面与数据面隔离、密钥加密、跨用户日志不可见。
4. **许可证与依赖**：AGPL/商业模块/镜像来源；修改与去品牌是否触发额外义务。
5. **退出**：渠道、模型映射、用量日志可导出；客户端只依赖标准 Base URL 时迁移成本最低。

## 参考链接

- One API：https://github.com/songquanpeng/one-api  
- New API：https://github.com/QuantumNous/new-api · https://www.newapi.ai  
- LiteLLM：https://github.com/BerriAI/litellm · https://docs.litellm.ai  
- GPT-Load：https://github.com/tbphp/gpt-load · https://www.gpt-load.com  
- Sub2API：https://github.com/Wei-Shaw/sub2api（阅读其 ToS 警告）  
- VoAPI：https://github.com/VoAPI/VoAPI  
- 本项目：[architecture.md](architecture.md)（含渠道健康）· [operations.md](operations.md)（运维与云平台部署）

第三方综述（有商业利益披露，结论需交叉验证）：[GPTZZZ 选型文](https://gptzzz.ai/blog/open-source-api-relay-selection/)、[Awesome AI Gateway 对比](https://cuihuan.github.io/awesome-ai-gateway/compare/one-api-vs-new-api-vs-litellm.zh-CN.md)、[GatewayScore: New API vs LiteLLM](https://gatewayscore.com/compare/new-api-vs-litellm/)。
