//! Gravatar pictures, for people who choose to show theirs.
//!
//! Gravatar files a picture under a hash of its owner's email, and anyone
//! holding the hash can test guesses against it. So the hash stays here: the
//! API fetches the picture itself and serves it under a random id of its own
//! (`people.gravatar`), which is all the page ever sees.

use std::time::Duration;

use axum::body::Bytes;
use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use sha2::{Digest, Sha256};

/// Where Gravatar serves pictures, by hash.
const PUBLIC: &str = "https://gravatar.com/avatar/";
/// Pixels square: twice the largest size the page draws, for sharp screens.
const SIZE: u32 = 80;
/// The image types passed on to the page. Gravatar sends JPEG or PNG; SVG,
/// which can carry scripts, never gets through.
const IMAGE_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Gravatar, or a stand-in for it in tests.
#[derive(Clone)]
pub struct Gravatar {
    base: String,
    http: Client,
}

/// A picture as Gravatar sent it.
pub struct Picture {
    pub content_type: &'static str,
    pub bytes: Bytes,
}

impl Gravatar {
    /// The real Gravatar.
    pub fn public() -> Self {
        Self::at(PUBLIC)
    }

    /// A stand-in that serves pictures at `{base}{hash}`, for tests.
    pub fn at(base: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(5))
            // Straight there: a function has no proxy to go through, and a
            // test's stand-in is on this machine.
            .no_proxy()
            .build()
            .expect("a client with only a timeout set builds");
        Gravatar { base: base.to_owned(), http }
    }

    /// The picture for `email`, or None if there isn't one. `d=404` asks for
    /// a 404 instead of Gravatar's placeholder, and Gravatar serves only
    /// pictures rated G unless asked for more.
    pub async fn picture(&self, email: &str) -> Result<Option<Picture>, String> {
        let url = format!("{}{}?s={SIZE}&d=404", self.base, hash(email));
        let response = self.http.get(url).send().await.map_err(|e| e.to_string())?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(format!("Gravatar answered {}", response.status()));
        }
        let sent = response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let essence = sent.split(';').next().unwrap_or("").trim();
        let Some(content_type) = IMAGE_TYPES.iter().copied().find(|t| t.eq_ignore_ascii_case(essence)) else {
            return Err(format!("Gravatar sent {sent:?}, which isn't an image type we pass on"));
        };
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok(Some(Picture { content_type, bytes }))
    }
}

/// Gravatar's key for an email: SHA-256 of it trimmed and lowercased, in hex.
fn hash(email: &str) -> String {
    Sha256::digest(email.trim().to_lowercase()).iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_the_one_gravatar_documents() {
        // The worked example on docs.gravatar.com/general/hash/.
        assert_eq!(
            hash(" MyEmailAddress@example.com "),
            "84059b07d4be67b806386c0aad8070a23f18836bbaae342275dc0a83414c32ee"
        );
    }
}
