#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include SHIM

/* Software-only measurement. No device binding/query may run. */
int serein_video_bind_adapter(AVCodecContext *codec, int backend, const SereinVideoAdapter *adapter)
{ (void)codec; (void)backend; (void)adapter; assert(0); return 0; }
int serein_nvenc_features(int codec, const SereinVideoAdapter *adapter, int *b, int *lookahead)
{ (void)codec; (void)adapter; (void)b; (void)lookahead; assert(0); return -1; }

int main(int argc, char **argv)
{
    assert(argc == 3);
    const int baseline = atoi(argv[1]);
    const size_t size = 640 * 480 * 3 / 2, capacity = 128 * 1024;
    void *encoder = serein_avc_open(640, 480, 15, 600000, baseline, 0, 0, capacity);
    assert(encoder);
    uint8_t *input = malloc(size), *output = malloc(capacity);
    assert(input && output);
    memset(input + 640 * 480, 128, size - 640 * 480);
    FILE *stream = fopen(argv[2], "wb");
    assert(stream);
    struct timespec start, end;
    size_t bytes = 0;
    unsigned int keyframes = 0;
    for (int frame = 0; frame < 330; frame++) {
        for (size_t pixel = 0; pixel < 640 * 480; pixel++)
            input[pixel] = (uint8_t)(16 + ((pixel % 640 + pixel / 640 + (size_t)frame) % 220));
        if (frame == 30)
            assert(clock_gettime(CLOCK_MONOTONIC, &start) == 0);
        size_t length = 0;
        int keyframe = 0;
        assert(serein_avc_encode(encoder, input, size, frame == 0, output, capacity, &length, &keyframe) == 1);
        assert(length > 0 && length <= capacity);
        if (frame >= 30) {
            bytes += length;
            keyframes += keyframe != 0;
        }
        assert(fwrite(output, 1, length, stream) == length);
    }
    assert(clock_gettime(CLOCK_MONOTONIC, &end) == 0);
    printf("{\"elapsed_ms\":%.6f,\"encoded_bytes\":%zu,\"keyframes\":%u}\n",
        (end.tv_sec - start.tv_sec) * 1000.0 + (end.tv_nsec - start.tv_nsec) / 1000000.0,
        bytes, keyframes);
    assert(fclose(stream) == 0);
    serein_avc_close(encoder);
    free(input);
    free(output);
    return 0;
}
