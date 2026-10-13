#ifndef SONORA_CORE_H
#define SONORA_CORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// A static, NUL-terminated version string. Never free it.
const char *sonora_version(void);

/// Plays a 440 Hz sine for `seconds` on a background thread.
/// Returns 0 when the thread started, a negative number otherwise.
int32_t sonora_play_tone(uint32_t seconds);

#ifdef __cplusplus
}
#endif

#endif
