## Dockerfile for devcontainer

FROM ghcr.io/hyperlight-dev/hyperlight-devcontainer:latest
ARG USER=vscode
ARG GROUP=vscode

# use root to install tools then switch back to vscode user
USER root

# Install dependencies
RUN apt-get update \
    && apt-get -y install \
        netcat-openbsd

ARG GCC_VERSION=12

RUN apt-get install -y g++-multilib \
    && apt-get install -y libgcc-${GCC_VERSION}-dev \
    && apt-get install -y lib32gcc-${GCC_VERSION}-dev 

ARG WASI_SDK_VERSION_FULL=25.0
ARG WASI_SDK_VERSION_MAJOR=${WASI_SDK_VERSION_FULL%%.*}

# Install wasi-sdk
RUN wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK_VERSION_MAJOR}/wasi-sdk-${WASI_SDK_VERSION_FULL}-linux.tar.gz \
    && tar xvf wasi-sdk-${WASI_SDK_VERSION_FULL}-linux.tar.gz \
    && rm wasi-sdk-${WASI_SDK_VERSION_FULL}-linux.tar.gz \
    && mv /wasi-sdk-${WASI_SDK_VERSION_FULL}  /opt/wasi-sdk

USER $USER
ARG RUST_TOOLCHAIN=1.85.0

# Install rust and component tools
RUN rustup default ${RUST_TOOLCHAIN} \
    && cargo install --locked wasm-tools \
    && cargo install wkg \
    && cargo install wac-cli \
    && cargo install cargo-component --locked
