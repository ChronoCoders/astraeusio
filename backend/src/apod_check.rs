//! Whether an APOD record is a real picture of the day or the site's own chrome.
//!
//! APOD moved from apod.nasa.gov to science.nasa.gov in 2026 August and
//! September, and from 2026-09-30 the APOD API has answered every date with the
//! new landing page instead of the day's entry: the site logo as `url` and
//! `hdurl`, the site name as `title`, and whatever the landing page features as
//! `explanation`, including a browser's video fallback sentence. Public reports
//! are nasa/apod-api#172, #181 and #184.
//!
//! Every rule here was derived from the 202 rows stored between 2026-05-09 and
//! 2026-11-26, 144 of them real and 58 of them chrome, and each one separates
//! the two sets on its own terms rather than matching one record's strings.

use crate::nasa::Apod;
use std::fmt;

/// The publisher's own name. A page title is either this or `<page> - <this>`,
/// and neither is ever the title of a picture.
const SITE_NAME: &str = "NASA Science";

/// Path fragments of a WordPress theme or plugin asset, which is site furniture
/// and never an archived image. Every chrome row's `url` sat under the first.
const CHROME_PATHS: [&str; 2] = ["/wp-content/themes/", "/wp-content/plugins/"];

/// Phrases of the text a browser shows in place of a video or a script,
/// matched after whitespace is collapsed. The stored fallback carries five tabs
/// mid sentence and is cut off at `HTML5 vide`, so both phrases are ones that
/// record really contains.
const FALLBACK_PHRASES: [&str; 2] = ["enable javascript", "supports html5"];

/// The shortest real explanation stored was 658 characters and the fallback
/// sentence is 114, so 200 rejects the second with wide room under the first.
pub const MIN_EXPLANATION_CHARS: usize = 200;

#[derive(Debug, PartialEq, Eq)]
pub enum ApodReject {
    EmptyField(&'static str),
    ChromeImage(&'static str),
    SiteTitle,
    FallbackText,
    ShortExplanation(usize),
}

impl fmt::Display for ApodReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} is empty"),
            Self::ChromeImage(field) => write!(f, "{field} is a site asset, not an image"),
            Self::SiteTitle => write!(f, "title is the site name"),
            Self::FallbackText => write!(f, "explanation is browser fallback text"),
            Self::ShortExplanation(n) => write!(
                f,
                "explanation is {n} characters, under {MIN_EXPLANATION_CHARS}"
            ),
        }
    }
}

fn is_chrome(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let path = lower.split(['?', '#']).next().unwrap_or_default();
    let file = path.rsplit('/').next().unwrap_or_default();
    CHROME_PATHS.iter().any(|p| path.contains(p)) || file.contains("logo")
}

