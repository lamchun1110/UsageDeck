#[cfg(target_os = "macos")]
const MACOS_ITEM_NOT_FOUND: i32 = -25_300;

/// Per-service probe bookkeeping: completed answers are cached for the session
/// (keychain service existence is stable), and one in-flight probe per service
/// makes overlapping callers wait for that answer instead of failing. A probe
/// that exceeds its deadline is simply retried by the next caller, so a single
/// wedged search can no longer poison every later probe.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct ServiceProbeState {
    results: std::collections::HashMap<String, bool>,
    in_flight: std::collections::HashSet<String>,
}

#[cfg(target_os = "macos")]
static SERVICE_PROBES: std::sync::LazyLock<(
    std::sync::Mutex<ServiceProbeState>,
    std::sync::Condvar,
)> = std::sync::LazyLock::new(|| {
    (
        std::sync::Mutex::new(ServiceProbeState::default()),
        std::sync::Condvar::new(),
    )
});

#[cfg(target_os = "macos")]
pub fn generic_password_service_exists(
    service: &str,
    timeout: std::time::Duration,
) -> Option<bool> {
    use std::time::Instant;

    if timeout.is_zero() {
        return None;
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(|| Instant::now() + std::time::Duration::from_secs(60));
    let (probe_lock, probe_done) = &*SERVICE_PROBES;
    let mut probes = probe_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        if let Some(exists) = probes.results.get(service) {
            return Some(*exists);
        }
        if probes.in_flight.insert(service.to_owned()) {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let (guard, _) = probe_done
            .wait_timeout(probes, remaining)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        probes = guard;
    }
    drop(probes);

    let service = service.to_owned();
    let probe_service = service.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("usagedeck-keychain-probe".into())
        .spawn(move || {
            let _ = sender.send(generic_password_service_exists_blocking(&probe_service));
        })
        .is_err()
    {
        release_probe(service, None);
        return None;
    }
    let answer = receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
        .flatten();
    release_probe(service, answer);
    answer
}

#[cfg(target_os = "macos")]
fn release_probe(service: String, answer: Option<bool>) {
    let (probe_lock, probe_done) = &*SERVICE_PROBES;
    let mut probes = probe_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    probes.in_flight.remove(&service);
    if let Some(exists) = answer {
        probes.results.insert(service, exists);
    }
    drop(probes);
    probe_done.notify_all();
}

#[cfg(target_os = "macos")]
fn generic_password_service_exists_blocking(service: &str) -> Option<bool> {
    use security_framework::item::{ItemClass, ItemSearchOptions};

    match ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(service)
        .load_attributes(true)
        .skip_authenticated_items(true)
        .search()
    {
        Ok(_) => Some(true),
        Err(error) if error.code() == MACOS_ITEM_NOT_FOUND => Some(false),
        Err(_) => None,
    }
}

#[cfg(target_os = "macos")]
pub fn read_generic_password(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    use security_framework::passwords::{generic_password, PasswordOptions};

    match generic_password(PasswordOptions::new_generic_password(service, account)) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == MACOS_ITEM_NOT_FOUND => Ok(None),
        Err(_) => Err("The macOS Keychain could not be read.".into()),
    }
}

#[cfg(target_os = "macos")]
pub fn read_owned_password(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    read_generic_password(service, account)
}

/// Updates a Keychain item owned by another application, and never creates
/// one. `SecItemAdd` here would mint an item whose access control trusts only
/// UsageDeck, locking the owning CLI out of its own credential; an absent item
/// means the provider is logged out, which is the provider's to fix. Linux and
/// Windows already refuse to create through this path.
#[cfg(target_os = "macos")]
pub fn write_generic_password(service: &str, account: &str, value: &[u8]) -> Result<(), String> {
    use core_foundation::data::CFData;
    use security_framework::item::{
        update_item, ItemClass, ItemSearchOptions, ItemUpdateOptions, ItemUpdateValue,
    };

    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(account);
    let mut update = ItemUpdateOptions::new();
    update.set_value(ItemUpdateValue::Data(CFData::from_buffer(value)));

    match update_item(&search, &update) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == MACOS_ITEM_NOT_FOUND => {
            Err("The credential owned by the provider no longer exists.".into())
        }
        Err(_) => Err("The macOS Keychain could not be updated.".into()),
    }
}

