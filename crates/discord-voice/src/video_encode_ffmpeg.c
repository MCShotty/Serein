/* Bounded FFmpeg H.264 encoding, shared by the camera and screen workers.
 * This translation unit uses only libavcodec/libavutil: it opens no devices,
 * containers, capture sessions or network transports. Its three explicit
 * encoder names cannot select Media Foundation, VA-API or a GPL encoder.
 */
#include "video_encode_ffmpeg.h"

#include <errno.h>
#include <limits.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>

#define SEREIN_MAX_PACKET_BYTES ((size_t)2 * 1024 * 1024)
#define SEREIN_MAX_IN_FLIGHT 4
#define SEREIN_MAX_NALS 2048

typedef struct SereinAvc {
    AVCodecContext *codec;
    AVFrame *frame;
    AVPacket *packet;
    size_t input_bytes;
    size_t max_bytes;
    int64_t next_pts;
    int in_flight;
    int baseline;
    int failed;
} SereinAvc;

/* This callback reads immutable state only, including if libavcodec invokes
 * it from an encoder thread. Reject the size before the default allocator or
 * FFmpeg's codec-specific packet-copy routine can touch the payload. */
static int bounded_encode_buffer(AVCodecContext *codec, AVPacket *packet,
                                 int flags)
{
    const SereinAvc *encoder = codec->opaque;
    if (!encoder || packet->size <= 0 ||
        (size_t)packet->size > encoder->max_bytes)
        return AVERROR(ENOSPC);
    return avcodec_default_get_encode_buffer(codec, packet, flags);
}

static int set_option(AVCodecContext *codec, const char *name,
                      const char *value)
{
    return av_opt_set(codec->priv_data, name, value, 0) >= 0;
}

static int configure_backend(SereinAvc *encoder, int backend)
{
    AVCodecContext *codec = encoder->codec;
    if (backend == 0) {
        /* OpenH264 2.6 defaults to CAMERA_VIDEO_REAL_TIME / LOW_COMPLEXITY.
         * FFmpeg preserves these defaults. Avoid skipped frames so input and
         * output accounting stays bounded and every camera picture is an IDR. */
        return set_option(codec, "profile", encoder->baseline ?
                          "constrained_baseline" : "main") &&
               set_option(codec, "coder", encoder->baseline ? "cavlc" : "cabac") &&
               set_option(codec, "rc_mode", "bitrate") &&
               set_option(codec, "allow_skip_frames", "0");
    }
    if (backend == 1) {
        return set_option(codec, "profile", encoder->baseline ? "baseline" : "main") &&
               set_option(codec, "preset", "p4") &&
               set_option(codec, "tune", "ull") &&
               set_option(codec, "rc", "cbr") &&
               set_option(codec, "rc-lookahead", "0") &&
               set_option(codec, "zerolatency", "1") &&
               set_option(codec, "forced-idr", "1") &&
               set_option(codec, "surfaces", "4") &&
               set_option(codec, "delay", "0");
    }
    return set_option(codec, "profile", encoder->baseline ? "baseline" : "main") &&
           set_option(codec, "realtime", "1") &&
           set_option(codec, "allow_sw", "0") &&
           set_option(codec, "max_ref_frames", "1");
}

void serein_avc_close(void *opaque)
{
    SereinAvc *encoder = opaque;
    if (!encoder)
        return;
    /* Closing libavcodec stops its native work before the immutable callback
     * state or retained frame storage is released. There is no flush on close:
     * stopping a share/camera must discard pending pictures. */
    avcodec_free_context(&encoder->codec);
    av_frame_free(&encoder->frame);
    av_packet_free(&encoder->packet);
    av_free(encoder);
}

