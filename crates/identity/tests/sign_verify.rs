//! Black-box tests for `identity::context`, exercised only through its public API. Living as an
//! integration test (rather than `#[cfg(test)]` inside `context.rs`) is itself part of the proof
//! for ARIA-28's central property: everything here is reachable from outside the crate.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use identity::context::{Role, SigningKey, VerifyingKeySet, sign, verify};
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Test-only Ed25519 keypairs, generated once with
//   openssl genpkey -algorithm ed25519 -out priv.pem
//   openssl pkey -in priv.pem -pubout -out pub.pem
// These are not real secrets -- fine to hardcode as fixtures.

const PRIV_1: &str = "-----BEGIN PRIVATE KEY-----\n\
MC4CAQAwBQYDK2VwBCIEIML/fremsJJpKFIw0dGjCsdNMD16F/05s5CWCsAH93Ti\n\
-----END PRIVATE KEY-----\n";
const PUB_1: &str = "-----BEGIN PUBLIC KEY-----\n\
MCowBQYDK2VwAyEAKbagvmvNeKsVeLqUvme3PkqFke70gWIo5OqnedzzEzM=\n\
-----END PUBLIC KEY-----\n";

const PUB_2: &str = "-----BEGIN PUBLIC KEY-----\n\
MCowBQYDK2VwAyEADSsEyJ5PqbPiHqwgV1A6dO31jkJovZpohSfgdYwn5z8=\n\
-----END PUBLIC KEY-----\n";

const PRIV_3: &str = "-----BEGIN PRIVATE KEY-----\n\
MC4CAQAwBQYDK2VwBCIEIJxmOFX7V3vYbJSZ2/2GVSZpAtW7iCtCgiNIFkaMSjVB\n\
-----END PRIVATE KEY-----\n";
const PUB_3: &str = "-----BEGIN PUBLIC KEY-----\n\
MCowBQYDK2VwAyEAr5ZhAJY1mXSrXZR2Mu4QbFjKADFTWBJUhMGUEdgcEss=\n\
-----END PUBLIC KEY-----\n";

const AUD: &str = "aria-gateway";
const ISS: &str = "aria-identity";

fn signing_key(pem: &str) -> SigningKey {
    SigningKey::new(EncodingKey::from_ed_pem(pem.as_bytes()).unwrap()).unwrap()
}

fn verifying_set(pem: &str) -> VerifyingKeySet {
    VerifyingKeySet::new(DecodingKey::from_ed_pem(pem.as_bytes()).unwrap()).unwrap()
}

fn now() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

// Splits a JWT, decodes+mutates the payload segment via `mutate`, and reassembles it with the
// original header and signature -- so the signature is now over stale bytes and must fail.
fn tamper_payload(token: &str, mutate: impl FnOnce(&mut Value)) -> String {
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "expected header.payload.signature");
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).unwrap();
    let mut payload: Value = serde_json::from_slice(&payload_bytes).unwrap();
    mutate(&mut payload);
    let new_payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
    format!("{}.{}.{}", parts[0], new_payload, parts[2])
}

// Same idea as `tamper_payload`, but for the header segment -- used to construct tokens whose
// header is missing fields (e.g. `kid`) that a hand-built `jsonwebtoken::Header` could also
// produce, without needing access to this crate's private `Claims` type.
fn tamper_header(token: &str, mutate: impl FnOnce(&mut Value)) -> String {
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "expected header.payload.signature");
    let header_bytes = URL_SAFE_NO_PAD.decode(parts[0]).unwrap();
    let mut header: Value = serde_json::from_slice(&header_bytes).unwrap();
    mutate(&mut header);
    let new_header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    format!("{}.{}.{}", new_header, parts[1], parts[2])
}

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

// Writes `contents` to a fresh, uniquely-named file under the OS temp dir, for exercising the
// `*_path`/`*_cert` constructors without committing key material to the repo. Unique per call
// (pid + monotonic counter + wall-clock nanos) so parallel test runs never collide.
fn write_temp_pem(label: &str, contents: &str) -> PathBuf {
    let n = FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("identity-test-{label}-{pid}-{n}-{nanos}.pem"));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn round_trip_succeeds() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User, Role::Admin],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let ctx = verify(&token, &[AUD], ISS, &verifying).unwrap();
    assert_eq!(ctx.user_id(), "user-42");
    assert_eq!(ctx.roles(), &[Role::User, Role::Admin]);
}

#[test]
fn tampered_user_id_is_rejected() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let tampered = tamper_payload(&token, |payload| {
        payload["user_id"] = json!("attacker");
    });

    assert!(verify(&tampered, &[AUD], ISS, &verifying).is_err());
}

#[test]
fn tampered_roles_are_rejected() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let tampered = tamper_payload(&token, |payload| {
        payload["roles"] = json!(["aria-admin"]);
    });

    assert!(verify(&tampered, &[AUD], ISS, &verifying).is_err());
}

#[test]
fn unsigned_context_is_rejected() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let parts: Vec<&str> = token.split('.').collect();
    let unsigned = format!("{}.{}.", parts[0], parts[1]);

    assert!(verify(&unsigned, &[AUD], ISS, &verifying).is_err());
}

