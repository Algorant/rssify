use crate::db::{EpisodePreview, FeedRecord};
use chrono::{DateTime, Local};
use html2text::from_read;
use serde_json::{json, Value};

pub fn render_episode_text(preview: &EpisodePreview) -> String {
    let title = preview
        .episode_title
        .as_deref()
        .unwrap_or("<untitled episode>");
    let mut lines = vec![preview.feed_title.clone(), format!("*{title}*")];

    let meta = render_episode_meta(preview);
    if !meta.is_empty() {
        lines.push(meta);
    }

    if let Some(link) = preview.link.as_deref() {
        lines.push(format!("Link: {link}"));
    }
    if let Some(feed_artwork_url) = preview.feed_artwork_url.as_deref() {
        lines.push(format!("Podcast artwork: {feed_artwork_url}"));
    }
    if let Some(summary) = preview.summary.as_deref() {
        lines.push(String::new());
        lines.push(render_summary_preview(summary));
    }

    if let Some(artwork_url) = preview.artwork_url.as_deref() {
        lines.push(String::new());
        lines.push(format!("Episode artwork: {artwork_url}"));
    }

    lines.push(String::new());
    lines.push("via RSSify".to_string());

    lines.join("\n")
}

pub fn render_slack_payload(preview: &EpisodePreview) -> Value {
    let title = preview
        .episode_title
        .as_deref()
        .unwrap_or("<untitled episode>");
    let meta = render_episode_meta(preview);
    let mut header = format!(
        "{}\n*{}*",
        slack_escape_mrkdwn(&preview.feed_title),
        slack_escape_mrkdwn(title)
    );
    if !meta.is_empty() {
        header.push('\n');
        header.push_str(&slack_escape_mrkdwn(&meta));
    }

    let mut blocks = Vec::new();
    let top_section = json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": header,
        }
    });
    blocks.push(top_section);

    if let Some(feed_artwork_url) = preview.feed_artwork_url.as_deref() {
        blocks.push(json!({
            "type": "image",
            "image_url": feed_artwork_url,
            "alt_text": format!("{} podcast artwork", preview.feed_title),
        }));
    }

    let mut link_lines = Vec::new();
    if let Some(link) = preview.link.as_deref() {
        link_lines.push(format!("<{}|Episode link>", link));
    }
    if let Some(enclosure_url) = preview.enclosure_url.as_deref() {
        link_lines.push(format!("<{}|Audio file>", enclosure_url));
    }
    if !link_lines.is_empty() {
        blocks.push(json!({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": link_lines.join("\n"),
            }
        }));
    }

    if let Some(summary) = preview.summary.as_deref() {
        let summary = render_slack_summary(summary);
        if !summary.is_empty() {
            blocks.push(json!({
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": slack_escape_mrkdwn_with_limit(&summary, SLACK_SECTION_TEXT_LIMIT),
                }
            }));
        }
    }

    if let Some(episode_artwork_url) = preview.artwork_url.as_deref() {
        blocks.push(json!({
            "type": "image",
            "image_url": episode_artwork_url,
            "alt_text": format!("{} episode artwork", title),
        }));
    }

    blocks.push(json!({
        "type": "context",
        "elements": [
            {
                "type": "mrkdwn",
                "text": "via RSSify"
            }
        ]
    }));

    blocks.push(json!({
        "type": "divider"
    }));

    json!({
        "text": render_episode_text(preview),
        "blocks": blocks,
    })
}

