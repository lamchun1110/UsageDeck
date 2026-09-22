import fs from 'node:fs';

// Every locale file must carry the same keys as English, and each translated value must
// keep the `{placeholder}` names its English source uses. A locale that drifts here fails
// silently at runtime — `t()` falls back to English and then to the raw key — so the
// contract is checked before the app ships instead.
const root = new URL('../../', import.meta.url);
const locales = ['en', 'zh-TW', 'zh-CN', 'ja', 'ko'];
const referenceLocale = 'en';

// Keys and values are string literals; a value may be split onto its own line or span
// several, so the value pattern is allowed to cross newlines. `\\.` keeps escaped quotes
// from ending the literal early.
const entryPattern = /^\s*(?:'([^']+)'|"([^"]+)")\s*:\s*('(?:\\.|[^'])*'|"(?:\\.|[^"])*")/gm;
const placeholderPattern = /\{(\w+)\}/g;

function readCatalog(locale) {
  const source = fs.readFileSync(new URL(`src/lib/messages/${locale}.ts`, root), 'utf8');
  const entries = new Map();
  for (const match of source.matchAll(entryPattern)) {
    const key = match[1] ?? match[2];
    if (entries.has(key)) throw new Error(`Duplicate translation key "${key}" in ${locale}.ts.`);
    entries.set(key, match[3].slice(1, -1));
  }
  if (entries.size === 0) throw new Error(`No translation keys found in ${locale}.ts.`);
  return entries;
}

function placeholders(value) {
  return [...value.matchAll(placeholderPattern)].map((match) => match[1]).sort();
}

const reference = readCatalog(referenceLocale);
const problems = [];

for (const locale of locales) {
  if (locale === referenceLocale) continue;
  const catalog = readCatalog(locale);

  for (const key of reference.keys()) {
    if (!catalog.has(key)) problems.push(`${locale} is missing "${key}".`);
  }
  for (const key of catalog.keys()) {
    if (!reference.has(key)) problems.push(`${locale} has extra key "${key}".`);
  }

  for (const [key, value] of catalog) {
    const source = reference.get(key);
    if (source === undefined) continue;
    const expected = placeholders(source).join(',');
    const actual = placeholders(value).join(',');
    if (expected !== actual) {
      problems.push(
        `${locale} "${key}" placeholders {${actual}} do not match ${referenceLocale} {${expected}}.`,
      );
    }
  }
}

if (problems.length > 0) {
  throw new Error(`Translation contract is out of sync:\n  ${problems.join('\n  ')}`);
}

console.log(
  `${reference.size} translation keys match across ${locales.length} locales, with placeholder parity.`,
);
