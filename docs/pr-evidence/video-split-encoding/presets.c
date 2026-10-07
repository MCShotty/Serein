/* Offline setup-cost comparison. No codec is opened and no driver is loaded.
 * Compile with SHIM naming each revision's production shim, and SPLIT_ENABLED
 * only for the changed revision. Link each matching FFmpeg diagnostic build. */
#define _POSIX_C_SOURCE 200809L
#include SHIM
#include <assert.h>
#include <stdio.h>
#include <time.h>

int serein_video_bind_adapter(AVCodecContext *codec, int backend, const SereinVideoAdapter *adapter)
{ (void)codec; (void)backend; (void)adapter; assert(0); return 0; }
int serein_nvenc_features(int codec, const SereinVideoAdapter *adapter, int *b, int *lookahead)
{ (void)codec; (void)adapter; assert(0); *b = *lookahead = 0; return 0; }

int main(void)
{
    const char *names[][2] = {{"hevc_nvenc", "av1_nvenc"}, {"hevc_amf", "av1_amf"}, {"hevc_qsv", "av1_qsv"}};
    const int backends[] = {1, 3, 4};
    struct timespec start, stop;
    assert(clock_gettime(CLOCK_MONOTONIC, &start) == 0);
    for (int iteration = 0; iteration < 10000; iteration++) {
        for (int backend = 0; backend < 3; backend++) {
            for (int kind = 1; kind <= 2; kind++) {
                const AVCodec *implementation = avcodec_find_encoder_by_name(names[backend][kind - 1]);
                assert(implementation);
                AVCodecContext *codec = avcodec_alloc_context3(implementation);
                assert(codec);
                codec->width = 2560; codec->height = 1440;
                SereinAvc encoder = {.codec = codec, .kind = kind, .features = 2, .max_bytes = 2097152};
#ifdef SPLIT_ENABLED
                encoder.split_requested = 1;
#endif
                assert(configure_backend(&encoder, backends[backend]));
                avcodec_free_context(&codec);
            }
        }
    }
    assert(clock_gettime(CLOCK_MONOTONIC, &stop) == 0);
    double milliseconds = (stop.tv_sec - start.tv_sec) * 1000.0 + (stop.tv_nsec - start.tv_nsec) / 1000000.0;
    printf("{\"contexts\":60000,\"elapsed_ms\":%.6f}\n", milliseconds);
}
