use crate::db::EpisodePreview;

pub fn render_episode_text(preview: &EpisodePreview) -> String {
    let title = preview
        .episode_title
        .as_deref()
        .unwrap_or("<untitled episode>");
    let mut lines = vec![
        format!("New podcast episode: {}", preview.feed_title),
        format!("*{title}*"),
        render_episode_meta(preview),
    ];

    if let Some(link) = preview.link.as_deref() {
        lines.push(format!("Link: {link}"));
    }
    if let Some(artwork_url) = preview.artwork_url.as_deref() {
        lines.push(format!("Artwork: {artwork_url}"));
    }
    if let Some(summary) = preview.summary.as_deref() {
        lines.push(String::new());
        lines.push(truncate_summary(summary));
    }

    lines.join("\n")
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

pub fn render_episode_meta(preview: &EpisodePreview) -> String {
    let mut parts = Vec::new();

    if let Some(episode_number) = preview.episode_number {
        parts.push(format!("Episode #{episode_number}"));
    }
    if let Some(duration_seconds) = preview.duration_seconds {
        parts.push(format_duration(duration_seconds));
    }
    if let Some(published_at) = preview.published_at.as_deref() {
        parts.push(published_at.to_string());
    }

    if parts.is_empty() {
        format!(
            "Feed ID {} · Episode ID {}",
            preview.feed_id, preview.episode_id
        )
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

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
