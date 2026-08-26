import { afterEach, describe, expect, it } from 'vitest';

import { resolveLanguage, setLanguage, t } from './i18n.svelte';

describe('i18n', () => {
  afterEach(() => setLanguage('en'));

  it('falls back to English and then the key for unknown strings', () => {
    setLanguage('ja');
    expect(t('quota.limitReached')).toBe('上限に到達');
    expect(t('not.a.real.key')).toBe('not.a.real.key');
  });

  it('interpolates placeholders', () => {
    setLanguage('en');
    expect(t('quota.spare', { percent: 12 })).toBe('~12% spare');
    setLanguage('zh-CN');
    expect(t('app.refreshProviderFailed', { provider: 'Codex' })).toBe('无法刷新 Codex 的用量。');
  });

  it('resolves the system locale to a supported language', () => {
    expect(resolveLanguage('ko')).toBe('ko');
    const original = navigator.language;
    const setLocale = (value: string) =>
      Object.defineProperty(navigator, 'language', { value, configurable: true });
    setLocale('zh-Hant-TW');
    expect(resolveLanguage('system')).toBe('zh-TW');
    setLocale('zh-CN');
    expect(resolveLanguage('system')).toBe('zh-CN');
    setLocale('zh');
    expect(resolveLanguage('system')).toBe('zh-CN');
    setLocale('fr-FR');
    expect(resolveLanguage('system')).toBe('en');
    Object.defineProperty(navigator, 'language', {
      value: original,
      configurable: true,
    });
  });

  it('ignores an unsupported preference instead of crashing', () => {
    expect(resolveLanguage('xx' as never)).toBe('en');
  });
});
