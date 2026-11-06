//! Pure text handling for notes: YAML front matter, wiki links, file names,
//! and the Notion export clean-up. No I/O here, so it is easy to test.
//!
//! The front matter parser understands only what notes need (scalars and
//! lists of strings). Any key it doesn't own is kept verbatim, so properties
//! Obsidian or another tool added survive a save.

/// The app's own front matter fields plus any lines it doesn't manage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontMatter {
    pub id: Option<String>,
    pub tags: Vec<String>,
    /// Project titles.
    pub projects: Vec<String>,
    pub created: Option<String>,
    pub updated: Option<String>,
    /// Other keys, each with its continuation lines, in file order.
    pub other: Vec<String>,
}

const OWN_KEYS: &[&str] = &["id", "tags", "projects", "created", "updated"];

/// Splits a file into front matter and body. A file without a leading
/// `---` block has empty front matter and is all body.
pub fn split(text: &str) -> (FrontMatter, String) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else {
        return (FrontMatter::default(), text.to_string());
    };
    // The block ends at the first line that is exactly `---`.
    let mut offset = 0;
    let mut end = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            end = Some((offset, offset + line.len()));
            break;
        }
        offset += line.len();
    }
    let Some((yaml_end, body_start)) = end else {
        return (FrontMatter::default(), text.to_string());
    };
    let fm = parse_yaml(&rest[..yaml_end]);
    let body = rest[body_start..].strip_prefix('\n').unwrap_or(&rest[body_start..]);
    (fm, body.to_string())
}

fn parse_yaml(yaml: &str) -> FrontMatter {
    let mut fm = FrontMatter::default();
    // Group each top-level key with its indented or `- ` continuation lines.
    let mut entries: Vec<Vec<&str>> = vec![];
    for line in yaml.lines() {
        let top = !line.starts_with([' ', '\t', '-']) && !line.trim().is_empty();
        match entries.last_mut() {
            Some(entry) if !top => entry.push(line),
            _ => entries.push(vec![line]),
        }
    }
    for entry in entries {
        let first = entry[0];
        let Some((key, value)) = first.split_once(':') else {
            fm.other.push(entry.join("\n"));
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let list = || -> Vec<String> {
            if value.is_empty() {
                entry[1..]
                    .iter()
                    .filter_map(|l| l.trim().strip_prefix('-'))
                    .map(unquote)
                    .filter(|s| !s.is_empty())
                    .collect()
            } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                split_inline_list(inner).into_iter().map(|s| unquote(&s)).filter(|s| !s.is_empty()).collect()
            } else {
                // `tags: one, two` or a single value.
                value.split(',').map(unquote).filter(|s| !s.is_empty()).collect()
            }
        };
        match key {
            "id" => fm.id = Some(unquote(value)).filter(|s| !s.is_empty()),
            "created" => fm.created = Some(unquote(value)).filter(|s| !s.is_empty()),
            "updated" => fm.updated = Some(unquote(value)).filter(|s| !s.is_empty()),
            "tags" => fm.tags = list().into_iter().map(|t| t.trim_start_matches('#').to_string()).collect(),
            "projects" => {
                fm.projects = list()
                    .into_iter()
                    .map(|p| {
                        // Accept `[[@Title]]` and `[[Title]]` as well as plain titles.
                        let p = p.trim_start_matches("[[").trim_end_matches("]]");
                        p.trim_start_matches('@').to_string()
                    })
                    .collect()
            }
            _ => fm.other.push(entry.join("\n")),
        }
    }
    fm
}

/// Splits `a, "b, c", 'd'` on commas outside quotes.
fn split_inline_list(inner: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in inner.chars() {
        match (quote, ch) {
            (None, '"' | '\'') => {
                quote = Some(ch);
                cur.push(ch);
            }
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(ch);
            }
            (None, ',') => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner.replace("\\\"", "\"");
        }
    }
    s.to_string()
}

fn quote_if_needed(s: &str) -> String {
    let plain = !s.is_empty()
        && !s.starts_with([' ', '-', '[', '{', '#', '&', '*', '!', '|', '>', '%', '@', '`', '"', '\''])
        && !s.contains([':', ',', '[', ']', '{', '}', '#'])
        && !s.ends_with(' ');
    if plain { s.to_string() } else { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }
}

