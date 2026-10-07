/* Exercise the production shim's delayed/reordered packet ABI without drivers. */
#define avcodec_send_frame fixture_send
#define avcodec_receive_packet fixture_receive
#include "video_encode_ffmpeg.c"
#undef avcodec_send_frame
#undef avcodec_receive_packet
#include <assert.h>

static int submitted, emitted, never_output;
static const int64_t decode_order[] = {0, 3, 1, 2};

int fixture_send(AVCodecContext *codec, const AVFrame *frame)
{
    (void)codec;
    assert(frame->pts == submitted);
    submitted++;
    return 0;
}

int fixture_receive(AVCodecContext *codec, AVPacket *packet)
{
    (void)codec;
    if (never_output || submitted <= 16 || emitted >= 4)
        return AVERROR(EAGAIN);
    const uint8_t idr[] = {0,0,1,0x67,0x80,0,0,1,0x68,0x80,0,0,1,0x65,0x88};
    const uint8_t delta[] = {0,0,1,0x41,0x88};
    const size_t bytes = emitted ? sizeof(delta) : sizeof(idr);
    assert(av_new_packet(packet, (int)bytes) == 0);
    memcpy(packet->data, emitted ? delta : idr, bytes);
    packet->pts = decode_order[emitted];
    if (!emitted)
        packet->flags |= AV_PKT_FLAG_KEY;
    emitted++;
    return 0;
}

/* Configuration fixtures never identify or load a physical GPU. */
int serein_video_bind_adapter(AVCodecContext *codec, int backend, const SereinVideoAdapter *adapter)
{ (void)codec; (void)backend; (void)adapter; assert(0); return 0; }
int serein_nvenc_features(int codec, const SereinVideoAdapter *adapter, int *b, int *lookahead)
{ (void)codec; (void)adapter; *b = 2; *lookahead = 1; return 1; }

static void check_presets(void)
{
    const char *names[] = {"h264_nvenc", "hevc_nvenc", "av1_nvenc"};
    for (int kind = 0; kind < 3; kind++) {
        const AVCodec *implementation = avcodec_find_encoder_by_name(names[kind]);
        if (!implementation)
            continue;
        AVCodecContext *codec = avcodec_alloc_context3(implementation);
        assert(codec);
        SereinAvc encoder = {.codec = codec, .kind = kind, .features = kind ? 3 : 2};
        assert(configure_backend(&encoder, 1));
        assert(codec->refs == (kind ? 0 : 1));
        int64_t value;
        const AVOption *p5 = av_opt_find(codec->priv_data, "p5", "preset", 0, 0);
        assert(p5 && av_opt_get_int(codec->priv_data, "preset", 0, &value) == 0);
        assert(value == p5->default_val.i64);
        assert(av_opt_get_int(codec->priv_data, "rc-lookahead", 0, &value) == 0 && value == 16);
        assert(av_opt_get_int(codec->priv_data, "surfaces", 0, &value) == 0 && value == 24);
        avcodec_free_context(&codec);
    }
    const char *qsv[] = {"h264_qsv", "hevc_qsv", "av1_qsv"};
    for (int kind = 0; kind < 3; kind++) {
        const AVCodec *implementation = avcodec_find_encoder_by_name(qsv[kind]);
        if (!implementation)
            continue;
        AVCodecContext *codec = avcodec_alloc_context3(implementation);
        assert(codec);
        SereinAvc encoder = {.codec = codec, .kind = kind, .features = kind ? 3 : 2};
        assert(configure_backend(&encoder, 4));
        assert(codec->refs == (kind ? 0 : 1));
        avcodec_free_context(&codec);
    }
}

int main(void)
{
    check_presets();
    uint8_t input[64 * 64 * 3 / 2] = {0}, output[4096];
    SereinVideoAdapter unknown = {0};
    /* A supplied unidentified renderer must still permit software fallback. */
    void *encoder = serein_avc_open_on_adapter(64, 64, 30, 100000, 0, 0, 0, sizeof(output), &unknown, 0);
    assert(encoder);
    for (int index = 0; index < 20; index++) {
        size_t bytes = 99;
        int keyframe = 99;
        int64_t pts = 99;
        int result = serein_avc_encode_timed(encoder, input, sizeof(input), 0, output, sizeof(output), &bytes, &keyframe, &pts);
        if (index < 16) {
            assert(result == 0 && bytes == 0 && keyframe == 0 && pts == -1);
        } else {
            assert(result == 1 && bytes > 0 && pts == decode_order[index - 16]);
            assert(keyframe == (index == 16));
        }
    }
    serein_avc_close(encoder);
    submitted = emitted = 0;
    never_output = 1;
    encoder = serein_avc_open(64, 64, 30, 100000, 0, 0, 0, sizeof(output));
    assert(encoder);
    for (int index = 0; index <= 48; index++) {
        size_t bytes = 99;
        int keyframe = 99;
        int64_t pts = 99;
        const int result = serein_avc_encode_timed(encoder, input, sizeof(input), 0, output, sizeof(output), &bytes, &keyframe, &pts);
        assert(result == (index < 48 ? 0 : -1));
        assert(bytes == 0 && keyframe == 0 && pts == -1);
    }
    assert(submitted == 48);
    serein_avc_close(encoder);
    assert(!serein_avc_open_on_adapter(64, 64, 30, 100000, 0, 1, 0, sizeof(output), NULL, 1));
    puts("NVENC P5/lookahead options, delayed/reordered PTS, software fallback and 48-picture bound passed");
    return 0;
}
