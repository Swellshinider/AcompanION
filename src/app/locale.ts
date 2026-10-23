import { registerLocaleData } from '@angular/common';
import { loadTranslations } from '@angular/localize';
import { loadAppSettings } from './app-settings';

/** Translated locales by language; the source language (English) needs no entry. */
const TRANSLATED: Record<string, string> = { pt: 'pt-BR' };

/**
 * `{ message id: translation }` from an XLIFF 1.2 file, with `<x id="P"/>` as `{$P}` the way `$localize` expects.
 * Inside an ICU message (`{VAR_PLURAL, plural, ...}`) the placeholder stays a plain `{P}`: the ICU is one literal to `$localize`.
 */
export function parseXlf(xml: string): Record<string, string> {
  const doc = new DOMParser().parseFromString(xml, 'application/xml');
  const out: Record<string, string> = {};
  for (const unit of Array.from(doc.getElementsByTagName('trans-unit'))) {
    const target = unit.getElementsByTagName('target')[0];
    if (!target) continue;
    const icu = /^\{\w+, (plural|select),/.test(target.textContent ?? '');
    out[unit.getAttribute('id')!] = Array.from(target.childNodes)
      .map(n => (n.nodeType === Node.ELEMENT_NODE ? `{${icu ? '' : '$'}${(n as Element).getAttribute('id')}}` : n.textContent))
      .join('');
  }
  return out;
}

/**
 * Picks the UI locale (the Settings choice, or the system language when it is `auto`) and, for a
 * translated one, loads its strings before the app starts. Returns the locale for `LOCALE_ID`; any
 * failure leaves the English source text. The main and overlay windows share the saved choice.
 */
export async function initLocale(system = navigator.language, chosen = loadAppSettings().language): Promise<string> {
  const language = chosen === 'auto' ? system : chosen;
  const locale = TRANSLATED[language.toLowerCase().split('-')[0]];
  if (!locale) return 'en-US';
  try {
    const r = await fetch(`locale/messages.${locale}.xlf`);
    if (!r.ok) throw new Error(`messages.${locale}.xlf: HTTP ${r.status}`);
    loadTranslations(parseXlf(await r.text()));
    registerLocaleData((await import('@angular/common/locales/pt')).default);
    return locale;
  } catch (e) {
    console.error(e);
    return 'en-US';
  }
}
