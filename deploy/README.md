# 部署（Docker Compose / 云 Serverless 容器）

`danmo-relay` / `danmo-admin` / `danmo-user` 监听 HTTP（默认 3000/3001/3002）。公网入口是 **Compose 内置的 `danmo-edge`（Nginx）**；Portal 静态资源打进 edge 镜像，**不走 OSS/CDN**。Compose 下后端使用 `network_mode: service:edge`，与 Nginx **共享网络命名空间**，仅经 `127.0.0.1` 反代；**宿主机只映射 edge 的 80/443**。

Schema 由进程启动时按引擎目录增量迁移（`crates/core/migrations/sqlite/`；MySQL 即将支持）。

**发布产物：** GitHub Release / `v*` tag 推送 **GHCR 镜像**（见 [RELEASE-LAYOUT.md](RELEASE-LAYOUT.md)），无二进制 tar。

## 推荐拓扑

```text
api.example.com:80/443   ──► edge (Nginx) ──► relay:3000   （Compose 内网）
admin.example.com        ──► edge (静态 admin + /api) ──► admin:3001
user.example.com         ──► edge (静态 user  + /api) ──► user:3002
```

三进程共用同一逻辑库与 `ENCRYPTION_KEY`。可选前置四层 LB → 主机 edge:443。

## 首次发布（云本机 / 本机整栈）

```bash
cp .env.example .env   # 修改 ENCRYPTION_KEY、域名、密码；生产 SEED_DEMO=0
./deploy/scripts/install.sh
# 或：
# COMPOSE_PROFILES=all docker compose up -d --build
```

`install.sh`：写入/更新 `.env`、按 `--role=` 选择 Compose **profiles**、`docker compose up -d`。SQL **不由脚本执行**。

分包角色示例：

```bash
./deploy/scripts/install.sh --role=relay
./deploy/scripts/install.sh --role=admin --api-domain=api.example.com --admin-domain=admin.example.com
```

TLS（edge 终结）：证书放入 `./certs/fullchain.pem` 与 `privkey.pem`，然后：

```bash
./deploy/scripts/install.sh --enable-tls --api-domain=api.example.com \
  --admin-domain=admin.example.com --user-domain=user.example.com
```

## 升级

```bash
./deploy/scripts/upgrade.sh          # pull GHCR 镜像并 recreate
./deploy/scripts/upgrade.sh --build  # 从当前源码重建
```

升级前备份 Compose 卷中的 SQLite，并保管 `.env` 里的 `ENCRYPTION_KEY`。

## 云 Serverless 容器

同一套 GHCR 镜像：

| 镜像 | 说明 |
|------|------|
| `ghcr.io/danmo-ai/danmo-relay` | 转发 |
| `ghcr.io/danmo-ai/danmo-admin` | 管理 API |
| `ghcr.io/danmo-ai/danmo-user` | 用户 API |
| `ghcr.io/danmo-ai/danmo-edge` | Nginx + Portal 静态 |

探活 `GET /healthz`；relay 长连接需加大空闲/请求超时；SQLite 需持久卷（或待 MySQL）。Portal 仍由 **edge** 提供，不用对象存储 CDN。厂商说明见 [docs/operations.md](../docs/operations.md)。

## 文件说明

| 路径 | 用途 |
|------|------|
| [`compose.yml`](../compose.yml) | 四服务 Compose |
| [`Dockerfile`](../Dockerfile) | 多 target：relay/admin/user/edge |
| `compose/nginx/` | edge Nginx 模板与入口脚本 |
| `scripts/install.sh` | 首次 Compose 发布 |
| `scripts/upgrade.sh` | Compose 升级 |
| `packages/README.md` | 镜像 / profile 角色说明 |

开发与单测仍用 `make check` / `make run-*`；整栈冒烟用 Compose。
