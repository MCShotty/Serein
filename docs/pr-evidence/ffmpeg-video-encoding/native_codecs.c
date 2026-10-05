/* Device-free native bitstream framing tests. These are minimal grammar
 * fixtures, not full encoded pictures or live/hardware compatibility proof. */
#include "../../../crates/discord-voice/src/video_encode_ffmpeg.c"
#include <assert.h>
#include <stdio.h>

int main(void)
{
    static const char *const hardware[][5] = {
        {NULL, "h264_nvenc", "h264_videotoolbox", "h264_amf", "h264_qsv"},
        {NULL, "hevc_nvenc", "hevc_videotoolbox", "hevc_amf", "hevc_qsv"},
        {NULL, "av1_nvenc", NULL, "av1_amf", "av1_qsv"}
    };
    int configured = 0;
    /* Test the real private option tables for every compiled codec. This
     * creates a codec context only: no encoder initialization or device open. */
    for (int kind = 0; kind < 3; kind++) {
        for (int backend = 1; backend <= 4; backend++) {
            const AVCodec *codec;
            if (!hardware[kind][backend])
                continue;
            codec = avcodec_find_encoder_by_name(hardware[kind][backend]);
            if (!codec)
                continue;
            for (int baseline = 0; baseline <= 1; baseline++) {
                SereinAvc encoder = {0};
                encoder.codec = avcodec_alloc_context3(codec);
                assert(encoder.codec);
                encoder.kind = kind;
                encoder.baseline = baseline;
                encoder.max_bytes = 128 * 1024;
                assert(configure_backend(&encoder, backend));
                avcodec_free_context(&encoder.codec);
                configured++;
            }
        }
    }
    printf("Codec hardware options: %d profile/backend combinations accepted; no device opened\n", configured);
    static const uint8_t hevc[] = {
        0,0,0,1, 0x40,1,0x55, /* VPS */
        0,0,0,1, 0x42,1,0x55, /* SPS */
        0,0,0,1, 0x44,1,0x55, /* PPS */
        0,0,0,1, 0x26,1,0x55  /* IDR */
    };
    static const uint8_t bad_hevc[] = {0,0,1,0x26,0,0x55};
    static const uint8_t hevc_delta[] = {0,0,1,0x02,1,0x55};
    static const uint8_t av1_key[] = {0x0a,1,0, 0x1a,1,0x10};
    static const uint8_t av1_reduced[] = {0x0a,1,0x18, 0x1a,1,0x20};
    static const uint8_t av1_inter[] = {0x0a,1,0, 0x1a,1,0x30};
    static const uint8_t av1_intra_only[] = {0x0a,1,0, 0x1a,1,0x50};
    static const uint8_t av1_existing[] = {0x0a,1,0, 0x1a,1,0x80};
    static const uint8_t bad_size[] = {0x32,0x80,0x80,0x80,0x80,0x80,0x80,0x80,0x80};
    static const uint8_t truncated_size[] = {0x32,0x80};
    static const uint8_t truncated_payload[] = {0x32,20,0x10};
    static const uint8_t bad_reserved[] = {0x33,1,0x10};
    static const uint8_t bad_extension[] = {0x36,1,1,0x10};
    static const uint8_t missing_size[] = {0x30,0x10};
    static const uint8_t late_sequence[] = {0x1a,1,0x10, 0x0a,1,0};
    PacketInfo info;
    int reduced = 0;
    assert(inspect_annex_b(hevc, sizeof(hevc), 1, &info));
    assert(info.picture && info.keyframe && info.key_parameters == 7);
    assert(!inspect_annex_b(bad_hevc, sizeof(bad_hevc), 1, &info));
    assert(inspect_annex_b(hevc_delta, sizeof(hevc_delta), 1, &info));
    assert(info.picture && !info.keyframe);
    assert(inspect_av1(av1_key, sizeof(av1_key), &reduced, &info));
    assert(info.picture && info.keyframe && info.key_parameters == 1 && !reduced);
    assert(inspect_av1(av1_reduced, sizeof(av1_reduced), &reduced, &info));
    assert(info.picture && info.keyframe && reduced);
    assert(inspect_av1(av1_inter, sizeof(av1_inter), &reduced, &info));
    assert(info.picture && !info.keyframe && !reduced);
    assert(inspect_av1(av1_intra_only, sizeof(av1_intra_only), &reduced, &info));
    assert(info.picture && !info.keyframe);
    assert(inspect_av1(av1_existing, sizeof(av1_existing), &reduced, &info));
    assert(info.picture && !info.keyframe);
    assert(!inspect_av1(bad_size, sizeof(bad_size), &reduced, &info));
    assert(!inspect_av1(truncated_size, sizeof(truncated_size), &reduced, &info));
    assert(!inspect_av1(truncated_payload, sizeof(truncated_payload), &reduced, &info));
    assert(!inspect_av1(bad_reserved, sizeof(bad_reserved), &reduced, &info));
    assert(!inspect_av1(bad_extension, sizeof(bad_extension), &reduced, &info));
    assert(!inspect_av1(missing_size, sizeof(missing_size), &reduced, &info));
    assert(inspect_av1(late_sequence, sizeof(late_sequence), &reduced, &info));
    assert(info.keyframe && info.parameters == 1 && info.key_parameters == 0);
    puts("HEVC/AV1 framing: parameters precede key pictures; malformed lengths/headers rejected");
    return 0;
}
