use std::fs;
use std::io::{self, BufRead};
use std::path::Path;

use serde_json::Value;

use crate::types::{AssistantMessage, ParseError};

#[derive(Debug)]
pub enum TranscriptFormat {
    ClaudeCodeJsonl,
    Markdown,
}

pub fn detect_format(path: &Path) -> Result<TranscriptFormat, ParseError> {
    if !path.exists() {
        return Err(ParseError::new(format!(
            "file not found: {}",
            path.display()
        )));
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("jsonl") => Ok(TranscriptFormat::ClaudeCodeJsonl),
        Some("md") | Some("markdown") => Ok(TranscriptFormat::Markdown),
        _ => Err(ParseError::new(
            "unsupported file format; supported formats: .jsonl, .md, .markdown",
        )),
    }
}

pub fn parse_transcript(path: &Path) -> Result<Vec<AssistantMessage>, ParseError> {
    if !path.exists() {
        return Err(ParseError::new(format!(
            "file not found: {}",
            path.display()
        )));
    }
    match detect_format(path)? {
        TranscriptFormat::ClaudeCodeJsonl => parse_jsonl(path),
        TranscriptFormat::Markdown => parse_markdown(path),
    }
}

fn parse_jsonl(path: &Path) -> Result<Vec<AssistantMessage>, ParseError> {
    let file = fs::File::open(path)
        .map_err(|e| ParseError::new(format!("failed to open {}: {}", path.display(), e)))?;

    let reader = io::BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("warning: skipping malformed JSON line");
                continue;
            }
        };

        // Try Claude Code format first, then Cursor format
        if let Some(text) =
            extract_claude_message(&value).or_else(|| extract_cursor_message(&value))
        {
            messages.push(AssistantMessage { content: text });
        }
    }

    Ok(messages)
}

/// Claude Code JSONL: `{"role": "assistant", "content": "..." | [...]}`
fn extract_claude_message(value: &Value) -> Option<String> {
    if value.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return None;
    }
    extract_text_content(value)
}

/// Cursor JSONL formats:
/// - Composer: `{"type": "assistant", "text": "..."}`
/// - Chat:     `{"role": "assistant", "parts": [{"type": "text", "text": "..."}]}`
fn extract_cursor_message(value: &Value) -> Option<String> {
    if value.get("type").and_then(|t| t.as_str()) == Some("assistant") {
        if let Some(text) = value.get("text").and_then(|t| t.as_str()) {
            return Some(text.to_string());
        }
    }

    if value.get("role").and_then(|r| r.as_str()) == Some("assistant") {
        if let Some(parts) = value.get("parts").and_then(|p| p.as_array()) {
            let text: Vec<&str> = parts
                .iter()
                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text.is_empty() {
                return Some(text.join("\n"));
            }
        }
    }

    None
}

/// Handles both `"content": "..."` and `"content": [{"type": "text", "text": "..."}]`.
fn extract_text_content(value: &Value) -> Option<String> {
    let content = value.get("content")?;

    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }

    if let Some(arr) = content.as_array() {
        let parts: Vec<&str> = arr
            .iter()
            .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .collect();
        if parts.is_empty() {
            return None;
        }
        return Some(parts.join("\n"));
    }

    None
}

pub fn parse_markdown(path: &Path) -> Result<Vec<AssistantMessage>, ParseError> {
    let content = fs::read_to_string(path)
        .map_err(|e| ParseError::new(format!("failed to read {}: {}", path.display(), e)))?;
    parse_markdown_content(&content)
}

const ASSISTANT_NAMES: &[&str] = &["Assistant", "Claude"];

enum MarkerMatch {
    AssistantWithInline(String),
    AssistantBlockStart,
    OtherSpeaker,
}

fn classify_line(line: &str) -> Option<MarkerMatch> {
    let trimmed = line.trim();

    if let Some(rest) = trimmed.strip_prefix("## ") {
        for name in ASSISTANT_NAMES {
            if rest.starts_with(name) {
                return Some(MarkerMatch::AssistantBlockStart);
            }
        }
        return Some(MarkerMatch::OtherSpeaker);
    }

    if trimmed.starts_with("**") {
        for name in ASSISTANT_NAMES {
            let colon_prefix = format!("**{}:**", name);
            if trimmed.starts_with(&colon_prefix) {
                let after = trimmed[colon_prefix.len()..].trim();
                if after.is_empty() {
                    return Some(MarkerMatch::AssistantBlockStart);
                }
                return Some(MarkerMatch::AssistantWithInline(after.to_string()));
            }
            let exact = format!("**{}**", name);
            if trimmed == exact {
                return Some(MarkerMatch::AssistantBlockStart);
            }
        }
        if trimmed.ends_with("**") || trimmed.contains(":**") {
            return Some(MarkerMatch::OtherSpeaker);
        }
    }

    None
}

