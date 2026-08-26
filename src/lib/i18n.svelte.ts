import { en } from './messages/en';
import { ja } from './messages/ja';
import { ko } from './messages/ko';
import { zhCN } from './messages/zh-CN';
import { zhTW } from './messages/zh-TW';

export type Language = 'en' | 'zh-CN' | 'zh-TW' | 'ja' | 'ko';
export type LanguagePreference = 'system' | Language;

export const LANGUAGE_PREFERENCES: { value: LanguagePreference; label: string }[] = [
  { value: 'system', label: 'System' },
  { value: 'en', label: 'English' },
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'ja', label: '日本語' },
  { value: 'ko', label: '한국어' },
];

const catalogs: Record<Language, Record<string, string>> = {
  en,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  ja,
  ko,
};

let currentLanguage = $state<Language>('en');

export function resolveLanguage(preference: LanguagePreference): Language {
  if (preference !== 'system' && preference in catalogs) return preference;
  const locale = typeof navigator === 'undefined' ? 'en' : navigator.language;
  // Traditional Chinese only when the script or region says so; bare zh falls to Simplified.
  if (/^zh[-_](?:hant|tw|hk|mo)/i.test(locale)) return 'zh-TW';
  if (/^zh/i.test(locale)) return 'zh-CN';
  if (/^ja/i.test(locale)) return 'ja';
  if (/^ko/i.test(locale)) return 'ko';
  return 'en';
}

export function setLanguage(preference: LanguagePreference) {
  currentLanguage = resolveLanguage(preference);
}

export function language(): Language {
  return currentLanguage;
}

/** Locale used for Intl formatting; follows the UI language rather than the OS. */
export function formattingLocale(): string {
  return currentLanguage;
}

/**
 * Translates a key with `{placeholder}` interpolation, falling back to English and then to
 * the key itself so an untranslated string can never blank out the UI.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  let text = catalogs[currentLanguage]?.[key] ?? catalogs.en[key] ?? key;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}