void *serein_avc_open(int width, int height, int fps, int bitrate, int baseline,
                     int backend, size_t max_bytes)
{
    static const char *const names[] = {
        "libopenh264", "h264_nvenc", "h264_videotoolbox"
    };
    const AVCodec *implementation;
    SereinAvc *encoder;
    AVCodecContext *codec;

    if (width <= 0 || height <= 0 || width > 1920 || height > 1080 ||
        (width & 1) || (height & 1) || fps <= 0 || fps > 60 ||
        bitrate <= 0 || bitrate > 100000000 ||
        (baseline != 0 && baseline != 1) || backend < 0 || backend > 2 ||
        max_bytes == 0 || max_bytes > SEREIN_MAX_PACKET_BYTES)
        return NULL;

    implementation = avcodec_find_encoder_by_name(names[backend]);
    if (!implementation || implementation->id != AV_CODEC_ID_H264)
        return NULL;

    encoder = av_mallocz(sizeof(*encoder));
    if (!encoder)
        return NULL;
    /* Dimension checks above bound every multiplication to at most 3 MiB. */
    encoder->input_bytes = (size_t)width * (size_t)height * 3 / 2;
    encoder->max_bytes = max_bytes;
    encoder->baseline = baseline;
    encoder->codec = avcodec_alloc_context3(implementation);
    encoder->frame = av_frame_alloc();
    encoder->packet = av_packet_alloc();
    if (!encoder->codec || !encoder->frame || !encoder->packet)
        goto failed;

    codec = encoder->codec;
    /* The default FFmpeg logger masks the low byte of a level. 128 keeps every
     * documented level (FATAL..TRACE) above TRACE without wrapping that byte.
     * This is per context: never replace another component's global callback.
     * FFmpeg routes the OpenH264 trace callback through this context as well. */
    codec->log_level_offset = 128;
    codec->opaque = encoder;
    codec->get_encode_buffer = bounded_encode_buffer;
    codec->width = width;
    codec->height = height;
    codec->pix_fmt = AV_PIX_FMT_YUV420P;
    /* Both RGB/BGRA converters pack limited-range BT.601 samples, including
     * at HD resolutions. Signal that matrix instead of allowing a decoder or
     * hardware encoder to infer BT.709 from picture size. Matrix conversion
     * preserves the captured SDR RGB primaries and sRGB transfer function. */
    codec->color_range = AVCOL_RANGE_MPEG;
    codec->colorspace = AVCOL_SPC_SMPTE170M;
    codec->color_primaries = AVCOL_PRI_BT709;
    codec->color_trc = AVCOL_TRC_IEC61966_2_1;
    codec->time_base = (AVRational){1, fps};
    codec->framerate = (AVRational){fps, 1};
    codec->sample_aspect_ratio = (AVRational){1, 1};
    codec->bit_rate = bitrate;
    codec->rc_min_rate = bitrate;
    codec->rc_max_rate = bitrate;
    codec->rc_buffer_size = bitrate;
    codec->rc_initial_buffer_occupancy = bitrate / 2;
    codec->gop_size = baseline ? 1 : fps * 2;
    codec->max_b_frames = 0;
    codec->thread_count = width * height <= 640 * 480 ? 2 : 4;
    codec->refs = 1;
    codec->profile = baseline ? AV_PROFILE_H264_BASELINE : AV_PROFILE_H264_MAIN;
    codec->flags |= AV_CODEC_FLAG_LOW_DELAY;
    /* All three pinned FFmpeg encoders repeat parameter sets on IDRs without
     * GLOBAL_HEADER. VideoToolbox's wrapper converts native AVCC to Annex B
     * and prepends its CMSampleBuffer's current SPS/PPS, so no out-of-band or
     * untrusted extradata parser is needed here. Verify the result below. */
    codec->flags &= ~AV_CODEC_FLAG_GLOBAL_HEADER;
    if (!configure_backend(encoder, backend) || avcodec_open2(codec, implementation, NULL) < 0)
        goto failed;

    encoder->frame->format = codec->pix_fmt;
    encoder->frame->width = width;
    encoder->frame->height = height;
    /* VideoToolbox also takes color attachments from the submitted frame. */
    encoder->frame->color_range = codec->color_range;
    encoder->frame->colorspace = codec->colorspace;
    encoder->frame->color_primaries = codec->color_primaries;
    encoder->frame->color_trc = codec->color_trc;
    if (av_frame_get_buffer(encoder->frame, 32) < 0)
        goto failed;
    return encoder;

failed:
    serein_avc_close(encoder);
    return NULL;
}

static size_t start_code_size(const uint8_t *bytes, size_t length, size_t at)
{
    if (length - at >= 4 && bytes[at] == 0 && bytes[at + 1] == 0 &&
        bytes[at + 2] == 0 && bytes[at + 3] == 1)
        return 4;
    if (length - at >= 3 && bytes[at] == 0 && bytes[at + 1] == 0 &&
        bytes[at + 2] == 1)
        return 3;
    return 0;
}

