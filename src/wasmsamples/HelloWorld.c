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

#include <stdio.h>
#include <stdlib.h>

__attribute__((export_name("Hello"))) 
int Hello()
{
    printf("Hello from Wasm in Hyperlight \n");
    return 0;
}


__attribute__((export_name("HelloWorld"))) 
int HelloWorld(char* msg)
{
    printf("%s\n", msg);

    char* buf = malloc(1024);
    if (!buf) {
        printf("%s", "malloc buf failed\n");
        return -1;
    }

    printf("buffer address: %p\n", buf);

    snprintf(buf, 1024, "%s", "1234");
    printf("contents of buffer after snprintf: %s\n", buf);

    free(buf);

    return 0;
}
