# Build context is the local service delivery directory, with an already
# verified Linux binary in bin/. No Rust toolchain is downloaded at install.
FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3t64 curl && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home termterm-team
COPY bin/termterm-team-service /usr/local/bin/termterm-team-service
ARG VERSION
ARG REVISION
LABEL org.opencontainers.image.title="TermTerm Team Service" \
      org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION \
      org.opencontainers.image.source="https://github.com/EnezMutluoglu/TermTerm"
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/termterm-team-service", "/etc/termterm-team/service.json"]
