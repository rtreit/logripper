#include "logripper_dsp.h"

int32_t lr_dsp_version(void) {
    return 1;
}

uint64_t lr_dsp_hz_to_khz(uint64_t freq_hz) {
    return (freq_hz + 500) / 1000;
}

double lr_dsp_moving_average(const double *samples, size_t count) {
    if (samples == NULL || count == 0) {
        return 0.0;
    }
    double sum = 0.0;
    for (size_t i = 0; i < count; i++) {
        sum += samples[i];
    }
    return sum / (double)count;
}
