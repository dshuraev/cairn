# Packages statically-linked (musl) cairn binaries, cross-compiled by Taskfile.yml
# (`task build`), into a minimal Alpine-based distribution image.
FROM alpine:3.20

ARG TARGETARCH

RUN apk add --no-cache ca-certificates

COPY dist/linux-${TARGETARCH}/cairn-digest /usr/local/bin/cairn-digest
COPY dist/linux-${TARGETARCH}/cairn-dirtree /usr/local/bin/cairn-dirtree
COPY dist/linux-${TARGETARCH}/cairn-reconstruct /usr/local/bin/cairn-reconstruct

ENTRYPOINT ["cairn-digest"]