#[cfg(target_os = "macos")]
pub fn delete_generic_password(service: &str, account: &str) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password as delete_password;

    match delete_password(service, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == MACOS_ITEM_NOT_FOUND => Ok(()),
        Err(_) => Err("The macOS Keychain item could not be removed.".into()),
    }
}

#[cfg(target_os = "windows")]
pub fn read_generic_password(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    use std::{ptr, slice};
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };
    let target = format!("{service}:{account}");
    let wide = target.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    let found = unsafe { CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if found == 0 {
        let code = std::io::Error::last_os_error().raw_os_error();
        return if code == Some(1168) {
            Ok(None)
        } else {
            Err("Windows Credential Manager could not be read.".into())
        };
    }
    if credential.is_null() {
        return Ok(None);
    }
    let value = unsafe {
        let credential_ref = &*credential;
        let bytes = slice::from_raw_parts(
            credential_ref.CredentialBlob,
            credential_ref.CredentialBlobSize as usize,
        )
        .to_vec();
        CredFree(credential.cast());
        bytes
    };
    Ok(Some(value))
}

#[cfg(target_os = "windows")]
pub fn generic_password_service_exists(
    service: &str,
    _timeout: std::time::Duration,
) -> Option<bool> {
    use std::{ptr, slice};
    use windows_sys::Win32::Security::Credentials::{CredEnumerateW, CredFree, CREDENTIALW};
    let filter = format!("{service}:*")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut count = 0_u32;
    let mut credentials: *mut *mut CREDENTIALW = ptr::null_mut();
    let found = unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut credentials) };
    if found == 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(1168) => Some(false),
            _ => None,
        };
    }
    let exists = !credentials.is_null()
        && unsafe { slice::from_raw_parts(credentials, count as usize) }
            .iter()
            .any(|credential| !credential.is_null());
    if !credentials.is_null() {
        unsafe { CredFree(credentials.cast()) };
    }
    Some(exists)
}

#[cfg(target_os = "windows")]
pub fn read_owned_password(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    read_generic_password(service, account)
}

#[cfg(target_os = "windows")]
pub fn write_generic_password(_service: &str, _account: &str, _value: &[u8]) -> Result<(), String> {
    Err("UsageDeck does not overwrite credentials owned by another Windows application.".into())
}

#[cfg(target_os = "windows")]
pub fn write_owned_password(service: &str, account: &str, value: &[u8]) -> Result<(), String> {
    use std::ptr;
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };
    let target = format!("{service}:{account}")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let username = account.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let comment = format!("UsageDeck {account} API key")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut blob = zeroize::Zeroizing::new(value.to_vec());
    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr().cast_mut(),
        Comment: comment.as_ptr().cast_mut(),
        LastWritten: Default::default(),
        CredentialBlobSize: u32::try_from(blob.len())
            .map_err(|_| "The API key is too large for Windows Credential Manager.")?,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: ptr::null_mut(),
        TargetAlias: ptr::null_mut(),
        UserName: username.as_ptr().cast_mut(),
    };
    let written = unsafe { CredWriteW(&credential, 0) };
    if written == 0 {
        Err("Windows Credential Manager could not be updated.".into())
    } else {
        Ok(())
    }
}

/// Writes an item UsageDeck owns (the saved-key service), where creating the
/// item is the point. Deliberately not routed through `write_generic_password`,
/// which must never create.
#[cfg(target_os = "macos")]
pub fn write_owned_password(service: &str, account: &str, value: &[u8]) -> Result<(), String> {
    security_framework::passwords::set_generic_password(service, account, value)
        .map_err(|_| "The macOS Keychain could not be updated.".into())
}

#[cfg(target_os = "windows")]
pub fn delete_generic_password(service: &str, account: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = format!("{service}:{account}")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if deleted != 0 {
        return Ok(());
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(1168) => Ok(()),
        _ => Err("Windows Credential Manager item could not be removed.".into()),
    }
}

#[cfg(target_os = "windows")]
pub fn delete_owned_password(service: &str, account: &str) -> Result<(), String> {
    delete_generic_password(service, account)
}

#[cfg(target_os = "macos")]
pub fn delete_owned_password(service: &str, account: &str) -> Result<(), String> {
    delete_generic_password(service, account)
}

