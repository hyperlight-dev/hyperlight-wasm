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
#include <string.h>
#include <limits.h>
#include <stdint.h>
#include <ctype.h>

int HostPrint(char* msg); // Implementation of this will be available in the native host

int64_t GetTimeSinceBootMicrosecond();

__attribute__((export_name("CalcFib")))
int CalcFib(int n)
{
    if (n == 0 || n == 1) {
        return n;
    }
    else {
        return CalcFib(n - 1) + CalcFib(n - 2);
    }
}

// This function receives an array of bytes followed by a length and then returns a buffer where the first 4 bytes is the length followed by the data.
__attribute__((export_name("ReceiveByteArray")))
void* ReceiveByteArray(void* data, int length) {
  
    void * result = malloc(length + 4);
    memcpy(result, &length, 4);
    result += 4;
    memcpy(result, data, length);
    result -= 4;
    free(data); // Free the guest parameter, which we own
    return result; // Transfer ownership of return value to host
}

__attribute__((export_name("WasmPrintUsingHostPrint")))
int WasmPrintUsingHostPrint(char* msg)
{
    HostPrint(msg); // Host borrows msg
    int len = strlen(msg);
    free(msg); // Free the param since we own
    return len;
}

__attribute__((export_name("PrintHelloWorld")))
void PrintHelloWorld()
{
	printf("Hello World from Wasm!\n");
}


__attribute__((export_name("Print")))
void Print(char* msg)
{
    HostPrint(msg);
    free(msg); // Free the msg since we own it
}

__attribute__((export_name("Echo")))
char* Echo(char* msg)
{
    return msg; // Transfer ownership to host
}

__attribute__((export_name("ToUpper"), optnone))
char* ToUpper(char* msg)
{
    int len = strlen(msg);
    if (len)
    {
        char* ret = msg;
        while (*msg)
        {
            *msg = toupper((unsigned char)*msg);
            msg++;
        }
        return ret;
    }
    return 0;
}

__attribute__((export_name("PrintUpper"), optnone))
void PrintUpper(char* msg)
{
    HostPrint(ToUpper(msg));
    free(msg);
}

__attribute__((export_name("KeepCPUBusy")))
int KeepCPUBusy(int ms)
{
    int64_t start, end;
    double duration = 0.0;
    /* Store start time here */
    start = GetTimeSinceBootMicrosecond();
    int iter = 0;
    int fib = 0;

    while (1) {
        fib = CalcFib(10);
        end = GetTimeSinceBootMicrosecond();
        duration = (double)(end - start) / 1000.0;
        if (duration >= ms) {
            break;
        }
        iter++;
        if (iter == INT_MAX) {
            printf("Reached int max -reset i");
            iter = 0;
        }
    }
    
    printf("Kept CPU busy for %d ms using %d iterations of fib(10) %d|toreach max = %d|", ms, iter, INT_MAX, INT_MAX-iter);
    return ms;
}