pub fn render_preview_html(previews: &[EpisodePreview]) -> String {
    let cards = previews
        .iter()
        .map(render_episode_card_html)
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>RSSify Preview</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f7f3eb;
      --panel: #fffdf8;
      --ink: #1f1b17;
      --muted: #6d6458;
      --line: #ddd0bf;
      --accent: #b85c38;
      --accent-soft: #f3d8c9;
      --shadow: rgba(85, 56, 33, 0.08);
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: Georgia, "Times New Roman", serif;
      color: var(--ink);
      background:
        radial-gradient(circle at top left, #fff8ef 0, transparent 35%),
        linear-gradient(180deg, #f2ebe0 0%, var(--bg) 100%);
    }}
    .wrap {{
      max-width: 980px;
      margin: 0 auto;
      padding: 32px 20px 56px;
    }}
    .header {{
      margin-bottom: 24px;
    }}
    .eyebrow {{
      margin: 0 0 8px;
      font-size: 12px;
      letter-spacing: 0.18em;
      text-transform: uppercase;
      color: var(--muted);
    }}
    h1 {{
      margin: 0 0 8px;
      font-size: clamp(32px, 5vw, 54px);
      line-height: 0.95;
      font-weight: 700;
    }}
    .sub {{
      margin: 0;
      max-width: 640px;
      color: var(--muted);
      font-size: 17px;
      line-height: 1.5;
    }}
    .grid {{
      display: grid;
      gap: 16px;
    }}
    .card {{
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 16px;
      padding: 18px;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 18px;
      box-shadow: 0 10px 25px var(--shadow);
    }}
    .feed {{
      margin: 0 0 6px;
      color: var(--accent);
      font-size: 13px;
      font-weight: 700;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }}
    .title {{
      margin: 0 0 8px;
      font-size: 28px;
      line-height: 1.08;
    }}
    .meta {{
      margin: 0 0 14px;
      color: var(--muted);
      font-size: 14px;
      line-height: 1.5;
    }}
    .summary {{
      margin: 0;
      font-size: 16px;
      line-height: 1.6;
      white-space: pre-wrap;
    }}
    .art {{
      width: 112px;
      height: 112px;
      object-fit: cover;
      border-radius: 14px;
      border: 1px solid var(--line);
      background: var(--accent-soft);
    }}
    .top {{
      display: flex;
      gap: 16px;
      align-items: flex-start;
      justify-content: space-between;
    }}
    .links {{
      margin-top: 14px;
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
    }}
    .links a {{
      color: var(--ink);
      text-decoration: none;
      font-size: 14px;
      border-bottom: 1px solid var(--accent);
    }}
    @media (max-width: 640px) {{
      .top {{
        flex-direction: column-reverse;
      }}
      .art {{
        width: 100%;
        height: auto;
        aspect-ratio: 1 / 1;
      }}
    }}
  </style>
</head>
<body>
  <main class="wrap">
    <header class="header">
      <p class="eyebrow">RSSify Preview</p>
      <h1>Episode update mockup</h1>
      <p class="sub">Local static preview of recent episode updates pulled from DuckDB. Use this to review message shape alongside Slack delivery.</p>
    </header>
    <section class="grid">
      {cards}
    </section>
  </main>
</body>
</html>
"#
    )
}

