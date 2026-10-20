# Nginx 模板已迁移

生产与本机整栈的 Nginx 配置在 **[`deploy/compose/nginx/`](../../compose/nginx/)**，由 **`danmo-edge`** 容器渲染并监听 **80/443**。

后端 `relay` / `admin` / `user` 只在 Compose 内部网络可达，**不要**再往宿主机安装 systemd 或 `/etc/nginx/conf.d` 站点。

```bash
./deploy/scripts/install.sh
# 或
docker compose --profile all up -d --build
```

TLS：将证书放到仓库根 `certs/fullchain.pem` 与 `certs/privkey.pem`，并设 `ENABLE_TLS=1`。云四层 LB 终结 TLS 时可保持 `ENABLE_TLS=0`，只暴露 edge `:80`。
