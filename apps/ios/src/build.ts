/**
 * Which build this is. CI stamps the `EXPO_PUBLIC_SONORA_*` variables in when it bundles the
 * app (see `.github/workflows/ios.yml`); a build from a laptop has none and calls itself `dev`.
 */
export const build = {
  version: process.env.EXPO_PUBLIC_SONORA_VERSION ?? null,
  number: process.env.EXPO_PUBLIC_SONORA_BUILD ?? null,
  commit: process.env.EXPO_PUBLIC_SONORA_COMMIT ?? null,
};
