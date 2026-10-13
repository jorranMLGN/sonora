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

/// Every call below answers with an owned JSON string, `{"error": "..."}` on failure,
/// which must be released with `sonora_free`.

/// Signs in to SoundCloud as a guest, keeping its files under `data_dir`. Blocks.
char *sonora_init(const char *data_dir);

/// Searches SoundCloud for tracks: `[{id, title, artist, artwork, duration, playable}]`. Blocks.
char *sonora_search(const char *query);

/// Plays a track a search returned.
char *sonora_play(const char *track_id);

/// Pauses or resumes: `{"playing": bool}`.
char *sonora_toggle(void);

/// `{id, title, artist, artwork, playing, loading, position, duration, error}`.
char *sonora_now_playing(void);

/// Releases a string one of the calls above returned.
void sonora_free(char *text);

#ifdef __cplusplus
}
#endif

#endif
