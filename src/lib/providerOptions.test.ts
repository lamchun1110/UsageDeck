import { describe, expect, it } from 'vitest';
import { providerOptions, selectedChoice, withProviderOption } from './providerOptions';
import type { AppSettings, ProviderDefinition, ProviderOption } from './types';

const endpoint: ProviderOption = {
  id: 'endpoint',
  label: 'Endpoint',
  description: 'Pick a domain.',
  defaultChoice: 'primary',
  choices: [
    { id: 'primary', label: 'primary.example', description: null },
    { id: 'alternate', label: 'alternate.example', description: null },
  ],
};

function definition(options: ProviderOption[]): ProviderDefinition {
  return {
    id: 'sample',
    displayName: 'Sample',
    shortName: 'S',
    fallbackEnabled: false,
    localUsageSourceNote: null,
    links: [],
    options,
    metrics: [],
  };
}

function settings(providerOptionValues: Record<string, Record<string, string>>): AppSettings {
  return { providerOptions: providerOptionValues } as unknown as AppSettings;
}

describe('providerOptions', () => {
  it('ignores options that do not declare their own default', () => {
    const incoherent: ProviderOption = { ...endpoint, defaultChoice: 'missing' };
    expect(providerOptions(definition([incoherent]))).toEqual([]);
    expect(providerOptions(definition([{ ...endpoint, choices: [] }]))).toEqual([]);
    expect(providerOptions(definition([endpoint]))).toEqual([endpoint]);
  });

  it('treats a provider without options as having none', () => {
    expect(providerOptions(undefined)).toEqual([]);
    const withoutOptions: ProviderDefinition = { ...definition([]) };
    delete withoutOptions.options;
    expect(providerOptions(withoutOptions)).toEqual([]);
  });
});

describe('selectedChoice', () => {
  it('falls back to the default when nothing is stored', () => {
    expect(selectedChoice(settings({}), 'sample', endpoint)).toBe('primary');
  });

  it('returns a stored choice the option still offers', () => {
    const stored = settings({ sample: { endpoint: 'alternate' } });
    expect(selectedChoice(stored, 'sample', endpoint)).toBe('alternate');
  });

  it('falls back to the default when the stored choice is no longer offered', () => {
    const stored = settings({ sample: { endpoint: 'retired' } });
    expect(selectedChoice(stored, 'sample', endpoint)).toBe('primary');
  });
});

describe('withProviderOption', () => {
  it('returns new settings without mutating the original', () => {
    const original = settings({ sample: { endpoint: 'primary' } });
    const next = withProviderOption(original, 'sample', endpoint, 'alternate');

    expect(next).not.toBe(original);
    expect(next.providerOptions.sample.endpoint).toBe('alternate');
    expect(original.providerOptions.sample.endpoint).toBe('primary');
  });

  it('keeps other providers and options untouched', () => {
    const original = settings({ sample: { other: 'kept' }, another: { endpoint: 'primary' } });
    const next = withProviderOption(original, 'sample', endpoint, 'alternate');

    expect(next.providerOptions.sample).toEqual({ other: 'kept', endpoint: 'alternate' });
    expect(next.providerOptions.another).toEqual({ endpoint: 'primary' });
  });

  it('returns the same object for a no-op or an unavailable choice', () => {
    const original = settings({ sample: { endpoint: 'alternate' } });
    expect(withProviderOption(original, 'sample', endpoint, 'alternate')).toBe(original);
    expect(withProviderOption(original, 'sample', endpoint, 'retired')).toBe(original);
  });

  it('records a first selection when the provider has no stored options', () => {
    const original = settings({});
    const next = withProviderOption(original, 'sample', endpoint, 'alternate');
    expect(next.providerOptions.sample).toEqual({ endpoint: 'alternate' });
  });
});
