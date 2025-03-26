## Dockerfile for devcontainer

FROM mcr.microsoft.com/devcontainers/base:debian AS base

ARG USER=vscode
ARG GROUP=vscode

ENV HOME="/home/${USER}"
ENV PATH="$HOME/.cargo/bin:$PATH"

# Install dependencies
RUN apt-get update \
    && apt-get -y install \
        build-essential \
        cmake \
        curl \
        gdb \
        git \
        gnupg \
        gnuplot \
        lsb-release \
        make \
        software-properties-common \
        sudo \
        wget \
        netcat-openbsd

ARG GCC_VERSION=12

RUN apt-get install -y g++-multilib \
    && apt-get install -y libgcc-${GCC_VERSION}-dev \
    && apt-get install -y lib32gcc-${GCC_VERSION}-dev 

ARG LLVM_VERSION=17

# Install llvm
RUN wget https://apt.llvm.org/llvm.sh \
    && chmod +x ./llvm.sh         \
    && sudo ./llvm.sh ${LLVM_VERSION} all      \
    && sudo ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/clang-cl /usr/bin/clang-cl \
    && sudo ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/llvm-lib /usr/bin/llvm-lib \
    && sudo ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/lld-link /usr/bin/lld-link \
    && sudo ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/llvm-ml /usr/bin/llvm-ml   \
    && sudo ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/ld.lld /usr/bin/ld.lld     \
    && sudo ln -s /usr/lib/llvm-${LLVM_VERSION}/bin/clang /usr/bin/clang

FROM base AS dev

# Make sure the devcontainer user has sudo access
RUN chown -R "${USER}:${GROUP}" /home/${USER} \
    && echo "${USER} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Persist bash history
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
    && mkdir /commandhistory \
    && touch /commandhistory/.bash_history \
    && chown -R "${USER}" /commandhistory \
    && echo "$SNIPPET" >> "/home/${USER}/.bashrc"
# Install python3
ARG WASI_SDK_VERSION_FULL=20.0
ARG WASI_SDK_VERSION_MAJOR=${WASI_SDK_VERSION_FULL%%.*}
# Install wasi-sdk
RUN wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK_VERSION_MAJOR}/wasi-sdk-${WASI_SDK_VERSION_FULL}-linux.tar.gz \
    && tar xvf wasi-sdk-${WASI_SDK_VERSION_FULL}-linux.tar.gz \
    && rm wasi-sdk-${WASI_SDK_VERSION_FULL}-linux.tar.gz \
    && mv /wasi-sdk-${WASI_SDK_VERSION_FULL}  /opt/wasi-sdk

USER $USER

ARG RUST_TOOLCHAIN=1.82.0

# Install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && rustup default ${RUST_TOOLCHAIN} \
    && rustup target add x86_64-unknown-linux-gnu \
    && rustup target add x86_64-unknown-none      \
    && rustup target add x86_64-pc-windows-msvc   \
    && rustup toolchain add nightly-x86_64-unknown-linux-gnu \
    && cargo install just \
    && cargo install --locked wasm-tools \
    && cargo install wkg
