import type { AppSettings, ProviderDefinition, ProviderOption } from './types';

/** The options a provider declares, or an empty list when it declares none. */
export function providerOptions(definition: ProviderDefinition | undefined): ProviderOption[] {
  return (definition?.options ?? []).filter(
    (option) =>
      option.choices.length > 0 &&
      option.choices.some((choice) => choice.id === option.defaultChoice),
  );
}

/**
 * The choice currently selected for an option, falling back to its default whenever nothing is
 * stored or the stored value is no longer offered. Mirrors the backend's resolution so the UI
 * never shows a selection a provider would not act on.
 */
export function selectedChoice(
  settings: AppSettings,
  providerId: string,
  option: ProviderOption,
): string {
  const stored = settings.providerOptions?.[providerId]?.[option.id]?.trim();
  return option.choices.some((choice) => choice.id === stored) && stored
    ? stored
    : option.defaultChoice;
}

/**
 * Returns settings with one provider option set, without mutating the input. Returns the original
 * object when the choice is unchanged or not offered, so callers can skip a no-op save.
 */
export function withProviderOption(
  settings: AppSettings,
  providerId: string,
  option: ProviderOption,
  choiceId: string,
): AppSettings {
  if (!option.choices.some((choice) => choice.id === choiceId)) return settings;
  if (selectedChoice(settings, providerId, option) === choiceId) return settings;
  const providerOptions = settings.providerOptions ?? {};
  return {
    ...settings,
    providerOptions: {
      ...providerOptions,
      [providerId]: { ...(providerOptions[providerId] ?? {}), [option.id]: choiceId },
    },
  };
}