fn parse_markdown_content(content: &str) -> Result<Vec<AssistantMessage>, ParseError> {
    let lines: Vec<&str> = content.lines().collect();
    let mut messages = Vec::new();

    struct Turn {
        line: usize,
        kind: MarkerMatch,
    }
    let mut turns: Vec<Turn> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if let Some(kind) = classify_line(l) {
            turns.push(Turn { line: i, kind });
        }
    }

    for (pos, turn) in turns.iter().enumerate() {
        let next_turn_line = turns.get(pos + 1).map(|t| t.line).unwrap_or(lines.len());

        let text = match &turn.kind {
            MarkerMatch::OtherSpeaker => continue,
            MarkerMatch::AssistantWithInline(inline) => {
                let mut parts = vec![inline.as_str()];
                for l in &lines[turn.line + 1..next_turn_line] {
                    parts.push(l);
                }
                parts.join("\n").trim().to_string()
            }
            MarkerMatch::AssistantBlockStart => lines[turn.line + 1..next_turn_line]
                .join("\n")
                .trim()
                .to_string(),
        };

        if !text.is_empty() {
            messages.push(AssistantMessage { content: text });
        }
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(suffix: &str, content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn detect_jsonl() {
        assert!(matches!(
            detect_format(write_temp(".jsonl", "").path()),
            Ok(TranscriptFormat::ClaudeCodeJsonl)
        ));
    }

    #[test]
    fn detect_md() {
        assert!(matches!(
            detect_format(write_temp(".md", "").path()),
            Ok(TranscriptFormat::Markdown)
        ));
    }

    #[test]
    fn detect_markdown_ext() {
        assert!(matches!(
            detect_format(write_temp(".markdown", "").path()),
            Ok(TranscriptFormat::Markdown)
        ));
    }

    #[test]
    fn detect_unsupported() {
        assert!(detect_format(write_temp(".txt", "").path())
            .unwrap_err()
            .message
            .contains("unsupported"));
    }

    #[test]
    fn detect_missing_file() {
        assert!(detect_format(Path::new("/nonexistent/file.jsonl"))
            .unwrap_err()
            .message
            .contains("file not found"));
    }

    #[test]
    fn jsonl_string_content() {
        let f = write_temp(".jsonl", "{\"role\":\"user\",\"content\":\"hi\"}\n{\"role\":\"assistant\",\"content\":\"world\"}\n{\"role\":\"assistant\",\"content\":\"second\"}\n");
        let msgs = parse_transcript(f.path()).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "world");
        assert_eq!(msgs[1].content, "second");
    }

    #[test]
    fn jsonl_array_content() {
        let f = write_temp(".jsonl", "{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"},{\"type\":\"tool_use\",\"id\":\"x\"}]}");
        assert_eq!(parse_transcript(f.path()).unwrap()[0].content, "hello");
    }

    #[test]
    fn jsonl_skips_malformed() {
        let f = write_temp(
            ".jsonl",
            "not json\n{\"role\":\"assistant\",\"content\":\"ok\"}\n",
        );
        assert_eq!(parse_transcript(f.path()).unwrap().len(), 1);
    }

    #[test]
    fn cursor_composer_format() {
        let f = write_temp(
            ".jsonl",
            "{\"type\":\"assistant\",\"text\":\"I created src/app.ts\"}\n",
        );
        let msgs = parse_transcript(f.path()).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "I created src/app.ts");
    }

    #[test]
    fn cursor_chat_parts_format() {
        let f = write_temp(".jsonl", "{\"role\":\"assistant\",\"parts\":[{\"type\":\"text\",\"text\":\"installed axios\"},{\"type\":\"code\",\"text\":\"npm i axios\"}]}\n");
        let msgs = parse_transcript(f.path()).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "installed axios");
    }

    #[test]
    fn file_not_found_includes_path() {
        let err = parse_transcript(Path::new("/nonexistent/transcript.jsonl")).unwrap_err();
        assert!(err.message.contains("/nonexistent/transcript.jsonl"));
    }

    #[test]
    fn unsupported_format_lists_supported() {
        let err = parse_transcript(write_temp(".txt", "content").path()).unwrap_err();
        assert!(err.message.contains(".jsonl"));
    }

    #[test]
    fn empty_jsonl() {
        assert!(parse_transcript(write_temp(".jsonl", "").path())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn empty_markdown() {
        assert!(parse_transcript(write_temp(".md", "").path())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn jsonl_preserves_order() {
        let lines: String = (0..5)
            .map(|i| format!("{{\"role\":\"assistant\",\"content\":\"{}\"}}\n", i))
            .collect();
        let msgs = parse_transcript(write_temp(".jsonl", &lines).path()).unwrap();
        for (i, msg) in msgs.iter().enumerate() {
            assert_eq!(msg.content, i.to_string());
        }
    }

    #[test]
    fn markdown_heading_markers() {
        let msgs = parse_markdown_content("## User\nHello\n\n## Assistant\nI created src/main.rs\n\n## User\nThanks\n\n## Assistant\nI also installed express\n").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "I created src/main.rs");
        assert_eq!(msgs[1].content, "I also installed express");
    }

    #[test]
    fn markdown_bold_markers() {
        let msgs = parse_markdown_content("**User**\nHello\n\n**Assistant**\nI wrote the tests\n\n**User**\nGreat\n\n**Assistant**\nAll tests pass\n").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "I wrote the tests");
        assert_eq!(msgs[1].content, "All tests pass");
    }

    #[test]
    fn markdown_bold_colon_inline() {
        let msgs = parse_markdown_content(
            "**User:** Hello\n\n**Assistant:** I created src/auth.ts\n\n**User:** Thanks\n",
        )
        .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "I created src/auth.ts");
    }

    #[test]
    fn markdown_bold_colon_block() {
        let msgs = parse_markdown_content("**Assistant:**\nLine one\nLine two\n").unwrap();
        assert_eq!(msgs[0].content, "Line one\nLine two");
    }

    #[test]
    fn markdown_claude_inline() {
        let msgs =
            parse_markdown_content("**Human:** Hi\n\n**Claude:** I fixed the bug in parser.rs\n")
                .unwrap();
        assert_eq!(msgs[0].content, "I fixed the bug in parser.rs");
    }

    #[test]
    fn markdown_claude_heading() {
        let msgs = parse_markdown_content("## Human\nHi\n\n## Claude\nI created foo.rs\n").unwrap();
        assert_eq!(msgs[0].content, "I created foo.rs");
    }

    #[test]
    fn markdown_claude_colon_multiline() {
        let msgs = parse_markdown_content("**Claude:** First line\nSecond line\n\n**Human:** ok\n")
            .unwrap();
        assert_eq!(msgs[0].content, "First line\nSecond line");
    }

    #[test]
    fn markdown_skips_empty_blocks() {
        let msgs =
            parse_markdown_content("## Assistant\n\n## Assistant\nNon-empty block\n").unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn markdown_multiline() {
        let msgs =
            parse_markdown_content("## Assistant\nLine one\nLine two\nLine three\n").unwrap();
        assert_eq!(msgs[0].content, "Line one\nLine two\nLine three");
    }

    #[test]
    fn markdown_no_assistant() {
        assert!(parse_markdown_content("## User\nHello\n")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn markdown_trims_whitespace() {
        let msgs =
            parse_markdown_content("## Assistant\n\n   trimmed content   \n\n## User\ndone\n")
                .unwrap();
        assert_eq!(msgs[0].content, "trimmed content");
    }

    #[test]
    fn markdown_via_parse_transcript() {
        let f = write_temp(
            ".md",
            "## User\nHello\n\n## Assistant\nI created src/lib.rs\n",
        );
        assert_eq!(parse_transcript(f.path()).unwrap().len(), 1);
    }

    #[test]
    fn markdown_sequential_count() {
        let msgs = parse_markdown_content(
            "## Assistant\nFirst\n\n## Assistant\nSecond\n\n## Assistant\nThird\n",
        )
        .unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "First");
        assert_eq!(msgs[1].content, "Second");
        assert_eq!(msgs[2].content, "Third");
    }

    #[test]
    fn markdown_mixed_formats() {
        let input = "## User\nHi\n\n## Assistant\nBlock content\n\n**User:** Thanks\n\n**Assistant:** Inline content\n";
        let msgs = parse_markdown_content(input).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "Block content");
        assert_eq!(msgs[1].content, "Inline content");
    }

    proptest! {
        #[test]
        fn prop_jsonl_ordering(
            contents in proptest::collection::vec("[!#-\\[\\]-~][!#-\\[\\]-~ ]{0,79}", 1..=20usize)
        ) {
            let jsonl: String = contents.iter()
                .map(|c| format!("{{\"role\":\"assistant\",\"content\":\"{}\"}}\n", c))
                .collect();
            let f = write_temp(".jsonl", &jsonl);
            let msgs = parse_transcript(f.path()).unwrap();
            prop_assert_eq!(msgs.len(), contents.len());
            for (msg, expected) in msgs.iter().zip(contents.iter()) {
                prop_assert_eq!(&msg.content, expected);
            }
        }

        #[test]
        fn prop_markdown_ordering(
            contents in proptest::collection::vec("[a-zA-Z0-9][a-zA-Z0-9 .,!?;:_-]{0,79}", 1..=20usize)
        ) {
            let md: String = contents.iter()
                .map(|c| format!("## Assistant\n{}\n\n", c))
                .collect();
            let msgs = parse_markdown_content(&md).unwrap();
            prop_assert_eq!(msgs.len(), contents.len());
            for (msg, expected) in msgs.iter().zip(contents.iter()) {
                prop_assert_eq!(msg.content.as_str(), expected.trim());
            }
        }
    }
}
