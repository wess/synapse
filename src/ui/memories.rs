use crate::brain::{Memory, MemoryScope};
use crate::imports::{ImportBatch, ImportProvider, ImportSummary};
use crate::ui::Notice;
use chrono::{DateTime, Local};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Entity, Window, div, px};
use guise::markdown::MarkdownEditor;
use guise::prelude::*;

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct View {
    pub memories: Vec<Memory>,
    pub selected: Option<i64>,
    pub query: Entity<TextInput>,
    pub body: Entity<MarkdownEditor>,
    pub source: Entity<TextInput>,
    pub project: Entity<TextInput>,
    pub scope: MemoryScope,
    pub pendingdelete: Option<i64>,
    pub pendingwipe: bool,
    pub imports: Vec<ImportSummary>,
    pub batches: Vec<ImportBatch>,
    pub pendingbatch: Option<i64>,
    pub notice: Notice,
}

pub struct Actions {
    pub search: Click,
    pub select: Box<dyn Fn(i64) -> Click>,
    pub save: Click,
    pub global: Click,
    pub project: Click,
    pub delete: Click,
    pub wipe: Click,
    pub import: Box<dyn Fn(ImportProvider) -> Click>,
    pub review: Box<dyn Fn(ImportProvider) -> Click>,
    pub undo: Box<dyn Fn(i64) -> Click>,
}

struct EditorView {
    selected: Option<Memory>,
    body: Entity<MarkdownEditor>,
    source: Entity<TextInput>,
    projectpath: Entity<TextInput>,
    scope: MemoryScope,
    pendingdelete: Option<i64>,
}

struct EditorActions {
    save: Click,
    global: Click,
    project: Click,
    delete: Click,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let selected = view
        .memories
        .iter()
        .find(|memory| Some(memory.id) == view.selected)
        .cloned();
    let Actions {
        search,
        select,
        save,
        global,
        project,
        delete,
        wipe,
        import,
        review,
        undo,
    } = actions;

    div()
        .id("memoriesmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(px(1040.0))
                .mx_auto()
                .px(px(34.0))
                .py(px(28.0))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(hero(&view, cx))
                .child(importpanel(
                    &view.imports,
                    &view.batches,
                    view.pendingbatch,
                    import,
                    review,
                    undo,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(18.0))
                        .child(
                            div()
                                .w(px(320.0))
                                .flex_none()
                                .rounded(px(14.0))
                                .border_1()
                                .border_color(border)
                                .bg(surface)
                                .p(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_end()
                                        .gap(px(7.0))
                                        .child(div().flex_1().min_w(px(0.0)).child(view.query))
                                        .child(
                                            Button::new("searchmemories", "Search")
                                                .variant(Variant::Light)
                                                .color(ColorName::Violet)
                                                .size(Size::Sm)
                                                .left_section(
                                                    Icon::new(IconName::Search).size(Size::Xs),
                                                )
                                                .on_click(move |event, window, cx| {
                                                    search(event, window, cx)
                                                }),
                                        ),
                                )
                                .child(memorylist(
                                    &view.memories,
                                    view.selected,
                                    select,
                                    cx,
                                )),
                        )
                        .child(editor(
                            EditorView {
                                selected,
                                body: view.body,
                                source: view.source,
                                projectpath: view.project,
                                scope: view.scope,
                                pendingdelete: view.pendingdelete,
                            },
                            EditorActions {
                                save,
                                global,
                                project,
                                delete,
                            },
                            cx,
                        )),
                )
                .child(
                    div()
                        .rounded(px(14.0))
                        .border_1()
                        .border_color(border)
                        .bg(surface)
                        .p(px(18.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(24.0))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(Text::new("Delete all memories").size(Size::Sm).bold())
                                .child(
                                    Text::new(
                                        "Vault labels, settings, and approved scopes are not affected.",
                                    )
                                    .size(Size::Xs)
                                    .dimmed(),
                                ),
                        )
                        .child(
                            Button::new(
                                "wipememories",
                                if view.pendingwipe {
                                    "Confirm wipe"
                                } else {
                                    "Wipe memories"
                                },
                            )
                            .variant(Variant::Light)
                            .color(ColorName::Red)
                            .size(Size::Sm)
                            .disabled(view.memories.is_empty())
                            .left_section(Icon::new(IconName::Trash2).size(Size::Xs))
                            .on_click(move |event, window, cx| wipe(event, window, cx)),
                        ),
                ),
        )
        .into_any_element()
}

