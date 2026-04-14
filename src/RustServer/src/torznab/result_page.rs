//! Torznab search result page (RSS 2.0 with the `atom` and `torznab`
//! namespace extensions).
//!
//! The shape mirrors `ResultPage.ToXml` on the .NET side. Sonarr, Radarr
//! and every other Torznab client Zilean has ever served parse by element
//! name, not by byte-for-byte equality, so we use quick-xml to emit the
//! document with the same child ordering and attribute set the original
//! implementation produced.

use std::io::Cursor;

use chrono::{DateTime, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use super::xml_utils::{strip_invalid_xml_chars, xml_date_format};

const ATOM_NS: &str = "http://www.w3.org/2005/Atom";
const TORZNAB_NS: &str = "http://torznab.com/schemas/2015/feed";

const CHANNEL_TITLE: &str = "Zilean Indexer";
const CHANNEL_DESC: &str = "DMM Cached RD Indexer";
const CHANNEL_LINK: &str = "https://github.com/iPromKnight/zilean";
const CHANNEL_LANGUAGE: &str = "en-US";
const CHANNEL_CATEGORY: &str = "search";

/// Fake but stable values advertised per item. Matches
/// `ReleaseInfo.Seeders` / `ReleaseInfo.Peers`.
pub const FAKE_SEEDERS: i64 = 999;
pub const FAKE_PEERS: i64 = 999;
pub const ITEM_TYPE: &str = "Zilean";
pub const ENCLOSURE_TYPE: &str = "application/x-bittorrent";

/// A single result emitted inside the `<channel>`. The handler builds one
/// of these per torrent row returned by the search.
#[derive(Debug, Clone, Default)]
pub struct Release {
    pub title: String,
    pub guid: String,
    pub info_hash: String,
    pub magnet: String,
    pub publish_date: Option<DateTime<Utc>>,
    pub size: Option<i64>,
    /// Torznab category ids emitted in both `<category>` elements and
    /// `<torznab:attr name="category">` attrs.
    pub categories: Vec<i32>,
    /// IMDb numeric id. Rendered with D7 zero-padding; prefixed `tt` for
    /// the `imdbid` attr variant.
    pub imdb: Option<u64>,
    /// Languages to emit as `<torznab:attr name="language">` repeats.
    pub languages: Vec<String>,
    pub year: Option<i32>,
}

/// Serialise the full RSS document. `self_link` is the absolute URL of
/// the current request, echoed back in the `<atom:link rel="self">`
/// element the way ASP.NET's XDocument builder did.
pub fn render(self_link: &str, releases: &[Release]) -> String {
    let mut w = Writer::new(Cursor::new(Vec::<u8>::new()));
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .unwrap();

    // <rss version="2.0" xmlns:atom="..." xmlns:torznab="...">
    let mut rss = BytesStart::new("rss");
    rss.push_attribute(("version", "2.0"));
    rss.push_attribute(("xmlns:atom", ATOM_NS));
    rss.push_attribute(("xmlns:torznab", TORZNAB_NS));
    w.write_event(Event::Start(rss)).unwrap();

    // <channel>
    w.write_event(Event::Start(BytesStart::new("channel"))).unwrap();

    // <atom:link href="..." rel="self" type="application/rss+xml"/>
    {
        let mut el = BytesStart::new("atom:link");
        el.push_attribute(("href", self_link));
        el.push_attribute(("rel", "self"));
        el.push_attribute(("type", "application/rss+xml"));
        w.write_event(Event::Empty(el)).unwrap();
    }

    write_text_el(&mut w, "title", CHANNEL_TITLE);
    write_text_el(&mut w, "description", CHANNEL_DESC);
    write_text_el(&mut w, "link", CHANNEL_LINK);
    write_text_el(&mut w, "language", CHANNEL_LANGUAGE);
    write_text_el(&mut w, "category", CHANNEL_CATEGORY);

    for r in releases {
        write_item(&mut w, r);
    }

    w.write_event(Event::End(BytesEnd::new("channel"))).unwrap();
    w.write_event(Event::End(BytesEnd::new("rss"))).unwrap();

    let buf = w.into_inner().into_inner();
    String::from_utf8(buf).expect("quick-xml produces valid UTF-8")
}

fn write_text_el(w: &mut Writer<Cursor<Vec<u8>>>, name: &str, text: &str) {
    w.write_event(Event::Start(BytesStart::new(name))).unwrap();
    w.write_event(Event::Text(BytesText::new(text))).unwrap();
    w.write_event(Event::End(BytesEnd::new(name))).unwrap();
}

fn write_attr(w: &mut Writer<Cursor<Vec<u8>>>, name: &str, value: &str) {
    let mut el = BytesStart::new("torznab:attr");
    el.push_attribute(("name", name));
    el.push_attribute(("value", value));
    w.write_event(Event::Empty(el)).unwrap();
}

fn write_item(w: &mut Writer<Cursor<Vec<u8>>>, r: &Release) {
    w.write_event(Event::Start(BytesStart::new("item"))).unwrap();

    let safe_title = strip_invalid_xml_chars(&r.title);
    write_text_el(w, "title", &safe_title);
    write_text_el(w, "guid", &r.guid);
    write_text_el(w, "type", ITEM_TYPE);

    // pubDate: fall back to "now" if the caller didn't set one, matching
    // the .NET behaviour (DateTime.MinValue -> DateTime.Now).
    let pub_dt = r.publish_date.unwrap_or_else(Utc::now);
    write_text_el(w, "pubDate", &xml_date_format(pub_dt));

    if let Some(size) = r.size {
        write_text_el(w, "size", &size.to_string());
    }

    write_text_el(w, "link", &r.magnet);

    for cat in &r.categories {
        write_text_el(w, "category", &cat.to_string());
    }

    // <enclosure url="..." length="..." type="application/x-bittorrent"/>
    {
        let mut el = BytesStart::new("enclosure");
        el.push_attribute(("url", r.magnet.as_str()));
        let size_str;
        if let Some(size) = r.size {
            size_str = size.to_string();
            el.push_attribute(("length", size_str.as_str()));
        }
        el.push_attribute(("type", ENCLOSURE_TYPE));
        w.write_event(Event::Empty(el)).unwrap();
    }

    for cat in &r.categories {
        write_attr(w, "category", &cat.to_string());
    }

    if let Some(imdb) = r.imdb {
        write_attr(w, "imdb", &format!("{:07}", imdb));
        write_attr(w, "imdbid", &format!("tt{:07}", imdb));
    }

    for lang in &r.languages {
        write_attr(w, "language", lang);
    }

    if let Some(year) = r.year {
        write_attr(w, "year", &year.to_string());
    }

    write_attr(w, "seeders", &FAKE_SEEDERS.to_string());
    write_attr(w, "peers", &FAKE_PEERS.to_string());

    let safe_hash = strip_invalid_xml_chars(&r.info_hash);
    write_attr(w, "infohash", &safe_hash);
    write_attr(w, "magneturl", &r.magnet);

    w.write_event(Event::End(BytesEnd::new("item"))).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> Release {
        Release {
            title: "The Lord of the Rings (2001) 1080p BluRay".into(),
            guid: "ab7fb8de-77d1-bb15-31ad-4cf4ffb9494e".into(),
            info_hash: "0123456789abcdef0123456789abcdef01234567".into(),
            magnet: "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567".into(),
            publish_date: Some(Utc.with_ymd_and_hms(2015, 3, 14, 17, 10, 42).unwrap()),
            size: Some(12_345_678_900),
            categories: vec![2000],
            imdb: Some(120737),
            languages: vec!["English".into()],
            year: Some(2001),
        }
    }

    #[test]
    fn envelope_contains_namespaces_and_channel_metadata() {
        let xml = render("https://indexer.example.com/torznab/api", &[sample()]);
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(xml.contains(r#"xmlns:atom="http://www.w3.org/2005/Atom""#));
        assert!(xml.contains(r#"xmlns:torznab="http://torznab.com/schemas/2015/feed""#));
        assert!(xml.contains("<title>Zilean Indexer</title>"));
        assert!(xml.contains("<language>en-US</language>"));
        assert!(xml.contains("<category>search</category>"));
        assert!(
            xml.contains(r#"<atom:link href="https://indexer.example.com/torznab/api" rel="self" type="application/rss+xml"/>"#)
        );
    }

    #[test]
    fn item_contains_required_fields() {
        let xml = render("https://x/y", &[sample()]);
        assert!(xml.contains("<title>The Lord of the Rings (2001) 1080p BluRay</title>"));
        assert!(xml.contains("<guid>ab7fb8de-77d1-bb15-31ad-4cf4ffb9494e</guid>"));
        assert!(xml.contains("<type>Zilean</type>"));
        assert!(xml.contains("<pubDate>Sat, 14 Mar 2015 17:10:42 +0000</pubDate>"));
        assert!(xml.contains("<size>12345678900</size>"));
        assert!(xml.contains("<link>magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567</link>"));
        assert!(xml.contains(r#"<enclosure url="magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567" length="12345678900" type="application/x-bittorrent"/>"#));
    }

    #[test]
    fn torznab_attrs_layout() {
        let xml = render("https://x/y", &[sample()]);
        // IMDb formatted as D7 (zero-padded 7 digits); with and without `tt`.
        assert!(xml.contains(r#"<torznab:attr name="imdb" value="0120737"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="imdbid" value="tt0120737"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="category" value="2000"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="language" value="English"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="year" value="2001"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="seeders" value="999"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="peers" value="999"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="infohash" value="0123456789abcdef0123456789abcdef01234567"/>"#));
        assert!(xml.contains(r#"<torznab:attr name="magneturl" value="magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"/>"#));
    }

    #[test]
    fn omits_size_and_length_when_unknown() {
        let r = Release { size: None, ..sample() };
        let xml = render("https://x/y", &[r]);
        assert!(!xml.contains("<size>"));
        // Enclosure still present, but without length attribute.
        assert!(xml.contains(r#"<enclosure url="magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567" type="application/x-bittorrent"/>"#));
    }

    #[test]
    fn omits_imdb_when_absent() {
        let r = Release { imdb: None, ..sample() };
        let xml = render("https://x/y", &[r]);
        assert!(!xml.contains(r#"name="imdb""#));
        assert!(!xml.contains(r#"name="imdbid""#));
    }
}