/// The first rule a record breaks, or `Ok` for a real entry.
pub fn check(a: &Apod) -> Result<(), ApodReject> {
    for (field, value) in [
        ("date", &a.date),
        ("title", &a.title),
        ("explanation", &a.explanation),
        ("url", &a.url),
    ] {
        if value.trim().is_empty() {
            return Err(ApodReject::EmptyField(field));
        }
    }
    if is_chrome(&a.url) {
        return Err(ApodReject::ChromeImage("url"));
    }
    if a.hdurl.as_deref().is_some_and(is_chrome) {
        return Err(ApodReject::ChromeImage("hdurl"));
    }
    let title = a.title.trim();
    if title.eq_ignore_ascii_case(SITE_NAME)
        || title
            .to_ascii_lowercase()
            .ends_with(&format!(" - {}", SITE_NAME.to_ascii_lowercase()))
    {
        return Err(ApodReject::SiteTitle);
    }
    let explanation = a
        .explanation
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if FALLBACK_PHRASES.iter().any(|p| explanation.contains(p)) {
        return Err(ApodReject::FallbackText);
    }
    let len = a.explanation.trim().chars().count();
    if len < MIN_EXPLANATION_CHARS {
        return Err(ApodReject::ShortExplanation(len));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stored 2026-09-29 entry, the last real one before the migration,
    /// with its explanation cut to the first three sentences.
    fn real() -> Apod {
        Apod {
            date: "2026-09-29".into(),
            title: "Sh2-188: The Shrimp Nebula".into(),
            explanation: "What causes the swirl in the Shrimp Nebula? Its high speed is likely.  \
                What is sure is that Sh2-188 is one of the larger planetary nebulas on the night \
                sky, by angular size, spanning about half the diameter of the Moon."
                .into(),
            url: "https://apod.nasa.gov/apod/image/2609/Shrimp_Pawel_960.jpg".into(),
            media_type: "image".into(),
            hdurl: Some("https://apod.nasa.gov/apod/image/2609/Shrimp_Pawel_2048.jpg".into()),
            credit: None,
            copyright: None,
        }
    }

    const LOGO: &str =
        "https://science.nasa.gov/wp-content/themes/nasa-child/assets/images/nasa-logo@2x.png";

    /// The 2026-11-26 explanation exactly as stored: five tabs mid sentence and
    /// cut off at `HTML5 vide`.
    const VIDEO_FALLBACK: &str = "To view this video please enable JavaScript, and consider \
        upgrading to a web browser that \t\t\t\t\tsupports HTML5 vide";

    #[test]
    fn a_real_image_passes() {
        assert_eq!(check(&real()), Ok(()));
    }

    #[test]
    fn a_real_video_with_no_hdurl_passes() {
        let a = Apod {
            url: "https://www.youtube.com/embed/6_cH5-daLjg?si=i9geSInQj3VMZwx3".into(),
            media_type: "video".into(),
            hdurl: None,
            ..real()
        };
        assert_eq!(check(&a), Ok(()));
    }

    /// The record served on 2026-11-26, every field as stored. It breaks four
    /// rules and reports the first.
    #[test]
    fn the_stored_2026_11_26_record_is_rejected() {
        let a = Apod {
            date: "2026-11-26".into(),
            title: "NASA Science".into(),
            explanation: VIDEO_FALLBACK.into(),
            url: LOGO.into(),
            media_type: "image".into(),
            hdurl: Some(LOGO.into()),
            credit: None,
            copyright: None,
        };
        assert_eq!(check(&a), Err(ApodReject::ChromeImage("url")));
    }

    /// The variant of the same day with a real looking explanation, the one the
    /// API returned later. A rule keyed on the explanation alone would pass it.
    #[test]
    fn the_landing_page_variant_with_a_real_explanation_is_rejected() {
        let a = Apod {
            title: "NASA Science".into(),
            url: LOGO.into(),
            hdurl: Some(LOGO.into()),
            ..real()
        };
        assert!(check(&a).is_err());
    }

    #[test]
    fn each_required_field_left_empty_is_named() {
        for (field, a) in [
            (
                "date",
                Apod {
                    date: " ".into(),
                    ..real()
                },
            ),
            (
                "title",
                Apod {
                    title: String::new(),
                    ..real()
                },
            ),
            (
                "explanation",
                Apod {
                    explanation: "\n\t".into(),
                    ..real()
                },
            ),
            (
                "url",
                Apod {
                    url: String::new(),
                    ..real()
                },
            ),
        ] {
            assert_eq!(check(&a), Err(ApodReject::EmptyField(field)), "{field}");
        }
    }

    #[test]
    fn a_url_under_a_theme_directory_is_chrome() {
        let a = Apod {
            url: "https://example.org/wp-content/themes/x/bg.jpg".into(),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::ChromeImage("url")));
    }

    #[test]
    fn a_url_under_a_plugin_directory_is_chrome() {
        let a = Apod {
            url: "https://example.org/wp-content/plugins/x/hero.jpg".into(),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::ChromeImage("url")));
    }

    #[test]
    fn a_url_naming_a_logo_file_is_chrome() {
        let a = Apod {
            url: "https://cdn.example.org/img/Site-LOGO.svg?v=3".into(),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::ChromeImage("url")));
    }

    /// "logo" in a directory or a query is not the file, so a picture stored
    /// under such a path is still a picture.
    #[test]
    fn logo_outside_the_file_name_is_not_chrome() {
        let a = Apod {
            url: "https://apod.nasa.gov/apod/image/logo-month/M31.jpg?from=logo".into(),
            ..real()
        };
        assert_eq!(check(&a), Ok(()));
    }

    #[test]
    fn a_chrome_hdurl_is_rejected_even_when_url_is_real() {
        let a = Apod {
            hdurl: Some(LOGO.into()),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::ChromeImage("hdurl")));
    }

    #[test]
    fn the_site_name_as_title_is_rejected_in_any_case() {
        let a = Apod {
            title: "  nasa SCIENCE ".into(),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::SiteTitle));
    }

    #[test]
    fn a_page_title_ending_in_the_site_name_is_rejected() {
        let a = Apod {
            title: "APOD - NASA Science".into(),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::SiteTitle));
    }

    /// A real title may mention NASA. Only the site name as the whole title,
    /// or as the page title suffix, is chrome.
    #[test]
    fn a_title_that_mentions_nasa_passes() {
        let a = Apod {
            title: "NASA Science Team Spots a Comet".into(),
            ..real()
        };
        assert_eq!(check(&a), Ok(()));
    }

    /// The stored fallback text padded past the length floor, so only the
    /// phrase rule can reject it.
    #[test]
    fn the_stored_video_fallback_is_rejected_by_its_phrases() {
        let a = Apod {
            explanation: format!("{VIDEO_FALLBACK} {}", "x".repeat(200)),
            ..real()
        };
        assert_eq!(check(&a), Err(ApodReject::FallbackText));
    }

    #[test]
    fn each_fallback_phrase_rejects_on_its_own() {
        for phrase in [
            "Please ENABLE   JavaScript to continue.",
            "Your browser\tsupports\nHTML5.",
        ] {
            let a = Apod {
                explanation: format!("{phrase} {}", "x".repeat(200)),
                ..real()
            };
            assert_eq!(check(&a), Err(ApodReject::FallbackText), "{phrase}");
        }
    }

    #[test]
    fn an_explanation_one_short_of_the_floor_is_rejected_and_the_floor_passes() {
        let short = Apod {
            explanation: "é".repeat(MIN_EXPLANATION_CHARS - 1),
            ..real()
        };
        assert_eq!(
            check(&short),
            Err(ApodReject::ShortExplanation(MIN_EXPLANATION_CHARS - 1))
        );
        let exact = Apod {
            explanation: "é".repeat(MIN_EXPLANATION_CHARS),
            ..real()
        };
        assert_eq!(check(&exact), Ok(()));
    }

    #[test]
    fn every_reason_names_what_failed() {
        assert_eq!(
            ApodReject::EmptyField("title").to_string(),
            "title is empty"
        );
        assert_eq!(
            ApodReject::ChromeImage("hdurl").to_string(),
            "hdurl is a site asset, not an image"
        );
        assert_eq!(ApodReject::SiteTitle.to_string(), "title is the site name");
        assert_eq!(
            ApodReject::FallbackText.to_string(),
            "explanation is browser fallback text"
        );
        assert_eq!(
            ApodReject::ShortExplanation(114).to_string(),
            "explanation is 114 characters, under 200"
        );
    }
}
