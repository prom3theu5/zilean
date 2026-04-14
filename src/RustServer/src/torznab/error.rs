//! Torznab error envelope.
//!
//! ```xml
//! <?xml version="1.0" encoding="UTF-8"?>
//! <error code="900" description="Invalid query conversion"/>
//! ```
//!
//! Matches `TorznabErrorResponse.Create` on the .NET side.

use std::io::Cursor;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesStart, Event};

use super::xml_utils::strip_invalid_xml_chars;

/// Render an `<error code="..." description="..."/>` XML document.
pub fn render(code: i32, description: &str) -> String {
    let mut w = Writer::new(Cursor::new(Vec::<u8>::new()));
    // XML declaration.
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect("writing XML decl to in-memory buffer");

    let code_str = code.to_string();
    let desc = strip_invalid_xml_chars(description);
    let mut elem = BytesStart::new("error");
    elem.push_attribute(("code", code_str.as_str()));
    elem.push_attribute(("description", desc.as_str()));
    w.write_event(Event::Empty(elem))
        .expect("writing empty element to in-memory buffer");

    let buf = w.into_inner().into_inner();
    String::from_utf8(buf).expect("quick-xml produces valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_standard_shape() {
        let xml = render(900, "Invalid query conversion");
        assert!(xml.contains(r#"code="900""#));
        assert!(xml.contains(r#"description="Invalid query conversion""#));
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
    }

    #[test]
    fn escapes_attribute_quotes() {
        let xml = render(201, r#"He said "hi""#);
        assert!(xml.contains(r#"description="He said &quot;hi&quot;""#));
    }
}
