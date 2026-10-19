/**
 * Spotify's image ids carry their size in the eight hex digits after `ab67616d`: `00004851` is
 * 64 px, `00001e02` 300 px and `0000b273` 640 px, all of the same picture.
 */
const SPOTIFY = /^(https:\/\/i\.scdn\.co\/image\/ab67616d)(00004851|00001e02)/;
const SPOTIFY_LARGE = '0000b273';
/** Google's image hosts, YouTube Music's, take the size as `=w<n>-h<n>` at the end. */
const GOOGLE = /^(https:\/\/(?:lh3\.googleusercontent\.com|yt3\.ggpht\.com)\/.+=)w\d+-h\d+/;
const GOOGLE_LARGE = 'w544-h544';

/**
 * The same artwork at the largest size its host serves without another request, for the full
 * player. A url it does not know comes back as it is; SoundCloud's already are 500 px.
 */
export function largestArtwork(uri: string | null): string | null {
  if (uri === null) return null;
  if (SPOTIFY.test(uri)) return uri.replace(SPOTIFY, `$1${SPOTIFY_LARGE}`);
  if (GOOGLE.test(uri)) return uri.replace(GOOGLE, `$1${GOOGLE_LARGE}`);
  return uri;
}
