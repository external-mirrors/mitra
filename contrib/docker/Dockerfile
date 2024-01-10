FROM alpine:3.18 as builder

RUN apk update && apk add --no-cache curl gcc musl-dev libressl-dev ca-certificates rust cargo

ENV OPENSSL_NO_VENDOR="Y"

WORKDIR /app

COPY ./mitra ./

RUN cargo build --release --features production

# --------------
FROM alpine:3.18

WORKDIR /usr/bin

COPY ./dist /usr/share/mitra/www

COPY --from=builder /app/target/release/mitra /app/target/release/mitractl .

CMD mitra
