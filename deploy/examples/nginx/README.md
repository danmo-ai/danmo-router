# Nginx 示例配置

复制到 `/etc/nginx/conf.d/`（或 Debian/Ubuntu 的 `sites-available` + symlink），然后 `nginx -t && systemctl reload nginx`。

## 必改变量

- `server_name`：你的域名
- `ssl_certificate` / `ssl_certificate_key`：证书路径
- Portal 配置里的 `root`：指向 `apps/admin/dist` 或 `apps/user/dist`（如 `/opt/danmo/apps/admin`）

## Let’s Encrypt（Certbot + Nginx）

适合 **域名直接解析到本机**、且可用 **HTTP-01**（公网 80）的场景。若前面只有四层 LB 且不做 80 探活，请改用 **DNS-01** 或 **云证书**，再手动配置 `ssl_certificate`。

```bash
# 示例：Ubuntu/Debian，先装 certbot python3 插件
sudo apt install nginx certbot python3-certbot-nginx

# 先用仅 listen 80 的临时 server 或注释 ssl 行，确保 nginx -t 通过
sudo certbot --nginx -d api.example.com
sudo certbot --nginx -d admin.example.com -d user.example.com

# 自动续期由 certbot.timer 负责；检查：
systemctl status certbot.timer
```

Certbot 会改写配置中的 `ssl_certificate` 路径（通常在 `/etc/letsencrypt/live/<domain>/`）。**relay 配置**请保留 `proxy_buffering off` 与长超时，不要被通用模板覆盖。

## 与四层 LB 同用

- LB **TCP 443** → 节点 **Nginx:443** → **127.0.0.1:3000** relay。
- 目标组健康检查：**HTTP** → 实例 **3000** → `/healthz`（不经过 Nginx 亦可）。
- `api.example.com` **不要**经 CDN。
