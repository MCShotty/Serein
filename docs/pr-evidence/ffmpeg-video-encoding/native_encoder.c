#include "video_encode_ffmpeg.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CAMERA_LIMIT ((size_t)128 * 1024)
#define SCREEN_LIMIT ((size_t)2 * 1024 * 1024)

static void fill_i420(unsigned char *picture, int width, int height, int index)
{
    size_t pixels = (size_t)width * (size_t)height;
    memset(picture, 64, pixels);
    memset(picture + pixels, 128, pixels / 2);
    for (int row = 8; row < 24; row++) {
        for (int column = 8 + index; column < 24 + index; column++)
            picture[(size_t)row * (size_t)width + (size_t)column] = 180;
    }
}

static int sps_profile(const unsigned char *packet, size_t length)
{
    for (size_t i = 0; i + 5 < length; i++) {
        if (packet[i] == 0 && packet[i + 1] == 0 && packet[i + 2] == 1 &&
            (packet[i + 3] & 31) == 7)
            return packet[i + 4];
    }
    return -1;
}

static void exercise_stream(int baseline, int width, int height, int fps,
                            int bitrate, size_t limit, const char *path)
{
    void *encoder = serein_avc_open(width, height, fps, bitrate, baseline, 0, limit);
    size_t input_bytes = (size_t)width * (size_t)height * 3 / 2;
    unsigned char *picture = malloc(input_bytes);
    unsigned char *output = malloc(limit + 32);
    FILE *file = fopen(path, "wb");
    int idrs = 0, deltas = 0;
    assert(encoder && picture && output && file);
    fill_i420(picture, width, height, 0);
    for (size_t invalid = 0; invalid < 2; invalid++) {
        size_t produced = 99;
        int keyframe = 99;
        memset(output, 0xa5, limit + 32);
        assert(serein_avc_encode(encoder, picture,
                                invalid ? input_bytes + 1 : input_bytes - 1,
                                1, output, limit, &produced, &keyframe) == -1);
        assert(produced == 0 && keyframe == 0);
        for (size_t i = 0; i < limit + 32; i++)
            assert(output[i] == 0xa5);
    }
    for (int frame = 0; frame < 8; frame++) {
        size_t produced = 99;
        int keyframe = 99;
        fill_i420(picture, width, height, frame);
        memset(output, 0xa5, limit + 32);
        int status = serein_avc_encode(encoder, picture, input_bytes, frame == 4,
                                       output, limit, &produced, &keyframe);
        assert(status == 1 && produced > 0 && produced <= limit);
        for (size_t i = limit; i < limit + 32; i++)
            assert(output[i] == 0xa5);
        if (baseline || frame == 0 || frame == 4) {
            assert(keyframe == 1);
            assert(sps_profile(output, produced) == (baseline ? 66 : 77));
            idrs++;
        } else {
            assert(keyframe == 0);
            deltas++;
        }
        assert(fwrite(output, 1, produced, file) == produced);
    }
    assert(fclose(file) == 0);
    printf("%s: 8 packets; IDR=%d delta=%d; SPS profile=%d\n",
           baseline ? "camera" : "screen", idrs, deltas, baseline ? 66 : 77);
    serein_avc_close(encoder);
    free(picture);
    free(output);
}

static void exercise_rejected_configuration(void)
{
    const int invalid[][7] = {
        {0, 480, 30, 1500000, 1, 0, 128 * 1024},
        {640, -1, 30, 1500000, 1, 0, 128 * 1024},
        {641, 480, 30, 1500000, 1, 0, 128 * 1024},
        {640, 481, 30, 1500000, 1, 0, 128 * 1024},
        {1922, 1080, 30, 6000000, 0, 0, 128 * 1024},
        {1920, 1082, 30, 6000000, 0, 0, 128 * 1024},
        {640, 480, 0, 1500000, 1, 0, 128 * 1024},
        {640, 480, 61, 1500000, 1, 0, 128 * 1024},
        {640, 480, 30, 0, 1, 0, 128 * 1024},
        {640, 480, 30, INT_MAX, 1, 0, 128 * 1024},
        {640, 480, 30, 1500000, 2, 0, 128 * 1024},
        {640, 480, 30, 1500000, 1, -1, 128 * 1024},
        {640, 480, 30, 1500000, 1, 5, 128 * 1024},
        {640, 480, 30, 1500000, 1, 0, 0},
        {640, 480, 30, 1500000, 1, 0, 2 * 1024 * 1024 + 1}
    };
    for (size_t i = 0; i < sizeof(invalid) / sizeof(*invalid); i++) {
        const int *item = invalid[i];
        assert(serein_avc_open(item[0], item[1], item[2], item[3], item[4],
                               item[5], (size_t)item[6]) == NULL);
    }
    assert(serein_avc_open(640, 480, 30, 1500000, 1, 0, SIZE_MAX) == NULL);
    size_t produced = 99;
    int keyframe = 99;
    unsigned char bytes[32] = {0};
    assert(serein_avc_encode(NULL, bytes, 32, 0, bytes, 32,
                             &produced, &keyframe) == -1);
    assert(produced == 0 && keyframe == 0);
    serein_avc_close(NULL);
    puts("configuration/null checks: 17 rejected requests; outputs reset");
}

static void exercise_packet_caps(void)
{
    size_t input_bytes = (size_t)640 * 480 * 3 / 2;
    unsigned char *picture = malloc(input_bytes);
    unsigned char output[32];
    assert(picture);
    fill_i420(picture, 640, 480, 0);
    for (int tiny_native_limit = 0; tiny_native_limit < 2; tiny_native_limit++) {
        void *encoder = serein_avc_open(640, 480, 30, 1500000, 1, 0,
                                        tiny_native_limit ? 1 : CAMERA_LIMIT);
        size_t produced = 99;
        int keyframe = 99;
        assert(encoder);
        memset(output, 0xa5, sizeof(output));
        assert(serein_avc_encode(encoder, picture, input_bytes, 1, output, 1,
                                 &produced, &keyframe) == -1);
        assert(produced == 0 && keyframe == 0);
        for (size_t i = 0; i < sizeof(output); i++)
            assert(output[i] == 0xa5);
        assert(serein_avc_encode(encoder, picture, input_bytes, 1, output,
                                 sizeof(output), &produced, &keyframe) == -1);
        serein_avc_close(encoder);
    }
    free(picture);
    puts("packet caps: 1-byte native/output bounds reject without writing");
}

int main(void)
{
    exercise_rejected_configuration();
    exercise_packet_caps();
    exercise_stream(1, 640, 480, 30, 1500000, CAMERA_LIMIT,
                    "camera.h264");
    exercise_stream(0, 320, 180, 30, 2000000, SCREEN_LIMIT,
                    "screen.h264");
    return 0;
}
