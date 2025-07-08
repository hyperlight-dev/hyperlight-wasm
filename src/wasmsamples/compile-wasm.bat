@echo off
SETLOCAL EnableDelayedExpansion
IF "%~1" == "" GOTO Error
IF "%~2" == "" GOTO Error


set "dockercmd=docker"
set "dockerinput=%~1"
set "dockeroutput=%~2"

where docker || (
	set "dockercmd=wsl docker"
	set "dockerinput=$(wslpath '%~1')"
	set "dockeroutput=$(wslpath '%~2')"
)

%dockercmd% pull ghcr.io/hyperlight-dev/wasm-clang-builder:latest

echo Building docker image that has Wasm sdk. Should be quick if no changes to docker image.
echo Log in %2\dockerbuild.log
%dockercmd% build --build-arg GCC_VERSION=12 --build-arg WASI_SDK_VERSION_FULL=25.0 --cache-from ghcr.io/hyperlight-dev/wasm-clang-builder:latest -t wasm-clang-builder:latest !dockerinput! 2> %2dockerbuild.log

echo Building Wasm files in %1 and output to %2
for /R "%1" %%i in (*.c) do (
    echo %%~ni.c
    %dockercmd% run --rm -i -v !dockerinput!:/tmp/host1 -v  !dockeroutput!/:/tmp/host2 wasm-clang-builder /opt/wasi-sdk/bin/clang -flto -ffunction-sections -mexec-model=reactor -O3 -z stack-size=4096 -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors -Wl,--strip-all,--no-entry -Wl,--allow-undefined -Wl,--gc-sections -o /tmp/host2/%%~ni.wasm /tmp/host1/%%~ni.c
    echo  %2\%%~ni.wasm
    rem Build AOT for Wasmtime; note that Wasmtime does not support
    rem interpreting, so its wasm binary is secretly an AOT binary.
    cargo run -p hyperlight-wasm-aot compile %2\%%~ni.wasm  %2\%%~ni.aot 
    copy  %2\%%~ni.aot  %2\%%~ni.wasm
)

echo Building components
for %%j in (%~1\components\*.wit) do (
    set "COMPONENT_NAME=%%~nj"
    echo Building component: !COMPONENT_NAME!

    rem Generate bindings for the component
    wit-bindgen c %%j --out-dir %~1\components\bindings

    rem Build the wasm file with wasi-libc for wasmtime
    %dockercmd% run --rm -i -v !dockerinput!:/tmp/host1 -v !dockeroutput!:/tmp/host2 wasm-clang-builder /opt/wasi-sdk/bin/wasm32-wasip2-clang -ffunction-sections -mexec-model=reactor -O3 -z stack-size=4096 -Wl,--initial-memory=65536 -Wl,--export=__data_end -Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors -Wl,--strip-all,--no-entry -Wl,--allow-undefined -Wl,--gc-sections -o /tmp/host2/!COMPONENT_NAME!-p2.wasm /tmp/host1/components/!COMPONENT_NAME!.c /tmp/host1/components/bindings/!COMPONENT_NAME!.c /tmp/host1/components/bindings/!COMPONENT_NAME!_component_type.o

    rem Build AOT for Wasmtime
    cargo run -p hyperlight-wasm-aot compile --component %2\!COMPONENT_NAME!-p2.wasm %2\!COMPONENT_NAME!.aot
    copy %2\!COMPONENT_NAME!.aot %2\!COMPONENT_NAME!.wasm
)

goto :EOF
:Error
echo Usage - compile-wasm ^<source directory^> ^<output directory^>
