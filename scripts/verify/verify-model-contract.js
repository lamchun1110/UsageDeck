import fs from 'node:fs';

const root = new URL('../../', import.meta.url);
const rustSource = [
  'src-tauri/src/models.rs',
  'src-tauri/src/service.rs',
  'src-tauri/src/commands/bootstrap.rs',
]
  .map((file) => fs.readFileSync(new URL(file, root), 'utf8'))
  .join('\n');
const typeScriptSource = fs.readFileSync(new URL('src/lib/types.ts', root), 'utf8');
const contracts = [
  'QuotaWindow',
  'MetricValue',
  'ValueMetric',
  'StatusMetric',
  'ProviderNotice',
  'UsagePeriod',
  'ModelUsageEntry',
  'ModelUsageVariant',
  'ModelUsageBreakdown',
  'DailyUsage',
  'UsageHistory',
  'ProviderSnapshot',
  'ProviderViewState',
  'UsageViewState',
  'TrayMetricDefinition',
  'MetricDefinition',
  'ProviderLink',
  'ProviderApiKeyState',
  'ProviderDefinition',
  'ProviderCatalog',
  'MetricLayout',
  'ProviderLayout',
  'NotificationPreferences',
  'AppSettings',
  'SettingsViewState',
  'BootstrapState',
  'QuotaHistorySample',
  'ApiKeyMutationOutcome',
  'ProviderOption',
  'ProviderOptionChoice',
];

function camelCase(value) {
  return value.replace(/_([a-zA-Z0-9])/g, (_, character) => character.toUpperCase());
}

