//! Sign in with Aero. Identity only, and not OAuth: Aero's consent screen
//! sends the browser back to `/auth/aero` with a PASETO `v4.local` token,
//! the visitor's email encrypted with a key only Aero and Champt hold. The
//! browser keeps it as an opaque string and sends it as a bearer token; the
//! API decrypts it on every call that needs to know who is asking.
//!
//! Nothing here may log a token's contents or an email.

use pasetors::claims::ClaimsValidationRules;
use pasetors::keys::SymmetricKey;
use pasetors::token::UntrustedToken;
use pasetors::{Local, local, version4::V4};
use uuid::Uuid;

/// Champt's client id on Aero, from its "Login with Aero" registration. The
/// page has the same value in `frontend/src/auth.ts`. Until it is set, every
/// sign-in is refused.
pub const AERO_CLIENT_ID: &str = "";

/// Environment variable holding the key Aero gave at registration: a PASERK
/// `k4.local.…` string. It lives only in the backend's environment.
pub const KEY_VAR: &str = "AERO_LOGIN_KEY";

/// Turns an email into an author id. Changing it would make everyone a new
/// person, so it never changes.
const AUTHOR_NAMESPACE: Uuid = Uuid::from_u128(0x1a3a0201_cac2_4fc4_ae78_549765a7426e);

/// Someone signed in.
pub struct Person {
    /// Stable for the same email in any case, so the turn rule and `mine`
    /// can compare people without handling emails.
    pub id: Uuid,
    /// Lowercased. Kept with each post for audits; never sent to others.
    pub email: String,
}

pub struct Verifier {
    /// None when the key is missing or unreadable: then nobody can sign in.
    key: Option<SymmetricKey<V4>>,
    client_id: String,
}

impl Verifier {
    /// Checks tokens encrypted with the PASERK `key` (`k4.local.…`) and
    /// addressed to `client_id`.
    pub fn new(key: &str, client_id: &str) -> Result<Verifier, pasetors::errors::Error> {
        let key = SymmetricKey::<V4>::try_from(key)?;
        Ok(Verifier { key: Some(key), client_id: client_id.to_owned() })
    }

    /// The real thing: the key from the environment and Champt's client id.
    /// Without a usable key the API still serves reads, and says so once.
    pub fn aero() -> Verifier {
        let key = std::env::var(KEY_VAR).ok().and_then(|text| SymmetricKey::<V4>::try_from(text.trim()).ok());
        if key.is_none() {
            eprintln!("{KEY_VAR} is missing or not a k4.local key; sign-ins will be refused");
        }
        Verifier { key, client_id: AERO_CLIENT_ID.to_owned() }
    }

    /// Who a token names, if it was made with Champt's key, is addressed to
    /// Champt and is currently valid. A token that decrypts with the key was
    /// made by Aero (or by Champt), so there is no signature to check besides.
    pub fn person(&self, token: &str) -> Option<Person> {
        let key = self.key.as_ref()?;
        if self.client_id.is_empty() {
            return None;
        }
        // `iat`, `nbf` and `exp` are checked by default, and `exp` is required.
        let mut rules = ClaimsValidationRules::new();
        rules.validate_audience_with(&self.client_id);
        let untrusted = UntrustedToken::<Local, V4>::try_from(token).ok()?;
        let trusted = local::decrypt(key, &untrusted, &rules, None, None).ok()?;
        let email = trusted.payload_claims()?.get_claim("email")?.as_str()?.trim().to_lowercase();
        if email.is_empty() {
            return None;
        }
        let id = Uuid::new_v5(&AUTHOR_NAMESPACE, email.as_bytes());
        Some(Person { id, email })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pasetors::claims::Claims;
    use pasetors::keys::Generate;
    use pasetors::paserk::FormatAsPaserk;

    fn paserk(key: &SymmetricKey<V4>) -> String {
        let mut text = String::new();
        key.fmt(&mut text).unwrap();
        text
    }

    fn token(key: &SymmetricKey<V4>, aud: &str, email: &str) -> String {
        let mut claims = Claims::new().unwrap();
        claims.audience(aud).unwrap();
        claims.add_additional("state", "s").unwrap();
        claims.add_additional("email", email).unwrap();
        local::encrypt(key, &claims, None, None).unwrap()
    }

    #[test]
    fn one_person_one_id_whatever_the_case() {
        let key = SymmetricKey::<V4>::generate().unwrap();
        let verifier = Verifier::new(&paserk(&key), "champt").unwrap();
        let ada = verifier.person(&token(&key, "champt", "Ada@Example.com")).unwrap();
        assert_eq!(ada.email, "ada@example.com");
        let again = verifier.person(&token(&key, "champt", " ada@example.com ")).unwrap();
        assert_eq!(again.id, ada.id);
        let grace = verifier.person(&token(&key, "champt", "grace@example.com")).unwrap();
        assert_ne!(grace.id, ada.id);
    }

    #[test]
    fn nothing_gets_in_before_champt_is_registered() {
        // A good token, addressed to whatever id, is still refused while
        // Champt has no client id of its own.
        let key = SymmetricKey::<V4>::generate().unwrap();
        let verifier = Verifier::new(&paserk(&key), "").unwrap();
        assert!(verifier.person(&token(&key, "champt", "ada@example.com")).is_none());
    }

    #[test]
    fn a_key_must_be_a_k4_local_paserk() {
        assert!(Verifier::new("not-a-key", "champt").is_err());
        assert!(Verifier::new("k4.public.AAAA", "champt").is_err());
    }
}
