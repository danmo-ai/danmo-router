SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO ?= cargo
NPM ?= npm
RELEASE_DIR := target/release
DIST_DIR := dist
PKG_DIR := $(DIST_DIR)/packages
PORTALS := admin user

RELAY_PORT ?= 3000
ADMIN_PORT ?= 3001
USER_PORT ?= 3002

.PHONY: help check fmt fmt-check clippy test build release \
	run-relay run-admin run-user run-admin-api run-user-api run-all-in-one \
	portals-install portals-build portals-dev-admin portals-dev-user \
	package clean

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

check: fmt-check clippy test ## CI: fmt + clippy + test

## Build
build: ## Debug binaries
	$(CARGO) build -p danmo-server --bins

release: ## Release binaries
	$(CARGO) build -p danmo-server --bins --release

## Local run
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

## Dist
package: release portals-build ## dist/ + dist/packages/{relay,admin-portal,user-portal}
	rm -rf $(DIST_DIR)
	mkdir -p $(DIST_DIR)/bin $(DIST_DIR)/apps/admin $(DIST_DIR)/apps/user \
		$(PKG_DIR)/danmo-relay/bin \
		$(PKG_DIR)/danmo-admin-portal/bin $(PKG_DIR)/danmo-admin-portal/apps/admin \
		$(PKG_DIR)/danmo-user-portal/bin $(PKG_DIR)/danmo-user-portal/apps/user
	cp $(RELEASE_DIR)/danmo-relay $(RELEASE_DIR)/danmo-admin $(RELEASE_DIR)/danmo-user $(DIST_DIR)/bin/
	cp $(RELEASE_DIR)/danmo-relay $(PKG_DIR)/danmo-relay/bin/
	cp $(RELEASE_DIR)/danmo-admin $(PKG_DIR)/danmo-admin-portal/bin/
	cp $(RELEASE_DIR)/danmo-user $(PKG_DIR)/danmo-user-portal/bin/
	cp -R apps/admin/dist/. $(DIST_DIR)/apps/admin/
	cp -R apps/user/dist/. $(DIST_DIR)/apps/user/
	cp -R apps/admin/dist/. $(PKG_DIR)/danmo-admin-portal/apps/admin/
	cp -R apps/user/dist/. $(PKG_DIR)/danmo-user-portal/apps/user/
	cp .env.example $(DIST_DIR)/
	cp -R deploy/. $(DIST_DIR)/deploy/
	cp deploy/packages/README.md $(PKG_DIR)/README.md
	@echo "Artifacts: $(DIST_DIR)/ and $(PKG_DIR)/"

clean: ## Clean Rust + portal + dist
	$(CARGO) clean
	rm -rf apps/admin/dist apps/user/dist $(DIST_DIR)
