//! Core-only chat binding policy. Send, manual extraction, queueing, and rebuild share it.

use crate::data::preferences::ActivePromptManager::ActivePromptManager;
use crate::data::preferences::CharacterCardManager::CharacterCardManager;
use operit_model::ChatHistory::ChatHistory;
use operit_providers::runtime_support::ProviderRuntimeContext;
use operit_store::PreferencesDataStore::PreferencesDataStoreError;

/// Mirrors the normal-send role selection, including group/unbound active-card fallback.
pub(crate) fn resolveRoleCardId(
    chat: Option<&ChatHistory>,
    cards: &CharacterCardManager,
) -> Result<String, PreferencesDataStoreError> {
    resolveBinding(
        chat.and_then(|chat| chat.characterCardName.as_deref()),
        chat.and_then(|chat| chat.characterGroupId.as_deref()),
        |name| Ok(cards.findCharacterCardByName(name)?.map(|card| card.id)),
        || ActivePromptManager::getInstance().resolveActiveCardIdForSend(),
    )
}

/// Provider runtime support owns the character-to-shared memory mapping.
pub(crate) fn resolveMemoryOwner(
    runtime: &ProviderRuntimeContext,
    chat: &ChatHistory,
    cards: &CharacterCardManager,
) -> Result<String, String> {
    let role = resolveRoleCardId(Some(chat), cards).map_err(|error| error.to_string())?;
    runtime.support().memoryOwnerKeyForCharacterCard(&role)
}

fn resolveBinding<E>(
    cardName: Option<&str>,
    groupId: Option<&str>,
    lookup: impl FnOnce(&str) -> Result<Option<String>, E>,
    fallback: impl FnOnce() -> Result<String, E>,
) -> Result<String, E> {
    let hasGroup = groupId.is_some_and(|value| !value.trim().is_empty());
    if !hasGroup {
        if let Some(name) = cardName.map(str::trim).filter(|name| !name.is_empty()) {
            if let Some(id) = lookup(name)? {
                return Ok(id);
            }
        }
    }
    fallback()
}

#[cfg(test)]
mod tests {
    use super::resolveBinding;

    #[test]
    fn explicit_card_wins_over_active_card() {
        let resolved = resolveBinding::<String>(
            Some(" bound "),
            None,
            |name| {
                assert_eq!(name, "bound");
                Ok(Some("bound-id".into()))
            },
            || panic!("bound chat must not use the active card"),
        );
        assert_eq!(resolved.unwrap(), "bound-id");
    }

    #[test]
    fn group_chat_uses_active_card_without_looking_up_stale_single_card_binding() {
        let resolved = resolveBinding::<String>(
            Some("stale card"),
            Some("group-id"),
            |_| panic!("group chat must not resolve its stale single-card binding"),
            || Ok("active-id".into()),
        );
        assert_eq!(resolved.unwrap(), "active-id");
    }

    #[test]
    fn missing_and_unknown_binding_use_active_card() {
        for name in [None, Some(" "), Some("removed card")] {
            let resolved =
                resolveBinding::<String>(name, Some(" "), |_| Ok(None), || Ok("active-id".into()));
            assert_eq!(resolved.unwrap(), "active-id");
        }
    }

    #[test]
    fn lookup_failure_is_not_silently_converted_to_another_owner() {
        let resolved = resolveBinding(
            Some("bound"),
            None,
            |_| Err::<Option<String>, _>("storage failed"),
            || panic!("storage errors must not cause writes to a fallback owner"),
        );
        assert_eq!(resolved.unwrap_err(), "storage failed");
    }
}
