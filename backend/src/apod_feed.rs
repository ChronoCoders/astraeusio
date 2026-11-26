//! NASA's own APOD feed, read into [`Apod`] records.
//!
//! The APOD API has answered every date with the science.nasa.gov landing page
//! since 2026-09-30 (AUD-074), so the ingest reads
//! `https://science.nasa.gov/feed/apod-basic/` instead: RSS with the last 30
//! days, one item per day, the APOD fields in their own namespace. What each
//! rule below rests on was measured over the 330 items of 2026 and is in
//! AUD-074.
//!
//! The feed is XML and the explanation and credit inside it are HTML in CDATA.
//! Both layers are read by event with `quick-xml`, and neither by pattern.

use crate::nasa::Apod;
use chrono::{DateTime, NaiveDate, NaiveTime, TimeDelta, Utc};
use quick_xml::escape::{resolve_html5_entity, resolve_xml_entity};
use quick_xml::events::{BytesRef, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::{NsReader, Reader};
use reqwest::Url;
use std::fmt;

pub const FEED_URL: &str = "https://science.nasa.gov/feed/apod-basic/";

const APOD_NS: &str = "https://science.nasa.gov/apod/";

/// Tags that sit inside a line of text. Every other tag separates words, so
/// `a<br>b` reads `a b` while `<a>name</a>,` keeps its comma against the name.
const INLINE_TAGS: [&str; 8] = ["a", "b", "strong", "i", "em", "span", "sup", "sub"];

/// 328 of 330 explanations open with this label in bold, and 2026-10-19
/// carries it twice, the second time as `Explanation :`. It is page layout,
/// not part of the text.
const EXPLANATION_LABEL: &str = "Explanation";

/// Marks an Adobe rendition of an asset. Six of 330 `apod:hdurl` values were
/// the 48 pixel thumbnail rendition, and the path before this is the image.
const RENDITION_MARK: &str = "/jcr:content/";

#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("feed is not well formed XML: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("feed uses the undefined entity &{0};")]
    Entity(String),
}

/// Why one item was not turned into a record. The rest of the feed is still
/// read.
#[derive(Debug, PartialEq, Eq)]
pub enum ItemError {
    Missing(&'static str),
    DiscussionDate(String),
    PubDate(String),
    OutsideDay { date: NaiveDate, published: String },
    Hdurl(String),
    Markup { field: &'static str, reason: String },
}

impl fmt::Display for ItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(field) => write!(f, "{field} is missing or empty"),
            Self::DiscussionDate(v) => write!(f, "apod:discussion carries no date=YYMMDD: {v}"),
            Self::PubDate(v) => write!(f, "pubDate is not an RFC 2822 time: {v}"),
            Self::OutsideDay { date, published } => write!(
                f,
                "pubDate {published} is outside the US Eastern day {date}"
            ),
            Self::Hdurl(v) => write!(f, "apod:hdurl is not a URL: {v}"),
            Self::Markup { field, reason } => write!(f, "{field} markup is unreadable: {reason}"),
        }
    }
}

#[derive(Clone, Copy)]
enum Field {
    Title,
    PubDate,
    Hdurl,
    Discussion,
    Explanation,
    Credit,
    Copyright,
}

