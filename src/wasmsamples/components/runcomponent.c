#include "bindings/runcomponent.h"
#include <stdlib.h>
#include <string.h>

void exports_example_runcomponent_guest_echo(runcomponent_string_t *msg, runcomponent_string_t *ret)
{
  ret->len = msg->len;
  ret->ptr = (uint8_t *) malloc(ret->len);
  memcpy(ret->ptr, msg->ptr, ret->len);
  runcomponent_string_free(msg);
}