function rustFields(name, visiting = new Set()) {
  if (visiting.has(name)) throw new Error(`Circular Rust contract flattening at ${name}.`);
  const body = rustSource.match(new RegExp(`pub struct ${name}\\s*\\{([\\s\\S]*?)\\n\\}`))?.[1];
  if (!body) throw new Error(`Rust contract ${name} was not found.`);
  const fields = new Set();
  // #[serde(flatten)] inlines the inner struct's fields on the wire, matching
  // TypeScript interface inheritance.
  for (const match of body.matchAll(
    /(?:#\[serde\(flatten\)\]\s*\n\s*)?pub\s+([a-zA-Z0-9_]+)\s*:\s*([a-zA-Z0-9_]+)/g,
  )) {
    if (match[0].includes('#[serde(flatten)]')) {
      visiting.add(name);
      for (const field of rustFields(match[2], visiting)) fields.add(field);
    } else {
      fields.add(camelCase(match[1]));
    }
  }
  return fields;
}

const typeScriptInterfaces = new Map(
  [
    ...typeScriptSource.matchAll(
      /export interface\s+([a-zA-Z0-9_]+)(?:\s+extends\s+([a-zA-Z0-9_]+))?\s*\{([\s\S]*?)\n\}/g,
    ),
  ].map((match) => [match[1], { parent: match[2], body: match[3] }]),
);

function typeScriptFields(name, visiting = new Set()) {
  if (visiting.has(name)) throw new Error(`Circular TypeScript contract inheritance at ${name}.`);
  const contract = typeScriptInterfaces.get(name);
  if (!contract) throw new Error(`TypeScript contract ${name} was not found.`);
  const fields = new Set(
    [...contract.body.matchAll(/^\s*([a-zA-Z0-9_]+)\??\s*:/gm)].map((match) => match[1]),
  );
  if (contract.parent) {
    visiting.add(name);
    for (const field of typeScriptFields(contract.parent, visiting)) fields.add(field);
  }
  return fields;
}

for (const contract of contracts) {
  const rust = rustFields(contract);
  const typeScript = typeScriptFields(contract);
  const missing = [...rust].filter((field) => !typeScript.has(field));
  const extra = [...typeScript].filter((field) => !rust.has(field));
  if (missing.length > 0 || extra.length > 0) {
    throw new Error(
      `${contract} field mismatch; missing in TypeScript: ${missing.join(', ') || 'none'}; ` +
        `extra in TypeScript: ${extra.join(', ') || 'none'}`,
    );
  }
}

// Shared enums: the Rust variant (serde camelCase) must appear verbatim in the
// TypeScript union — either a named `export type` or an inline field union.
const enumContracts = [
  { rust: 'QuotaFormat', ts: { interface: 'QuotaWindow', field: 'format' } },
  { rust: 'MetricValueKind', ts: { interface: 'MetricValue', field: 'kind' } },
  { rust: 'StatusTone', ts: { interface: 'StatusMetric', field: 'tone' } },
  { rust: 'ProviderNoticeTone', ts: { interface: 'ProviderNotice', field: 'tone' } },
  { rust: 'SnapshotSource', ts: { interface: 'ProviderViewState', field: 'source' } },
  { rust: 'ProviderErrorKind', ts: { named: 'ProviderErrorKind' } },
  { rust: 'MetricSection', ts: { named: 'MetricSection' } },
  { rust: 'ApiKeyStatus', ts: { named: 'ApiKeyStatus' } },
  { rust: 'ThemePreference', ts: { interface: 'AppSettings', field: 'theme' } },
  { rust: 'AccentPreference', ts: { interface: 'AppSettings', field: 'accent' } },
  { rust: 'DensityPreference', ts: { interface: 'AppSettings', field: 'density' } },
  { rust: 'MenuBarStyle', ts: { interface: 'AppSettings', field: 'menuBarStyle' } },
  { rust: 'UsageDisplay', ts: { interface: 'AppSettings', field: 'usageDisplay' } },
  { rust: 'ResetDisplay', ts: { interface: 'AppSettings', field: 'resetDisplay' } },
  { rust: 'TimeFormatPreference', ts: { interface: 'AppSettings', field: 'timeFormat' } },
  { rust: 'LanguagePreference', ts: { interface: 'AppSettings', field: 'language' } },
  { rust: 'LogLevel', ts: { interface: 'AppSettings', field: 'logLevel' } },
  { rust: 'WindowMode', ts: { interface: 'AppSettings', field: 'windowMode' } },
  {
    rust: 'NotificationPermission',
    ts: { interface: 'SettingsViewState', field: 'notificationPermission' },
  },
];

function lowerFirst(value) {
  return value.length === 0 ? value : value[0].toLowerCase() + value.slice(1);
}

function rustEnumVariants(name) {
  const body = rustSource.match(new RegExp(`pub enum ${name}\\s*\\{([\\s\\S]*?)\\n\\}`))?.[1];
  if (!body) throw new Error(`Rust enum ${name} was not found.`);
  // Walk line by line: an optional #[serde(rename = "…")] binds to the next
  // variant; other attributes (#[default], #[serde(other)]) and docs are
  // skipped. #[serde(other)] only widens deserialization — the variant still
  // serializes under its own name, so it stays part of the wire contract.
  const variants = new Set();
  let pendingRename = null;
  for (const line of body.split('\n')) {
    const rename = line.match(/#\[serde\(rename = "([^"]+)"\)\]/);
    if (rename) {
      pendingRename = rename[1];
      continue;
    }
    if (/^\s*#\[/.test(line)) continue;
    const variant = line.match(/^\s*([A-Z][A-Za-z0-9_]*)(?:\s*=\s*\d+)?\s*,?\s*$/);
    if (variant) {
      variants.add(pendingRename ?? lowerFirst(variant[1]));
      pendingRename = null;
    }
  }
  if (variants.size === 0) throw new Error(`Rust enum ${name} has no unit variants.`);
  return variants;
}

function typeScriptLiterals(contract) {
  if (contract.named) {
    const match = typeScriptSource.match(
      new RegExp(`export type ${contract.named}\\s*=\\s*([^;]+);`),
    );
    if (!match) throw new Error(`TypeScript type ${contract.named} was not found.`);
    return new Set([...match[1].matchAll(/'([^']+)'/g)].map((literal) => literal[1]));
  }
  const body = typeScriptInterfaces.get(contract.interface)?.body;
  if (!body) throw new Error(`TypeScript interface ${contract.interface} was not found.`);
  const match = body.match(
    new RegExp(`${contract.field}\\??\\s*:\\s*((?:'[^']+'\\s*\\|\\s*)*'[^']+')`),
  );
  if (!match) {
    throw new Error(
      `${contract.interface}.${contract.field} is not a string-literal union; ` +
        `if the Rust type is no longer an enum, update the contract table.`,
    );
  }
  return new Set([...match[1].matchAll(/'([^']+)'/g)].map((literal) => literal[1]));
}

let enumContractsChecked = 0;
for (const contract of enumContracts) {
  const rust = rustEnumVariants(contract.rust);
  const typeScript = typeScriptLiterals(contract.ts);
  const missing = [...rust].filter((variant) => !typeScript.has(variant));
  const extra = [...typeScript].filter((literal) => !rust.has(literal));
  if (missing.length > 0 || extra.length > 0) {
    throw new Error(
      `${contract.rust} variant mismatch; missing in TypeScript: ${missing.join(', ') || 'none'}; ` +
        `extra in TypeScript: ${extra.join(', ') || 'none'}`,
    );
  }
  enumContractsChecked += 1;
}

console.log(
  `${contracts.length} Rust/TypeScript model field contracts and ` +
    `${enumContractsChecked} enum variant contracts match.`,
);
