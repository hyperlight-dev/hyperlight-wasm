#!/usr/bin/env bash

set -o errexit
set -o nounset
set -o pipefail

pushd "$(dirname "${BASH_SOURCE[0]}")/../../wasmsamples"
OUTPUT_DIR="../../x64/${1:-"debug"}"
BUILD_TYPE="${1:-"debug"}"
FEATURES="${2:-""}"
mkdir -p ${OUTPUT_DIR}
OUTPUT_DIR=$(realpath $OUTPUT_DIR)

# Set stripping flags based on whether features are enabled
if [ -n "$FEATURES" ]; then
    STRIP_FLAGS=""
    DEBUG_FLAGS="-g"
    OPT_FLAGS="-O0"
else
    STRIP_FLAGS="-Wl,--strip-all"
    DEBUG_FLAGS=""
    OPT_FLAGS="-O3"
fi

# Set AOT debug flags if gdb feature is enabled
if [[ "$FEATURES" == *"gdb"* ]]; then
    AOT_DEBUG_FLAGS="--debug"
else
    AOT_DEBUG_FLAGS=""
fi

# Set AOT LTS flag if wasmtime_lts feature is enabled
if [[ "$FEATURES" == *"wasmtime_lts"* ]]; then
    AOT_LTS_FLAGS="--lts"
else
    AOT_LTS_FLAGS=""
fi

