//! Runtime access to the provider options the user selected in settings.
//!
//! Providers are constructed before the settings service exists, so they cannot be handed a
//! settings handle. Settings instead publish the current selections here whenever they load or
//! change, mirroring how `logging::set_level` distributes a live preference. Each publish swaps
//! the whole map rather than mutating it in place.

use std::{collections::BTreeMap, sync::RwLock};

type Selections = BTreeMap<String, BTreeMap<String, String>>;

static SELECTIONS: RwLock<Selections> = RwLock::new(BTreeMap::new());

/// Replaces the published selections. Settings call this after normalization, so every value
/// here is already known to be one of the provider's declared choices.
pub fn publish(selections: &Selections) {
    if let Ok(mut current) = SELECTIONS.write() {
        *current = selections.clone();
    }
}

/// The stored choice for one provider option, or `None` when the user has not picked one.
///
/// Callers resolve `None` through their own `ProviderOption`, so a poisoned lock or an empty
/// store degrades to the provider's default rather than to an error.
pub fn selection(provider_id: &str, option_id: &str) -> Option<String> {
    SELECTIONS
        .read()
        .ok()?
        .get(provider_id)?
        .get(option_id)
        .cloned()
}

/// Serializes tests that publish or read the process-global selection map.
/// `normalize` replaces the whole map, so a parallel test's normalization
/// wipes what another test just published between its write and read.
#[cfg(test)]
pub(crate) fn selections_guard() -> std::sync::MutexGuard<'static, ()> {
    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    GUARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{publish, selection, selections_guard};

    #[test]
    fn publishes_and_reads_back_a_selection() {
        let _guard = selections_guard();
        let mut provider = BTreeMap::new();
        provider.insert("endpoint".to_owned(), "global".to_owned());
        let mut selections = BTreeMap::new();
        selections.insert("published-provider".to_owned(), provider);

        publish(&selections);

        assert_eq!(
            selection("published-provider", "endpoint").as_deref(),
            Some("global")
        );
        assert_eq!(selection("published-provider", "missing-option"), None);
        assert_eq!(selection("missing-provider", "endpoint"), None);
    }
}
