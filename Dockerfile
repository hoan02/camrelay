FROM node:22-bookworm-slim AS web-build
WORKDIR /workspace
COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/package.json
COPY packages/design-tokens/package.json packages/design-tokens/package.json
RUN npm ci
COPY apps/web apps/web
COPY packages/design-tokens packages/design-tokens
RUN npm run web:build

FROM rust:1.86-bookworm AS rust-build
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY src src
COPY dh-p2p.lua .
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates ffmpeg rclone wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /data --shell /usr/sbin/nologin camrelay

WORKDIR /data
COPY --from=rust-build /workspace/target/release/camrelay /usr/local/bin/camrelay
COPY --from=rust-build /workspace/dh-p2p.lua /opt/camrelay/dh-p2p.lua
COPY static /opt/camrelay/static
COPY --from=web-build /workspace/apps/web/dist /opt/camrelay/apps/web/dist
RUN chown -R camrelay:camrelay /data /opt/camrelay

USER camrelay
EXPOSE 8080
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/camrelay"]