if [ -f "/.dockerenv" ] || grep -q docker /proc/1/cgroup; then
    # running in a container so use the installed wasi-sdk as the devcontainer has this installed  
    for FILENAME in $(find . -name '*.c' -not -path './components/*')
    do
        echo Building ${FILENAME}
        # Build the wasm file with wasi-libc for wasmtime
        /opt/wasi-sdk/bin/clang ${DEBUG_FLAGS} -flto -ffunction-sections -mexec-model=reactor ${OPT_FLAGS} -z stack-size=4096 -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors ${STRIP_FLAGS} -Wl,--no-entry -Wl,--allow-undefined -Wl,--gc-sections  -o ${OUTPUT_DIR}/${FILENAME%.*}-wasi-libc.wasm ${FILENAME}

        cargo run -p hyperlight-wasm-aot compile ${AOT_DEBUG_FLAGS} ${AOT_LTS_FLAGS} ${OUTPUT_DIR}/${FILENAME%.*}-wasi-libc.wasm ${OUTPUT_DIR}/${FILENAME%.*}.aot
    done

    for WIT_FILE in ${PWD}/components/*.wit; do
        COMPONENT_NAME=$(basename ${WIT_FILE} .wit)
        echo Building component: ${COMPONENT_NAME}

        # Generate bindings for the component
        wit-bindgen c ${WIT_FILE} --out-dir ${PWD}/components/bindings

        # Build the wasm file with wasi-libc for wasmtime
        /opt/wasi-sdk/bin/wasm32-wasip2-clang \
            -ffunction-sections -mexec-model=reactor ${OPT_FLAGS} -z stack-size=4096 \
            -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors \
            ${STRIP_FLAGS} -Wl,--no-entry -Wl,--allow-undefined -Wl,--gc-sections \
            -o ${OUTPUT_DIR}/${COMPONENT_NAME}-p2.wasm \
            ${PWD}/components/${COMPONENT_NAME}.c \
            ${PWD}/components/bindings/${COMPONENT_NAME}.c \
            ${PWD}/components/bindings/${COMPONENT_NAME}_component_type.o

        # Build AOT for Wasmtime
        cargo run -p hyperlight-wasm-aot compile ${AOT_DEBUG_FLAGS} ${AOT_LTS_FLAGS} --component ${OUTPUT_DIR}/${COMPONENT_NAME}-p2.wasm ${OUTPUT_DIR}/${COMPONENT_NAME}.aot
    done

else 
    # not running in a container so use the docker image to build the wasm files
    echo Building docker image that has Wasm sdk. Should be quick if preivoulsy built and no changes to dockerfile.
    echo This will take a while if it is the first time you are building the docker image.
    echo Log in ${OUTPUT_DIR}/dockerbuild.log

    docker pull ghcr.io/hyperlight-dev/wasm-clang-builder:latest

    docker build --build-arg GCC_VERSION=12 --build-arg WASI_SDK_VERSION_FULL=25.0 --cache-from ghcr.io/hyperlight-dev/wasm-clang-builder:latest -t wasm-clang-builder:latest . 2> ${OUTPUT_DIR}/dockerbuild.log

    for FILENAME in $(find . -name '*.c' -not -path './components/*')
    do
        echo Building ${FILENAME}
        OUTPUT_WASM="${OUTPUT_DIR}/${FILENAME%.*}-wasi-libc.wasm"
        ABS_INPUT="$(realpath ${FILENAME})"
        ABS_OUTPUT="$(realpath ${OUTPUT_WASM})"
        INPUT_DIR="$(dirname ${ABS_INPUT})"
        OUTPUT_DIR_REAL="$(dirname ${ABS_OUTPUT})"
        INPUT_BASE="$(basename ${ABS_INPUT})"
        OUTPUT_BASE="$(basename ${ABS_OUTPUT})"
        # Map parent directories to the same path in the container
        docker run --rm -i \
            -v "${INPUT_DIR}:${INPUT_DIR}" \
            -v "${OUTPUT_DIR_REAL}:${OUTPUT_DIR_REAL}" \
            wasm-clang-builder:latest /bin/bash -c "/opt/wasi-sdk/bin/clang ${DEBUG_FLAGS} -flto -ffunction-sections -mexec-model=reactor ${OPT_FLAGS} -z stack-size=4096 -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors ${STRIP_FLAGS} -Wl,--no-entry -Wl,--allow-undefined -Wl,--gc-sections  -o ${ABS_OUTPUT} ${ABS_INPUT}"

        cargo run -p hyperlight-wasm-aot compile ${AOT_DEBUG_FLAGS} ${AOT_LTS_FLAGS} ${OUTPUT_WASM} ${OUTPUT_DIR}/${FILENAME%.*}.aot
    done

    echo Building components
    # Iterate over all .wit files in the components folder
    for WIT_FILE in ${PWD}/components/*.wit; do
        COMPONENT_NAME=$(basename ${WIT_FILE} .wit)
        echo Building component: ${COMPONENT_NAME}

        # Generate bindings for the component
        wit-bindgen c ${WIT_FILE} --out-dir ${PWD}/components/bindings

        COMPONENT_C="${PWD}/components/${COMPONENT_NAME}.c"
        BINDINGS_C="${PWD}/components/bindings/${COMPONENT_NAME}.c"
        BINDINGS_TYPE_O="${PWD}/components/bindings/${COMPONENT_NAME}_component_type.o"
        OUTPUT_WASM="${OUTPUT_DIR}/${COMPONENT_NAME}-p2.wasm"
        ABS_COMPONENT_C="$(realpath ${COMPONENT_C})"
        ABS_BINDINGS_C="$(realpath ${BINDINGS_C})"
        ABS_BINDINGS_TYPE_O="$(realpath ${BINDINGS_TYPE_O})"
        ABS_OUTPUT_WASM="$(realpath ${OUTPUT_WASM})"
        COMPONENT_C_DIR="$(dirname ${ABS_COMPONENT_C})"
        BINDINGS_C_DIR="$(dirname ${ABS_BINDINGS_C})"
        BINDINGS_TYPE_O_DIR="$(dirname ${ABS_BINDINGS_TYPE_O})"
        OUTPUT_WASM_DIR="$(dirname ${ABS_OUTPUT_WASM})"
        # Map all parent directories to the same path in the container
        docker run --rm -i \
            -v "${COMPONENT_C_DIR}:${COMPONENT_C_DIR}" \
            -v "${BINDINGS_C_DIR}:${BINDINGS_C_DIR}" \
            -v "${BINDINGS_TYPE_O_DIR}:${BINDINGS_TYPE_O_DIR}" \
            -v "${OUTPUT_WASM_DIR}:${OUTPUT_WASM_DIR}" \
            wasm-clang-builder:latest /bin/bash -c "/opt/wasi-sdk/bin/wasm32-wasip2-clang \
            -ffunction-sections -mexec-model=reactor ${OPT_FLAGS} -z stack-size=4096 \
            -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors \
            ${STRIP_FLAGS} -Wl,--no-entry -Wl,--allow-undefined -Wl,--gc-sections \
            -o ${ABS_OUTPUT_WASM} \
            ${ABS_COMPONENT_C} \
            ${ABS_BINDINGS_C} \
            ${ABS_BINDINGS_TYPE_O}"

        # Build AOT for Wasmtime
        cargo run -p hyperlight-wasm-aot compile ${AOT_DEBUG_FLAGS} ${AOT_LTS_FLAGS} --component ${OUTPUT_WASM} ${OUTPUT_DIR}/${COMPONENT_NAME}.aot
    done
fi

popd
