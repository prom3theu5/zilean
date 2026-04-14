//! Torznab capabilities document for `?t=caps`.
//!
//! Mirrors the shape emitted by `TorznabCapabilities.ToXml` on the .NET
//! side so Sonarr and Radarr see the same advertised features they always
//! have. The response is intentionally terse: all parameters are hard-coded
//! because Zilean's capabilities are a property of the server, not of
//! configuration.

use std::io::Cursor;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};

use super::categories::{EXPOSED_CATEGORIES, TorznabCategory};

/// Advertised maximum rows per search. Matches
/// `TorznabCapabilities.LimitsMax = 5000`.
pub const LIMITS_MAX: i32 = 5000;
/// Advertised default rows per search when `limit` is unspecified. Matches
/// `TorznabCapabilities.LimitsDefault = 100`.
pub const LIMITS_DEFAULT: i32 = 100;

/// Build the capabilities XML document.
pub fn render() -> String {
    let mut w = Writer::new(Cursor::new(Vec::<u8>::new()));
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .unwrap();

    // <caps>
    w.write_event(Event::Start(BytesStart::new("caps"))).unwrap();

    // <server title="Zilean"/>
    {
        let mut srv = BytesStart::new("server");
        srv.push_attribute(("title", "Zilean"));
        w.write_event(Event::Empty(srv)).unwrap();
    }

    // <limits default="100" max="5000"/>
    {
        let default = LIMITS_DEFAULT.to_string();
        let max = LIMITS_MAX.to_string();
        let mut lim = BytesStart::new("limits");
        lim.push_attribute(("default", default.as_str()));
        lim.push_attribute(("max", max.as_str()));
        w.write_event(Event::Empty(lim)).unwrap();
    }

    // <searching>...</searching>
    w.write_event(Event::Start(BytesStart::new("searching")))
        .unwrap();
    write_search_el(&mut w, "search", "yes", "q");
    write_search_el(&mut w, "tv-search", "yes", "q,season,ep,imdbid,year");
    write_search_el(&mut w, "movie-search", "yes", "q,imdbid,year");
    write_search_el(&mut w, "xxx-search", "yes", "q,imdbid,year");
    w.write_event(Event::End(BytesEnd::new("searching"))).unwrap();

    // <categories> with exposed parents + their sub-categories.
    w.write_event(Event::Start(BytesStart::new("categories")))
        .unwrap();
    for cat in EXPOSED_CATEGORIES {
        write_category(&mut w, cat);
    }
    w.write_event(Event::End(BytesEnd::new("categories"))).unwrap();

    w.write_event(Event::End(BytesEnd::new("caps"))).unwrap();

    let buf = w.into_inner().into_inner();
    String::from_utf8(buf).expect("quick-xml produces valid UTF-8")
}

fn write_search_el(
    w: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    available: &str,
    supported_params: &str,
) {
    let mut el = BytesStart::new(name);
    el.push_attribute(("available", available));
    el.push_attribute(("supportedParams", supported_params));
    w.write_event(Event::Empty(el)).unwrap();
}

fn write_category(w: &mut Writer<Cursor<Vec<u8>>>, cat: &TorznabCategory) {
    // Sort sub-categories by id to match the .NET GetTorznabCategoryTree
    // ordering. The parent list is already in spec order.
    let mut subs: Vec<&TorznabCategory> = cat.sub_categories.iter().collect();
    subs.sort_by_key(|s| s.id);

    let id_s = cat.id.to_string();
    let mut start = BytesStart::new("category");
    start.push_attribute(("id", id_s.as_str()));
    start.push_attribute(("name", cat.name));
    w.write_event(Event::Start(start)).unwrap();

    for sc in subs {
        let sid = sc.id.to_string();
        let mut sub = BytesStart::new("subcat");
        sub.push_attribute(("id", sid.as_str()));
        sub.push_attribute(("name", sc.name));
        w.write_event(Event::Empty(sub)).unwrap();
    }

    w.write_event(Event::End(BytesEnd::new("category"))).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decl_and_structure() {
        let xml = render();
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(xml.contains(r#"<server title="Zilean"/>"#));
        assert!(xml.contains(r#"<limits default="100" max="5000"/>"#));
    }

    #[test]
    fn search_feature_flags() {
        let xml = render();
        assert!(xml.contains(r#"<search available="yes" supportedParams="q"/>"#));
        assert!(
            xml.contains(r#"<tv-search available="yes" supportedParams="q,season,ep,imdbid,year"/>"#)
        );
        assert!(
            xml.contains(r#"<movie-search available="yes" supportedParams="q,imdbid,year"/>"#)
        );
        assert!(
            xml.contains(r#"<xxx-search available="yes" supportedParams="q,imdbid,year"/>"#)
        );
    }

    #[test]
    fn exposes_three_top_level_categories() {
        let xml = render();
        assert!(xml.contains(r#"<category id="2000" name="Movies">"#));
        assert!(xml.contains(r#"<category id="5000" name="TV">"#));
        assert!(xml.contains(r#"<category id="6000" name="XXX">"#));
    }

    #[test]
    fn subcategories_are_id_sorted() {
        let xml = render();
        // Find the Movies block and check that MoviesForeign (2010) comes
        // before MoviesHD (2040) in output order.
        let start = xml.find(r#"<category id="2000""#).unwrap();
        let end = xml[start..].find("</category>").unwrap();
        let block = &xml[start..start + end];
        let i2010 = block.find(r#"<subcat id="2010""#).unwrap();
        let i2040 = block.find(r#"<subcat id="2040""#).unwrap();
        assert!(i2010 < i2040);
    }
}
