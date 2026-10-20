# For MCP directories that start a server on Linux to list its tools, such as
# Glama. Waldo drives Mac apps, so here it lists its tools and answers every
# call with "Waldo runs on macOS only". To use Waldo, install it on a Mac:
# https://justusewaldo.com
#
#   docker build -f docker/Dockerfile -t waldo-mcp .
#   docker run -i --rm waldo-mcp

FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p waldo-mcp --bin waldo-mcp

FROM debian:stable-slim
COPY --from=build /src/target/release/waldo-mcp /usr/local/bin/waldo-mcp
ENTRYPOINT ["waldo-mcp"]