fn hero(view: &View, cx: &App) -> impl IntoElement {
    let theme = guise::theme(cx);
    let color = match view.notice {
        Notice::Ready => theme.dimmed(),
        Notice::Success(_) => theme.success(),
        Notice::Error(_) => theme.danger(),
    };
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_end()
                .justify_between()
                .gap(px(24.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.0))
                        .child(Title::new("Memory").order(2))
                        .child(
                            Text::new(
                                "Search and correct durable context before connected tools recall it.",
                            )
                            .size(Size::Sm)
                            .dimmed(),
                        ),
                )
                .child(
                    Badge::new(format!("{} shown", view.memories.len()))
                        .color(ColorName::Violet),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(color.hsla())
                .child(Icon::new(IconName::DatabaseCheck).size(Size::Sm))
                .child(Text::new(view.notice.message().to_owned()).size(Size::Sm)),
        )
}

fn importpanel(
    summaries: &[ImportSummary],
    batches: &[ImportBatch],
    pendingbatch: Option<i64>,
    import: impl Fn(ImportProvider) -> Click,
    review: impl Fn(ImportProvider) -> Click,
    undo: impl Fn(i64) -> Click,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let mut panel = div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .overflow_hidden()
        .child(
            div()
                .p(px(18.0))
                .flex()
                .items_start()
                .justify_between()
                .gap(px(24.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Icon::new(IconName::Import).size(Size::Sm))
                                .child(Text::new("Bring existing memory into Synapse").size(Size::Sm).bold()),
                        )
                        .child(
                            Text::new(
                                "Import project memory from both tools. Originals stay untouched; suspicious content is held for review.",
                            )
                            .size(Size::Xs)
                            .dimmed(),
                        ),
                )
                .child(Badge::new("Previewed locally").color(ColorName::Violet)),
        );
    for summary in summaries {
        let provider = summary.provider;
        let importclick = import(provider);
        let reviewclick = review(provider);
        let detail = summary.error.clone().unwrap_or_else(|| {
            format!(
                "{} ready · {} already imported · {} need review",
                summary.ready, summary.existing, summary.flagged
            )
        });
        panel = panel.child(
            div()
                .border_t_1()
                .border_color(border)
                .px(px(18.0))
                .py(px(13.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(Icon::new(IconName::Database).size(Size::Sm))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(Text::new(summary.provider.name()).size(Size::Sm).bold())
                                .child(Text::new(detail).size(Size::Xs).dimmed()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new(
                                gpui::ElementId::Name(
                                    format!("review{}", summary.provider.value()).into(),
                                ),
                                "Review source",
                            )
                            .variant(Variant::Subtle)
                            .color(ColorName::Gray)
                            .size(Size::Xs)
                            .left_section(Icon::new(IconName::FolderOpen).size(Size::Xs))
                            .on_click(move |event, window, cx| reviewclick(event, window, cx)),
                        )
                        .child(
                            Button::new(
                                gpui::ElementId::Name(
                                    format!("import{}", summary.provider.value()).into(),
                                ),
                                "Import safe",
                            )
                            .variant(Variant::Light)
                            .color(ColorName::Violet)
                            .size(Size::Xs)
                            .disabled(summary.ready == 0 || summary.error.is_some())
                            .left_section(Icon::new(IconName::Import).size(Size::Xs))
                            .on_click(move |event, window, cx| importclick(event, window, cx)),
                        ),
                ),
        );
    }
    if let Some(batch) = batches.iter().find(|batch| !batch.undone) {
        let id = batch.id;
        let undoclick = undo(id);
        panel = panel.child(
            div()
                .border_t_1()
                .border_color(border)
                .px(px(18.0))
                .py(px(11.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.0))
                .child(
                    Text::new(format!(
                        "Latest active batch #{} · {} · {} stored",
                        batch.id, batch.provider, batch.imported
                    ))
                    .size(Size::Xs)
                    .dimmed(),
                )
                .child(
                    Button::new(
                        "undoimport",
                        if pendingbatch == Some(id) {
                            "Confirm undo"
                        } else {
                            "Undo batch"
                        },
                    )
                    .variant(Variant::Subtle)
                    .color(if pendingbatch == Some(id) {
                        ColorName::Red
                    } else {
                        ColorName::Gray
                    })
                    .size(Size::Xs)
                    .left_section(Icon::new(IconName::Undo2).size(Size::Xs))
                    .on_click(move |event, window, cx| undoclick(event, window, cx)),
                ),
        );
    }
    panel.into_any_element()
}