#[cfg(target_os = "linux")]
const SECRET_SERVICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Bound potentially wedged Secret Service calls while still allowing the
/// startup detector to probe several providers concurrently. Calls above the
/// limit wait for a worker slot within their own deadline instead of spawning
/// another thread.
#[cfg(target_os = "linux")]
const SECRET_SERVICE_MAX_WORKERS: usize = 4;

#[cfg(target_os = "linux")]
static SECRET_SERVICE_ACTIVE_WORKERS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "linux")]
struct SecretServiceWorkerGuard;

#[cfg(target_os = "linux")]
impl Drop for SecretServiceWorkerGuard {
    fn drop(&mut self) {
        SECRET_SERVICE_ACTIVE_WORKERS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(target_os = "linux")]
fn acquire_secret_service_worker(
    timeout: std::time::Duration,
) -> Result<SecretServiceWorkerGuard, String> {
    let started = std::time::Instant::now();
    loop {
        let active = SECRET_SERVICE_ACTIVE_WORKERS.load(std::sync::atomic::Ordering::Acquire);
        if active < SECRET_SERVICE_MAX_WORKERS
            && SECRET_SERVICE_ACTIVE_WORKERS
                .compare_exchange_weak(
                    active,
                    active + 1,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                )
                .is_ok()
        {
            return Ok(SecretServiceWorkerGuard);
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(linux_secret_service_operation_pending());
        }
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(5)));
    }
}

/// Runs a Secret Service operation with a deadline. A keyring D-Bus call with no daemon can
/// block forever, which would wedge the provider refresh worker; the bounded wait keeps the
/// failure reportable and the worker limiter caps abandoned threads process-wide.
#[cfg(target_os = "linux")]
fn with_secret_service_timeout<T, F>(operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    with_secret_service_deadline(operation, SECRET_SERVICE_TIMEOUT)
}

#[cfg(target_os = "linux")]
fn with_secret_service_deadline<T, F>(
    operation: F,
    timeout: std::time::Duration,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    use std::sync::mpsc;
    let started = std::time::Instant::now();
    let guard = acquire_secret_service_worker(timeout)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("usagedeck-secret-service".into())
        .spawn(move || {
            let result = operation();
            drop(guard);
            let _ = sender.send(result);
        })
        .is_err()
    {
        return Err("Linux Secret Service worker could not be started.".into());
    }
    let remaining = timeout.saturating_sub(started.elapsed());
    receiver
        .recv_timeout(remaining)
        .unwrap_or_else(|_| Err(linux_secret_service_timed_out()))
}

#[cfg(target_os = "linux")]
pub fn read_generic_password(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    let service = service.to_owned();
    let account = account.to_owned();
    with_secret_service_timeout(move || read_generic_password_blocking(&service, &account))
}

#[cfg(target_os = "linux")]
fn read_generic_password_blocking(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;

    let secret_service = SecretService::connect(EncryptionType::Dh)
        .map_err(|_| linux_secret_service_unavailable())?;
    let mut matches = secret_service
        .search_items(HashMap::from([("service", service), ("username", account)]))
        .map_err(|_| "Linux Secret Service could not be searched.")?;
    if let Some(item) = matches.unlocked.pop() {
        return item
            .get_secret()
            .map(Some)
            .map_err(|_| "Linux Secret Service item could not be read.".into());
    }
    let Some(item) = matches.locked.pop() else {
        return Ok(None);
    };
    item.unlock()
        .map_err(|_| "Linux Secret Service item could not be unlocked.")?;
    item.get_secret()
        .map(Some)
        .map_err(|_| "Linux Secret Service item could not be read.".into())
}

#[cfg(target_os = "linux")]
pub fn generic_password_service_exists(
    service: &str,
    timeout: std::time::Duration,
) -> Option<bool> {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;
    if timeout.is_zero() {
        return None;
    }
    let service = service.to_owned();
    with_secret_service_deadline(
        move || {
            let result = (|| {
                let secret_service = SecretService::connect(EncryptionType::Dh).ok()?;
                let matches = secret_service
                    .search_items(HashMap::from([("service", service.as_str())]))
                    .ok()?;
                Some(!matches.unlocked.is_empty() || !matches.locked.is_empty())
            })();
            Ok(result)
        },
        timeout,
    )
    .ok()
    .flatten()
}

