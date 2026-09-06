# Local demo image for Hemsidor24. Default build only — no handover feature,
# no Sijill, nothing signed. See compose.yaml.
FROM rust:1.95-slim-bookworm AS builder

WORKDIR /build
COPY . .

# sijill-protocol is a private repository, and this demo deliberately builds
# without it. Its crates are optional dependencies used only by the `handover`
# feature, but Cargo still tries to fetch a git dependency it can see, and the
# build has no credentials — nor should it.
#
# Deleting every line mentioning sijill from that one manifest removes both the
# optional dependencies and the feature entries that name them. The default
# build compiles the handover crate to an empty library either way, so nothing
# this demo runs is affected. The repository itself is untouched; this happens
# only inside the image.
RUN sed -i '/sijill/d' crates/hemsidor24-handover/Cargo.toml

# The .sqlx cache is committed, so the compile-time query checks need no
# database to build against.
ENV SQLX_OFFLINE=true
RUN cargo build --release --bin hemsidor24-web --bin hemsidor24-admin

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/hemsidor24-web  /usr/local/bin/
COPY --from=builder /build/target/release/hemsidor24-admin /usr/local/bin/

# routes.rs bakes the asset path in at compile time as CARGO_MANIFEST_DIR/static,
# so the runtime image has to keep the build path. Making that configurable is a
# code change this demo does not need.
COPY --from=builder /build/crates/hemsidor24-web/static /build/crates/hemsidor24-web/static

COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 3000 3001
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
