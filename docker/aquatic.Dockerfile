# syntax=docker/dockerfile:1

# aquatic_udp
#
# Please note that running aquatic_udp under Docker is NOT RECOMMENDED due to
# suboptimal performance. This file is provided as a starting point for those
# who still wish to do so.
#
# Customize by setting CONFIG_FILE_CONTENTS and
# ACCESS_LIST_CONTENTS environment variables.
#
# Alternatively, customize by setting the /app/config.toml and
# /app/access-list.txt files.
#
# Note: Customizing via environment variables will directly overwrite
# the contents of /app/config.toml and /app/access-list.txt files,
# remember to backup.
#
# By default runs tracker on port 3000 without info hash access control.
#
# Run from repository root directory with:
# $ DOCKER_BUILDKIT=1 docker build -t aquatic-udp -f docker/aquatic_udp.Dockerfile .
# $ docker run -it -p 0.0.0.0:3000:3000/udp --name aquatic-udp aquatic-udp
#
# Pass --network="host" to run command for much better performance.

FROM rust:latest AS builder

# According to the protocol changes: udp http ws
ARG PROTOCOL=udp

WORKDIR /usr/src/aquatic

# Create entry point script for setting config and access
# list file contents at runtime
COPY <<-"EOT" /entrypoint.sh
#!/bin/bash

# Handle configuration file
if [[ "${CONFIG_FILE_CONTENTS:-ChangeMe}" != "ChangeMe" ]]; then
    echo "Applying custom configuration from CONFIG_FILE_CONTENTS..."
    printf "%s" "$CONFIG_FILE_CONTENTS" > ./config.toml
fi

# Handle access list
if [[ "${ACCESS_LIST_CONTENTS:-ChangeMe}" != "ChangeMe" ]]; then
    echo "Applying custom access list from ACCESS_LIST_CONTENTS..."
    printf "%s" "$ACCESS_LIST_CONTENTS" > ./access-list.txt
fi

# Generate default config file (if not exists)
if [ ! -f "./config.toml" ]; then
    echo "Generating default config.toml..."
    if ! aquatic_Protocol -p > "./config.toml"; then
        echo "Error: Failed to generate config.toml"
        exit 1
    fi
fi

# Create empty access list file (if not exists)
if [ ! -f "./access-list.txt" ]; then
    echo "Creating empty access-list.txt..."
    if ! touch "./access-list.txt"; then
        echo "Error: Failed to create access-list.txt"
        exit 1
    fi
fi

exec aquatic_Protocol "$@"
EOT

COPY . .

RUN --mount=type=cache,target=/usr/src/aquatic/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    . ./scripts/env-native-cpu-without-avx-512 && \
    cargo build --release -p aquatic_$PROTOCOL && \
    cp target/release/aquatic_$PROTOCOL /usr/local/bin/ && \
    sed -i "s|aquatic_Protocol|aquatic_$PROTOCOL|g" /entrypoint.sh

FROM debian:stable-slim

# According to the protocol changes: udp http ws
ARG PROTOCOL=udp

ENV CONFIG_FILE_CONTENTS "ChangeMe" \
    ACCESS_LIST_CONTENTS "ChangeMe"

WORKDIR /app/

COPY --from=builder /entrypoint.sh /entrypoint.sh
COPY --from=builder /usr/local/bin/aquatic_$PROTOCOL /usr/local/bin

RUN aquatic_$PROTOCOL -p > "./config.toml" && \
    chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["-c", "./config.toml"]
