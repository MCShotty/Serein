#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
#ifdef NDEBUG
#error Driver query fixtures require assertions enabled
#endif
int serein_query_nvenc(int codec);
void fixture_cuda_reset(void);
int fixture_cuda_created(void);
int fixture_cuda_destroyed(void);
void fixture_nv_reset(void);
int fixture_nv_opened(void);
int fixture_nv_closed(void);
int fixture_nv_forbidden(void);
int main(void) {
    const struct { const char *mode; int codec, expected, contexts; } cases[] = {
        {"normal", 0, 1, 1}, {"normal", 1, 0, 1}, {"normal", 2, 0, 1},
        {"multi", 2, 1, 2}, {"error-then-supported", 2, 1, 2},
        {"no-device", 0, 0, 0}, {"init-no-device", 0, 0, 0},
        {"init-failed", 0, -1, 0}, {"count-failed", 0, -1, 0},
        {"context-failed", 0, -1, 0}, {"open-unavailable", 0, 0, 1},
        {"open-error", 0, -1, 1}, {"guid-error", 0, -1, 1},
        {"zero-guids", 0, 0, 1}, {"excess-guids", 0, -1, 1},
        {"short-guids", 0, -1, 1}, {"caps-error", 0, -1, 1},
        {"zero-dimensions", 0, 0, 1}, {"close-error", 0, -1, 1},
        {"old-api", 0, -1, 0}, {"missing-caps", 0, -1, 0},
        {"bound", 2, -1, 32}, {"normal", -1, 0, 0}, {"normal", 3, 0, 0},
    };
    for (unsigned int i = 0; i < sizeof(cases)/sizeof(cases[0]); i++) {
        assert(setenv("SEREIN_QUERY_FIXTURE", cases[i].mode, 1) == 0);
        fixture_cuda_reset(); fixture_nv_reset();
        int result = serein_query_nvenc(cases[i].codec);
        if (result != cases[i].expected) {
            fprintf(stderr, "%s codec%d expected%d got%d\n", cases[i].mode, cases[i].codec, cases[i].expected, result);
            return 1;
        }
        assert(fixture_cuda_created() == cases[i].contexts);
        assert(fixture_cuda_destroyed() == fixture_cuda_created());
        assert(fixture_nv_closed() == fixture_nv_opened());
        assert(fixture_nv_forbidden() == 0);
    }
    printf("NVENC query: %zu offline driver-boundary cases passed, contexts/sessions released, no encoding calls\n", sizeof(cases)/sizeof(cases[0]));
}
