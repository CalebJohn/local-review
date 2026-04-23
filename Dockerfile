FROM node:22-bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    ca-certificates \
    curl \
    ripgrep \
    less \
    procps \
    sudo \
    gcc \
    libc6-dev \
 && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /home/node/.claude /workspace \
 && chown -R node:node /home/node /workspace

# Install Rust via rustup as the node user
USER node
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/home/node/.cargo/bin:${PATH}"

WORKDIR /workspace
