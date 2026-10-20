# 角色与 Compose profiles

生产不再提供「解压二进制到 `/opt/danmo-*`」分包。按角色选择 **镜像** 与 Compose **profiles**：

| Profile / 角色 | 启动的服务 | 镜像 |
|----------------|------------|------|
| `all`（默认） | relay + admin + user + edge | 四镜像 |
| `relay` | relay + edge | `danmo-relay`, `danmo-edge` |
| `admin` | admin + edge | `danmo-admin`, `danmo-edge` |
| `user` | user + edge | `danmo-user`, `danmo-edge` |

```bash
./deploy/scripts/install.sh --role=user
# 等价：COMPOSE_PROFILES=user docker compose up -d
```

多机拆分时各机使用不同 `--role=`，仍须共享同一 `DATABASE_URL`（持久卷 / 网络库）与 `ENCRYPTION_KEY`。edge 在每台需要公网域名的机器上运行；Portal 静态已打进 `danmo-edge`，无需单独 portal 包。
