@echo off
pushd %~dp0\..\..\wasmsamples
call :NORMALIZEPATH "..\..\x64\%1"
echo "%ABSPATH%"
call compile-wasm.bat "." "%ABSPATH%"
popd

:: ========== FUNCTIONS ==========
exit /B

:NORMALIZEPATH
  set ABSPATH=%~f1
  exit /B
