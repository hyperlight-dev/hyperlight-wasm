#!/usr/bin/env bash

set -o errexit
set -o nounset
set -o pipefail

pushd "$(dirname "${BASH_SOURCE[0]}")/../../wasmsamples"
OUTPUT_DIR="../../x64/${1:-"debug"}"
mkdir -p ${OUTPUT_DIR}
OUTPUT_DIR=$(realpath $OUTPUT_DIR)


if [ -f "/.dockerenv" ] || grep -q docker /proc/1/cgroup; then
    # running in a container so use the installed wasi-sdk as the devcontainer has this installed  
    for FILENAME in $(find . -name '*.c')
    do
        echo Building ${FILENAME}
        # Build the wasm file with wasi-libc for wasmtime
        /opt/wasi-sdk/bin/clang -flto -ffunction-sections -mexec-model=reactor -O3 -z stack-size=4096 -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors -Wl,--strip-all,--no-entry -Wl,--allow-undefined -Wl,--gc-sections  -o ${OUTPUT_DIR}/${FILENAME%.*}-wasi-libc.wasm ${FILENAME}

        # Build AOT for Wasmtime; note that Wasmtime does not support
        # interpreting, so its wasm binary is secretly an AOT binary.
        cargo run -p hyperlight-wasm-aot compile ${OUTPUT_DIR}/${FILENAME%.*}-wasi-libc.wasm ${OUTPUT_DIR}/${FILENAME%.*}.aot
        cp ${OUTPUT_DIR}/${FILENAME%.*}.aot ${OUTPUT_DIR}/${FILENAME%.*}.wasm
    done
else 
    # not running in a container so use the docker image to build the wasm files
    echo Building docker image that has Wasm sdk. Should be quick if preivoulsy built and no changes to dockerfile.
    echo This will take a while if it is the first time you are building the docker image.
    echo Log in ${OUTPUT_DIR}/dockerbuild.log

    docker pull ghcr.io/deislabs/wasm-clang-builder:latest

    docker build --build-arg GCC_VERSION=12 --build-arg WASI_SDK_VERSION_FULL=20.0 --cache-from ghcr.io/deislabs/wasm-clang-builder:latest -t wasm-clang-builder:latest . 2> ${OUTPUT_DIR}/dockerbuild.log

    for FILENAME in $(find . -name '*.c')
    do
        echo Building ${FILENAME}
        # Build the wasm file with wasi-libc for wasmtime
        docker run --rm -i -v "${PWD}:/tmp/host" -v "${OUTPUT_DIR}:/tmp/output" wasm-clang-builder:latest /opt/wasi-sdk/bin/clang -flto -ffunction-sections -mexec-model=reactor -O3 -z stack-size=4096 -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors -Wl,--strip-all,--no-entry -Wl,--allow-undefined -Wl,--gc-sections  -o /tmp/output/${FILENAME%.*}-wasi-libc.wasm /tmp/host/${FILENAME}

        # Build AOT for Wasmtime; note that Wasmtime does not support
        # interpreting, so its wasm binary is secretly an AOT binary.
        cargo run -p hyperlight-wasm-aot compile ${OUTPUT_DIR}/${FILENAME%.*}-wasi-libc.wasm ${OUTPUT_DIR}/${FILENAME%.*}.aot
        cp ${OUTPUT_DIR}/${FILENAME%.*}.aot ${OUTPUT_DIR}/${FILENAME%.*}.wasm
    done
fi

popd
