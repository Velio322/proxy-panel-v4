# mypanel3 - multi-stage build
#   stage 1: frontend (client/dist, embedded into the binary via rust-embed)
#   stage 2: rust release build (SQLX_OFFLINE=true, uses .sqlx/)
#   stage 3: minimal runtime with postgresql-client for backups

# Stage 1: frontend
FROM node:22-alpine AS client
WORKDIR /build/client
COPY client/package.json client/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY client/ ./
RUN npm run build

# Stage 2: rust release binary
FROM rust:1-bookworm AS builder
WORKDIR /src
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY crates ./crates
COPY install.sh ./install.sh
COPY deploy ./deploy
COPY .sqlx ./.sqlx
COPY --from=client /build/client/dist ./client/dist
ENV SQLX_OFFLINE=true
RUN cargo build --release --locked

# Stage 3: runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl postgresql-client \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system mypanel3 \
    && useradd --system --gid mypanel3 --home-dir /var/lib/mypanel3 \
        --shell /usr/sbin/nologin mypanel3 \
    && install -d -m 0750 -o mypanel3 -g mypanel3 /var/lib/mypanel3
COPY --from=builder /src/target/release/mypanel3 /usr/local/bin/mypanel3

USER mypanel3
ENV DATA_DIR=/var/lib/mypanel3
EXPOSE 3000
VOLUME /var/lib/mypanel3
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/api/health >/dev/null || exit 1

ENTRYPOINT ["mypanel3"]
CMD ["serve"]
