#ifndef SEREIN_VIDEO_ENCODE_FFMPEG_H
#define SEREIN_VIDEO_ENCODE_FFMPEG_H

#include <stddef.h>
#include <stdint.h>
#ifndef SEREIN_VIDEO_GPU_H
#define SEREIN_VIDEO_GPU_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Physical renderer identity, never a vendor-only or performance preference.
 * All fields use fixed-width integers so Rust and native helper ABIs agree. */
typedef struct SereinVideoAdapter {
    uint32_t identity;
    uint32_t vendor_id;
    uint32_t device_id;
    uint32_t domain;
    uint32_t bus;
    uint32_t slot;
    uint32_t function;
    uint64_t value;
} SereinVideoAdapter;

enum {
    SEREIN_GPU_UNIDENTIFIED = 0,
    SEREIN_GPU_WINDOWS_LUID = 1,
    SEREIN_GPU_PCI = 2,
    SEREIN_GPU_METAL_REGISTRY = 3
};

struct AVCodecContext;
int serein_video_adapter_valid(const SereinVideoAdapter *adapter);
int serein_video_adapter_equal(const SereinVideoAdapter *a, const SereinVideoAdapter *b);
int serein_video_cuda_device(const SereinVideoAdapter *adapter);
int serein_video_dxgi_device(const SereinVideoAdapter *adapter);
int serein_video_drm_device(const SereinVideoAdapter *adapter, char *path, size_t capacity);
/* Returns a +1 CFDictionary on macOS; the caller must release it. */
void *serein_video_vt_specification(const SereinVideoAdapter *adapter);
/* Returns a retained AVBufferRef to a Vulkan device derived from the exact
 * physical DRM node, or NULL. No encoder or input surface is initialized. */
void *serein_video_vulkan_device(const SereinVideoAdapter *adapter);
/* NULL means the legacy standalone/example path. A supplied target must be
 * bound exactly or fail, so another physical GPU is never selected silently. */
int serein_video_bind_adapter(struct AVCodecContext *codec, int backend,
                             const SereinVideoAdapter *adapter);
int serein_video_query_on_adapter(int backend, int codec, const SereinVideoAdapter *adapter);
int serein_query_nvenc_on_adapter(int codec, const SereinVideoAdapter *adapter);
int serein_query_qsv_on_adapter(int codec, const SereinVideoAdapter *adapter);
int serein_query_amf_on_adapter(int codec, const SereinVideoAdapter *adapter);
int serein_query_videotoolbox_on_adapter(int codec, const SereinVideoAdapter *adapter);
/* Startup capability metadata only; no pictures or synthetic encoding. */
int serein_nvenc_features(int codec, const SereinVideoAdapter *adapter,
                          int *max_b_frames, int *lookahead);

#ifdef __cplusplus
}
#endif
#endif


#ifdef __cplusplus
extern "C" {
#endif

/* The returned encoder must be used and closed on its owning media worker.
 * Backend: 0 = OpenH264, 1 = NVENC, 2 = VideoToolbox (hardware required),
 * 3 = AMD AMF, 4 = Intel QSV (hardware-only D3D11/VA driver session).
 * baseline selects independent Stable/test pictures (GOP 1); Experimental
 * camera and screen pictures use Main profile and a two-second GOP. Codec: 0 H.264, 1 HEVC,
 * 2 AV1. Software is provided only for H.264. NULL means unavailable. */
void *serein_avc_open(int width, int height, int fps, int bitrate, int baseline,
                     int backend, int codec, size_t max_bytes);
/* Features: bit 0 requests up to two B frames (HEVC/AV1 only); bit 1 requests
 * sixteen-frame lookahead. Unsupported combinations fail without switching GPU. */
void *serein_avc_open_on_adapter(int width, int height, int fps, int bitrate,
                                int baseline, int backend, int codec,
                                size_t max_bytes, const SereinVideoAdapter *adapter,
                                int features);
void serein_avc_close(void *encoder);
/* AMF split request on this live session: 0 = off, 1 = declined, 2 = accepted.
 * Accepted means SetProperty succeeded and the encoder opened successfully.
 * A successful retry without split remains declined. This is diagnostic metadata. */
int serein_avc_amf_split(void *encoder);

/* Input is exactly width * height * 3 / 2 contiguous limited-range BT.601
 * I420 bytes, preserving the callers' SDR sRGB primaries/transfer. All pointers
 * must refer to live nonoverlapping storage of the indicated lengths. The
 * output buffer is written only after the complete packet passes its bounds
 * and Annex B / sized OBU checks. Returns 1 for a packet, 0 for pending output, or -1 for
 * failure. Output length/keyframe are reset even when encoding fails. */
int serein_avc_encode(void *encoder, const uint8_t *contiguous_i420,
                      size_t length, int force_keyframe, uint8_t *output,
                      size_t output_capacity, size_t *output_length,
                      int *keyframe);
int serein_avc_encode_timed(void *encoder, const uint8_t *contiguous_i420,
                            size_t length, int force_keyframe, uint8_t *output,
                            size_t output_capacity, size_t *output_length,
                            int *keyframe, int64_t *presentation_index);

#ifdef __cplusplus
}
#endif

#endif