/* Inspect in place before copying. SPS/PPS must occur before every IDR, in
 * this same access unit. Native keyframe metadata alone does not establish
 * independent decodability. Rust additionally validates the packet before
 * encryption/packetization. No cached parameter sets or growable scratch. */
static int inspect_annex_b(const uint8_t *bytes, size_t length, int baseline,
                           int *keyframe)
{
    size_t at = 0;
    int nals = 0, sps = 0, pps = 0, slice = 0, idr = 0;
    while (at < length) {
        size_t prefix = start_code_size(bytes, length, at);
        size_t payload, end;
        int kind;
        if (!prefix || ++nals > SEREIN_MAX_NALS)
            return 0;
        payload = at + prefix;
        if (payload == length)
            return 0;
        end = payload;
        while (end < length && !start_code_size(bytes, length, end))
            end++;
        if (end == payload || (bytes[payload] & 0x80))
            return 0;
        kind = bytes[payload] & 0x1f;
        if (kind == 0 || kind >= 24)
            return 0;
        if (kind == 7)
            sps = 1;
        else if (kind == 8)
            pps = 1;
        else if (kind == 5) {
            if (!sps || !pps)
                return 0;
            idr = slice = 1;
        } else if (kind == 1)
            slice = 1;
        at = end;
    }
    if (!slice || (baseline && !idr))
        return 0;
    *keyframe = idr;
    return 1;
}

int serein_avc_encode(void *opaque, const uint8_t *input, size_t length,
                      int force_keyframe, uint8_t *output,
                      size_t output_capacity, size_t *output_length,
                      int *keyframe)
{
    SereinAvc *encoder = opaque;
    int status, is_keyframe = 0;
    size_t offset = 0;

    if (output_length)
        *output_length = 0;
    if (keyframe)
        *keyframe = 0;
    if (!encoder || !input || !output || !output_length || !keyframe ||
        encoder->failed || length != encoder->input_bytes || output_capacity == 0)
        return -1;
    if (encoder->in_flight >= SEREIN_MAX_IN_FLIGHT || encoder->next_pts == INT64_MAX)
        goto failed;
    if (av_frame_make_writable(encoder->frame) < 0)
        goto failed;

    for (int plane = 0; plane < 3; plane++) {
        int width = encoder->codec->width >> (plane != 0);
        int height = encoder->codec->height >> (plane != 0);
        for (int row = 0; row < height; row++) {
            memcpy(encoder->frame->data[plane] +
                   (size_t)row * (size_t)encoder->frame->linesize[plane],
                   input + offset, (size_t)width);
            offset += (size_t)width;
        }
    }
    encoder->frame->pts = encoder->next_pts++;
    encoder->frame->duration = 1;
    encoder->frame->pict_type = (force_keyframe || encoder->baseline) ?
                               AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
    status = avcodec_send_frame(encoder->codec, encoder->frame);
    /* We drain output on every submission and forbid B frames/lookahead. An
     * unexpected send-side EAGAIN would mean this input was not accepted; the
     * ABI cannot ambiguously return an older packet while losing that input. */
    if (status < 0)
        goto failed;
    encoder->in_flight++;
    av_packet_unref(encoder->packet);
    status = avcodec_receive_packet(encoder->codec, encoder->packet);
    if (status == AVERROR(EAGAIN))
        return 0;
    if (status < 0 || !encoder->packet->data || encoder->packet->size <= 0 ||
        (size_t)encoder->packet->size > encoder->max_bytes ||
        (size_t)encoder->packet->size > output_capacity ||
        !inspect_annex_b(encoder->packet->data, (size_t)encoder->packet->size,
                         encoder->baseline, &is_keyframe))
        goto failed;

    encoder->in_flight--;
    memcpy(output, encoder->packet->data, (size_t)encoder->packet->size);
    *output_length = (size_t)encoder->packet->size;
    *keyframe = is_keyframe;
    av_packet_unref(encoder->packet);
    return 1;

failed:
    encoder->failed = 1;
    av_packet_unref(encoder->packet);
    return -1;
}