#[cfg(target_os = "linux")]
pub fn read_owned_password(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    read_generic_password(service, account)
}

#[cfg(target_os = "linux")]
pub fn write_generic_password(service: &str, account: &str, value: &[u8]) -> Result<(), String> {
    let service = service.to_owned();
    let account = account.to_owned();
    let value = zeroize::Zeroizing::new(value.to_vec());
    with_secret_service_timeout(move || write_generic_password_blocking(&service, &account, &value))
}

#[cfg(target_os = "linux")]
fn write_generic_password_blocking(
    service: &str,
    account: &str,
    value: &[u8],
) -> Result<(), String> {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;

    let secret_service = SecretService::connect(EncryptionType::Dh)
        .map_err(|_| linux_secret_service_unavailable())?;
    let mut matches = secret_service
        .search_items(HashMap::from([("service", service), ("username", account)]))
        .map_err(|_| "Linux Secret Service could not be searched.")?;
    let item = matches
        .unlocked
        .pop()
        .or_else(|| matches.locked.pop())
        .ok_or("The credential owned by the provider no longer exists.")?;
    item.unlock()
        .map_err(|_| "Linux Secret Service item could not be unlocked.")?;
    item.set_secret(value, "text/plain; charset=utf8")
        .map_err(|_| "Linux Secret Service item could not be updated.".into())
}

#[cfg(target_os = "linux")]
pub fn write_owned_password(service: &str, account: &str, value: &[u8]) -> Result<(), String> {
    let service = service.to_owned();
    let account = account.to_owned();
    let value = zeroize::Zeroizing::new(value.to_vec());
    with_secret_service_timeout(move || write_owned_password_blocking(&service, &account, &value))
}

#[cfg(target_os = "linux")]
fn write_owned_password_blocking(service: &str, account: &str, value: &[u8]) -> Result<(), String> {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;

    let secret_service = SecretService::connect(EncryptionType::Dh)
        .map_err(|_| linux_secret_service_unavailable())?;
    let collection = secret_service
        .get_default_collection()
        .or_else(|_| secret_service.create_collection("UsageDeck", "default"))
        .map_err(|_| {
            "The Linux Secret Service has no usable default collection. Start or unlock your keyring and try again."
        })?;
    collection.ensure_unlocked().map_err(|_| {
        "The Linux Secret Service collection is locked. Unlock your keyring and try again."
    })?;
    collection
        .create_item(
            &format!("UsageDeck {account} API Key"),
            HashMap::from([("service", service), ("username", account)]),
            value,
            true,
            "text/plain; charset=utf8",
        )
        .map(|_| ())
        .map_err(|_| "Linux Secret Service could not save the API key.".into())
}

#[cfg(target_os = "linux")]
pub fn delete_generic_password(service: &str, account: &str) -> Result<(), String> {
    let service = service.to_owned();
    let account = account.to_owned();
    with_secret_service_timeout(move || delete_generic_password_blocking(&service, &account))
}

