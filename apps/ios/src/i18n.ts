import { FluentBundle, FluentResource } from "@fluent/bundle";
import type { FluentVariable } from "@fluent/bundle";
import { getLocales } from "expo-localization";

import { ftl } from "./generated/ftl";
import { installPluralRules } from "./pluralRules";

// Fluent picks the right form of a counted string with `Intl.PluralRules`, which Hermes lacks.
installPluralRules();

const FALLBACK = "en-US";

/** Bundles are built on first use and kept, one per locale. */
const bundles = new Map<string, FluentBundle>();

let current = pick(getLocales().map((locale) => locale.languageTag));

/** Builds the bundle for a locale id that `ftl` carries. Isolation marks are off, they show up as stray glyphs in RN text. */
function bundle(locale: string): FluentBundle | undefined {
  const cached = bundles.get(locale);
  if (cached) return cached;

  const source = ftl[locale];
  if (source === undefined) return undefined;

  const made = new FluentBundle(locale, { useIsolating: false });
  made.addResource(new FluentResource(source));
  bundles.set(locale, made);
  return made;
}

/** Maps device language tags (`nl-NL`, `zh-Hans-CN`) to the first shipped locale, else `en-US`. */
function pick(tags: string[]): string {
  for (const tag of tags) {
    if (ftl[tag] !== undefined) return tag;
    const language = tag.split("-")[0].toLowerCase();
    const match = Object.keys(ftl).find((id) => id.split("-")[0].toLowerCase() === language);
    if (match) return match;
  }
  return FALLBACK;
}

function format(
  locale: string,
  key: string,
  args?: Record<string, string | number>,
): string | undefined {
  const found = bundle(locale);
  const message = found?.getMessage(key);
  if (!found || !message?.value) return undefined;
  return found.formatPattern(message.value, args as Record<string, FluentVariable>);
}

/** Selects the UI language. Accepts a shipped id (`de`) or a device tag (`de-AT`); unknown ones fall back to English. */
export function setLocale(locale: string): void {
  current = pick([locale]);
}

/** The locale id in use right now. */
export function locale(): string {
  return current;
}

/**
 * Resolves a Fluent key. A key missing from the current locale falls back to `en-US`, then to
 * the key itself. Resolve in render, not in a constructor, so a locale change shows up.
 */
export function t(key: string, args?: Record<string, string | number>): string {
  return format(current, key, args) ?? format(FALLBACK, key, args) ?? key;
}
