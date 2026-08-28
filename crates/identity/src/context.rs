//! Signed, verifiable user identity for internal service calls (ARIA-28).
//!
//! # Security invariant: `UserContext` is unforgeable
//!
//! [`verify`] is the *only* way to obtain a [`UserContext`]. `UserContext::new` is a private
//! associated fn and [`Claims`] is not `pub` at all, so there is no path from a raw,
//! attacker-controlled `String` (e.g. a prompt-injected tool argument claiming to be someone
//! else) to a trusted identity that skips signature verification. The two `compile_fail`
//! doctests below prove this at compile time: if either one started compiling, this invariant
//! would be silently broken.
//!
//! Nothing outside this crate can construct a `UserContext` directly:
//! ```compile_fail
//! // `UserContext::new` is private -- unreachable outside the `context` module.
//! let ctx = identity::context::UserContext::new("attacker".into(), vec![]);
//! ```
//!
//! Nor can it forge the `Claims` payload and convert it into one:
//! ```compile_fail
//! // `Claims` is not `pub` -- unreachable outside this crate.
//! let claims = identity::context::Claims {
//!     user_id: "attacker".into(),
//!     roles: vec![],
//!     aud: "x".into(),
//!     exp: 0,
//!     iat: 0,
//!     iss: "x".into(),
//!     nbf: 0,
//!     jti: "x".into(),
//! };
//! ```

use jsonwebtoken::errors::new_error;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation, decode, decode_header,
    encode,
};
use serde::{Deserialize, Serialize};
use shared::Result;
use std::collections::HashMap;
use std::fmt::Debug;
use uuid::Uuid;

pub const ALGORITHM: Algorithm = Algorithm::EdDSA;

fn kid_from_jwk(key: &Jwk) -> Result<String> {
    Ok(key.thumbprint(jsonwebtoken::jwk::ThumbprintHash::SHA256)?)
}

pub struct SigningKey {
    encoding: EncodingKey,
    kid: String,
}

impl SigningKey {
    pub fn new(encoding: EncodingKey) -> Result<Self> {
        if encoding.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }

        let kid = kid_from_jwk(&Jwk::from_encoding_key(&encoding, ALGORITHM)?)?;
        Ok(Self { encoding, kid })
    }

    pub fn from_cert_path(private_key: &str) -> Result<Self> {
        let private_contents = std::fs::read_to_string(private_key)?;
        let encoding = EncodingKey::from_ed_pem(private_contents.as_bytes())?;
        if encoding.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }

        let kid = kid_from_jwk(&Jwk::from_encoding_key(&encoding, ALGORITHM)?)?;

        Ok(Self { encoding, kid })
    }

    pub fn kid(&self) -> &str {
        &self.kid
    }
}

pub struct VerifyingKeySet(HashMap<String, DecodingKey>);

impl VerifyingKeySet {
    pub fn new(decoding_key: DecodingKey) -> Result<Self> {
        if decoding_key.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }
        let mut map = HashMap::new();
        let kid = kid_from_jwk(&Jwk::from_decoding_key(&decoding_key, Some(ALGORITHM))?)?;
        map.insert(kid, decoding_key);
        Ok(Self(map))
    }

    pub fn from_cert_path(public_key: &str) -> Result<Self> {
        let mut map = HashMap::new();
        let public_contents = std::fs::read_to_string(public_key)?;
        let decoding = DecodingKey::from_ed_pem(public_contents.as_bytes())?;
        if decoding.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }
        let kid = kid_from_jwk(&Jwk::from_decoding_key(&decoding, Some(ALGORITHM))?)?;
        map.insert(kid, decoding);
        Ok(Self(map))
    }

    pub fn from_jwks(set: &JwkSet) -> Result<Self> {
        let mut map = HashMap::new();
        for jwk in &set.keys {
            let kid = kid_from_jwk(jwk)?;
            let decoding = DecodingKey::from_jwk(jwk)?;
            map.insert(kid, decoding);
        }
        Ok(Self(map))
    }

    pub fn add(&mut self, decoding_key: DecodingKey) -> Result<String> {
        if decoding_key.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }
        let kid = kid_from_jwk(&Jwk::from_decoding_key(&decoding_key, Some(ALGORITHM))?)?;
        self.0.insert(kid.clone(), decoding_key);
        Ok(kid)
    }

    pub fn add_cert(&mut self, cert: &[u8]) -> Result<String> {
        let decoding = DecodingKey::from_ed_pem(cert)?;
        if decoding.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }
        let kid = kid_from_jwk(&Jwk::from_decoding_key(&decoding, Some(ALGORITHM))?)?;
        self.0.insert(kid.clone(), decoding);
        Ok(kid)
    }

    pub fn add_path(&mut self, path: &str) -> Result<String> {
        let content = std::fs::read_to_string(path)?;
        let decoding = DecodingKey::from_ed_pem(content.as_bytes())?;
        if decoding.family() != ALGORITHM.family() {
            return Err(new_error(jsonwebtoken::errors::ErrorKind::UnsupportedAlgorithm).into());
        }
        let kid = kid_from_jwk(&Jwk::from_decoding_key(&decoding, Some(ALGORITHM))?)?;
        self.0.insert(kid.clone(), decoding);
        Ok(kid)
    }

    pub fn remove(&mut self, kid: &str) {
        self.0.remove(kid);
    }

    pub fn contains(&self, kid: &str) -> bool {
        self.0.contains_key(kid)
    }

    pub fn get(&self, kid: &str) -> Option<&DecodingKey> {
        self.0.get(kid)
    }

    pub fn remove_by_path(&mut self, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let decoding = DecodingKey::from_ed_pem(content.as_bytes())?;
        // No check for family needed, as the hash will probably never match for another algo
        let kid = kid_from_jwk(&Jwk::from_decoding_key(&decoding, Some(ALGORITHM))?)?;
        self.remove(&kid);
        Ok(())
    }

    pub fn to_jwks(&self) -> Result<JwkSet> {
        let mut jwks = JwkSet::default();
        for decoding in self.0.values() {
            let mut jwk = Jwk::from_decoding_key(decoding, Some(ALGORITHM))?;
            jwk.common.key_id = Some(kid_from_jwk(&jwk)?);
            jwks.keys.push(jwk);
        }
        Ok(jwks)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "aria-user")]
    User,
    #[serde(rename = "aria-admin")]
    Admin,
}

