SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO ?= cargo
NPM ?= npm
RELEASE_DIR := target/release
DIST_DIR := dist
PKG_DIR := $(DIST_DIR)/packages

RELAY_PORT ?= 3000
ADMIN_PORT ?= 3001
USER_PORT ?= 3002

BINS := danmo-relay danmo-admin danmo-user danmo-router

.PHONY: help
help: ## Show targets
	@grep -E '^[a-zA-Z0-9_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  %-18s %s\n", $$1, $$2}'

.PHONY: fmt
fmt: ## Format Rust code
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Check Rust formatting
	$(CARGO) fmt --all -- --check

.PHONY: clippy
clippy: ## Run clippy
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: test
test: ## Run all Rust tests
	$(CARGO) test --workspace

.PHONY: check
check: fmt-check clippy test ## CI-style checks (fmt + clippy + test)

.PHONY: build
build: ## Debug build all binaries
	$(CARGO) build -p danmo-server --bins

.PHONY: release
release: ## Release build all binaries
	$(CARGO) build -p danmo-server --bins --release

.PHONY: build-relay build-admin build-user
build-relay: ## Release build relay only
	$(CARGO) build -p danmo-server --bin danmo-relay --release
build-admin: ## Release build admin API only
	$(CARGO) build -p danmo-server --bin danmo-admin --release
build-user: ## Release build user API only
	$(CARGO) build -p danmo-server --bin danmo-user --release

.PHONY: run-relay run-admin run-user run-all-in-one
run-relay: ## Run traffic proxy (default :3000)
	PORT=$(RELAY_PORT) $(CARGO) run -p danmo-server --bin danmo-relay
run-admin: ## Run admin API (default :3001)
	PORT=$(ADMIN_PORT) $(CARGO) run -p danmo-server --bin danmo-admin
run-user: ## Run user API (default :3002)
	PORT=$(USER_PORT) $(CARGO) run -p danmo-server --bin danmo-user
run-all-in-one: ## Run monolithic process (local only)
	PORT=$(RELAY_PORT) $(CARGO) run -p danmo-server --bin danmo-router

.PHONY: portals-install portals-build portals-dev-admin portals-dev-user
portals-install: ## npm install both portals
	cd apps/admin && $(NPM) ci
	cd apps/user && $(NPM) ci

portals-build: ## Production build both portals
	cd apps/admin && $(NPM) ci && $(NPM) run build
	cd apps/user && $(NPM) ci && $(NPM) run build

portals-dev-admin: ## Dev admin portal (:5173 → admin API)
	cd apps/admin && $(NPM) run dev

portals-dev-user: ## Dev user portal (:5174 → user API)
	cd apps/user && $(NPM) run dev

.PHONY: package
package: release portals-build ## Assemble release artifacts under dist/
	rm -rf $(DIST_DIR)
	mkdir -p $(DIST_DIR)/bin $(DIST_DIR)/apps/admin $(DIST_DIR)/apps/user
	mkdir -p $(PKG_DIR)/danmo-relay/bin
	mkdir -p $(PKG_DIR)/danmo-admin-portal/bin $(PKG_DIR)/danmo-admin-portal/apps/admin
	mkdir -p $(PKG_DIR)/danmo-user-portal/bin $(PKG_DIR)/danmo-user-portal/apps/user
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
	@echo "Artifacts in $(DIST_DIR)/ (full) and $(PKG_DIR)/ (relay | admin-portal | user-portal)"

.PHONY: clean
clean: ## Clean Rust + portal build outputs
	$(CARGO) clean
	rm -rf apps/admin/dist apps/user/dist $(DIST_DIR)
