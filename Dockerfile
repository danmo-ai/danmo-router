# syntax=docker/dockerfile:1.7
# Multi-target image build:
#   docker build --target relay -t danmo-relay .
#   docker build --target admin -t danmo-admin .
#   docker build --target user  -t danmo-user .
#   docker build --target edge  -t danmo-edge .

ARG RUST_VERSION=1.83
ARG NODE_VERSION=22

FROM rust:${RUST_VERSION}-bookworm AS rust-builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build -p danmo-server --release \
      --bin danmo-relay \
      --bin danmo-admin \
      --bin danmo-user \
 && strip target/release/danmo-relay target/release/danmo-admin target/release/danmo-user

FROM node:${NODE_VERSION}-bookworm AS portal-builder
WORKDIR /src
COPY apps/admin/package.json apps/admin/package-lock.json ./apps/admin/
COPY apps/user/package.json apps/user/package-lock.json ./apps/user/
RUN npm ci --prefix apps/admin && npm ci --prefix apps/user
COPY apps/admin ./apps/admin
COPY apps/user ./apps/user
ARG VITE_COMPLIANCE_URL=
ENV VITE_COMPLIANCE_URL=${VITE_COMPLIANCE_URL}
RUN npm run build --prefix apps/admin && npm run build --prefix apps/user

FROM debian:bookworm-slim AS runtime-base
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data --shell /usr/sbin/nologin danmo \
 && mkdir -p /data \
 && chown danmo:danmo /data
ENV HOST=0.0.0.0 \
    DATABASE_URL=sqlite:///data/danmo.db?mode=rwc \
    RUST_LOG=info
WORKDIR /data
USER danmo

FROM runtime-base AS relay
COPY --from=rust-builder /src/target/release/danmo-relay /usr/local/bin/danmo-relay
ENV PORT=3000
EXPOSE 3000
CMD ["danmo-relay"]

FROM runtime-base AS admin
COPY --from=rust-builder /src/target/release/danmo-admin /usr/local/bin/danmo-admin
ENV PORT=3001
EXPOSE 3001
CMD ["danmo-admin"]

FROM runtime-base AS user
COPY --from=rust-builder /src/target/release/danmo-user /usr/local/bin/danmo-user
ENV PORT=3002
EXPOSE 3002
CMD ["danmo-user"]

FROM nginx:1.27-alpine AS edge
RUN apk add --no-cache bash gettext \
 && rm -f /etc/nginx/conf.d/default.conf
COPY deploy/compose/nginx/templates/ /etc/nginx/templates-src/
COPY deploy/compose/nginx/docker-entrypoint.sh /docker-entrypoint.d/40-danmo-render.sh
RUN chmod +x /docker-entrypoint.d/40-danmo-render.sh
COPY --from=portal-builder /src/apps/admin/dist /usr/share/nginx/html/admin
COPY --from=portal-builder /src/apps/user/dist /usr/share/nginx/html/user
ENV API_DOMAIN=api.localhost \
    ADMIN_DOMAIN=admin.localhost \
    USER_DOMAIN=user.localhost \
    RELAY_UPSTREAM=relay:3000 \
    ADMIN_UPSTREAM=admin:3001 \
    USER_UPSTREAM=user:3002 \
    ENABLE_TLS=0
EXPOSE 80 443