fn memorylist(
    memories: &[Memory],
    selected: Option<i64>,
    select: impl Fn(i64) -> Click,
    cx: &App,
) -> AnyElement {
    let border = guise::theme(cx).border().hsla();
    if memories.is_empty() {
        return div()
            .min_h(px(220.0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                Text::new("No memories match this search.")
                    .size(Size::Sm)
                    .dimmed(),
            )
            .into_any_element();
    }
    let mut list = div()
        .id("memorylist")
        .max_h(px(520.0))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(5.0));
    for memory in memories {
        let click = select(memory.id);
        let source = format!(
            "{} · {}",
            if memory.scope == MemoryScope::Global {
                "global".to_owned()
            } else {
                memory
                    .project
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(|value| format!("project:{value}"))
                    .unwrap_or_else(|| "project".to_owned())
            },
            sourcepreview(&memory.source)
        );
        list = list.child(
            div()
                .border_b_1()
                .border_color(border)
                .pb(px(5.0))
                .child(
                    Button::new(
                        gpui::ElementId::Name(format!("memory{}", memory.id).into()),
                        preview(&memory.body),
                    )
                    .full_width(true)
                    .variant(if selected == Some(memory.id) {
                        Variant::Light
                    } else {
                        Variant::Subtle
                    })
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .left_section(Icon::new(IconName::Brain).size(Size::Xs))
                    .on_click(move |event, window, cx| click(event, window, cx)),
                )
                .child(Text::new(source).size(Size::Xs).dimmed()),
        );
    }
    list.into_any_element()
}

fn editor(view: EditorView, actions: EditorActions, cx: &App) -> AnyElement {
    let EditorView {
        selected,
        body,
        source,
        projectpath,
        scope,
        pendingdelete,
    } = view;
    let EditorActions {
        save,
        global,
        project,
        delete,
    } = actions;
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let Some(memory) = selected else {
        return div()
            .flex_1()
            .min_h(px(360.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(border)
            .bg(surface)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Text::new("Select a memory to inspect it.")
                    .size(Size::Sm)
                    .dimmed(),
            )
            .into_any_element();
    };
    let confirming = pendingdelete == Some(memory.id);
    let scopecard = div()
        .flex()
        .items_end()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(Text::new("Visibility").size(Size::Xs).dimmed())
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(
                            Button::new("globalscope", "Global")
                                .variant(if scope == MemoryScope::Global {
                                    Variant::Light
                                } else {
                                    Variant::Subtle
                                })
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| global(event, window, cx)),
                        )
                        .child(
                            Button::new("projectscope", "Project")
                                .variant(if scope == MemoryScope::Project {
                                    Variant::Light
                                } else {
                                    Variant::Subtle
                                })
                                .color(ColorName::Violet)
                                .size(Size::Xs)
                                .on_click(move |event, window, cx| project(event, window, cx)),
                        ),
                ),
        )
        .when(scope == MemoryScope::Project, |element| {
            element.child(div().flex_1().min_w(px(0.0)).child(projectpath))
        });
    div()
        .flex_1()
        .min_w(px(0.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Text::new(format!("Memory #{}", memory.id))
                                .size(Size::Sm)
                                .bold(),
                        )
                        .child(Badge::new(formatdate(memory.created)).size(Size::Xs)),
                )
                .child(
                    Button::new(
                        "deletememory",
                        if confirming {
                            "Confirm delete"
                        } else {
                            "Delete"
                        },
                    )
                    .variant(Variant::Subtle)
                    .color(ColorName::Red)
                    .size(Size::Xs)
                    .left_section(Icon::new(IconName::Trash2).size(Size::Xs))
                    .on_click(move |event, window, cx| delete(event, window, cx)),
                ),
        )
        .child(scopecard)
        .child(
            div()
                .min_h(px(280.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(border)
                .overflow_hidden()
                .child(body),
        )
        .child(source)
        .child(
            div().flex().justify_end().child(
                Button::new("savememory", "Save changes")
                    .variant(Variant::Filled)
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .left_section(Icon::new(IconName::Save).size(Size::Xs))
                    .on_click(move |event, window, cx| save(event, window, cx)),
            ),
        )
        .into_any_element()
}

fn preview(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = compact.chars();
    let preview = characters.by_ref().take(26).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn sourcepreview(value: &str) -> String {
    if value.is_empty() {
        return "No source".to_owned();
    }
    let mut characters = value.chars();
    let preview = characters.by_ref().take(38).collect::<String>();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn formatdate(created: i64) -> String {
    DateTime::from_timestamp(created, 0)
        .map(|date| {
            date.with_timezone(&Local)
                .format("%b %d, %Y · %I:%M %p")
                .to_string()
        })
        .unwrap_or_else(|| "Unknown date".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_compact_and_unicode_safe() {
        assert_eq!(preview("one\n  two"), "one two");
        assert!(preview(&"🧠".repeat(40)).ends_with('…'));
    }
}