impl Field {
    fn of(ns: &ResolveResult<'_>, local: &str) -> Option<Self> {
        match (ns, local) {
            (ResolveResult::Unbound, "title") => Some(Self::Title),
            (ResolveResult::Unbound, "pubDate") => Some(Self::PubDate),
            (ResolveResult::Bound(n), _) if n.into_inner() == APOD_NS => match local {
                "hdurl" => Some(Self::Hdurl),
                "discussion" => Some(Self::Discussion),
                "explanation" => Some(Self::Explanation),
                "credit" => Some(Self::Credit),
                "copyright" => Some(Self::Copyright),
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Default)]
struct RawItem {
    title: Option<String>,
    pub_date: Option<String>,
    hdurl: Option<String>,
    discussion: Option<String>,
    explanation: Option<String>,
    credit: Option<String>,
    copyright: Option<String>,
}

impl RawItem {
    fn set(&mut self, field: Field, value: String) {
        let slot = match field {
            Field::Title => &mut self.title,
            Field::PubDate => &mut self.pub_date,
            Field::Hdurl => &mut self.hdurl,
            Field::Discussion => &mut self.discussion,
            Field::Explanation => &mut self.explanation,
            Field::Credit => &mut self.credit,
            Field::Copyright => &mut self.copyright,
        };
        *slot = Some(value);
    }

    fn into_record(self) -> Result<Apod, ItemError> {
        let title = collapse(&required(self.title, "title")?);
        let date = apod_date(
            &required(self.discussion, "apod:discussion")?,
            &required(self.pub_date, "pubDate")?,
        )?;
        let image = image_asset(&required(self.hdurl, "apod:hdurl")?)?;
        let explanation = html_text(
            &required(self.explanation, "apod:explanation")?,
            "apod:explanation",
        )?;
        let explanation = without_label(&explanation).to_owned();
        Ok(Apod {
            date: date.format("%Y-%m-%d").to_string(),
            title,
            explanation,
            url: image.clone(),
            // Video days carry a still frame in `apod:hdurl` and the video only
            // inside the HTML page, so what is stored and shown is an image.
            media_type: "image".into(),
            hdurl: Some(image),
            credit: optional_text(self.credit, "apod:credit")?,
            copyright: optional_text(self.copyright, "apod:copyright")?,
        })
    }
}

/// Every `<item>` in the feed, each a record or the reason it is not one. A
/// document that is not well formed fails as a whole.
pub fn parse(xml: &str) -> Result<Vec<Result<Apod, ItemError>>, FeedError> {
    let mut reader = NsReader::from_str(xml);
    let mut items = Vec::new();
    let mut item: Option<RawItem> = None;
    let mut field: Option<Field> = None;
    let mut text = String::new();
    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Start(e) => match item {
                None if matches!(ns, ResolveResult::Unbound)
                    && e.local_name().as_ref() == "item" =>
                {
                    item = Some(RawItem::default());
                }
                None => {}
                Some(_) => {
                    field = Field::of(&ns, e.local_name().as_ref());
                    text.clear();
                }
            },
            Event::Text(t) if field.is_some() => text.push_str(&t.xml10_content()),
            Event::CData(c) if field.is_some() => text.push_str(&c.xml10_content()),
            Event::GeneralRef(r) if field.is_some() => {
                push_ref(&mut text, &r, resolve_xml_entity).map_err(FeedError::Entity)?;
            }
            Event::End(e) => {
                if let Some(done) = field.take() {
                    if let Some(raw) = item.as_mut() {
                        raw.set(done, std::mem::take(&mut text));
                    }
                } else if e.local_name().as_ref() == "item"
                    && let Some(raw) = item.take()
                {
                    items.push(raw.into_record());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(items)
}

fn without_label(mut text: &str) -> &str {
    while let Some(rest) = text
        .strip_prefix(EXPLANATION_LABEL)
        .and_then(|r| r.trim_start().strip_prefix(':'))
    {
        text = rest.trim_start();
    }
    text
}

fn required(value: Option<String>, field: &'static str) -> Result<String, ItemError> {
    value
        .filter(|v| !v.trim().is_empty())
        .ok_or(ItemError::Missing(field))
}

fn optional_text(value: Option<String>, field: &'static str) -> Result<Option<String>, ItemError> {
    let Some(html) = value else {
        return Ok(None);
    };
    let text = html_text(&html, field)?;
    Ok((!text.is_empty()).then_some(text))
}

/// The APOD day of an item.
///
/// APOD days are US Eastern days. `pubDate` alone cannot name one: 04:05 UTC is
/// 00:05 on day D under daylight time and 23:05 on day D minus 1 under standard
/// time, and both offsets occur in the feed. `apod:discussion` carries
/// `date=YYMMDD`, APOD's own identifier for the day, so the day comes from there
/// and `pubDate` must fall inside it: from D 04:00 UTC, the earliest start of
/// an Eastern day, to D plus 1 05:00 UTC, the latest end. A wrong century in
/// `YY` fails the same check.
fn apod_date(discussion: &str, pub_date: &str) -> Result<NaiveDate, ItemError> {
    let bad = || ItemError::DiscussionDate(discussion.to_owned());
    let url = Url::parse(discussion.trim()).map_err(|_| bad())?;
    let id = url
        .query_pairs()
        .find(|(k, _)| k == "date")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(bad)?;
    let date = yymmdd(&id).ok_or_else(bad)?;
    let published = DateTime::parse_from_rfc2822(pub_date.trim())
        .map_err(|_| ItemError::PubDate(pub_date.to_owned()))?
        .with_timezone(&Utc);
    let start = date.and_time(NaiveTime::MIN).and_utc() + TimeDelta::hours(4);
    if published < start || published >= start + TimeDelta::hours(25) {
        return Err(ItemError::OutsideDay {
            date,
            published: pub_date.to_owned(),
        });
    }
    Ok(date)
}

fn yymmdd(id: &str) -> Option<NaiveDate> {
    if id.len() != 6 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let part = |at: usize| id[at..at + 2].parse::<u32>().ok();
    let year = 2000 + i32::try_from(part(0)?).ok()?;
    NaiveDate::from_ymd_opt(year, part(2)?, part(4)?)
}

/// The image asset an `apod:hdurl` names, with a rendition suffix removed.
fn image_asset(hdurl: &str) -> Result<String, ItemError> {
    let mut url = Url::parse(hdurl.trim()).map_err(|_| ItemError::Hdurl(hdurl.to_owned()))?;
    if let Some(at) = url.path().find(RENDITION_MARK) {
        let asset = url.path()[..at].to_owned();
        url.set_path(&asset);
    }
    Ok(url.into())
}

/// An HTML fragment as plain text: tags dropped, entities decoded, whitespace
/// collapsed. The text of a link is kept and its target is not.
fn html_text(fragment: &str, field: &'static str) -> Result<String, ItemError> {
    let markup = |reason: String| ItemError::Markup { field, reason };
    let mut reader = Reader::from_str(fragment);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.allow_unmatched_ends = true;
    let mut out = String::new();
    loop {
        match reader.read_event().map_err(|e| markup(e.to_string()))? {
            Event::Text(t) => out.push_str(&t.html_content()),
            Event::GeneralRef(r) => push_ref(&mut out, &r, resolve_html5_entity)
                .map_err(|name| markup(format!("undefined entity &{name};")))?,
            Event::Start(e) | Event::Empty(e) if !is_inline(e.local_name().as_ref()) => {
                out.push(' ')
            }
            Event::End(e) if !is_inline(e.local_name().as_ref()) => out.push(' '),
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(collapse(&out))
}

fn is_inline(tag: &str) -> bool {
    INLINE_TAGS.iter().any(|t| t.eq_ignore_ascii_case(tag))
}

/// Appends what a reference stands for, or returns its name when `resolve`
/// does not know it.
fn push_ref(
    out: &mut String,
    r: &BytesRef<'_>,
    resolve: fn(&str) -> Option<&'static str>,
) -> Result<(), String> {
    let name: &str = r;
    match r.resolve_char_ref() {
        Ok(Some(c)) => out.push(c),
        Ok(None) => out.push_str(resolve(name).ok_or_else(|| name.to_owned())?),
        Err(_) => return Err(name.to_owned()),
    }
    Ok(())
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The feed's header and five of its items, verbatim: 2026-11-26, the
    /// 2026-11-25 video day, the 2026-11-08 thumbnail rendition, 2026-11-01
    /// published at 04:05 UTC, and 2026-10-25 published at 08:19 UTC, the last
    /// from `?year=2026`.
    const FEED: &str = include_str!("../tests/fixtures/apod-feed-2026-11-26.xml");

    fn records() -> Vec<Apod> {
        parse(FEED)
            .expect("the fixture is well formed")
            .into_iter()
            .collect::<Result<_, _>>()
            .expect("every fixture item reads")
    }

    fn day(date: &str) -> Apod {
        records()
            .into_iter()
            .find(|a| a.date == date)
            .unwrap_or_else(|| panic!("no record for {date}"))
    }

    /// One item with every field valid, and `swap` replacing one of them.
    fn feed_with(swap: &[(&str, &str)]) -> String {
        let mut fields = vec![
            ("title", "<title>Test Nebula</title>".to_owned()),
            (
                "pubDate",
                "<pubDate>Thu, 26 Nov 2026 05:05:00 +0000</pubDate>".to_owned(),
            ),
            (
                "hdurl",
                "<apod:hdurl>https://assets.science.nasa.gov/a/b.jpg</apod:hdurl>".to_owned(),
            ),
            (
                "discussion",
                "<apod:discussion>https://asterisk.apod.com/discuss_apod.php?date=261126</apod:discussion>"
                    .to_owned(),
            ),
            (
                "explanation",
                "<apod:explanation><![CDATA[<strong>Explanation:</strong> Text.]]></apod:explanation>"
                    .to_owned(),
            ),
            (
                "credit",
                "<apod:credit><![CDATA[<a href=\"https://x.example\">A. Name</a>]]></apod:credit>"
                    .to_owned(),
            ),
            (
                "copyright",
                "<apod:copyright><![CDATA[A. Name]]></apod:copyright>".to_owned(),
            ),
        ];
        for (name, xml) in swap {
            let slot = fields
                .iter_mut()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("no field {name}"));
            slot.1 = (*xml).to_owned();
        }
        let body: String = fields.into_iter().map(|(_, xml)| xml).collect();
        format!(
            "<?xml version=\"1.0\"?><rss xmlns:apod=\"https://science.nasa.gov/apod/\">\
             <channel><title>APOD Basic</title><item>{body}</item></channel></rss>"
        )
    }

    fn one(xml: &str) -> Result<Apod, ItemError> {
        let mut items = parse(xml).expect("well formed");
        assert_eq!(items.len(), 1);
        items.remove(0)
    }

    #[test]
    fn every_fixture_item_reads_with_its_apod_day() {
        let dates: Vec<String> = records().into_iter().map(|a| a.date).collect();
        assert_eq!(
            dates,
            [
                "2026-11-26",
                "2026-11-25",
                "2026-11-08",
                "2026-11-01",
                "2026-10-25"
            ]
        );
    }

    /// The validator applies unchanged to the new source, and real items pass.
    #[test]
    fn every_fixture_record_passes_the_validator() {
        for a in records() {
            assert_eq!(crate::apod_check::check(&a), Ok(()), "{}", a.date);
        }
    }

    #[test]
    fn the_2026_11_26_item_reads_as_plain_text() {
        let a = day("2026-11-26");
        assert_eq!(a.title, "Eye in the Sky: The Milky Way over Lençóis");
        assert!(
            a.explanation.starts_with(
                "Beauty is in the eye of the beholder. And there is much to behold in \
                 today's featured image. The Milky Way shines bright in the sky of Brazil,"
            ),
            "{}",
            a.explanation
        );
        // Two spaces in the source between "of" and "the".
        assert!(
            a.explanation
                .contains("different parts of the Milky Way as it apparently")
        );
        for text in [
            &a.explanation,
            a.credit.as_ref().unwrap(),
            a.copyright.as_ref().unwrap(),
        ] {
            assert!(!text.contains(['<', '>']), "a tag survived: {text}");
            assert!(
                !text.contains("&amp;") && !text.contains("&#"),
                "an entity survived: {text}"
            );
            assert!(
                !text.contains("  ") && text.trim() == text.as_str(),
                "{text:?}"
            );
        }
        assert_eq!(a.credit.as_deref(), Some("Ignacio Fernández"));
        assert_eq!(a.copyright.as_deref(), Some("Ignacio Fernández"));
    }

    /// The image shown is the `apod:hdurl` asset, with the XML `&#038;`
    /// decoded, and never the article page in `apod:url`.
    #[test]
    fn the_image_is_the_hdurl_asset_not_the_article_page() {
        let a = day("2026-11-26");
        let asset = "https://assets.science.nasa.gov/dynamicimage/assets/science/cds/apod/apod/\
                     2026/november/Milky_Way_Lencois.jpg?w=2576&h=2276&fit=clip";
        assert_eq!(a.url, asset);
        assert_eq!(a.hdurl.as_deref(), Some(asset));
        assert_eq!(a.media_type, "image");
    }

    #[test]
    fn a_thumbnail_rendition_is_read_as_its_asset() {
        let a = day("2026-11-08");
        assert_eq!(
            a.url,
            "https://assets.science.nasa.gov/content/dam/science/cds/apod/apod/2026/november/\
             meteor2017_Taylor_1200.jpg"
        );
    }

    #[test]
    fn a_video_day_stores_its_still_frame_as_an_image() {
        let a = day("2026-11-25");
        assert_eq!(a.media_type, "image");
        assert!(a.url.contains("/spherex_1.2micron.png?"), "{}", a.url);
    }

    #[test]
    fn the_channel_title_and_unknown_elements_are_not_item_fields() {
        let a = one(&feed_with(&[])).unwrap();
        assert_eq!(a.title, "Test Nebula");
        assert_eq!(a.explanation, "Text.");
    }

    #[test]
    fn an_item_in_another_namespace_is_not_an_item() {
        let xml = feed_with(&[])
            .replace("<item>", "<x:item xmlns:x=\"https://other.example/\">")
            .replace("</item>", "</x:item>");
        assert_eq!(parse(&xml).expect("well formed").len(), 0);
    }

    #[test]
    fn a_title_is_read_with_its_whitespace_collapsed() {
        let a = one(&feed_with(&[(
            "title",
            "<title>\n  Test\t Nebula </title>",
        )]))
        .unwrap();
        assert_eq!(a.title, "Test Nebula");
    }

    #[test]
    fn a_field_in_another_namespace_is_not_taken() {
        for hdurl in [
            "<hdurl>https://assets.science.nasa.gov/a/b.jpg</hdurl>",
            "<x:hdurl xmlns:x=\"https://other.example/\">https://a.example/b.jpg</x:hdurl>",
        ] {
            assert_eq!(
                one(&feed_with(&[("hdurl", hdurl)])).err(),
                Some(ItemError::Missing("apod:hdurl")),
                "{hdurl}"
            );
        }
        assert_eq!(
            one(&feed_with(&[("title", "<apod:title>T</apod:title>")])).err(),
            Some(ItemError::Missing("title"))
        );
        assert_eq!(
            one(&feed_with(&[(
                "pubDate",
                "<apod:pubDate>Thu, 26 Nov 2026 05:05:00 +0000</apod:pubDate>"
            )]))
            .err(),
            Some(ItemError::Missing("pubDate"))
        );
    }

    #[test]
    fn each_required_field_missing_or_blank_is_named() {
        for (name, field, blank) in [
            ("title", "title", "<title> </title>"),
            ("pubDate", "pubDate", "<pubDate></pubDate>"),
            ("hdurl", "apod:hdurl", "<apod:hdurl>\n</apod:hdurl>"),
            ("discussion", "apod:discussion", "<apod:discussion/>"),
            (
                "explanation",
                "apod:explanation",
                "<apod:explanation><![CDATA[ ]]></apod:explanation>",
            ),
        ] {
            assert_eq!(
                one(&feed_with(&[(name, "")])).err(),
                Some(ItemError::Missing(field)),
                "{name}"
            );
            assert_eq!(
                one(&feed_with(&[(name, blank)])).err(),
                Some(ItemError::Missing(field)),
                "{name} blank"
            );
        }
    }

    #[test]
    fn credit_and_copyright_are_optional_and_empty_markup_is_none() {
        let a = one(&feed_with(&[("credit", ""), ("copyright", "")])).unwrap();
        assert_eq!((a.credit, a.copyright), (None, None));
        let a = one(&feed_with(&[(
            "copyright",
            "<apod:copyright><![CDATA[<a href=\"x\"></a>]]></apod:copyright>",
        )]))
        .unwrap();
        assert_eq!(a.copyright, None);
        assert_eq!(a.credit.as_deref(), Some("A. Name"));
    }

    fn discussion(id: &str) -> String {
        format!(
            "<apod:discussion>https://asterisk.apod.com/discuss_apod.php?date={id}</apod:discussion>"
        )
    }

    fn published(at: &str) -> String {
        format!("<pubDate>{at}</pubDate>")
    }

    #[test]
    fn a_discussion_link_without_a_usable_date_is_refused() {
        for value in [
            "<apod:discussion>not a url</apod:discussion>".to_owned(),
            "<apod:discussion>https://asterisk.apod.com/discuss_apod.php?day=261126</apod:discussion>"
                .to_owned(),
            discussion("26112"),
            discussion("2611260"),
            discussion("26+126"),
            discussion("261131"),
        ] {
            assert!(
                matches!(one(&feed_with(&[("discussion", &value)])), Err(ItemError::DiscussionDate(_))),
                "{value}"
            );
        }
    }

    #[test]
    fn an_unreadable_pub_date_is_refused() {
        assert!(matches!(
            one(&feed_with(&[(
                "pubDate",
                &published("2026-11-26T05:05:00Z")
            )])),
            Err(ItemError::PubDate(_))
        ));
    }

    /// The Eastern day 2026-11-26 runs from 04:00 UTC that day, its earliest
    /// possible start, to 05:00 UTC the next, its latest possible end.
    #[test]
    fn pub_date_must_fall_inside_the_eastern_day() {
        for (at, inside) in [
            ("Thu, 26 Nov 2026 03:59:59 +0000", false),
            ("Thu, 26 Nov 2026 04:00:00 +0000", true),
            ("Wed, 25 Nov 2026 23:30:00 -0500", true),
            ("Fri, 27 Nov 2026 04:59:59 +0000", true),
            ("Fri, 27 Nov 2026 05:00:00 +0000", false),
        ] {
            let got = one(&feed_with(&[("pubDate", &published(at))]));
            if inside {
                assert_eq!(
                    got.map(|a| a.date).ok().as_deref(),
                    Some("2026-11-26"),
                    "{at}"
                );
            } else {
                assert!(
                    matches!(got, Err(ItemError::OutsideDay { .. })),
                    "{at}: {got:?}"
                );
            }
        }
    }

    /// 04:05 UTC on 2026-11-01 is 00:05 daylight time that day, and the UTC
    /// day would only agree by coincidence. The identifier decides.
    #[test]
    fn the_day_comes_from_the_identifier_not_the_utc_day() {
        let a = one(&feed_with(&[
            ("discussion", &discussion("261031")),
            ("pubDate", &published("Sun, 01 Nov 2026 04:05:00 +0000")),
        ]));
        assert_eq!(a.map(|a| a.date).ok().as_deref(), Some("2026-10-31"));
    }

    #[test]
    fn a_wrong_century_fails_the_day_check() {
        assert!(matches!(
            one(&feed_with(&[("discussion", &discussion("951126"))])),
            Err(ItemError::OutsideDay { .. })
        ));
    }

    #[test]
    fn an_hdurl_that_is_not_a_url_is_refused() {
        assert_eq!(
            one(&feed_with(&[(
                "hdurl",
                "<apod:hdurl>/relative/b.jpg</apod:hdurl>"
            )]))
            .err(),
            Some(ItemError::Hdurl("/relative/b.jpg".into()))
        );
    }

    #[test]
    fn an_hdurl_without_a_rendition_keeps_its_query() {
        let a = one(&feed_with(&[(
            "hdurl",
            "<apod:hdurl>https://a.example/x/b.jpg?w=10&amp;h=20</apod:hdurl>",
        )]))
        .unwrap();
        assert_eq!(a.url, "https://a.example/x/b.jpg?w=10&h=20");
    }

    fn text(html: &str) -> Result<String, ItemError> {
        html_text(html, "apod:credit")
    }

    #[test]
    fn inline_tags_join_and_other_tags_separate() {
        assert_eq!(
            text("<a href=\"x\">Name</a>, <b>B</b><i>i</i><em>e</em><strong>s</strong>"),
            Ok("Name, Bies".into())
        );
        assert_eq!(
            text("<span>s</span><sup>2</sup><sub>3</sub>."),
            Ok("s23.".into())
        );
        assert_eq!(
            text("one<br>two<p>three</p>four<br/>five"),
            Ok("one two three four five".into())
        );
        assert_eq!(text("<STRONG>x</STRONG>y<BR>z"), Ok("xy z".into()));
        // An end tag that does not close the innermost open one.
        assert_eq!(text("<p>a<br>b</p>c"), Ok("a b c".into()));
    }

    #[test]
    fn entities_decode_and_comments_drop() {
        assert_eq!(
            text("a &amp; b &eacute; &#233; &#x2019; &lt;c&gt;<!-- note -->d"),
            Ok("a & b é é \u{2019} <c>d".into())
        );
    }

    #[test]
    fn unmatched_end_tags_are_read() {
        assert_eq!(text("</p>Ollie</a> Taylor"), Ok("Ollie Taylor".into()));
    }

    #[test]
    fn unreadable_markup_is_refused_with_its_field() {
        for html in ["a &bogus; b", "a &#0; b", "a <b"] {
            assert!(
                matches!(
                    text(html),
                    Err(ItemError::Markup {
                        field: "apod:credit",
                        ..
                    })
                ),
                "{html}: {:?}",
                text(html)
            );
        }
    }

    #[test]
    fn the_explanation_label_is_dropped_and_text_without_it_is_kept() {
        let labelled = one(&feed_with(&[])).unwrap();
        assert_eq!(labelled.explanation, "Text.");
        let plain = one(&feed_with(&[(
            "explanation",
            "<apod:explanation><![CDATA[What causes it? <a>Explanation:</a> none.]]></apod:explanation>",
        )]))
        .unwrap();
        assert_eq!(plain.explanation, "What causes it? Explanation: none.");
    }

    /// 2026-10-19 as the feed has it, and words that only start like the label.
    #[test]
    fn a_repeated_label_is_dropped_and_lookalikes_are_kept() {
        assert_eq!(
            without_label("Explanation: Explanation : What is Treble Clef?"),
            "What is Treble Clef?"
        );
        assert_eq!(
            without_label("Explanations differ."),
            "Explanations differ."
        );
        assert_eq!(
            without_label("Explanation without a colon."),
            "Explanation without a colon."
        );
    }

    #[test]
    fn a_document_that_is_not_xml_fails_whole() {
        assert!(matches!(
            parse("<rss><channel><item></channel>"),
            Err(FeedError::Xml(_))
        ));
    }

    #[test]
    fn an_entity_xml_does_not_define_fails_the_document() {
        let xml = feed_with(&[("title", "<title>A&nbsp;B</title>")]);
        assert!(matches!(parse(&xml), Err(FeedError::Entity(name)) if name == "nbsp"));
    }

    #[test]
    fn every_item_reason_names_what_failed() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        for (err, want) in [
            (ItemError::Missing("title"), "title is missing or empty"),
            (
                ItemError::DiscussionDate("u".into()),
                "apod:discussion carries no date=YYMMDD: u",
            ),
            (
                ItemError::PubDate("p".into()),
                "pubDate is not an RFC 2822 time: p",
            ),
            (
                ItemError::OutsideDay {
                    date,
                    published: "p".into(),
                },
                "pubDate p is outside the US Eastern day 2026-11-26",
            ),
            (ItemError::Hdurl("h".into()), "apod:hdurl is not a URL: h"),
            (
                ItemError::Markup {
                    field: "apod:credit",
                    reason: "r".into(),
                },
                "apod:credit markup is unreadable: r",
            ),
        ] {
            assert_eq!(err.to_string(), want);
        }
    }
}