/// The whole file: front matter (own keys first, then the rest) and body.
pub fn render(fm: &FrontMatter, body: &str) -> String {
    let mut out = String::from("---\n");
    if let Some(id) = &fm.id {
        out.push_str(&format!("id: {id}\n"));
    }
    let list = |out: &mut String, key: &str, items: &[String]| {
        if items.is_empty() {
            return;
        }
        out.push_str(key);
        out.push_str(":\n");
        for i in items {
            out.push_str(&format!("  - {}\n", quote_if_needed(i)));
        }
    };
    list(&mut out, "tags", &fm.tags);
    list(&mut out, "projects", &fm.projects);
    if let Some(c) = &fm.created {
        out.push_str(&format!("created: {c}\n"));
    }
    if let Some(u) = &fm.updated {
        out.push_str(&format!("updated: {u}\n"));
    }
    for o in &fm.other {
        if !OWN_KEYS.iter().any(|k| o.starts_with(&format!("{k}:"))) {
            out.push_str(o);
            out.push('\n');
        }
    }
    out.push_str("---\n");
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikiLink {
    pub project: bool,
    pub title: String,
}

/// Every `[[Title]]`, `[[Title|alias]]`, `[[Title#heading]]` and
/// `[[@Project]]` in a body. Links to files with an extension (images and
/// other attachments) are left out. Code spans and fences are skipped.
pub fn wiki_links(body: &str) -> Vec<WikiLink> {
    let mut out: Vec<WikiLink> = vec![];
    let mut in_fence = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        let mut in_code = false;
        while !rest.is_empty() {
            if rest.starts_with('`') {
                in_code = !in_code;
                rest = &rest[1..];
                continue;
            }
            if !in_code
                && rest.starts_with("[[")
                && let Some(end) = rest[2..].find("]]")
            {
                let inner = &rest[2..2 + end];
                rest = &rest[2 + end + 2..];
                let target = inner.split(['|', '#']).next().unwrap_or("").trim();
                let (project, title) = match target.strip_prefix('@') {
                    Some(t) => (true, t.trim()),
                    None => (false, target),
                };
                let is_file = !project
                    && title.rsplit_once('.').is_some_and(|(_, ext)| {
                        (1..=5).contains(&ext.len())
                            && ext.chars().all(|c| c.is_ascii_alphanumeric())
                            && ext != "md"
                    });
                let title = title.strip_suffix(".md").unwrap_or(title);
                // A link may carry a folder path; the title is the last part.
                let title = if project { title } else { title.rsplit('/').next().unwrap_or(title) };
                let link = WikiLink { project, title: title.to_string() };
                if !title.is_empty() && !is_file && !out.contains(&link) {
                    out.push(link);
                }
                continue;
            }
            let ch = rest.chars().next().unwrap();
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// A safe file name stem from a title: no path separators or characters
/// that Windows, macOS or Obsidian reject, at most 120 characters.
pub fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !c.is_control())
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(['.', ' ']);
    let short: String = trimmed.chars().take(120).collect();
    if short.is_empty() { "Untitled".into() } else { short }
}

/// The first non-empty line of a body with markdown punctuation removed,
/// up to 200 characters.
pub fn excerpt(body: &str) -> String {
    let mut in_fence = false;
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || t.is_empty() || t.starts_with('#') && t.trim_start_matches('#').trim().is_empty() {
            continue;
        }
        let t = t
            .trim_start_matches(['#', '>', '-', '*', ' '])
            .trim_start_matches("[ ] ")
            .trim_start_matches("[x] ");
        let t = t.replace("[[", "").replace("]]", "").replace("**", "");
        if t.trim().is_empty() {
            continue;
        }
        return t.chars().take(200).collect();
    }
    String::new()
}

/// A title from the body's first `# Heading`, if it has one.
pub fn heading_title(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .and_then(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
}

/// Notion appends ` <32 hex>` to every exported file and folder name.
pub fn strip_notion_id(name: &str) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !e.contains(' ') && e.len() <= 5 => (s, Some(e)),
        _ => (name, None),
    };
    let stem = match stem.rsplit_once(' ') {
        Some((head, id)) if id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()) => head,
        _ => stem,
    };
    match ext {
        Some(e) => format!("{stem}.{e}"),
        None => stem.to_string(),
    }
}

