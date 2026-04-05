use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct OpmlFeed {
    pub title: Option<String>,
    pub xml_url: String,
}

pub fn parse_opml(path: &Path) -> Result<Vec<OpmlFeed>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read OPML file {}", path.display()))?;
    parse_opml_str(&content)
}

fn parse_opml_str(content: &str) -> Result<Vec<OpmlFeed>> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut feeds = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(event)) | Ok(Event::Start(event))
                if event.name().as_ref() == b"outline" =>
            {
                let mut title = None;
                let mut xml_url = None;

                for attribute in event.attributes() {
                    let attribute = attribute?;
                    match attribute.key.as_ref() {
                        b"text" => {
                            title = Some(attribute.decode_and_unescape_value(reader.decoder())?.into_owned())
                        }
                        b"title" if title.is_none() => {
                            title = Some(attribute.decode_and_unescape_value(reader.decoder())?.into_owned())
                        }
                        b"xmlUrl" => {
                            xml_url = Some(
                                attribute
                                    .decode_and_unescape_value(reader.decoder())?
                                    .into_owned(),
                            )
                        }
                        _ => {}
                    }
                }

                if let Some(xml_url) = xml_url {
                    feeds.push(OpmlFeed { title, xml_url });
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => return Err(err).context("failed to parse OPML XML"),
        }
    }

    Ok(feeds)
}

#[cfg(test)]
mod tests {
    use super::parse_opml_str;

    #[test]
    fn parses_opml_feed_outlines() {
        let content = r#"
<?xml version="1.0" encoding="utf-8"?>
<opml version="1.0">
  <body>
    <outline text="feeds">
      <outline text="Feed One" xmlUrl="https://example.com/feed-1.xml" />
      <outline title="Feed Two" xmlUrl="https://example.com/feed-2.xml" />
      <outline text="No Feed Url" />
    </outline>
  </body>
</opml>
        "#;

        let feeds = parse_opml_str(content).expect("opml should parse");

        assert_eq!(feeds.len(), 2);
        assert_eq!(feeds[0].title.as_deref(), Some("Feed One"));
        assert_eq!(feeds[0].xml_url, "https://example.com/feed-1.xml");
        assert_eq!(feeds[1].title.as_deref(), Some("Feed Two"));
        assert_eq!(feeds[1].xml_url, "https://example.com/feed-2.xml");
    }
}