#[test]
fn unknown_signing_key_is_rejected() {
    let signing = signing_key(PRIV_1);
    let unrelated_verifying = verifying_set(PUB_2);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    assert!(verify(&token, &[AUD], ISS, &unrelated_verifying).is_err());
}

#[test]
fn expired_context_is_rejected() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() - 3600; // already expired when signed

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    assert!(verify(&token, &[AUD], ISS, &verifying).is_err());
}

#[test]
fn old_key_still_valid_during_rotation_overlap() {
    let old_signing = signing_key(PRIV_1);
    let new_decoding = DecodingKey::from_ed_pem(PUB_3.as_bytes()).unwrap();

    // Gateway publishes both the old and new public keys during rotation overlap.
    let mut set = verifying_set(PUB_1);
    set.add(new_decoding).unwrap();

    let exp = now() + 3600;
    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &old_signing,
    )
    .unwrap();

    let ctx = verify(&token, &[AUD], ISS, &set).unwrap();
    assert_eq!(ctx.user_id(), "user-42");
}

#[test]
fn two_keys_live_simultaneously_via_jwks() {
    let signing_a = signing_key(PRIV_1);
    let signing_b = signing_key(PRIV_3);

    let mut set = verifying_set(PUB_1);
    set.add(DecodingKey::from_ed_pem(PUB_3.as_bytes()).unwrap())
        .unwrap();

    let exp = now() + 3600;
    let token_a = sign(
        "user-a".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_a,
    )
    .unwrap();
    let token_b = sign(
        "user-b".into(),
        vec![Role::Admin],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_b,
    )
    .unwrap();

    let jwks = set.to_jwks().unwrap();
    assert_eq!(jwks.keys.len(), 2);
    // Regression test: to_jwks() must set `common.key_id` per key, or `from_jwks()` round-trips
    // into a set that can't be looked up by `kid`.
    for jwk in &jwks.keys {
        assert!(jwk.common.key_id.is_some());
    }

    let reimported = VerifyingKeySet::from_jwks(&jwks).unwrap();

    let ctx_a = verify(&token_a, &[AUD], ISS, &reimported).unwrap();
    let ctx_b = verify(&token_b, &[AUD], ISS, &reimported).unwrap();
    assert_eq!(ctx_a.user_id(), "user-a");
    assert_eq!(ctx_b.user_id(), "user-b");
}

