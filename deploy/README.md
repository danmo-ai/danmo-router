# 部署示例（Nginx + systemd）

`danmo-relay` / `danmo-admin` / `danmo-user` 监听 **HTTP**（默认 3000/3001/3002）。公网 **TLS 在 Nginx 终结**，再反代到本机 loopback。

**发布包布局：** `make package` 后 **`dist/packages/danmo-user-portal/`** = 用户 API + 用户 Portal 静态；**`danmo-admin-portal/`** = 管理 API + 管理 Portal；**`danmo-relay/`** = 仅转发。完整 **`dist/`** 根目录含三者 + `deploy/`。见 [packages/README.md](packages/README.md)。

## 推荐拓扑

```text
api.example.com:443   ──► Nginx ──► http://127.0.0.1:3000  (danmo-relay)
admin.example.com:443 ──► Nginx ──► 静态 Portal + /api ──► http://127.0.0.1:3001
user.example.com:443  ──► Nginx ──► 静态 Portal + /api ──► http://127.0.0.1:3002
```

前面可有 **四层 LB（TCP:443）** 做流量分发；Nginx 跑在每台 relay/Portal 机器上。LB 健康检查可直连 **3000** 的 `GET /healthz`（不经 TLS）。

## 安装步骤（单机示例）

1. 安装二进制与静态资源（`make package` 产物）到 `/opt/danmo/`。
2. 复制 [examples/systemd/danmo-relay.service](examples/systemd/danmo-relay.service) 等到 `/etc/systemd/system/`，`systemctl enable --now danmo-relay`。
3. **Nginx 配置（二选一）**
   - **交互脚本（推荐首次部署）：** `sudo ./scripts/setup-nginx.sh` — 引导输入域名、安装路径、本机要启用的 relay/admin/user 站点，渲染配置到 `/etc/nginx/conf.d/`。不捆绑 nginx/certbot；可选 apt 安装 nginx。
   - **手工：** 复制 [examples/nginx/](examples/nginx/) 下配置，自行改 `server_name`、证书路径、`root`。
4. 证书（二选一）：
   - **Let’s Encrypt（Certbot）**：见 [examples/nginx/README.md](examples/nginx/README.md)。
   - **云证书 / 自签**：把 `ssl_certificate` / `ssl_certificate_key` 指到 PEM 文件。

## 文件说明

| 文件 | 用途 |
|------|------|
| `examples/nginx/danmo-relay.conf` | 转发 API，流式友好 |
| `examples/nginx/danmo-portal-admin.conf` | 管理 Portal + `/api` → admin |
| `examples/nginx/danmo-portal-user.conf` | 用户 Portal + `/api` → user |
| `examples/systemd/danmo-*.service` | systemd 单元模板 |
| `scripts/setup-nginx.sh` | 交互式 Nginx 配置向导 |

**说明：** 脚本适合 **单机/自管 VM**；云上若 Portal 走 OSS/CDN、API 走 NLB，通常只需在 relay 节点跑脚本中的 relay 部分，或继续手工改示例。

详细说明见 [docs/operations.md](../docs/operations.md)。
