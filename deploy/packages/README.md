# 按角色划分的发布包

`make package` 在 `dist/packages/` 下生成 **三个可独立部署单元**（仍共享同一 `DATABASE_URL` / `ENCRYPTION_KEY` 时才能协同工作）：

| 目录 | 内容 | 典型安装路径 |
|------|------|----------------|
| `danmo-relay/` | `bin/danmo-relay` | `/opt/danmo-relay` |
| `danmo-admin-portal/` | `bin/danmo-admin` + `apps/admin/` 静态 | `/opt/danmo-admin` |
| `danmo-user-portal/` | `bin/danmo-user` + `apps/user/` 静态 | `/opt/danmo-user` |

**用户面 = API + Portal 同一包**：`danmo-user-portal` 内含 `danmo-user` 与用户中心静态页，装在一台机用 Nginx 提供 HTTPS + `/api` 反代即可，无需再从别处找 Portal 资产。

**管理面同理：** `danmo-admin-portal`。

完整单机安装仍可用 **`dist/` 根目录**（含三个二进制、`apps/`、`deploy/`、`.env.example`），等价于一次装齐。

共享配置：各包需同一份 `.env`（或相同 `DATABASE_URL` 与 `ENCRYPTION_KEY`）。从 `dist/.env.example` 复制。

Nginx / systemd 示例在 **`dist/deploy/`**（或仓库 `deploy/`）。`setup-nginx.sh` 里安装根目录填对应路径，例如用户面 `DANMO_ROOT=/opt/danmo-user`。
