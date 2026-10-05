#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include SHIM

static void check_options(int normal) {
    const char *names[3][5] = {
        {"libopenh264", "h264_nvenc", "h264_videotoolbox", "h264_amf", "h264_qsv"},
        {NULL, "hevc_nvenc", "hevc_videotoolbox", "hevc_amf", "hevc_qsv"},
        {NULL, "av1_nvenc", NULL, "av1_amf", "av1_qsv"}
    };
    int checked = 0;
    for (int kind = 0; kind < 3; kind++) for (int backend = 0; backend < 5; backend++) {
        if (!names[kind][backend]) continue;
        const AVCodec *implementation = avcodec_find_encoder_by_name(names[kind][backend]);
        if (!implementation) continue;
        for (int baseline = 0; baseline < 2; baseline++) {
            SereinAvc encoder = {.kind = kind, .baseline = baseline, .max_bytes = 128 * 1024};
            encoder.codec = avcodec_alloc_context3(implementation);
            assert(encoder.codec && configure_backend(&encoder, backend));
            const char *keys[3] = {NULL}, *values[3] = {NULL};
            if (backend == 1) {
                keys[0] = "tune"; values[0] = normal ? "hq" : "ull";
                keys[1] = "zerolatency"; values[1] = normal ? "0" : "1";
            } else if (backend == 3) {
                keys[0] = "usage"; values[0] = normal ? "transcoding" : "ultralowlatency";
                keys[1] = "quality"; values[1] = normal ? "balanced" : "speed";
                keys[2] = "latency"; values[2] = normal ? (kind == 2 ? "none" : "0") : (kind == 2 ? "lowest_latency" : "1");
            } else if (backend == 4) {
                keys[0] = "preset"; values[0] = normal ? "medium" : "veryfast";
                keys[1] = "async_depth"; values[1] = normal ? "4" : "1";
            } else if (backend == 2) {
                keys[0] = "realtime"; values[0] = normal ? "0" : "1";
            }
            for (int index = 0; index < 3 && keys[index]; index++) {
                int expected;
                int64_t actual;
                const AVOption *option = av_opt_find(encoder.codec->priv_data, keys[index], NULL, 0, 0);
                assert(option);
                if (option->type == AV_OPT_TYPE_BOOL) expected = atoi(values[index]);
                else assert(av_opt_eval_int(encoder.codec->priv_data, option, values[index], &expected) >= 0);
                assert(av_opt_get_int(encoder.codec->priv_data, keys[index], 0, &actual) >= 0);
                assert(actual == expected);
            }
            avcodec_free_context(&encoder.codec);
            checked++;
        }
    }
    printf("validated backend/codec/profile configurations=%d\n", checked);
    assert(checked >= 20);
}

static double elapsed(struct timespec start) {
    struct timespec end;
    assert(clock_gettime(CLOCK_MONOTONIC, &end) == 0);
    return (end.tv_sec - start.tv_sec) * 1000.0 + (end.tv_nsec - start.tv_nsec) / 1000000.0;
}

static void check_encoding(int baseline, const char *path) {
    const size_t picture_bytes = 640 * 480 * 3 / 2, capacity = 128 * 1024;
    void *opaque = serein_avc_open(640, 480, 15, 600000, baseline, 0, 0, capacity);
    assert(opaque);
    SereinAvc *encoder = opaque;
    if (NORMAL) assert(!(encoder->codec->flags & AV_CODEC_FLAG_LOW_DELAY));
    uint8_t *picture = malloc(picture_bytes), *output = malloc(capacity);
    assert(picture && output);
    for (size_t i = 0; i < 640 * 480; i++) picture[i] = 16 + ((i % 640 + i / 640) % 220);
    memset(picture + 640 * 480, 128, picture_bytes - 640 * 480);
    FILE *stream = fopen(path, "wb"); assert(stream);
    size_t total = 0;
    struct timespec start;
    for (int i = 0; i < 330; i++) {
        size_t length = 999;
        int keyframe = -1;
        if (i == 30) assert(clock_gettime(CLOCK_MONOTONIC, &start) == 0);
        int force = i == 77;
        assert(serein_avc_encode(opaque, picture, picture_bytes, force, output, capacity, &length, &keyframe) == 1);
        assert(length > 0 && length <= capacity);
        if (baseline || force || i == 0) assert(keyframe);
        if (i >= 30) total += length;
        assert(fwrite(output, 1, length, stream) == length);
    }
    printf("%s encode_300_ms=%.3f bytes=%zu\n", baseline ? "camera" : "screen", elapsed(start), total);
    fclose(stream);
    size_t length = 1; int keyframe = 1;
    assert(serein_avc_encode(opaque, picture, picture_bytes - 1, 0, output, capacity, &length, &keyframe) == -1);
    assert(length == 0 && keyframe == 0);
    serein_avc_close(opaque);
    free(picture); free(output);
}

int main(int argc, char **argv) {
    assert(argc == 3);
    check_options(NORMAL);
    check_encoding(1, argv[1]);
    check_encoding(0, argv[2]);
    return 0;
}
