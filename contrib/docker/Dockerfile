FROM alpine:3.24 AS base

FROM base AS builder
RUN apk add --no-cache \
    gcc \
    musl-dev \
    rust \
    cargo
WORKDIR /app/mitra
ENV DEFAULT_CONFIG_PATH=/etc/mitra/config.yaml
COPY . .
RUN cargo build --release --features production

FROM base AS web
# libgcc is needed to prevent "symbol not found" errors when executing `mitra --version`
RUN apk add --no-cache nodejs npm jq libgcc
WORKDIR /app/mitra-web
COPY --from=builder /app/mitra/target/release/mitra /usr/bin/mitra
RUN export version=$(mitra --version | cut -d ' ' -f 2) && \
    export url=$( \
        (wget -O- "https://codeberg.org/api/v1/repos/silverpill/mitra-web/tags/v${version}" | jq -re '.tarball_url') || \
        echo "https://codeberg.org/api/v1/repos/silverpill/mitra-web/archive/main.tar.gz" \
    ) && \
    wget -O /tmp/mitra-web.tar.gz $url
RUN tar --strip-components=1 -xvf /tmp/mitra-web.tar.gz mitra-web
RUN npm install --no-save
RUN npx allow-scripts
RUN npm run build

FROM base AS prod
RUN apk add --no-cache ca-certificates libgcc
WORKDIR /usr/bin
RUN mkdir -p /var/lib/mitra && \
    mkdir -p /etc/mitra
COPY --from=web /app/mitra-web/dist /usr/share/mitra/www
COPY --from=builder /app/mitra/target/release/mitra .
COPY --from=builder /app/mitra/config.example.yaml /usr/share/mitra/examples/
RUN cp -n /usr/share/mitra/examples/config.example.yaml /etc/mitra/config.yaml
CMD [ "mitra", "server" ]