#[derive(Debug)]
pub struct UserContext {
    user_id: String,
    roles: Vec<Role>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Claims {
    user_id: String,
    roles: Vec<Role>,

    // Standard JWT
    aud: String, // Optional. Audience
    exp: usize, // Required (validate_exp defaults to true in validation). Expiration time (as UTC timestamp)
    iat: usize, // Optional. Issued at (as UTC timestamp)
    iss: String, // Optional. Issuer
    nbf: usize, // Optional. Not Before (as UTC timestamp)
    jti: String, // Optional. Unique Identifier for this token
}

impl UserContext {
    fn new(user_id: String, roles: Vec<Role>) -> Self {
        Self { user_id, roles }
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn roles(&self) -> &[Role] {
        &self.roles
    }
}

impl Claims {
    pub fn new(
        user_id: String,
        roles: Vec<Role>,
        aud: String,
        exp: usize,
        iss: String,
        nbf: usize,
    ) -> Self {
        Self {
            user_id,
            roles,
            aud,
            exp,
            iat: chrono::Utc::now().timestamp() as usize,
            iss,
            nbf,
            jti: Uuid::new_v4().to_string(),
        }
    }
}

impl From<Claims> for UserContext {
    fn from(claims: Claims) -> Self {
        UserContext::new(claims.user_id.clone(), claims.roles.clone())
    }
}

pub fn sign(
    user_id: String,
    roles: Vec<Role>,
    aud: String,
    exp: usize,
    iss: String,
    nbf: usize,
    keys: &SigningKey,
) -> Result<String> {
    let claims = Claims::new(user_id, roles, aud, exp, iss, nbf);

    let mut header = Header::new(ALGORITHM);
    header.kid = Some(keys.kid.clone());

    let token = encode(&header, &claims, &keys.encoding)?;
    Ok(token)
}

pub fn verify(
    token: &str,
    audiences: &[&str],
    issuer: &str,
    set: &VerifyingKeySet,
) -> Result<UserContext> {
    let header = decode_header(token)?;

    // Make sure we verify with the same key as we did create the token.
    let decoding: &DecodingKey;
    if let Some(kid) = header.kid {
        decoding = set
            .get(&kid)
            .ok_or_else(|| new_error(jsonwebtoken::errors::ErrorKind::InvalidToken))?;
    } else {
        return Err(new_error(jsonwebtoken::errors::ErrorKind::InvalidToken).into());
    }

    let mut validation = Validation::new(ALGORITHM);
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.set_audience(audiences);

    let token_data: TokenData<Claims> = decode(token, decoding, &validation)?;

    // Validate Issuer separately as the validation does not support it.
    if token_data.claims.iss != issuer {
        return Err(new_error(jsonwebtoken::errors::ErrorKind::InvalidToken).into());
    }

    Ok(token_data.claims.into())
}
