# Release 产物布局

推送 `v*` tag 时，[`.github/workflows/release.yml`](../.github/workflows/release.yml) **只发布 GHCR 容器镜像**（不再上传平台二进制 tar 或 portals tar）。

## 镜像

| 镜像 | Dockerfile target | 说明 |
|------|-------------------|------|
| `ghcr.io/danmo-ai/danmo-relay:<tag>` | `relay` | 转发 API |
| `ghcr.io/danmo-ai/danmo-admin:<tag>` | `admin` | 管理 API |
| `ghcr.io/danmo-ai/danmo-user:<tag>` | `user` | 用户 API |
| `ghcr.io/danmo-ai/danmo-edge:<tag>` | `edge` | Nginx + Portal 静态 |

平台：`linux/amd64`、`linux/arm64`。另打 `latest`（指向该 tag）。

## 使用

仓库内 [`compose.yml`](../compose.yml) 默认引用上述镜像名；可用 `DANMO_TAG` 固定版本：

```bash
export DANMO_TAG=v0.4.0
cp .env.example .env
./deploy/scripts/install.sh --pull --no-build
```

或源码构建：`docker compose --profile all up -d --build`。

GitHub Release 页面以 changelog / release notes 为主，**不含**大文件附件。
