/* Include the translation unit to exercise private helpers without exposing
 * production test symbols or opening an encoder/device. Compile this file on
 * its own, linked to Serein's private avcodec/avutil libraries. */
#include "../../../crates/discord-voice/src/video_encode_ffmpeg.c"
#include <assert.h>
#include <stdio.h>

int main(void)
{
    static const char *const hardware[] = {
        NULL, "h264_nvenc", "h264_videotoolbox", "h264_amf", "h264_qsv"
    };
    int configured = 0;
    /* Exercise the pinned libraries' actual option names, enum values and
     * ranges without initializing a device or claiming SDK execution. */
    for (int backend = 1; backend <= 4; backend++) {
        const AVCodec *codec = avcodec_find_encoder_by_name(hardware[backend]);
        if (!codec)
            continue;
        for (int baseline = 0; baseline <= 1; baseline++) {
            SereinAvc encoder = {0};
            encoder.codec = avcodec_alloc_context3(codec);
            assert(encoder.codec);
            encoder.baseline = baseline;
            encoder.max_bytes = 128 * 1024;
            assert(configure_backend(&encoder, backend));
            avcodec_free_context(&encoder.codec);
            configured++;
        }
    }
    printf("Hardware options: %d profile/backend combinations accepted; no device opened\n", configured);
    static const uint8_t input[] = {
        0,1,2,3,4,5, 6,7,8,9,10,11, 12,13,14,15,16,17, 18,19,20,21,22,23,
        10,20,30,40,50,60, 110,120,130,140,150,160
    };
    static const uint8_t expected_uv[][6] = {
        {10,110,20,120,30,130}, {40,140,50,150,60,160}
    };
    AVFrame *frame = av_frame_alloc();
    assert(frame);
    frame->format = AV_PIX_FMT_NV12;
    frame->width = 6;
    frame->height = 4;
    assert(av_frame_get_buffer(frame, 32) == 0);
    assert(frame->linesize[0] > 6 && frame->linesize[1] > 6);
    for (int plane = 0; plane < AV_NUM_DATA_POINTERS; plane++) {
        if (frame->buf[plane])
            memset(frame->buf[plane]->data, 0xa5, frame->buf[plane]->size);
    }
    copy_picture(frame, input);
    for (size_t row = 0; row < 4; row++) {
        const uint8_t *actual = frame->data[0] + row * (size_t)frame->linesize[0];
        assert(memcmp(actual, input + row * 6, 6) == 0);
        for (size_t column = 6; column < (size_t)frame->linesize[0]; column++)
            assert(actual[column] == 0xa5);
    }
    for (size_t row = 0; row < 2; row++) {
        const uint8_t *actual = frame->data[1] + row * (size_t)frame->linesize[1];
        assert(memcmp(actual, expected_uv[row], 6) == 0);
        for (size_t column = 6; column < (size_t)frame->linesize[1]; column++)
            assert(actual[column] == 0xa5);
    }
    av_frame_free(&frame);

    AVPacket *packet = av_packet_alloc();
    assert(packet && av_new_packet(packet, 16) == 0);
    assert(packet_fits(packet, 1024, 1024));
    /* Forged driver DataLength stays below the configured transport cap but
     * exceeds the actual 16-byte allocation. It must fail before a read. */
    packet->size = 32;
    assert(!packet_fits(packet, 1024, 1024));
    packet->size = 16;
    assert(!packet_fits(packet, 8, 1024));
    assert(!packet_fits(packet, 1024, 8));
    packet->data++;
    assert(!packet_fits(packet, 1024, 1024));
    packet->data--;
    av_packet_free(&packet);
    puts("NV12: luma/U/V packing and padding intact; forged packet lengths rejected");
    return 0;
}
