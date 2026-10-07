/* Codec-specific engine metadata only; no driver, surface or encoder Init. */
#include <assert.h>
#include <stdio.h>
#include <wchar.h>
#include <AMF/components/Component.h>
#include <AMF/components/VideoEncoderHEVC.h>
#include <AMF/components/VideoEncoderAV1.h>
#include "serein_amf_split_encoding.h"

static int codec, caps_calls, releases, flag_available = 1, bad_count_type;
static int caps_error, count_error, null_caps;
static const amf_int64 engines[] = {2, 1, 3, 0, -1};
static int engine_index;
static const wchar_t *counts[] = {AMF_VIDEO_ENCODER_HEVC_CAP_NUM_OF_HW_INSTANCES,
                                AMF_VIDEO_ENCODER_AV1_CAP_NUM_OF_HW_INSTANCES};
static const wchar_t *flags[] = {AMF_VIDEO_ENCODER_HEVC_MULTI_HW_INSTANCE_ENCODE,
                               AMF_VIDEO_ENCODER_AV1_MULTI_HW_INSTANCE_ENCODE};

static AMF_RESULT AMF_STD_CALL flag_property(AMFComponent *self, const wchar_t *name, AMFVariantStruct *value)
{
    (void)self;
    assert(wcscmp(name, flags[codec]) == 0);
    if (!flag_available)
        return AMF_NOT_FOUND;
    value->type = AMF_VARIANT_BOOL;
    value->boolValue = false;
    return AMF_OK;
}

static AMF_RESULT AMF_STD_CALL count_property(AMFCaps *self, const wchar_t *name, AMFVariantStruct *value)
{
    (void)self;
    assert(wcscmp(name, counts[codec]) == 0);
    value->type = bad_count_type ? AMF_VARIANT_BOOL : AMF_VARIANT_INT64;
    value->int64Value = engines[engine_index];
    return count_error ? AMF_FAIL : AMF_OK;
}

static amf_long AMF_STD_CALL release_caps(AMFCaps *self)
{ (void)self; releases++; return 0; }

static AMFCapsVtbl caps_vtable = {.GetProperty = count_property, .Release = release_caps};
static AMFCaps caps = {&caps_vtable};

static AMF_RESULT AMF_STD_CALL get_caps(AMFComponent *self, AMFCaps **out)
{
    (void)self;
    caps_calls++;
    *out = null_caps ? NULL : &caps;
    return caps_error ? AMF_FAIL : AMF_OK;
}

static AMFComponentVtbl component_vtable = {.GetProperty = flag_property, .GetCaps = get_caps};
static AMFComponent encoder = {&component_vtable};

int main(void)
{
    for (codec = 0; codec < 2; codec++) {
        for (engine_index = 0; engine_index < 5; engine_index++) {
            caps_calls = releases = 0;
            assert(serein_amf_split_eligible(&encoder, counts[codec], flags[codec]) == (engine_index == 0));
            assert(caps_calls == 1 && releases == 1);
        }
        engine_index = 0;
        bad_count_type = 1;
        assert(!serein_amf_split_eligible(&encoder, counts[codec], flags[codec]));
        bad_count_type = 0;
        count_error = 1;
        assert(!serein_amf_split_eligible(&encoder, counts[codec], flags[codec]));
        count_error = 0;
        caps_error = 1;
        caps_calls = releases = 0;
        assert(!serein_amf_split_eligible(&encoder, counts[codec], flags[codec]));
        assert(caps_calls == 1 && releases == 1);
        caps_error = 0;
        null_caps = 1;
        caps_calls = releases = 0;
        assert(!serein_amf_split_eligible(&encoder, counts[codec], flags[codec]));
        assert(caps_calls == 1 && releases == 0);
        null_caps = 0;
        flag_available = 0;
        caps_calls = releases = 0;
        assert(!serein_amf_split_eligible(&encoder, counts[codec], flags[codec]));
        assert(caps_calls == 0 && releases == 0);
        flag_available = 1;
    }
    puts("AMF split: 20 codec-specific engine/old-runtime/error cases, caps released, no encoding");
}
