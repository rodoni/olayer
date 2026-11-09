#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "libolayer_native.h"

static const uint8_t request[] =
    "{\"version\":1,\"operation\":{\"kind\":\"wms\",\"endpoint\":{\"base\":\"https://example.test\",\"path\":\"/ows?tenant=one#map\"},\"layers\":\"roads\",\"styles\":\"\",\"bounds\":{\"epsg\":4326,\"min_x\":10,\"min_y\":20,\"max_x\":30,\"max_y\":40},\"width\":256,\"height\":128,\"format\":\"image/png\",\"transparency\":\"transparent\"}}";

int main(void) {
    size_t required = 0;
    int error_code = -1;
    const int query_status = olayer_map_build_request_json(
        request, sizeof(request) - 1, NULL, 0, &required, &error_code);
    if (query_status != 1 || required == 0 || error_code != 0) return 1;

    uint8_t *output = malloc(required + 1);
    if (output == NULL) return 2;
    memset(output, 0xa5, required + 1);
    const int short_status = olayer_map_build_request_json(
        request, sizeof(request) - 1, output, required - 1, &required, &error_code);
    if (short_status != 1 || output[0] != 0xa5) { free(output); return 3; }

    const int status = olayer_map_build_request_json(
        request, sizeof(request) - 1, output, required, &required, &error_code);
    if (status != 0 || error_code != 0) { free(output); return 4; }
    output[required] = 0;
    if (strstr((const char *)output, "BBOX=20%2C10%2C40%2C30") == NULL ||
        strstr((const char *)output, "#map") == NULL) { free(output); return 5; }
    free(output);

    const uint8_t bad_version[] = "{\"version\":2,\"operation\":{}}";
    const int error_status = olayer_map_build_request_json(
        bad_version, sizeof(bad_version) - 1, NULL, 0, &required, &error_code);
    if (error_status != -2 || error_code != 12) return 6;

    puts("Map Core C consumer passed");
    return 0;
}
