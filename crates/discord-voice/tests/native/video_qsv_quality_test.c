/* Offline metadata correction matrix. This exact predicate is compiled into
 * the bundled FFmpeg Query path, before encoder initialization/input surfaces. */
#include <assert.h>
#include <stdio.h>
#include <vpl/mfxstructures.h>
#include "serein_qsv_feature_validation.h"
#ifdef NDEBUG
#error QSV quality fixtures require assertions
#endif
int main(void)
{
    assert(serein_qsv_quality_matches(0, 1, 0, 0, MFX_CODINGOPTION_OFF, 0, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(0, 0, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(0, 3, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(serein_qsv_quality_matches(2, 3, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(serein_qsv_quality_matches(2, 2, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(serein_qsv_quality_matches(2, 1, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(2, 0, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(2, 4, 0, 0, 0, 0, MFX_RATECONTROL_CBR));
    assert(serein_qsv_quality_matches(2, 3, 16, 1, MFX_CODINGOPTION_ON, 16, MFX_RATECONTROL_CBR));
    assert(serein_qsv_quality_matches(0, 1, 16, 1, MFX_CODINGOPTION_ON, 16, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(2, 3, 16, 1, MFX_CODINGOPTION_OFF, 16, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(2, 3, 16, 1, MFX_CODINGOPTION_ON, 0, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(2, 3, 16, 1, MFX_CODINGOPTION_ON, 15, MFX_RATECONTROL_CBR));
    assert(!serein_qsv_quality_matches(2, 3, 16, 1, MFX_CODINGOPTION_ON, 16, MFX_RATECONTROL_VBR));
    assert(!serein_qsv_quality_matches(2, 3, 16, 0, MFX_CODINGOPTION_ON, 16, MFX_RATECONTROL_CBR));
    puts("QSV quality: 15 driver-correction cases preserve no-reorder H264, bounded supported B frames, actual lookahead and CBR");
}
