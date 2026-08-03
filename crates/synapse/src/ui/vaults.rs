use crate::ui::Notice;
use crate::vault::{ScopeState, Secret, Vault};
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Entity, Window, div, px};
use guise::prelude::*;
use std::path::Path;

pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
pub type Select = Box<dyn Fn(i64) -> Click + 'static>;

pub struct Actions {
    pub createvault: Click,
    pub selectvault: Select,
    pub addsecret: Click,
    pub togglenewglobal: Click,
    pub toggleglobal: Select,
    pub replacesecret: Select,
    pub forgetsecret: Select,
    pub deletevault: Click,
    pub openscope: Click,
    pub trustscope: Click,
}

pub struct View {
    pub vaults: Vec<Vault>,
    pub selected: Option<i64>,
    pub secrets: Vec<Secret>,
    pub vaultname: Entity<TextInput>,
    pub secretname: Entity<TextInput>,
    pub secretenv: Entity<TextInput>,
    pub secretvalue: Entity<PasswordInput>,
    pub folderinput: Entity<FileInput>,
    pub folder: Option<std::path::PathBuf>,
    pub scope: Option<ScopeState>,
    pub addglobal: bool,
    pub pendingforget: Option<i64>,
    pub pendingvault: Option<i64>,
    pub notice: Notice,
}

pub fn render(view: View, actions: Actions, cx: &App) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let selectedname = view
        .vaults
        .iter()
        .find(|vault| Some(vault.id) == view.selected)
        .map(|vault| vault.name.clone());
    let scopepath = view
        .folder
        .as_deref()
        .map(|path| path.join(".synapse.yaml"));
    let Actions {
        createvault,
        selectvault,
        addsecret,
        togglenewglobal,
        toggleglobal,
        replacesecret,
        forgetsecret,
        deletevault,
        openscope,
        trustscope,
    } = actions;

    div()
        .id("vaultsmain")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(px(980.0))
                .mx_auto()
                .px(px(34.0))
                .py(px(28.0))
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(hero(&view, cx))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(18.0))
                        .child(
                            div()
                                .w(px(240.0))
                                .flex_none()
                                .rounded(px(14.0))
                                .border_1()
                                .border_color(border)
                                .bg(surface)
                                .p(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(Text::new("Vaults").size(Size::Sm).bold())
                                        .child(
                                            Badge::new(view.vaults.len().to_string())
                                                .color(ColorName::Gray),
                                        ),
                                )
                                .child(vaultlist(&view.vaults, view.selected, selectvault))
                                .child(div().h(px(1.0)).bg(border))
                                .child(view.vaultname)
                                .child(
                                    Button::new("createvault", "Create vault")
                                        .variant(Variant::Light)
                                        .color(ColorName::Violet)
                                        .size(Size::Sm)
                                        .left_section(Icon::new(IconName::Plus).size(Size::Xs))
                                        .on_click(move |event, window, cx| {
                                            createvault(event, window, cx)
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .rounded(px(14.0))
                                .border_1()
                                .border_color(border)
                                .bg(surface)
                                .overflow_hidden()
                                .child(secretpanel(
                                    selectedname,
                                    view.secrets,
                                    view.secretname,
                                    view.secretenv,
                                    view.secretvalue,
                                    view.addglobal,
                                    view.pendingforget,
                                    view.pendingvault,
                                    addsecret,
                                    togglenewglobal,
                                    toggleglobal,
                                    replacesecret,
                                    forgetsecret,
                                    deletevault,
                                    cx,
                                )),
                        ),
                )
                .child(scopepanel(
                    view.folderinput,
                    view.folder.as_deref(),
                    scopepath.as_deref(),
                    view.scope.as_ref(),
                    openscope,
                    trustscope,
                    cx,
                )),
        )
        .into_any_element()
}

