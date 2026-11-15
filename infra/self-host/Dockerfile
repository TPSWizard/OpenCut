# syntax=docker/dockerfile:1
#
# Self-hosted ConzaCut: the web editor and its API worker in one container,
# on one port. Build from the repository root:
#
#   docker build -f infra/self-host/Dockerfile -t conzacut .
#   docker run -p 8080:8080 -v conzacut-data:/data conzacut
#
# The editor still renders and exports in the browser; this container only
# serves the app and the small API (sharing, sound and stock search).

# 1. The shared Rust core, compiled to the wasm package the web app imports.
FROM rust:1.97-slim-bookworm AS wasm
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown \
 && cargo install wasm-pack --locked --version 0.15.1
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY apps/desktop apps/desktop
COPY apps/render-cli apps/render-cli
RUN cd crates/wasm && wasm-pack build . --target bundler --out-dir pkg --release

# 2. The web app (built to call its API at /api on its own origin) and the
#    API worker's dependencies.
FROM oven/bun:1.4.4 AS web
WORKDIR /src
COPY --from=wasm /src/crates/wasm/pkg crates/wasm/pkg
COPY apps/api/package.json apps/api/bun.lock apps/api/
COPY apps/web/package.json apps/web/bun.lock apps/web/
RUN cd apps/api && bun install --frozen-lockfile \
 && cd ../web && bun install --frozen-lockfile
COPY changelog changelog
COPY fixtures fixtures
COPY apps/api apps/api
COPY apps/web apps/web
COPY infra/self-host infra/self-host
RUN cd apps/web && VITE_API_URL=/api bun run build \
 && bun ../../infra/self-host/prepare-web.mjs dist/server/wrangler.json

# 3. The runtime: Cloudflare's workerd through wrangler, which runs the
#    gateway, the web app and the API worker together.
FROM node:24-bookworm-slim
ENV NODE_ENV=production \
    WRANGLER_SEND_METRICS=false \
    X_LOCAL_EXPLORER=false \
    X_LOCAL_OBSERVABILITY=false \
    PORT=8080
# workerd needs the system CA bundle for outbound HTTPS (Freesound, Pixabay).
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=web --chown=node:node /src/apps/api apps/api
COPY --from=web --chown=node:node /src/apps/web/dist apps/web/dist
COPY --from=web --chown=node:node /src/infra/self-host infra/self-host
RUN mkdir -p /data && chown node:node /data /app \
 && chmod +x infra/self-host/entrypoint.sh
USER node
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.PORT||8080)+'/api/health').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"
ENTRYPOINT ["infra/self-host/entrypoint.sh"]
