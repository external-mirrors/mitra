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
# libgcc is needed to prevent "symbol not found" errors when executing COPY
RUN apk add --no-cache jq libgcc
WORKDIR /app/mitra-web
COPY --from=builder /app/mitra/target/release/mitra /usr/bin/mitra
RUN export base_url="https://codeberg.org/api/v1/repos/silverpill/mitra-web/releases" && \
    export version=$(mitra --version) && \
    export url=$( \
        (wget -O- "${base_url}/${version}" | jq -re '.assets[0].browser_download_url') || \
        (wget -O- "${base_url}/latest"     | jq -re '.assets[0].browser_download_url') \
    ) && \
    wget -O /tmp/mitra-web.tar.gz $url
RUN tar -xvf /tmp/mitra-web.tar.gz

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
