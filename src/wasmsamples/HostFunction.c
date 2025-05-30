/*
Copyright 2024 The Hyperlight Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

#include <stdint.h>

int HostFuncWithBufferAndLength(const char* buffer, int length); // Implementation of this will be available in the host

__attribute__((export_name("PassBufferAndLengthToHost")))
int PassBufferAndLengthToHost()
{
    const char* helloWorld = "Hello World!";
    int helloWorldLength = 12; // Length of "Hello World!" excluding null terminator
    return HostFuncWithBufferAndLength(helloWorld, helloWorldLength);
}
