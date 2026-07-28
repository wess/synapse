use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, FocusHandle};
use guise::editor::{Editor, EditorStyle, Language};
use guise::markdown::{MarkdownEditor, MarkdownStyle};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Toml,
    Json,
    Yaml,
    Text,
}

#[derive(Clone)]
pub enum Buffer {
    Markdown(Entity<MarkdownEditor>),
    Code(Entity<Editor>),
}

pub fn format(path: &Path) -> Format {
    match path.extension().and_then(|value| value.to_str()) {
        Some("md") => Format::Markdown,
        Some("toml") => Format::Toml,
        Some("json") => Format::Json,
        Some("yaml" | "yml") => Format::Yaml,
        _ => Format::Text,
    }
}

pub fn markdown(content: &str, cx: &mut gpui::Context<MarkdownEditor>) -> MarkdownEditor {
    MarkdownEditor::new(cx)
        .value(content)
        .placeholder("Write global instructions in Markdown…")
        .font_size(15.0)
        .style(MarkdownStyle {
            bare: true,
            ..Default::default()
        })
}

pub fn memory(content: &str, cx: &mut gpui::Context<MarkdownEditor>) -> MarkdownEditor {
    MarkdownEditor::new(cx)
        .value(content)
        .placeholder("Write durable context…")
        .font_size(14.0)
        .rows(12)
        .style(MarkdownStyle {
            bare: true,
            ..Default::default()
        })
}

pub fn code(content: &str, format: Format, cx: &mut gpui::Context<Editor>) -> Editor {
    Editor::new(cx)
        .value(content)
        .language(language(format))
        .placeholder(placeholder(format))
        .font_size(13.0)
        .style(EditorStyle {
            bare: true,
            ..Default::default()
        })
}

pub fn focus(buffer: &Buffer, cx: &App) -> FocusHandle {
    match buffer {
        Buffer::Markdown(editor) => editor.read(cx).focus_handle(),
        Buffer::Code(editor) => editor.read(cx).focus_handle(),
    }
}

pub fn element(buffer: &Buffer) -> AnyElement {
    match buffer {
        Buffer::Markdown(editor) => editor.clone().into_any_element(),
        Buffer::Code(editor) => editor.clone().into_any_element(),
    }
}

pub fn label(format: Format) -> &'static str {
    match format {
        Format::Markdown => "Markdown live preview",
        Format::Toml => "TOML configuration",
        Format::Json => "JSON configuration",
        Format::Yaml => "YAML scope",
        Format::Text => "Plain text",
    }
}

fn language(format: Format) -> Language {
    match format {
        Format::Toml => Language::Toml,
        Format::Json => Language::Json,
        Format::Yaml => Language::None,
        Format::Markdown => Language::Markdown,
        Format::Text => Language::None,
    }
}

fn placeholder(format: Format) -> &'static str {
    match format {
        Format::Toml => "Add TOML configuration…",
        Format::Json => "Add JSON configuration…",
        Format::Yaml => "Add YAML scope configuration…",
        Format::Markdown => "Write Markdown…",
        Format::Text => "Start typing…",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_formats() {
        assert_eq!(format(Path::new("AGENTS.md")), Format::Markdown);
        assert_eq!(format(Path::new("config.toml")), Format::Toml);
        assert_eq!(format(Path::new("settings.json")), Format::Json);
        assert_eq!(format(Path::new(".synaps.yaml")), Format::Yaml);
        assert_eq!(format(Path::new("notes.txt")), Format::Text);
    }
}