#[test]
fn signing_key_from_cert_path_round_trip() {
    let path = write_temp_pem("priv1", PRIV_1);
    let signing = SigningKey::from_cert_path(path.to_str().unwrap()).unwrap();
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let ctx = verify(&token, &[AUD], ISS, &verifying).unwrap();
    assert_eq!(ctx.user_id(), "user-42");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn verifying_key_set_from_cert_path_round_trip() {
    let signing = signing_key(PRIV_1);
    let path = write_temp_pem("pub1", PUB_1);
    let verifying = VerifyingKeySet::from_cert_path(path.to_str().unwrap()).unwrap();
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let ctx = verify(&token, &[AUD], ISS, &verifying).unwrap();
    assert_eq!(ctx.user_id(), "user-42");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn add_cert_allows_verification_with_new_key() {
    let signing_new = signing_key(PRIV_3);
    let mut verifying = verifying_set(PUB_1);
    let kid = verifying.add_cert(PUB_3.as_bytes()).unwrap();
    assert!(verifying.contains(&kid));

    let exp = now() + 3600;
    let token = sign(
        "user-99".into(),
        vec![Role::Admin],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_new,
    )
    .unwrap();

    let ctx = verify(&token, &[AUD], ISS, &verifying).unwrap();
    assert_eq!(ctx.user_id(), "user-99");
}

#[test]
fn add_path_allows_verification_with_new_key() {
    let signing_new = signing_key(PRIV_3);
    let mut verifying = verifying_set(PUB_1);
    let path = write_temp_pem("pub3-add", PUB_3);
    let kid = verifying.add_path(path.to_str().unwrap()).unwrap();
    assert!(verifying.contains(&kid));

    let exp = now() + 3600;
    let token = sign(
        "user-99".into(),
        vec![Role::Admin],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_new,
    )
    .unwrap();

    let ctx = verify(&token, &[AUD], ISS, &verifying).unwrap();
    assert_eq!(ctx.user_id(), "user-99");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn remove_revokes_key_by_kid() {
    let signing_old = signing_key(PRIV_1);
    let signing_new = signing_key(PRIV_3);
    let mut verifying = verifying_set(PUB_1);
    let kid_new = verifying.add_cert(PUB_3.as_bytes()).unwrap();

    let exp = now() + 3600;
    let token_old = sign(
        "user-1".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_old,
    )
    .unwrap();
    let token_new = sign(
        "user-2".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_new,
    )
    .unwrap();

    verifying.remove(&kid_new);

    assert!(!verifying.contains(&kid_new));
    assert!(verify(&token_old, &[AUD], ISS, &verifying).is_ok());
    assert!(verify(&token_new, &[AUD], ISS, &verifying).is_err());
}

#[test]
fn remove_by_path_revokes_matching_key() {
    let signing_old = signing_key(PRIV_1);
    let signing_new = signing_key(PRIV_3);
    let mut verifying = verifying_set(PUB_1);
    verifying.add_cert(PUB_3.as_bytes()).unwrap();

    let exp = now() + 3600;
    let token_old = sign(
        "user-1".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_old,
    )
    .unwrap();
    let token_new = sign(
        "user-2".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing_new,
    )
    .unwrap();

    let path = write_temp_pem("pub3-remove", PUB_3);
    verifying.remove_by_path(path.to_str().unwrap()).unwrap();

    assert!(verify(&token_old, &[AUD], ISS, &verifying).is_ok());
    assert!(verify(&token_new, &[AUD], ISS, &verifying).is_err());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn contains_reports_key_presence_accurately() {
    let mut verifying = verifying_set(PUB_1);
    let kid = verifying.add_cert(PUB_3.as_bytes()).unwrap();

    assert!(verifying.contains(&kid));
    assert!(!verifying.contains("definitely-not-a-real-kid"));

    verifying.remove(&kid);
    assert!(!verifying.contains(&kid));
}

// `SigningKey::new`, `VerifyingKeySet::new`, and `VerifyingKeySet::add` all take an already-built
// `EncodingKey`/`DecodingKey` and check `key.family() != ALGORITHM.family()` themselves -- this
// is the actual regression guard against the original HMAC/EdDSA foot-gun, and it's genuinely
// reachable from outside the crate via `EncodingKey::from_secret` / `DecodingKey::from_secret`.
#[test]
fn signing_key_new_rejects_non_ed25519_family() {
    let hmac_key = EncodingKey::from_secret(b"some bytes");
    assert!(SigningKey::new(hmac_key).is_err());
}

#[test]
fn verifying_key_set_new_rejects_non_ed25519_family() {
    let hmac_key = DecodingKey::from_secret(b"some bytes");
    assert!(VerifyingKeySet::new(hmac_key).is_err());
}

#[test]
fn verifying_key_set_add_rejects_non_ed25519_family() {
    let mut verifying = verifying_set(PUB_1);
    let hmac_key = DecodingKey::from_secret(b"some bytes");
    assert!(verifying.add(hmac_key).is_err());
}

// NOTE on the family guard in the four PEM/path-based constructors below
// (`SigningKey::from_cert_path`, `VerifyingKeySet::from_cert_path`, `add_cert`, `add_path`):
// each carries the same `if key.family() != ALGORITHM.family() { return Err(...) }` check as
// `new`/`add` above, but per `jsonwebtoken` 11.0.0's own source
// (`EncodingKey::from_ed_pem`/`DecodingKey::from_ed_pem`, which these four exclusively build
// their key from), that constructor either fails to parse and returns `Err` *before* the family
// check is ever reached, or succeeds and unconditionally reports `AlgorithmFamily::Ed`. There is
// no byte sequence that reaches the guard with a mismatched family through these four functions
// -- the `if` is dead code here, not a gap in test effort. (The `EncodingKey::from_secret`-style
// trick used above can't apply: these functions never accept a pre-built key, only PEM bytes.)
// The test below documents that non-Ed key material is still rejected end-to-end (for the
// PEM-parse reason, not the family-check reason), so this is not silently masking broken input
// handling -- it's just proof the specific `family() != ALGORITHM.family()` *line* in these four
// functions is unreachable and cannot be covered by any passing (or failing-for-the-right-reason)
// test. This is flagged per the "if you find a bug" instructions; no production code was changed.
#[test]
fn from_cert_path_rejects_non_pem_content() {
    let path = write_temp_pem("not-a-pem", "not a pem file at all");
    assert!(SigningKey::from_cert_path(path.to_str().unwrap()).is_err());
    assert!(VerifyingKeySet::from_cert_path(path.to_str().unwrap()).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_kid_in_header_is_rejected() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    let no_kid = tamper_header(&token, |header| {
        if let Value::Object(map) = header {
            map.remove("kid");
        }
    });

    assert!(verify(&no_kid, &[AUD], ISS, &verifying).is_err());
}

#[test]
fn kid_is_stable_and_deterministic_for_the_same_key() {
    let a = signing_key(PRIV_1);
    let b = signing_key(PRIV_1);
    let c = signing_key(PRIV_3);

    assert_eq!(a.kid(), b.kid());
    assert_ne!(a.kid(), c.kid());
    assert!(!a.kid().is_empty());
}

#[test]
fn issuer_mismatch_is_rejected() {
    let signing = signing_key(PRIV_1);
    let verifying = verifying_set(PUB_1);
    let exp = now() + 3600;

    let token = sign(
        "user-42".into(),
        vec![Role::User],
        AUD.into(),
        exp,
        ISS.into(),
        0,
        &signing,
    )
    .unwrap();

    assert!(verify(&token, &[AUD], "some-other-issuer", &verifying).is_err());
}
