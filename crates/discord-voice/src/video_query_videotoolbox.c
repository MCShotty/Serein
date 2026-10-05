/* VideoToolbox's public encoder inventory, without creating an encoder or
 * submitting pictures. This is hardware codec discovery, not a guarantee that
 * a particular resolution/profile or a busy device can encode right now.
 */
#if defined(__APPLE__)
#include <CoreFoundation/CoreFoundation.h>
#include <CoreMedia/CoreMedia.h>
#include <VideoToolbox/VideoToolbox.h>
#include <stdint.h>

#define SEREIN_MAX_VT_ENCODERS 512

int serein_query_videotoolbox(int codec)
{
    CMVideoCodecType wanted;
    switch (codec) {
    case 0:
        wanted = kCMVideoCodecType_H264;
        break;
    case 1:
        wanted = kCMVideoCodecType_HEVC;
        break;
    case 2:
        /* kCMVideoCodecType_AV1's 'av01' FourCC, also usable with SDKs that
         * predate that enum constant. The caller separately checks whether
         * the bundled FFmpeg has an encoder for this codec.
         */
        wanted = (CMVideoCodecType)0x61763031u;
        break;
    default:
        return -1;
    }

    /* The inventory predates this OS version, but its public hardware flag
     * does not. Never infer encoding support from the decoding-only
     * VTIsHardwareDecodeSupported API or from vendor-specific encoder IDs.
     */
    if (__builtin_available(macOS 10.14, *)) {
        CFArrayRef encoders = NULL;
        const OSStatus status = VTCopyVideoEncoderList(NULL, &encoders);
        if (status != 0 || !encoders) {
            if (encoders)
                CFRelease(encoders);
            return -1;
        }
        if (CFGetTypeID(encoders) != CFArrayGetTypeID()) {
            CFRelease(encoders);
            return -1;
        }
        const CFIndex count = CFArrayGetCount(encoders);
        if (count < 0 || count > SEREIN_MAX_VT_ENCODERS) {
            CFRelease(encoders);
            return -1;
        }

        int available = 0;
        int unknown = 0;
        for (CFIndex index = 0; index < count; ++index) {
            const CFTypeRef entry = CFArrayGetValueAtIndex(encoders, index);
            if (!entry || CFGetTypeID(entry) != CFDictionaryGetTypeID()) {
                unknown = 1;
                continue;
            }
            const CFDictionaryRef encoder = (CFDictionaryRef)entry;
            const CFNumberRef kind = (CFNumberRef)CFDictionaryGetValue(
                encoder, kVTVideoEncoderList_CodecType);
            int64_t value = 0;
            if (!kind || CFGetTypeID(kind) != CFNumberGetTypeID() ||
                !CFNumberGetValue(kind, kCFNumberSInt64Type, &value)) {
                unknown = 1;
                continue;
            }
            if (value != (int64_t)wanted)
                continue;
            const CFBooleanRef hardware = (CFBooleanRef)CFDictionaryGetValue(
                encoder, kVTVideoEncoderList_IsHardwareAccelerated);
            /* This optional flag can be absent. Absence does not establish
             * that the encoder is software-only, so preserve Unknown.
             */
            if (!hardware || CFGetTypeID(hardware) != CFBooleanGetTypeID()) {
                unknown = 1;
                continue;
            }
            if (CFBooleanGetValue(hardware)) {
                available = 1;
                break;
            }
        }
        CFRelease(encoders);
        return available ? 1 : unknown ? -1 : 0;
    }
    return -1;
}
#else
int serein_query_videotoolbox(int codec)
{
    (void)codec;
    return 0;
}
#endif
