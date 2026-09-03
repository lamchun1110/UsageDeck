#!/bin/bash
# Regenerates the bundled pricing snapshots from the live feeds:
#
#   src-tauri/resources/pricing_litellm_snapshot.json      (LiteLLM model_prices)
#   src-tauri/resources/pricing_models_dev_snapshot.json   (models.dev api.json)
#   src-tauri/resources/pricing_openrouter_snapshot.json   (OpenRouter /api/v1/models)
#
# The snapshots are the offline fallback for first launch / no network; at runtime the app fetches
# the same feeds daily and its disk cache overrides these. Staleness is therefore harmless, but
# refreshing them at release time keeps first launches accurate. Run from the repo root:
#
#   bash scripts/update_pricing_snapshots.sh
#
# The compact format must stay in sync with src-tauri/src/pricing/codecs.rs (compact codec + the
# defaulting rules of the LiteLLM/models.dev parsers): per-million rates, cache write defaults to
# the input rate, cache read to a tenth of it. After regenerating, `cargo test` exercises the
# snapshots via the pricing resolution tests.
set -euo pipefail

cd "$(dirname "$0")/.."
RESOURCES="src-tauri/resources"

LITELLM_URL="https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json"
MODELS_DEV_URL="https://models.dev/api.json"
OPENROUTER_URL="https://openrouter.ai/api/v1/models"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

echo "Fetching LiteLLM pricing..."
curl -fsSL --max-time 120 "$LITELLM_URL" -o "$tmpdir/litellm.json"
echo "Fetching models.dev pricing..."
curl -fsSL --max-time 120 "$MODELS_DEV_URL" -o "$tmpdir/models_dev.json"
echo "Fetching OpenRouter pricing..."
curl -fsSL --max-time 120 "$OPENROUTER_URL" -o "$tmpdir/openrouter.json"

python3 - "$tmpdir" "$RESOURCES" << 'PY'
import json
import math
import sys
from datetime import datetime, timezone

tmpdir, resources = sys.argv[1], sys.argv[2]
retrieved_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def compact_model(input_pm, output_pm, cache_write_pm, cache_read_pm,
                  ia=None, oa=None, cwa=None, cra=None, fast=None, cre=None, cw1=None):
    model = {"i": input_pm, "o": output_pm, "cw": cache_write_pm, "cr": cache_read_pm}
    for key, value in (("ia", ia), ("oa", oa), ("cwa", cwa), ("cra", cra), ("fast", fast),
                       ("cre", cre), ("cw1", cw1)):
        if value is not None:
            model[key] = value
    return model

def number(value):
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None

# Mirrors MAX_PLAUSIBLE_RATE_PER_MILLION and ModelRates::is_plausible in
# src-tauri/src/pricing/rates.rs - keep the two in step. LiteLLM has published
# input_cost_per_token in the wrong unit (0.135 rather than 0.000000135), which
# lands at $135,000 per million; the dearest real rate any feed states is o1-pro
# at $600 out. "fast" is a multiplier and "cre" a flag, so neither is a rate.
MAX_PLAUSIBLE_RATE_PER_MILLION = 1000.0
RATE_KEYS = ("i", "o", "cw", "cr", "ia", "oa", "cwa", "cra", "cw1")

def plausible(model):
    for key in RATE_KEYS:
        value = model.get(key)
        if value is None:
            continue
        if not math.isfinite(value) or not 0.0 <= value <= MAX_PLAUSIBLE_RATE_PER_MILLION:
            return False
    return True

# OpenRouter quotes every rate as a decimal string.
def string_number(value):
    if isinstance(value, str):
        try:
            return float(value.strip())
        except ValueError:
            return None
    return number(value)

# LiteLLM: costs are per token; entries without both input and output cost are stubs -> skipped.
with open(f"{tmpdir}/litellm.json") as f:
    litellm = json.load(f)
