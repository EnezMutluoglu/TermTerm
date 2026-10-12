# Build context is the local service delivery directory, with an already
# verified Linux binary in bin/. No Rust toolchain is downloaded at install.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home termterm-team
COPY bin/termterm-team-service /usr/local/bin/termterm-team-service
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/termterm-team-service", "/etc/termterm-team/service.json"]
