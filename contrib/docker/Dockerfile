FROM alpine:3.22 AS builder

RUN apk update && apk add --no-cache curl gcc musl-dev rust cargo

WORKDIR /app

COPY ./mitra ./

ENV DEFAULT_CONFIG_PATH=/etc/mitra/config.yaml

RUN cargo build --release --features production

#--------------
FROM alpine:3.22

RUN apk add --no-cache ca-certificates gcc

WORKDIR /usr/bin

RUN mkdir -p /var/lib/mitra && mkdir -p /etc/mitra

COPY --from=builder /app/config.example.yaml /usr/share/mitra/examples/
RUN cp -n /usr/share/mitra/examples/config.example.yaml /etc/mitra/config.yaml

COPY ./dist /usr/share/mitra/www

COPY --from=builder /app/target/release/mitra .

CMD [ "mitra", "server" ]
