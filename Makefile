SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO ?= cargo
NPM ?= npm
DOCKER ?= docker
PORTALS := admin user
IMAGE_PREFIX ?= ghcr.io/danmo-ai
DANMO_TAG ?= local

RELAY_PORT ?= 3000
ADMIN_PORT ?= 3001
USER_PORT ?= 3002

.PHONY: help check fmt fmt-check clippy test test-stack build release \
	run-relay run-admin run-user run-admin-api run-user-api run-all-in-one \
	portals-install portals-build portals-dev-admin portals-dev-user \
	docker-build compose-up compose-down compose-config clean

help: ## Show targets
	@grep -E '^[a-zA-Z0-9_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  %-18s %s\n", $$1, $$2}'

## Quality
fmt: ## Format Rust
	$(CARGO) fmt --all

fmt-check: ## Check Rust formatting
	$(CARGO) fmt --all -- --check

clippy: ## Clippy (-D warnings)
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test: ## Rust tests
	$(CARGO) test --workspace

test-stack: ## Integration tests (clean DB, mock upstream)
	$(CARGO) test -p danmo-server --test stack

check: fmt-check clippy test ## CI: fmt + clippy + test

## Build
build: ## Debug binaries
	$(CARGO) build -p danmo-server --bins

release: test-stack ## Release binaries (local cargo; production uses Docker images)
	$(CARGO) build -p danmo-server --bins --release

## Local run (cargo / Vite — not Compose)
run-relay: ## Relay :3000
	PORT=$(RELAY_PORT) $(CARGO) run -p danmo-server --bin danmo-relay

run-admin-api: ## Admin API only :3001
	PORT=$(ADMIN_PORT) $(CARGO) run -p danmo-server --bin danmo-admin

run-user-api: ## User API only :3002
	PORT=$(USER_PORT) $(CARGO) run -p danmo-server --bin danmo-user

run-admin: ## Admin API :3001 + Portal :5173
	$(MAKE) -j2 run-admin-api portals-dev-admin

run-user: ## User API :3002 + Portal :5174
	$(MAKE) -j2 run-user-api portals-dev-user

run-all-in-one: ## Monolith (local only)
	PORT=$(RELAY_PORT) $(CARGO) run -p danmo-server --bin danmo-router

## Portals
portals-install: ## npm ci (admin + user)
	@for p in $(PORTALS); do (cd apps/$$p && $(NPM) ci); done

portals-build: ## Production Portal builds
	@for p in $(PORTALS); do (cd apps/$$p && $(NPM) ci && $(NPM) run build); done

portals-dev-admin: ## Admin Portal only :5173
	cd apps/admin && $(NPM) run dev

portals-dev-user: ## User Portal only :5174
	cd apps/user && $(NPM) run dev

## Docker / Compose
docker-build: test-stack ## Build local images (relay/admin/user/edge)
	$(DOCKER) build --target relay -t $(IMAGE_PREFIX)/danmo-relay:$(DANMO_TAG) .
	$(DOCKER) build --target admin -t $(IMAGE_PREFIX)/danmo-admin:$(DANMO_TAG) .
	$(DOCKER) build --target user  -t $(IMAGE_PREFIX)/danmo-user:$(DANMO_TAG) .
	$(DOCKER) build --target edge  -t $(IMAGE_PREFIX)/danmo-edge:$(DANMO_TAG) .

compose-config: ## Validate compose.yml
	COMPOSE_PROFILES=$${COMPOSE_PROFILES:-all} $(DOCKER) compose config >/dev/null

compose-up: ## Full stack via Compose (profile all)
	@test -f .env || cp .env.example .env
	COMPOSE_PROFILES=$${COMPOSE_PROFILES:-all} $(DOCKER) compose up -d --build

compose-down: ## Stop Compose stack
	COMPOSE_PROFILES=$${COMPOSE_PROFILES:-all} $(DOCKER) compose down

clean: ## Clean Rust + portal outputs
	$(CARGO) clean
	rm -rf apps/admin/dist apps/user/dist