models = {}
for key, entry in litellm.items():
    if not isinstance(entry, dict):
        continue
    i, o = number(entry.get("input_cost_per_token")), number(entry.get("output_cost_per_token"))
    if i is None or o is None:
        continue
    cw = number(entry.get("cache_creation_input_token_cost"))
    cr = number(entry.get("cache_read_input_token_cost"))
    provider_specific = entry.get("provider_specific_entry") or {}
    entry = compact_model(
        i * 1e6, o * 1e6,
        (cw if cw is not None else i) * 1e6,
        (cr if cr is not None else i * 0.1) * 1e6,
        ia=(lambda v: v * 1e6 if v is not None else None)(number(entry.get("input_cost_per_token_above_200k_tokens"))),
        oa=(lambda v: v * 1e6 if v is not None else None)(number(entry.get("output_cost_per_token_above_200k_tokens"))),
        cwa=(lambda v: v * 1e6 if v is not None else None)(number(entry.get("cache_creation_input_token_cost_above_200k_tokens"))),
        cra=(lambda v: v * 1e6 if v is not None else None)(number(entry.get("cache_read_input_token_cost_above_200k_tokens"))),
        fast=number(provider_specific.get("fast")) if isinstance(provider_specific, dict) else None,
    )
    if plausible(entry):
        models[key] = entry
if not models:
    sys.exit("LiteLLM feed produced no usable entries - aborting.")
with open(f"{resources}/pricing_litellm_snapshot.json", "w") as f:
    json.dump({"retrieved_at": retrieved_at, "models": models}, f, sort_keys=True, separators=(",", ":"))
print(f"pricing_litellm_snapshot.json: {len(models)} models")

# models.dev: costs are already per million; ids stored bare, first provider (sorted) wins.
with open(f"{tmpdir}/models_dev.json") as f:
    models_dev = json.load(f)
models = {}
for provider_name in sorted(models_dev):
    provider = models_dev[provider_name]
    if not isinstance(provider, dict):
        continue
    for model_id, model in (provider.get("models") or {}).items():
        if model_id in models or not isinstance(model, dict):
            continue
        cost = model.get("cost") or {}
        i, o = number(cost.get("input")), number(cost.get("output"))
        if i is None or o is None:
            continue
        cw, cr = number(cost.get("cache_write")), number(cost.get("cache_read"))
        entry = compact_model(
            i, o,
            cw if cw is not None else i,
            cr if cr is not None else i * 0.1,
        )
        if plausible(entry):
            models[model_id] = entry
if not models:
    sys.exit("models.dev feed produced no usable entries - aborting.")
with open(f"{resources}/pricing_models_dev_snapshot.json", "w") as f:
    json.dump({"retrieved_at": retrieved_at, "models": models}, f, sort_keys=True, separators=(",", ":"))
print(f"pricing_models_dev_snapshot.json: {len(models)} models")

# OpenRouter: costs are per token and quoted as strings. Variant slugs (":batch", ":free",
# ":nitro", ":floor") are routing tiers, not models, and would give fuzzy matching a cheaper twin
# of every model, so they are dropped along with zero-rated entries. pricing.overrides holds
# time-of-day discounts that cannot be applied to historical usage, so it is ignored. This mirrors
# catalog_from_openrouter in src-tauri/src/pricing/codecs.rs - keep the two in step.
with open(f"{tmpdir}/openrouter.json") as f:
    openrouter = json.load(f)
models = {}
for entry in openrouter.get("data") or []:
    if not isinstance(entry, dict):
        continue
    model_id = entry.get("id")
    if not isinstance(model_id, str) or ":" in model_id:
        continue
    pricing = entry.get("pricing")
    if not isinstance(pricing, dict):
        continue
    i, o = string_number(pricing.get("prompt")), string_number(pricing.get("completion"))
    if i is None or o is None or (i <= 0 and o <= 0):
        continue
    cw = string_number(pricing.get("input_cache_write"))
    cr = string_number(pricing.get("input_cache_read"))
    cw1 = string_number(pricing.get("input_cache_write_1h"))
    entry = compact_model(
        i * 1e6, o * 1e6,
        (cw if cw is not None else i) * 1e6,
        (cr if cr is not None else i * 0.1) * 1e6,
        cre=False if cr is None else None,
        cw1=cw1 * 1e6 if cw1 is not None else None,
    )
    if plausible(entry):
        models[model_id] = entry
if not models:
    sys.exit("OpenRouter feed produced no usable entries - aborting.")
with open(f"{resources}/pricing_openrouter_snapshot.json", "w") as f:
    json.dump({"retrieved_at": retrieved_at, "models": models}, f, sort_keys=True, separators=(",", ":"))
print(f"pricing_openrouter_snapshot.json: {len(models)} models")
PY

ls -lh "$RESOURCES"/pricing_*_snapshot.json