#[cfg(target_os = "linux")]
fn delete_generic_password_blocking(service: &str, account: &str) -> Result<(), String> {
    use secret_service::{blocking::SecretService, EncryptionType};
    use std::collections::HashMap;

    let secret_service = SecretService::connect(EncryptionType::Dh)
        .map_err(|_| linux_secret_service_unavailable())?;
    let matches = secret_service
        .search_items(HashMap::from([("service", service), ("username", account)]))
        .map_err(|_| "Linux Secret Service could not be searched.")?;
    for item in matches.unlocked {
        item.delete()
            .map_err(|_| "Linux Secret Service item could not be removed.")?;
    }
    for item in matches.locked {
        item.unlock()
            .map_err(|_| "Linux Secret Service item could not be unlocked for removal.")?;
        item.delete()
            .map_err(|_| "Linux Secret Service item could not be removed.")?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn delete_owned_password(service: &str, account: &str) -> Result<(), String> {
    delete_generic_password(service, account)
}

#[cfg(target_os = "linux")]
fn linux_secret_service_unavailable() -> String {
    "Linux Secret Service is unavailable. Start a Secret Service-compatible keyring, such as GNOME Keyring or KWallet, and try again."
        .into()
}

#[cfg(target_os = "linux")]
fn linux_secret_service_timed_out() -> String {
    "Linux Secret Service did not respond in time. Check that a Secret Service-compatible keyring, such as GNOME Keyring or KWallet, is running and unlocked."
        .into()
}

#[cfg(target_os = "linux")]
fn linux_secret_service_operation_pending() -> String {
    "A previous Linux Secret Service request is still pending. Check that the keyring is running and unlocked before trying again."
        .into()
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn generic_password_service_exists(
    _service: &str,
    _timeout: std::time::Duration,
) -> Option<bool> {
    Some(false)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn read_generic_password(_service: &str, _account: &str) -> Result<Option<Vec<u8>>, String> {
    Ok(None)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn read_owned_password(_service: &str, _account: &str) -> Result<Option<Vec<u8>>, String> {
    Ok(None)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn write_generic_password(_service: &str, _account: &str, _value: &[u8]) -> Result<(), String> {
    Err("The system credential store is unavailable on this platform.".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn write_owned_password(_service: &str, _account: &str, _value: &[u8]) -> Result<(), String> {
    Err("The system credential store is unavailable on this platform.".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn delete_generic_password(_service: &str, _account: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn delete_owned_password(_service: &str, _account: &str) -> Result<(), String> {
    Ok(())
}

pub fn decode_go_keyring_value(value: &[u8]) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let text = std::str::from_utf8(value).ok()?.trim();
    let encoded = text.strip_prefix("go-keyring-base64:")?;
    String::from_utf8(STANDARD.decode(encoded).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::STANDARD, Engine};

    use super::decode_go_keyring_value;

    #[test]
    fn decodes_go_keyring_wrapped_json() {
        let json = r#"{"access_token":"placeholder"}"#;
        let wrapped = format!("go-keyring-base64:{}", STANDARD.encode(json));
        assert_eq!(
            decode_go_keyring_value(wrapped.as_bytes()).as_deref(),
            Some(json)
        );
        assert!(decode_go_keyring_value(b"plain text").is_none());
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    #[test]
    fn system_credential_store_round_trip_when_requested() {
        if std::env::var("USAGEDECK_TEST_CREDENTIAL_STORE").as_deref() != Ok("1") {
            return;
        }

        let service = format!(
            "com.lamchun1110.usagedeck.credential-test.{}",
            std::process::id()
        );
        let account = "round-trip";
        let result = (|| -> Result<(), String> {
            super::delete_owned_password(&service, account)?;
            super::write_owned_password(&service, account, b"first-value")?;
            if super::read_owned_password(&service, account)?.as_deref()
                != Some(b"first-value".as_slice())
            {
                return Err("The first credential round-trip value did not match.".into());
            }
            super::write_owned_password(&service, account, b"second-value")?;
            if super::read_owned_password(&service, account)?.as_deref()
                != Some(b"second-value".as_slice())
            {
                return Err("The updated credential round-trip value did not match.".into());
            }
            Ok(())
        })();
        let cleanup = super::delete_owned_password(&service, account);

        result.unwrap();
        cleanup.unwrap();
        assert!(super::read_owned_password(&service, account)
            .unwrap()
            .is_none());
    }

    /// Writing a credential owned by a provider must never bring the item into
    /// existence: an item created here would carry an access control that
    /// trusts only UsageDeck, locking the owning CLI out of its own login.
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    #[test]
    fn writing_a_missing_provider_credential_fails_without_creating_it() {
        if std::env::var("USAGEDECK_TEST_CREDENTIAL_STORE").as_deref() != Ok("1") {
            return;
        }

        let service = format!(
            "com.lamchun1110.usagedeck.absent-test.{}",
            std::process::id()
        );
        let account = "never-created";
        super::delete_owned_password(&service, account).unwrap();

        let write = super::write_generic_password(&service, account, b"value");
        let read_back = super::read_generic_password(&service, account);
        let cleanup = super::delete_owned_password(&service, account);

        assert!(
            write.is_err(),
            "an absent provider credential must not be created"
        );
        assert_eq!(
            read_back.unwrap(),
            None,
            "no keychain item may be left behind"
        );
        cleanup.unwrap();
    }
}