fn hero(view: &View, cx: &App) -> impl IntoElement {
    let theme = guise::theme(cx);
    let noticecolor = match view.notice {
        Notice::Error(_) => theme.danger(),
        Notice::Success(_) => theme.success(),
        Notice::Ready => theme.dimmed(),
    };
    let message = match &view.notice {
        Notice::Ready => {
            "Values stay in Keychain. Synapse stores only labels and scoped references."
        }
        Notice::Success(message) | Notice::Error(message) => message,
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
                        .child(Title::new("Vaults and scopes").order(2))
                        .child(
                            Text::new(
                                "Keep credentials out of project files, then grant names at global, project, or folder level.",
                            )
                            .size(Size::Sm)
                            .dimmed(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(metric("Vaults", view.vaults.len()))
                        .child(metric("Secrets", view.secrets.len())),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(noticecolor.hsla())
                .child(Icon::new(IconName::ShieldCheck).size(Size::Sm))
                .child(Text::new(message.to_owned()).size(Size::Sm)),
        )
}

fn metric(label: &str, value: usize) -> impl IntoElement {
    Badge::new(format!("{value} {label}")).color(ColorName::Gray)
}

fn vaultlist(vaults: &[Vault], selected: Option<i64>, select: impl Fn(i64) -> Click) -> AnyElement {
    let mut list = div().flex().flex_col().gap(px(4.0));
    if vaults.is_empty() {
        list = list.child(
            Text::new("Create your first vault to add secret references.")
                .size(Size::Xs)
                .dimmed(),
        );
    }
    for vault in vaults {
        let click = select(vault.id);
        list = list.child(
            Button::new(
                gpui::ElementId::Name(format!("vault{}", vault.id).into()),
                vault.name.clone(),
            )
            .variant(if selected == Some(vault.id) {
                Variant::Light
            } else {
                Variant::Subtle
            })
            .color(ColorName::Violet)
            .size(Size::Sm)
            .left_section(Icon::new(IconName::LockKeyhole).size(Size::Xs))
            .on_click(move |event, window, cx| click(event, window, cx)),
        );
    }
    list.into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn secretpanel(
    selected: Option<String>,
    secrets: Vec<Secret>,
    name: Entity<TextInput>,
    env: Entity<TextInput>,
    value: Entity<PasswordInput>,
    addglobal: bool,
    pendingforget: Option<i64>,
    pendingvault: Option<i64>,
    add: Click,
    togglenewglobal: Click,
    toggleglobal: impl Fn(i64) -> Click,
    replace: impl Fn(i64) -> Click,
    forget: impl Fn(i64) -> Click,
    deletevault: Click,
    cx: &App,
) -> AnyElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let Some(selected) = selected else {
        return div()
            .min_h(px(280.0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .max_w(px(330.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(Icon::new(IconName::KeyRound).size(Size::Lg))
                    .child(Text::new("Select or create a vault").size(Size::Sm).bold())
                    .child(
                        Text::new("Secret values are written directly to macOS Keychain.")
                            .size(Size::Xs)
                            .dimmed(),
                    ),
            )
            .into_any_element();
    };

    let hassecrets = !secrets.is_empty();
    let mut rows = div().flex().flex_col();
    if !hassecrets {
        rows = rows.child(
            div().px(px(18.0)).py(px(20.0)).child(
                Text::new("No secrets in this vault yet.")
                    .size(Size::Sm)
                    .dimmed(),
            ),
        );
    }
    for secret in secrets {
        let globalclick = toggleglobal(secret.id);
        let replaceclick = replace(secret.id);
        let forgetclick = forget(secret.id);
        let confirming = pendingforget == Some(secret.id);
        rows = rows.child(
            div()
                .min_h(px(64.0))
                .px(px(18.0))
                .py(px(10.0))
                .border_t_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(Text::new(secret.name.clone()).size(Size::Sm).bold())
                                .child(Badge::new(secret.env.clone()).size(Size::Xs).color(
                                    if secret.global {
                                        ColorName::Teal
                                    } else {
                                        ColorName::Gray
                                    },
                                )),
                        )
                        .child(
                            Text::new(format!("Reference: {}.{}", secret.vault, secret.name))
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            Button::new(
                                gpui::ElementId::Name(format!("global{}", secret.id).into()),
                                if secret.global {
                                    "Global"
                                } else {
                                    "Project only"
                                },
                            )
                            .variant(Variant::Subtle)
                            .color(if secret.global {
                                ColorName::Teal
                            } else {
                                ColorName::Gray
                            })
                            .size(Size::Xs)
                            .on_click(move |event, window, cx| globalclick(event, window, cx)),
                        )
                        .child(
                            Button::new(
                                gpui::ElementId::Name(format!("replace{}", secret.id).into()),
                                "Replace",
                            )
                            .variant(Variant::Subtle)
                            .size(Size::Xs)
                            .on_click(move |event, window, cx| replaceclick(event, window, cx)),
                        )
                        .child(
                            Button::new(
                                gpui::ElementId::Name(format!("forget{}", secret.id).into()),
                                if confirming { "Confirm" } else { "Forget" },
                            )
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .size(Size::Xs)
                            .on_click(move |event, window, cx| forgetclick(event, window, cx)),
                        ),
                ),
        );
    }

    div()
        .flex()
        .flex_col()
        .child(
            div()
                .px(px(18.0))
                .py(px(15.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(Text::new(selected).size(Size::Sm).bold())
                        .child(
                            Text::new("Labels in SQLite · values in Keychain")
                                .size(Size::Xs)
                                .dimmed(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(Badge::new("Encrypted at rest").color(ColorName::Teal))
                        .child(
                            Button::new(
                                "deletevault",
                                if pendingvault.is_some() {
                                    "Confirm delete"
                                } else {
                                    "Delete vault"
                                },
                            )
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .size(Size::Xs)
                            .disabled(hassecrets)
                            .on_click(move |event, window, cx| deletevault(event, window, cx)),
                        ),
                ),
        )
        .child(rows)
        .child(
            div()
                .border_t_1()
                .border_color(border)
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(Text::new("Add a secret").size(Size::Xs).bold())
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(8.0))
                        .child(div().w(px(150.0)).child(name))
                        .child(div().w(px(170.0)).child(env))
                        .child(div().flex_1().min_w(px(150.0)).child(value)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            Button::new(
                                "newglobalsecret",
                                if addglobal {
                                    "Available globally"
                                } else {
                                    "Project scopes only"
                                },
                            )
                            .variant(Variant::Subtle)
                            .color(if addglobal {
                                ColorName::Teal
                            } else {
                                ColorName::Gray
                            })
                            .size(Size::Xs)
                            .left_section(Icon::new(IconName::Globe2).size(Size::Xs))
                            .on_click(move |event, window, cx| togglenewglobal(event, window, cx)),
                        )
                        .child(
                            Button::new("addsecret", "Save to Keychain")
                                .variant(Variant::Filled)
                                .color(ColorName::Violet)
                                .size(Size::Sm)
                                .left_section(Icon::new(IconName::Key).size(Size::Xs))
                                .on_click(move |event, window, cx| add(event, window, cx)),
                        ),
                ),
        )
        .into_any_element()
}

fn scopepanel(
    folderinput: Entity<FileInput>,
    folder: Option<&Path>,
    config: Option<&Path>,
    scope: Option<&ScopeState>,
    open: Click,
    trust: Click,
    cx: &App,
) -> impl IntoElement {
    let theme = guise::theme(cx);
    let border = theme.border().hsla();
    let surface = theme.surface().hsla();
    let exists = config.is_some_and(Path::exists);
    let (status, color) = match scope {
        Some(scope) if scope.trusted => ("Approved", ColorName::Teal),
        Some(scope) if scope.changed => ("Changed · approval required", ColorName::Orange),
        Some(_) => ("Approval required", ColorName::Orange),
        None if exists => ("Needs review", ColorName::Orange),
        None => ("No scope file", ColorName::Gray),
    };
    div()
        .rounded(px(14.0))
        .border_1()
        .border_color(border)
        .bg(surface)
        .p(px(18.0))
        .flex()
        .items_end()
        .gap(px(18.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(9.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(Icon::new(IconName::FolderKey).size(Size::Sm))
                        .child(Text::new("Project and folder scopes").size(Size::Sm).bold())
                        .child(Badge::new(status).color(color)),
                )
                .child(
                    Text::new(
                        ".synapse.yaml maps environment names to vault.secret references. Editing it invalidates approval; values never enter YAML.",
                    )
                    .size(Size::Xs)
                    .dimmed(),
                )
                .child(folderinput)
                .when_some(folder, |element, folder| {
                    element.child(
                        Text::new(folder.display().to_string())
                            .size(Size::Xs)
                            .dimmed(),
                    )
                }),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(7.0))
                .child(
                    Button::new(
                        "openscope",
                        if exists { "Edit YAML" } else { "Create YAML" },
                    )
                    .variant(Variant::Light)
                    .color(ColorName::Violet)
                    .size(Size::Sm)
                    .disabled(folder.is_none())
                    .left_section(Icon::new(IconName::FileKey).size(Size::Xs))
                    .on_click(move |event, window, cx| open(event, window, cx)),
                )
                .child(
                    Button::new("trustscope", "Approve")
                        .variant(Variant::Filled)
                        .color(ColorName::Teal)
                        .size(Size::Sm)
                        .disabled(!exists || scope.is_some_and(|scope| scope.trusted))
                        .left_section(Icon::new(IconName::ShieldCheck).size(Size::Xs))
                        .on_click(move |event, window, cx| trust(event, window, cx)),
                ),
        )
}
