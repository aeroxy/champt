//! A thread's link, `/t/{id}`, as a page: the app's own `index.html`, with
//! the tags chat apps read to preview a link. A shared link then shows the
//! thread's opening line, and its photo when it opens with one, or that the
//! thread is gone. `vercel.json` sends these links here; every other path
//! gets the static page as it is.
//!
//! The app's file names change with each build, and only its `index.html`
//! knows them, so the page is fetched from the deployment the link was
//! opened on: the same build that is serving the rest of the app.

use std::time::Duration;

use axum::http::{HeaderMap, header::HOST};
use reqwest::Client;

/// What a link preview shows.
pub struct Preview {
    pub title: String,
    pub description: String,
    /// A photo to show with it.
    pub image: Option<String>,
}

impl Preview {
    /// A live thread: its opening line, how many messages it has, and the
    /// photo it opens with, if any.
    pub fn thread(opening: String, messages: i32, photo: Option<String>) -> Preview {
        let count = if messages == 1 { "1 message".to_owned() } else { format!("{messages} messages") };
        Preview {
            title: opening,
            description: format!("{count} on Champt, chat threads pinned to places on a map."),
            image: photo,
        }
    }

    /// A thread that expired, or never was.
    pub fn gone() -> Preview {
        Preview {
            title: "This thread is gone".to_owned(),
            description: "A thread on Champt goes after 24 hours unless someone else replies.".to_owned(),
            image: None,
        }
    }
}

/// Fetches the app's page: briefly, and straight there, as `gravatar` does.
pub fn client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .expect("a client with only a timeout set builds")
}

/// The thread a link's last part names: a plain number, written the way the
/// page writes it, so `/t/017` and `/t/+17` name none.
pub fn thread_id(part: &str) -> Option<i64> {
    part.parse::<i64>().ok().filter(|id| *id > 0 && id.to_string() == part)
}

/// Where the request came in, such as `https://champt.vercel.app`: the
/// deployment to fetch the page from, and the start of the thread's link.
/// Vercel says `https` in `x-forwarded-proto`; without it, as through the
/// dev server's proxy, it is plain `http`. None for a host that isn't a
/// plain name or address, which could steer the fetch somewhere else.
pub fn origin(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(HOST)?.to_str().ok()?;
    let plain = host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    if host.is_empty() || !plain {
        return None;
    }
    let https = headers.get("x-forwarded-proto").is_some_and(|proto| proto == "https");
    Some(format!("{}://{host}", if https { "https" } else { "http" }))
}

/// The app's page as the deployment at `origin` serves it.
pub async fn shell(http: &Client, origin: &str) -> Result<String, String> {
    let url = format!("{origin}/index.html");
    let response = http.get(&url).send().await.map_err(|e| format!("{url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    response.text().await.map_err(|e| format!("{url}: {e}"))
}

/// `shell` with the preview's tags at the end of its head. The text comes
/// from the thread, so all of it is escaped.
pub fn with_preview(shell: &str, preview: &Preview, url: &str) -> String {
    let mut tags = String::new();
    let mut tag = |kind: &str, name: &str, content: &str| {
        tags.push_str(&format!("<meta {kind}=\"{name}\" content=\"{}\" />\n", escape(content)));
    };
    tag("property", "og:site_name", "Champt");
    tag("property", "og:type", "website");
    tag("property", "og:url", url);
    tag("property", "og:title", &preview.title);
    tag("property", "og:description", &preview.description);
    if let Some(image) = &preview.image {
        tag("property", "og:image", image);
    }
    tag("name", "twitter:card", if preview.image.is_some() { "summary_large_image" } else { "summary" });
    match shell.find("</head>") {
        Some(end) => format!("{}{tags}{}", &shell[..end], &shell[end..]),
        None => shell.to_owned(),
    }
}

/// Text made safe inside an HTML attribute in double quotes, or anywhere else.
fn escape(text: &str) -> String {
    let mut safe = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => safe.push_str("&amp;"),
            '<' => safe.push_str("&lt;"),
            '>' => safe.push_str("&gt;"),
            '"' => safe.push_str("&quot;"),
            '\'' => safe.push_str("&#39;"),
            c => safe.push(c),
        }
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHELL: &str = "<html><head><title>Champt</title></head><body></body></html>";

    #[test]
    fn a_preview_cannot_break_out_of_its_tag() {
        let preview = Preview::thread(r#"<script>alert("hi")</script> & 'bye'"#.to_owned(), 1, None);
        let page = with_preview(SHELL, &preview, "https://champt.example/t/1");
        assert!(page.contains(
            r#"<meta property="og:title" content="&lt;script&gt;alert(&quot;hi&quot;)&lt;/script&gt; &amp; &#39;bye&#39;" />"#
        ));
        assert!(!page.contains("<script>"));
        assert!(page.contains(r#"<meta property="og:description" content="1 message on Champt"#));
    }

    #[test]
    fn a_photo_makes_a_large_card() {
        let photo = "https://yourimageshare.com/ib/aB3xY9qRz1.jpg";
        let with = with_preview(SHELL, &Preview::thread("Look".to_owned(), 2, Some(photo.to_owned())), "u");
        assert!(with.contains(&format!(r#"<meta property="og:image" content="{photo}" />"#)));
        assert!(with.contains(r#"<meta name="twitter:card" content="summary_large_image" />"#));
        let without = with_preview(SHELL, &Preview::gone(), "u");
        assert!(!without.contains("og:image"));
        assert!(without.contains(r#"<meta name="twitter:card" content="summary" />"#));
        // The tags go in the head, and the rest of the page stays as it was.
        assert!(without.starts_with("<html><head><title>Champt</title><meta "));
        assert!(without.ends_with("</head><body></body></html>"));
    }

    #[test]
    fn only_plain_numbers_name_a_thread() {
        assert_eq!(thread_id("17"), Some(17));
        for other in ["017", "+17", "-1", "0", "", "abc", "1.5", "99999999999999999999"] {
            assert_eq!(thread_id(other), None, "{other}");
        }
    }

    #[test]
    fn the_origin_is_where_the_request_came_in() {
        let headers = |pairs: &[(&'static str, &str)]| {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.insert(*name, value.parse().unwrap());
            }
            map
        };
        let vercel = headers(&[("host", "champt.vercel.app"), ("x-forwarded-proto", "https")]);
        assert_eq!(origin(&vercel).as_deref(), Some("https://champt.vercel.app"));
        assert_eq!(origin(&headers(&[("host", "localhost:5176")])).as_deref(), Some("http://localhost:5176"));
        for bad in ["evil.example/x?", "user@evil.example", "evil.example#", ""] {
            assert_eq!(origin(&headers(&[("host", bad)])), None, "{bad}");
        }
        assert_eq!(origin(&HeaderMap::new()), None);
    }
}