pub fn render_feed_artwork_html(feeds: &[FeedRecord]) -> String {
    let cards = feeds
        .iter()
        .map(render_feed_artwork_card_html)
        .collect::<Vec<_>>()
        .join("\n");
    let with_artwork = feeds
        .iter()
        .filter(|feed| feed.feed_artwork_url.is_some())
        .count();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>RSSify Feed Artwork Preview</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f4f0ea;
      --panel: #fffdf9;
      --ink: #201a16;
      --muted: #766c62;
      --line: #d9cdc0;
      --accent: #9c4f2c;
      --good: #235a34;
      --bad: #8a2d23;
      --shadow: rgba(67, 45, 22, 0.08);
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      background: linear-gradient(180deg, #efe6da 0%, var(--bg) 100%);
      color: var(--ink);
      font-family: Inter, system-ui, sans-serif;
    }}
    .wrap {{ max-width: 1180px; margin: 0 auto; padding: 28px 20px 48px; }}
    h1 {{ margin: 0 0 8px; font-size: clamp(28px, 4vw, 44px); }}
    .sub {{ margin: 0; color: var(--muted); line-height: 1.5; max-width: 760px; }}
    .stats {{ margin: 18px 0 24px; color: var(--muted); font-size: 14px; }}
    .grid {{ display: grid; gap: 16px; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); }}
    .card {{ background: var(--panel); border: 1px solid var(--line); border-radius: 18px; padding: 16px; box-shadow: 0 10px 24px var(--shadow); }}
    .art {{ width: 100%; aspect-ratio: 1 / 1; object-fit: cover; border-radius: 14px; border: 1px solid var(--line); background: #eadcca; }}
    .missing {{ width: 100%; aspect-ratio: 1 / 1; display: grid; place-items: center; border-radius: 14px; border: 1px dashed var(--line); color: var(--bad); background: #fff6f4; font-size: 14px; text-align: center; padding: 16px; }}
    .title {{ margin: 14px 0 6px; font-size: 20px; line-height: 1.2; }}
    .meta {{ margin: 0 0 10px; color: var(--muted); font-size: 13px; line-height: 1.5; }}
    .status {{ display: inline-block; font-size: 12px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; margin-bottom: 12px; }}
    .status.good {{ color: var(--good); }}
    .status.bad {{ color: var(--bad); }}
    .url {{ color: var(--muted); font-size: 13px; line-height: 1.5; word-break: break-word; }}
    .url a {{ color: inherit; }}
  </style>
</head>
<body>
  <main class="wrap">
    <h1>Feed Artwork Preview</h1>
    <p class="sub">Review which podcast feeds have a stored feed thumbnail. Missing artwork usually means the feed has not been polled since feed artwork support was added, or the feed does not expose a usable feed-level image.</p>
    <p class="stats">Feeds with artwork: {with_artwork} / {total_feeds}</p>
    <section class="grid">
      {cards}
    </section>
  </main>
</body>
</html>
"#,
        total_feeds = feeds.len()
    )
}

pub fn render_episode_meta(preview: &EpisodePreview) -> String {
    let mut parts = Vec::new();

    if let Some(episode_number) = preview.episode_number {
        parts.push(format!("Episode #{episode_number}"));
    }
    if let Some(duration_seconds) = preview.duration_seconds {
        parts.push(format_duration(duration_seconds));
    }
    if let Some(published_at) = preview.published_at.as_deref() {
        parts.push(format_published_at(published_at));
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join(" · ")
    }
}

pub fn truncate_summary(summary: &str) -> String {
    const LIMIT: usize = 400;
    let trimmed = summary.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }

    let truncated: String = trimmed.chars().take(LIMIT).collect();
    format!("{truncated}...")
}

fn format_duration(duration_seconds: i64) -> String {
    let hours = duration_seconds / 3600;
    let minutes = (duration_seconds % 3600) / 60;
    let seconds = duration_seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn format_published_at(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%b %-d, %Y")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

fn render_summary_preview(summary: &str) -> String {
    let normalized = normalize_summary(&summary_to_text(summary));
    split_summary_at_break(&normalized)
}

const SLACK_SECTION_TEXT_LIMIT: usize = 3000;
const SLACK_SUMMARY_SOFT_LIMIT: usize = 2800;

fn render_slack_summary(summary: &str) -> String {
    let mut summary = render_summary_preview(summary);

    for marker in ["Sponsors", "Timestamps", "Get full access to"] {
        if let Some(index) = summary.find(marker) {
            summary.truncate(index);
            break;
        }
    }

    if summary.chars().count() > SLACK_SUMMARY_SOFT_LIMIT {
        let truncated: String = summary.chars().take(SLACK_SUMMARY_SOFT_LIMIT).collect();
        summary = format!("{}...", truncated.trim_end());
    }

    summary.trim().to_string()
}

fn summary_to_text(summary: &str) -> String {
    if !looks_like_html(summary) {
        return summary.to_string();
    }

    from_read(summary.as_bytes(), 80).unwrap_or_else(|_| summary.to_string())
}

fn looks_like_html(summary: &str) -> bool {
    let lower = summary.to_ascii_lowercase();
    ["<p", "<br", "<a ", "<ul", "<li", "<div", "<strong", "<em"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn normalize_summary(summary: &str) -> String {
    let mut normalized = Vec::new();
    let mut previous_was_blank = false;

    for line in summary.lines().map(str::trim) {
        if line.is_empty() {
            if !previous_was_blank {
                normalized.push(String::new());
            }
            previous_was_blank = true;
        } else {
            normalized.push(line.to_string());
            previous_was_blank = false;
        }
    }

    while normalized.first().is_some_and(|line| line.is_empty()) {
        normalized.remove(0);
    }
    while normalized.last().is_some_and(|line| line.is_empty()) {
        normalized.pop();
    }

    normalized.join("\n")
}

fn split_summary_at_break(summary: &str) -> String {
    let mut kept = Vec::new();

    for line in summary.lines() {
        let trimmed = line.trim();
        if is_summary_break(trimmed) {
            break;
        }
        kept.push(line);
    }

    kept.join("\n").trim().to_string()
}

fn is_summary_break(line: &str) -> bool {
    line.len() >= 3 && line.chars().all(|ch| ch == '-')
}

fn slack_escape_mrkdwn(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn slack_escape_mrkdwn_with_limit(value: &str, limit: usize) -> String {
    let escaped = slack_escape_mrkdwn(value);
    if escaped.chars().count() <= limit {
        return escaped;
    }

    const ELLIPSIS: &str = "...";
    let content_limit = limit.saturating_sub(ELLIPSIS.len());
    let mut truncated = String::new();
    let mut used = 0_usize;

    for ch in value.chars() {
        let escaped_ch = match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            _ => {
                if used + 1 > content_limit {
                    break;
                }
                truncated.push(ch);
                used += 1;
                continue;
            }
        };
        let escaped_len = escaped_ch.chars().count();
        if used + escaped_len > content_limit {
            break;
        }
        truncated.push_str(escaped_ch);
        used += escaped_len;
    }

    truncated.push_str(ELLIPSIS);
    truncated
}

fn render_episode_card_html(preview: &EpisodePreview) -> String {
    let title = html_escape(
        preview
            .episode_title
            .as_deref()
            .unwrap_or("<untitled episode>"),
    );
    let feed_title = html_escape(&preview.feed_title);
    let summary = html_escape(
        &preview
            .summary
            .as_deref()
            .map(truncate_summary)
            .unwrap_or_else(|| "No summary available.".to_string()),
    );
    let meta = html_escape(&render_episode_meta(preview));

    let artwork = preview
        .artwork_url
        .as_deref()
        .map(|url| {
            format!(
                r#"<img class="art" src="{}" alt="{} artwork">"#,
                html_escape(url),
                feed_title
            )
        })
        .unwrap_or_default();

    let mut links = Vec::new();
    if let Some(link) = preview.link.as_deref() {
        links.push(format!(
            r#"<a href="{}" target="_blank" rel="noreferrer">Episode link</a>"#,
            html_escape(link)
        ));
    }
    if let Some(enclosure_url) = preview.enclosure_url.as_deref() {
        links.push(format!(
            r#"<a href="{}" target="_blank" rel="noreferrer">Audio file</a>"#,
            html_escape(enclosure_url)
        ));
    }

    format!(
        r#"<article class="card">
  <div class="top">
    <div>
      <p class="feed">{feed_title}</p>
      <h2 class="title">{title}</h2>
      <p class="meta">{meta}</p>
    </div>
    {artwork}
  </div>
  <p class="summary">{summary}</p>
  <div class="links">{}</div>
</article>"#,
        links.join("")
    )
}

fn render_feed_artwork_card_html(feed: &FeedRecord) -> String {
    let title = html_escape(
        feed.title
            .as_deref()
            .or(feed.last_feed_title.as_deref())
            .unwrap_or("<untitled feed>"),
    );
    let url = html_escape(&feed.url);
    let last_fetch = html_escape(feed.last_successful_fetch_at.as_deref().unwrap_or("never"));
    let status_class = if feed.feed_artwork_url.is_some() {
        "good"
    } else {
        "bad"
    };
    let status_text = if feed.feed_artwork_url.is_some() {
        "Artwork captured"
    } else {
        "Artwork missing"
    };
    let artwork = feed
        .feed_artwork_url
        .as_deref()
        .map(|art| {
            format!(
                r#"<img class="art" src="{}" alt="{} podcast artwork">"#,
                html_escape(art),
                title
            )
        })
        .unwrap_or_else(|| "<div class=\"missing\">No feed artwork stored yet</div>".to_string());

    format!(
        r#"<article class="card">
  <span class="status {status_class}">{status_text}</span>
  {artwork}
  <h2 class="title">{title}</h2>
  <p class="meta">Feed ID {}</p>
  <p class="meta">Enabled: {}<br>Last successful fetch: {last_fetch}</p>
  <p class="url"><a href="{url}" target="_blank" rel="noreferrer">{url}</a></p>
</article>"#,
        feed.id, feed.enabled
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::{render_slack_payload, SLACK_SECTION_TEXT_LIMIT};
    use crate::db::EpisodePreview;

    #[test]
    fn slack_payload_shows_feed_thumbnail_and_episode_image_separately() {
        let preview = EpisodePreview {
            episode_id: 1,
            feed_id: 1,
            feed_title: "Example Podcast".to_string(),
            feed_url: "https://example.com/feed.xml".to_string(),
            feed_artwork_url: Some("https://example.com/feed-art.jpg".to_string()),
            episode_title: Some("Episode One".to_string()),
            summary: Some("Short summary\n\n---\nignored".to_string()),
            link: Some("https://example.com/episodes/1".to_string()),
            enclosure_url: None,
            artwork_url: Some("https://example.com/episode-art.jpg".to_string()),
            duration_seconds: Some(1800),
            episode_number: Some(1),
            published_at: Some("2026-04-08T23:05:00+00:00".to_string()),
            first_seen_at: "2026-04-09T12:00:00+00:00".to_string(),
        };

        let payload = render_slack_payload(&preview);
        let blocks = payload["blocks"]
            .as_array()
            .expect("blocks should be an array");

        assert_eq!(blocks[0]["type"], "section");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["image_url"], "https://example.com/feed-art.jpg");
        assert_eq!(blocks[3]["type"], "section");
        assert_eq!(blocks[4]["type"], "image");
        assert_eq!(
            blocks[4]["image_url"],
            "https://example.com/episode-art.jpg"
        );
    }

    #[test]
    fn slack_summary_stays_within_section_limit_after_escaping() {
        let preview = EpisodePreview {
            episode_id: 1,
            feed_id: 1,
            feed_title: "Example Podcast".to_string(),
            feed_url: "https://example.com/feed.xml".to_string(),
            feed_artwork_url: None,
            episode_title: Some("Ampersands".to_string()),
            summary: Some("&".repeat(2800)),
            link: None,
            enclosure_url: None,
            artwork_url: None,
            duration_seconds: None,
            episode_number: None,
            published_at: Some("2026-04-08T23:05:00+00:00".to_string()),
            first_seen_at: "2026-04-09T12:00:00+00:00".to_string(),
        };

        let payload = render_slack_payload(&preview);
        let summary = payload["blocks"][1]["text"]["text"]
            .as_str()
            .expect("summary section text should exist");

        assert!(summary.chars().count() <= SLACK_SECTION_TEXT_LIMIT);
        assert!(summary.ends_with("..."));
        assert!(!summary.ends_with("&am..."));
    }
}
