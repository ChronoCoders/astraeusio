//! Strips secrets out of text on its way to a log.
//!
//! An error from `reqwest` carries the URL that produced it, so a request whose
//! query string holds a key logs that key verbatim. That is a credential in
//! plaintext on disk, readable by anything that can read `docker logs`, and it
//! would travel into any log shipping added later. On 2026-08-11 the NASA key
//! sat in a 503 line for exactly that reason.
//!
//! The first defence is not putting secrets in URLs at all, which is why
//! `nasa.rs` sends `X-Api-Key` as a header. This is the second: anything that
//! still reaches a logger with a query parameter that looks like a credential
//! gets the value replaced. It runs on error paths, so it is deliberately cheap
//! and allocation free when there is nothing to redact.
//!
//! It is not a URL parser. It works on arbitrary text because what gets logged
//! is usually an error message with a URL somewhere inside it, not a bare URL.

use std::borrow::Cow;

/// Query parameter names whose values must never be logged.
///
/// Matched case insensitively, and only where the name begins right after a
/// `?`, `&` or `;`, so a path segment like `/monkey=1` is untouched.
const SENSITIVE: [&str; 12] = [
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "secret",
    "client_secret",
    "password",
    "signature",
    "sig",
];

const PLACEHOLDER: &str = "REDACTED";

/// True for the characters that can end a query parameter value in log text.
/// `&` and `;` separate parameters; the rest are the punctuation that tends to
/// wrap a URL when an error message embeds one, as in `for url (https://...)`.
fn ends_value(b: u8) -> bool {
    matches!(
        b,
        b'&' | b';'
            | b')'
            | b']'
            | b'}'
            | b'"'
            | b'\''
            | b'>'
            | b','
            | b' '
            | b'\t'
            | b'\n'
            | b'\r'
    )
}

/// Replaces the value of any sensitive query parameter with `REDACTED`.
pub fn secrets(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    let mut out: Option<Vec<u8>> = None;
    let mut i = 0;
    let mut copied_to = 0;

    while i < bytes.len() {
        // A parameter name can only start after one of these.
        if !(bytes[i] == b'?' || bytes[i] == b'&' || bytes[i] == b';') {
            i += 1;
            continue;
        }
        let name_start = i + 1;
        let Some(name) = SENSITIVE.iter().find(|name| {
            let end = name_start + name.len();
            end < bytes.len()
                && bytes[end] == b'='
                && input
                    .get(name_start..end)
                    .is_some_and(|s| s.eq_ignore_ascii_case(name))
        }) else {
            i += 1;
            continue;
        };

        let value_start = name_start + name.len() + 1;
        let mut value_end = value_start;
        while value_end < bytes.len() && !ends_value(bytes[value_end]) {
            value_end += 1;
        }
        // Nothing to hide, and skipping keeps `?key=` unchanged rather than
        // turning an empty value into something that looks like a secret.
        if value_end == value_start {
            i = value_end;
            continue;
        }

        let buf = out.get_or_insert_with(|| Vec::with_capacity(input.len()));
        buf.extend_from_slice(&bytes[copied_to..value_start]);
        buf.extend_from_slice(PLACEHOLDER.as_bytes());
        copied_to = value_end;
        i = value_end;
    }

    match out {
        None => Cow::Borrowed(input),
        Some(mut buf) => {
            buf.extend_from_slice(&bytes[copied_to..]);
            // Only whole bytes are copied and only at ASCII boundaries, so this
            // cannot fail. Falling back keeps a surprise from becoming a panic,
            // and the fallback is the already-redacted lossy form rather than
            // the raw input, which must never be returned from here.
            Cow::Owned(String::from_utf8_lossy(&buf).into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact line that started this: a reqwest error carrying the key.
    #[test]
    fn a_reqwest_error_with_the_key_in_the_url_is_scrubbed() {
        let line = "fetch: request failed: HTTP status server error (503 Service \
                    Unavailable) for url (https://api.nasa.gov/planetary/apod?api_key=tyOsDDof)";
        let got = secrets(line);
        assert!(!got.contains("tyOsDDof"), "the key survived: {got}");
        assert!(got.contains("api_key=REDACTED"));
        // The closing bracket must survive, or the message stops being readable.
        assert!(got.ends_with("REDACTED)"), "{got}");
    }

    #[test]
    fn a_value_in_the_middle_of_a_query_string_stops_at_the_ampersand() {
        let got = secrets(
            "https://api.nasa.gov/neo?start_date=2026-08-11&api_key=abc123&end_date=2026-08-12",
        );
        assert_eq!(
            got,
            "https://api.nasa.gov/neo?start_date=2026-08-11&api_key=REDACTED&end_date=2026-08-12"
        );
    }

    #[test]
    fn every_sensitive_name_is_covered_and_matching_is_case_insensitive() {
        for name in [
            "api_key",
            "APIKEY",
            "Token",
            "access_token",
            "client_secret",
            "password",
            "sig",
        ] {
            let line = format!("https://example.invalid/x?{name}=s3cret");
            let got = secrets(&line);
            assert!(!got.contains("s3cret"), "{name} leaked: {got}");
        }
    }

    #[test]
    fn several_secrets_in_one_line_are_all_removed() {
        let got = secrets("a?token=one&b=2&password=two&c=3&sig=three");
        assert!(
            !got.contains("one") && !got.contains("two") && !got.contains("three"),
            "{got}"
        );
        assert!(got.contains("b=2") && got.contains("c=3"), "{got}");
    }

    /// A name must start a parameter. Redacting inside a path or a word would
    /// mangle ordinary messages for no gain.
    #[test]
    fn a_lookalike_that_is_not_a_query_parameter_is_left_alone() {
        let untouched = [
            "the monkey=banana here",
            "/path/key=value/still-a-path",
            "user_api_key=notaparam",
        ];
        for s in untouched {
            assert!(
                matches!(secrets(s), Cow::Borrowed(_)),
                "should not have changed: {s}"
            );
        }
    }

    #[test]
    fn text_with_no_secret_is_returned_without_allocating() {
        let s = "poller/kp: 358 records";
        assert!(matches!(secrets(s), Cow::Borrowed(_)));
        let url = "https://services.swpc.noaa.gov/json/planetary_k_index_1m.json";
        assert!(matches!(secrets(url), Cow::Borrowed(_)));
    }

    #[test]
    fn an_empty_value_is_left_as_it_is() {
        assert_eq!(
            secrets("https://x.invalid/a?api_key=&b=1"),
            "https://x.invalid/a?api_key=&b=1"
        );
    }

    #[test]
    fn non_ascii_around_a_secret_survives() {
        let got = secrets("hata: https://x.invalid/a?token=gizli&ülke=tr");
        assert!(!got.contains("gizli"), "{got}");
        assert!(got.contains("ülke=tr"), "{got}");
    }
}