/// Rewrites Notion's markdown links to other exported pages,
/// `[Text](Some%20Page%20<id>.md)`, as `[[Some Page]]` wiki links. Links to
/// websites and to files other than `.md` are left alone.
pub fn notion_links_to_wiki(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("](") {
        let Some(open) = rest[..start].rfind('[') else {
            out.push_str(&rest[..start + 2]);
            rest = &rest[start + 2..];
            continue;
        };
        let Some(close) = rest[start + 2..].find(')') else { break };
        let target = &rest[start + 2..start + 2 + close];
        let decoded = percent_decode(target);
        let is_page = !decoded.contains("://") && decoded.ends_with(".md");
        if is_page && !rest[..open].ends_with('!') {
            let file = decoded.rsplit('/').next().unwrap_or(&decoded);
            let title = strip_notion_id(file);
            let title = title.strip_suffix(".md").unwrap_or(&title);
            out.push_str(&rest[..open]);
            out.push_str(&format!("[[{title}]]"));
        } else {
            out.push_str(&rest[..start + 2 + close + 1]);
        }
        rest = &rest[start + 2 + close + 1..];
    }
    out.push_str(rest);
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_round_trips_and_keeps_unknown_keys() {
        let text = "---\nid: abc\ntags: [cars, \"how-to\"]\nprojects:\n  - \"[[@Car restoration]]\"\naliases:\n  - Bleed\ncssclass: wide\n---\n# Bleeding brakes\n\nStart at the far wheel.\n";
        let (fm, body) = split(text);
        assert_eq!(fm.id.as_deref(), Some("abc"));
        assert_eq!(fm.tags, ["cars", "how-to"]);
        assert_eq!(fm.projects, ["Car restoration"]);
        assert_eq!(fm.other, ["aliases:\n  - Bleed", "cssclass: wide"]);
        assert_eq!(body, "# Bleeding brakes\n\nStart at the far wheel.\n");
        let again = render(&fm, &body);
        assert_eq!(split(&again), (fm, body));
        assert!(again.contains("aliases:\n  - Bleed\n"));
    }

    #[test]
    fn files_without_front_matter_are_all_body() {
        let (fm, body) = split("Just text\n---\nmore");
        assert_eq!(fm, FrontMatter::default());
        assert_eq!(body, "Just text\n---\nmore");
        let (fm, _) = split("---\ntags: one, two\n---\n");
        assert_eq!(fm.tags, ["one", "two"]);
    }

    #[test]
    fn finds_wiki_links() {
        let body = "See [[Bleeding brakes]] and [[Bleeding brakes|this]], [[@Car restoration]],\n\
                    [[Guides/Torque specs#Head]], ![[photo.jpg]], `[[not a link]]`\n```\n[[nor this]]\n```\n[[Next.md]]";
        let links = wiki_links(body);
        let titles: Vec<_> = links.iter().map(|l| (l.project, l.title.as_str())).collect();
        assert_eq!(
            titles,
            [(false, "Bleeding brakes"), (true, "Car restoration"), (false, "Torque specs"), (false, "Next")]
        );
    }

    #[test]
    fn makes_safe_file_names() {
        assert_eq!(file_stem("Ep. 6: ideas / notes?"), "Ep. 6 ideas notes");
        assert_eq!(file_stem("  ..  "), "Untitled");
        assert_eq!(file_stem(&"a".repeat(300)).len(), 120);
    }

    #[test]
    fn cleans_notion_exports() {
        assert_eq!(strip_notion_id("Parts list 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d.md"), "Parts list.md");
        assert_eq!(strip_notion_id("Car 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d"), "Car");
        assert_eq!(strip_notion_id("Notes v2.md"), "Notes v2.md");
        let body = "Read [Brakes](Car%201a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d/Bleeding%20brakes%20aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.md) \
                    or [the site](https://example.com/x.md) ![img](pic.png)";
        assert_eq!(
            notion_links_to_wiki(body),
            "Read [[Bleeding brakes]] or [the site](https://example.com/x.md) ![img](pic.png)"
        );
    }

    #[test]
    fn excerpts_and_heading_titles() {
        assert_eq!(excerpt("\n# Title\n\n- [ ] Buy **seals** for [[Car]]\n"), "Title");
        assert_eq!(excerpt("```\ncode\n```\nAfter"), "After");
        assert_eq!(heading_title("\n# Bleeding brakes\nbody"), Some("Bleeding brakes".into()));
        assert_eq!(heading_title("no heading"), None);
    }
}
