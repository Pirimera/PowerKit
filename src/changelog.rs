//! Embedded release notes for the Home page.
//!
//! Shows the Unreleased section when it has bullets; otherwise the newest
//! versioned section. Keeps the copy in sync with CHANGELOG.md at build time.

const RAW: &str = include_str!("../CHANGELOG.md");

/// Plain-text notes for the Home changelog card (no markdown fencing).
pub fn home_notes() -> String {
    let section = latest_section(RAW).unwrap_or("");
    let mut lines = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            lines.push(format!("• {rest}"));
        } else if !trimmed.starts_with('#') {
            lines.push(trimmed.to_string());
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    // Keep the card readable; the full file stays on GitHub.
    const MAX: usize = 12;
    if lines.len() > MAX {
        lines.truncate(MAX);
        lines.push("…".into());
    }
    lines.join("\n")
}

fn latest_section(text: &str) -> Option<&str> {
    let headings: Vec<(usize, &str)> = text
        .match_indices("\n## ")
        .map(|(index, _)| {
            let start = index + 1;
            let heading_line_end = text[start..]
                .find('\n')
                .map(|rel| start + rel)
                .unwrap_or(text.len());
            (start, text[start..heading_line_end].trim())
        })
        .collect();
    // Also accept a file that begins with ## Unreleased.
    let mut sections = Vec::new();
    if text.starts_with("## ") {
        let heading_line_end = text.find('\n').unwrap_or(text.len());
        sections.push((0, text[..heading_line_end].trim()));
    }
    sections.extend(headings);

    let mut unreleased = None;
    let mut first_versioned = None;
    for (index, (start, heading)) in sections.iter().enumerate() {
        let body_start = text[*start..]
            .find('\n')
            .map(|rel| *start + rel + 1)
            .unwrap_or(text.len());
        let body_end = sections
            .get(index + 1)
            .map(|(next, _)| *next)
            .unwrap_or(text.len());
        let section = text[body_start..body_end].trim();
        if heading.eq_ignore_ascii_case("## Unreleased") {
            if section
                .lines()
                .any(|line| line.trim_start().starts_with("- "))
            {
                unreleased = Some(section);
            }
        } else if heading.starts_with("## ") && first_versioned.is_none() {
            first_versioned = Some(section);
        }
    }
    unreleased.or(first_versioned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_notes_include_a_bullet_from_changelog() {
        let notes = home_notes();
        assert!(
            notes.contains('•'),
            "expected embedded changelog bullets, got: {notes:?}"
        );
    }

    #[test]
    fn prefers_unreleased_when_it_has_bullets() {
        let sample = "\
# Changelog\n\
\n\
## Unreleased\n\
\n\
- First change\n\
\n\
## 1.0.0\n\
\n\
- Old change\n";
        let section = latest_section(sample).unwrap();
        assert!(section.contains("First change"));
        assert!(!section.contains("Old change"));
    }
}
