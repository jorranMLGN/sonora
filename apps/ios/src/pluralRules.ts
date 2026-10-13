/**
 * `Intl.PluralRules` for engines that lack it, as Hermes on iOS does. Fluent builds one for
 * every `{ $count -> … }` selector, so without it any counted string throws. This covers the
 * cardinal rules of the locales the app ships (`assets/i18n`) for whole numbers, which is all a
 * count ever is; anything else, ordinals included, reads as `other`.
 */

type Category = 'zero' | 'one' | 'two' | 'few' | 'many' | 'other';

/** The cardinal category of a whole number `n`, by language. */
const RULES: Record<string, (n: number) => Category> = {
  cs: (n) => (n === 1 ? 'one' : n >= 2 && n <= 4 ? 'few' : 'other'),
  de: one,
  en: one,
  es: (n) => (n === 1 ? 'one' : n !== 0 && n % 1_000_000 === 0 ? 'many' : 'other'),
  fr: (n) => (n === 0 || n === 1 ? 'one' : n !== 0 && n % 1_000_000 === 0 ? 'many' : 'other'),
  id: () => 'other',
  it: (n) => (n === 1 ? 'one' : n !== 0 && n % 1_000_000 === 0 ? 'many' : 'other'),
  ja: () => 'other',
  pl: (n) => (n === 1 ? 'one' : few(n) ? 'few' : 'many'),
  pt: (n) => (n === 0 || n === 1 ? 'one' : n !== 0 && n % 1_000_000 === 0 ? 'many' : 'other'),
  ru: slavic,
  sq: one,
  tr: one,
  uk: slavic,
  zh: () => 'other',
};

function one(n: number): Category {
  return n === 1 ? 'one' : 'other';
}

/** Ends in 2, 3 or 4, but not in 12, 13 or 14. */
function few(n: number): boolean {
  const last = n % 10;
  const lastTwo = n % 100;
  return last >= 2 && last <= 4 && !(lastTwo >= 12 && lastTwo <= 14);
}

/** Russian and Ukrainian. */
function slavic(n: number): Category {
  if (n % 10 === 1 && n % 100 !== 11) return 'one';
  return few(n) ? 'few' : 'many';
}

class PluralRules {
  private readonly locale: string;
  private readonly ordinal: boolean;

  constructor(locales?: string | string[], options?: { type?: 'cardinal' | 'ordinal' }) {
    const first = Array.isArray(locales) ? locales[0] : locales;
    this.locale = (first ?? 'en').split('-')[0].toLowerCase();
    this.ordinal = options?.type === 'ordinal';
  }

  static supportedLocalesOf(locales: string | string[]): string[] {
    return (Array.isArray(locales) ? locales : [locales]).filter(
      (locale) => locale.split('-')[0].toLowerCase() in RULES,
    );
  }

  select(value: number): Category {
    const n = Math.abs(Number(value));
    if (this.ordinal || !Number.isInteger(n)) return 'other';
    return (RULES[this.locale] ?? one)(n);
  }

  resolvedOptions() {
    return { locale: this.locale, type: this.ordinal ? 'ordinal' : 'cardinal' };
  }
}

/** Puts `PluralRules` on `Intl` unless the engine already has one. Safe to call twice. */
export function installPluralRules(): void {
  const intl = globalThis.Intl as unknown as Record<string, unknown> | undefined;
  if (intl === undefined || typeof intl.PluralRules === 'function') return;
  intl.PluralRules = PluralRules;
}
