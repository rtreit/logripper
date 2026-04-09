#ifndef LOGRIPPER_DSP_H
#define LOGRIPPER_DSP_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int32_t lr_dsp_version(void);

/// Convert a frequency in Hz to the nearest kHz, rounding to nearest.
uint64_t lr_dsp_hz_to_khz(uint64_t freq_hz);

/// Compute a simple moving average over `count` samples.
/// Returns 0.0 if count is 0 or samples is NULL.
double lr_dsp_moving_average(const double *samples, size_t count);

#ifdef __cplusplus
}
#endif

#endif
