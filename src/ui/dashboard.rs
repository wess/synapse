use crate::agent::{self, Kind};
use crate::brain::{Brain, Memory, Optimization, Stats};
use crate::files;
use crate::ui::buffer::{self, Buffer, Format};
use crate::ui::{
    Document, Notice, Page, Row, SaveDocument, agentrow, document, header, memories, settings,
    summary, vaults,
};
use crate::vault::{ScopeState, Secret, Vault, VaultStore};
use gpui::prelude::*;
use gpui::{Context, Entity, IntoElement, Window, div, px};
use guise::editor::EditorEvent;
use guise::input::FileInputEvent;
use guise::markdown::MarkdownEditorEvent;
use guise::prelude::*;
use std::path::PathBuf;

pub struct Dashboard {
    rows: Vec<Row>,
    stats: Stats,
    database: PathBuf,
    notice: Notice,
    document: Option<Document>,
    page: Page,
    brain: Option<Brain>,
    memories: Vec<Memory>,
    selectedmemory: Option<i64>,
    memoryquery: Entity<TextInput>,
    memorybody: Entity<guise::markdown::MarkdownEditor>,
    memorysource: Entity<TextInput>,
    pendingmemory: Option<i64>,
    pendingwipe: bool,
    vaultstore: Option<VaultStore>,
    vaults: Vec<Vault>,
    selectedvault: Option<i64>,
    secrets: Vec<Secret>,
    vaultname: Entity<TextInput>,
    secretname: Entity<TextInput>,
    secretenv: Entity<TextInput>,
    secretvalue: Entity<PasswordInput>,
    folderinput: Entity<FileInput>,
    scopefolder: Option<PathBuf>,
    scopestate: Option<ScopeState>,
    addglobal: bool,
    pendingforget: Option<i64>,
    pendingvault: Option<i64>,
    appmenu: Option<Entity<MenuBar>>,
    optimization: Optimization,
}

struct VaultData {
    store: VaultStore,
    vaults: Vec<Vault>,
    selected: Option<i64>,
    secrets: Vec<Secret>,
}

struct MemoryData {
    brain: Brain,
    memories: Vec<Memory>,
}

impl Dashboard {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let database = files::database().unwrap_or_else(|_| PathBuf::from("brain.db"));
        let stats = loadstats(&database).unwrap_or_default();
        let optimization = loadoptimization(&database).unwrap_or_default();
        let (brain, memories, memoryerror) = match loadmemories(&database) {
            Ok(data) => (Some(data.brain), data.memories, None),
            Err(error) => (None, Vec::new(), Some(error)),
        };
        let selectedmemory = memories.first().map(|memory| memory.id);
        let selected = selectedmemory.and_then(|id| memories.iter().find(|item| item.id == id));
        let memoryquery = cx.new(|cx| {
            TextInput::new(cx)
                .label("Search memories")
                .placeholder("Project or decision…")
        });
        let memorybody = cx.new(|cx| {
            buffer::memory(
                selected.map(|memory| memory.body.as_str()).unwrap_or(""),
                cx,
            )
        });
        let memorysource = cx.new(|cx| {
            TextInput::new(cx)
                .label("Source")
                .placeholder("project path or topic")
                .value(selected.map(|memory| memory.source.as_str()).unwrap_or(""))
        });
        let document = std::env::var_os("SYNAPS_DOCUMENT")
            .map(PathBuf::from)
            .and_then(|path| Self::loaddocument("Synaps".to_owned(), path, cx).ok());
        let appmenu = crate::ui::menu::bar(cx);
        let vaultname = cx.new(|cx| TextInput::new(cx).label("New vault").placeholder("work"));
        let secretname = cx.new(|cx| TextInput::new(cx).label("Name").placeholder("database"));
        let secretenv = cx.new(|cx| {
            TextInput::new(cx)
                .label("Environment")
                .placeholder("DATABASE_URL")
        });
        let secretvalue = cx.new(|cx| {
            PasswordInput::new(cx)
                .label("Secret value")
                .placeholder("Stored in Keychain")
        });
        let folderinput = cx.new(|cx| {
            FileInput::new(cx)
                .directories()
                .label("Scope folder")
                .placeholder("Choose a project folder")
        });
        cx.subscribe(&folderinput, |this, _input, event: &FileInputEvent, cx| {
            this.scopefolder = event.0.first().cloned();
            this.refreshscope(cx);
        })
        .detach();
        let (vaultstore, vaults, selectedvault, secrets, mut notice) = match loadvaults(&database) {
            Ok(data) => (
                Some(data.store),
                data.vaults,
                data.selected,
                data.secrets,
                Notice::Ready,
            ),
            Err(error) => (
                None,
                Vec::new(),
                None,
                Vec::new(),
                Notice::Error(format!("Could not open vaults: {error}")),
            ),
        };
        if let Some(error) = memoryerror {
            notice = Notice::Error(format!("Could not open memories: {error}"));
        }
        Self {
            rows: loadrows(),
            stats,
            database,
            notice,
            document,
            page: initialpage(),
            brain,
            memories,
            selectedmemory,
            memoryquery,
            memorybody,
            memorysource,
            pendingmemory: None,
            pendingwipe: false,
            vaultstore,
            vaults,
            selectedvault,
            secrets,
            vaultname,
            secretname,
            secretenv,
            secretvalue,
            folderinput,
            scopefolder: None,
            scopestate: None,
            addglobal: false,
            pendingforget: None,
            pendingvault: None,
            appmenu,
            optimization,
        }
    }

    fn showconnections(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Connections;
        cx.notify();
    }

    fn showmemories(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Memories;
        self.refreshmemories(cx);
    }

    fn refreshmemories(&mut self, cx: &mut Context<Self>) {
        let Some(brain) = self.brain.clone() else {
            return;
        };
        let query = self.memoryquery.read(cx).text();
        match block(brain.search(&query, 100)) {
            Ok(memories) => {
                self.selectedmemory = self
                    .selectedmemory
                    .filter(|id| memories.iter().any(|memory| memory.id == *id))
                    .or_else(|| memories.first().map(|memory| memory.id));
                self.memories = memories;
                self.pendingmemory = None;
                self.pendingwipe = false;
                self.syncmemoryeditor(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not search memories: {error}"));
            }
        }
        cx.notify();
    }

    fn selectmemory(&mut self, id: i64, cx: &mut Context<Self>) {
        self.selectedmemory = Some(id);
        self.pendingmemory = None;
        self.pendingwipe = false;
        self.syncmemoryeditor(cx);
        cx.notify();
    }

    fn syncmemoryeditor(&mut self, cx: &mut Context<Self>) {
        let memory = self
            .selectedmemory
            .and_then(|id| self.memories.iter().find(|memory| memory.id == id));
        let body = memory.map(|memory| memory.body.as_str()).unwrap_or("");
        let source = memory.map(|memory| memory.source.as_str()).unwrap_or("");
        self.memorybody
            .update(cx, |input, cx| input.set_text(body, cx));
        self.memorysource
            .update(cx, |input, cx| input.set_text(source, cx));
    }

    fn savememory(&mut self, cx: &mut Context<Self>) {
        let (Some(brain), Some(id)) = (self.brain.clone(), self.selectedmemory) else {
            return;
        };
        let body = self.memorybody.read(cx).text();
        let source = self.memorysource.read(cx).text();
        match block(brain.updatememory(id, &body, Some(&source))) {
            Ok(Some(memory)) => {
                self.notice = Notice::Success(format!("Saved memory #{}.", memory.id));
                self.pendingmemory = None;
                self.refreshmemories(cx);
                self.refreshmemorystats();
            }
            Ok(None) => {
                self.notice = Notice::Error("That memory no longer exists.".to_owned());
                self.refreshmemories(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not save memory: {error}"));
                cx.notify();
            }
        }
    }

    fn deletememory(&mut self, cx: &mut Context<Self>) {
        let (Some(brain), Some(id)) = (self.brain.clone(), self.selectedmemory) else {
            return;
        };
        if self.pendingmemory != Some(id) {
            self.pendingmemory = Some(id);
            self.notice = Notice::Error("Choose Confirm delete to remove this memory.".to_owned());
            cx.notify();
            return;
        }
        match block(brain.deletememory(id)) {
            Ok(Some(_)) => {
                self.selectedmemory = None;
                self.pendingmemory = None;
                self.notice = Notice::Success(format!("Deleted memory #{id}."));
                self.refreshmemories(cx);
                self.refreshmemorystats();
            }
            Ok(None) => {
                self.notice = Notice::Error("That memory no longer exists.".to_owned());
                self.refreshmemories(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not delete memory: {error}"));
                cx.notify();
            }
        }
    }

    fn wipememories(&mut self, cx: &mut Context<Self>) {
        let Some(brain) = self.brain.clone() else {
            return;
        };
        if !self.pendingwipe {
            self.pendingwipe = true;
            self.notice = Notice::Error("Choose Confirm wipe to delete every memory.".to_owned());
            cx.notify();
            return;
        }
        match block(brain.wipememories()) {
            Ok(count) => {
                self.selectedmemory = None;
                self.pendingwipe = false;
                self.notice = Notice::Success(format!("Deleted {count} memories."));
                self.refreshmemories(cx);
                self.refreshmemorystats();
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not wipe memories: {error}"));
                cx.notify();
            }
        }
    }

    fn refreshmemorystats(&mut self) {
        if let Some(brain) = self.brain.clone()
            && let Ok(stats) = block(brain.stats())
        {
            self.stats = stats;
        }
    }

    fn showvaults(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Vaults;
        self.refreshvaults(cx);
    }

    fn showsettings(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Settings;
        cx.notify();
    }

    fn setoptimization(&mut self, optimization: Optimization, cx: &mut Context<Self>) {
        let database = self.database.clone();
        let result = block(async {
            let brain = crate::brain::Brain::open(database).await?;
            brain.setoptimization(optimization).await
        });
        match result {
            Ok(()) => {
                self.optimization = optimization;
                self.notice = Notice::Success(format!(
                    "Recall optimization set to {}.",
                    optimization.name()
                ));
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not update optimization: {error}"));
            }
        }
        cx.notify();
    }

    fn refreshvaults(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.vaultstore.clone() else {
            return;
        };
        let selected = self.selectedvault;
        match block(async move {
            let vaults = store.vaults().await?;
            let selected = selected
                .filter(|id| vaults.iter().any(|vault| vault.id == *id))
                .or_else(|| vaults.first().map(|vault| vault.id));
            let secrets = match selected {
                Some(id) => store.secrets(id).await?,
                None => Vec::new(),
            };
            Ok::<_, anyhow::Error>((vaults, selected, secrets))
        }) {
            Ok((vaults, selected, secrets)) => {
                self.vaults = vaults;
                self.selectedvault = selected;
                self.secrets = secrets;
            }
            Err(error) => self.notice = Notice::Error(format!("Could not refresh vaults: {error}")),
        }
        cx.notify();
    }

    fn createvault(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.vaultstore.clone() else {
            return;
        };
        let name = self.vaultname.read(cx).text();
        match block(store.createvault(&name)) {
            Ok(vault) => {
                self.selectedvault = Some(vault.id);
                self.vaultname
                    .update(cx, |input, cx| input.set_text("", cx));
                self.notice = Notice::Success(format!("Created the {} vault.", vault.name));
                self.refreshvaults(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not create vault: {error}"));
                cx.notify();
            }
        }
    }

    fn selectvault(&mut self, id: i64, cx: &mut Context<Self>) {
        self.selectedvault = Some(id);
        self.pendingforget = None;
        self.pendingvault = None;
        self.refreshvaults(cx);
    }

    fn deletevault(&mut self, cx: &mut Context<Self>) {
        let (Some(store), Some(id)) = (self.vaultstore.clone(), self.selectedvault) else {
            return;
        };
        if self.pendingvault != Some(id) {
            self.pendingvault = Some(id);
            self.notice =
                Notice::Error("Choose Confirm delete to remove the empty vault.".to_owned());
            cx.notify();
            return;
        }
        match block(store.deletevault(id)) {
            Ok(Some(vault)) => {
                self.pendingvault = None;
                self.selectedvault = None;
                self.notice = Notice::Success(format!("Deleted the {} vault.", vault.name));
                self.refreshvaults(cx);
            }
            Ok(None) => {
                self.pendingvault = None;
                self.notice = Notice::Error("That vault no longer exists.".to_owned());
                self.refreshvaults(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not delete vault: {error}"));
                cx.notify();
            }
        }
    }

    fn togglenewglobal(&mut self, cx: &mut Context<Self>) {
        self.addglobal = !self.addglobal;
        cx.notify();
    }

    fn addsecret(&mut self, cx: &mut Context<Self>) {
        let (Some(store), Some(vaultid)) = (self.vaultstore.clone(), self.selectedvault) else {
            return;
        };
        let name = self.secretname.read(cx).text();
        let env = self.secretenv.read(cx).text();
        let value = self.secretvalue.read(cx).text();
        if value.is_empty() {
            self.notice = Notice::Error("Secret value cannot be empty.".to_owned());
            cx.notify();
            return;
        }
        let global = self.addglobal;
        match block(async move {
            let secret = store.createsecret(vaultid, &name, &env, global).await?;
            if let Err(error) = crate::vault::setsecret(&secret.account, &value) {
                let _ = store.deletesecret(secret.id).await;
                return Err(error);
            }
            Ok::<_, anyhow::Error>(secret)
        }) {
            Ok(secret) => {
                self.secretname
                    .update(cx, |input, cx| input.set_text("", cx));
                self.secretenv
                    .update(cx, |input, cx| input.set_text("", cx));
                self.secretvalue
                    .update(cx, |input, cx| input.set_text("", cx));
                self.notice = Notice::Success(format!(
                    "Saved {}.{} in Keychain.",
                    secret.vault, secret.name
                ));
                self.refreshvaults(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not save secret: {error}"));
                cx.notify();
            }
        }
    }

    fn toggleglobal(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(store) = self.vaultstore.clone() else {
            return;
        };
        let global = self
            .secrets
            .iter()
            .find(|secret| secret.id == id)
            .is_some_and(|secret| !secret.global);
        match block(store.setglobal(id, global)) {
            Ok(()) => {
                self.notice = Notice::Success(if global {
                    "Secret is now available in the global scope.".to_owned()
                } else {
                    "Secret now requires an approved YAML scope.".to_owned()
                });
                self.refreshvaults(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not change scope: {error}"));
                cx.notify();
            }
        }
    }

    fn replacesecret(&mut self, id: i64, cx: &mut Context<Self>) {
        let value = self.secretvalue.read(cx).text();
        let Some(secret) = self.secrets.iter().find(|secret| secret.id == id) else {
            return;
        };
        if value.is_empty() {
            self.notice = Notice::Error(
                "Enter the replacement in Secret value, then choose Replace.".to_owned(),
            );
        } else {
            self.notice = match crate::vault::setsecret(&secret.account, &value) {
                Ok(()) => {
                    self.secretvalue
                        .update(cx, |input, cx| input.set_text("", cx));
                    Notice::Success(format!(
                        "Replaced {}.{} in Keychain.",
                        secret.vault, secret.name
                    ))
                }
                Err(error) => Notice::Error(format!("Could not replace secret: {error}")),
            };
        }
        cx.notify();
    }

    fn forgetsecret(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.pendingforget != Some(id) {
            self.pendingforget = Some(id);
            self.notice = Notice::Error("Choose Confirm to remove the Keychain value.".to_owned());
            cx.notify();
            return;
        }
        let (Some(store), Some(secret)) = (
            self.vaultstore.clone(),
            self.secrets.iter().find(|secret| secret.id == id).cloned(),
        ) else {
            return;
        };
        let result = crate::vault::deletesecret(&secret.account)
            .and_then(|_| block(store.deletesecret(id)).map(|_| ()));
        match result {
            Ok(()) => {
                self.pendingforget = None;
                self.notice = Notice::Success(format!("Forgot {}.{}.", secret.vault, secret.name));
                self.refreshvaults(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not forget secret: {error}"));
                cx.notify();
            }
        }
    }

    fn refreshscope(&mut self, cx: &mut Context<Self>) {
        self.scopestate = None;
        let (Some(store), Some(folder)) = (self.vaultstore.clone(), self.scopefolder.clone())
        else {
            cx.notify();
            return;
        };
        let path = folder.join(crate::vault::CONFIG);
        if path.exists() {
            let target = path.canonicalize().unwrap_or(path);
            match block(crate::vault::resolve(&store, &folder)) {
                Ok(resolved) => {
                    self.scopestate = resolved
                        .scopes
                        .into_iter()
                        .find(|scope| scope.path == target);
                }
                Err(error) => {
                    self.notice = Notice::Error(format!("Could not inspect scope: {error}"));
                }
            }
        }
        cx.notify();
    }

    fn openscope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = self.scopefolder.clone() else {
            return;
        };
        let path = folder.join(crate::vault::CONFIG);
        let result = if path.exists() {
            Ok(())
        } else {
            files::write(&path, crate::vault::template())
        };
        match result.and_then(|_| Self::loaddocument("Vault scope".to_owned(), path, cx)) {
            Ok(document) => {
                let focus = buffer::focus(&document.editor, cx);
                self.document = Some(document);
                window.focus(&focus);
                cx.notify();
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not open YAML scope: {error}"));
                cx.notify();
            }
        }
    }

    fn trustscope(&mut self, cx: &mut Context<Self>) {
        let (Some(store), Some(folder)) = (self.vaultstore.clone(), self.scopefolder.clone())
        else {
            return;
        };
        let path = folder.join(crate::vault::CONFIG);
        let result = crate::vault::readscope(&path)
            .and_then(|(_, digest)| block(store.trust(&path, &digest)));
        match result {
            Ok(()) => {
                self.notice = Notice::Success(format!("Approved {}.", path.display()));
                self.refreshscope(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not approve scope: {error}"));
                cx.notify();
            }
        }
    }

    fn setup(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.kind == kind).cloned() else {
            return;
        };
        let result = connectionserver()
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synaps MCP executable"))
            .and_then(|server| agent::setup(&row.agent, &row.detection, &server));
        self.notice = match result {
            Ok(()) => Notice::Success(format!("{} is connected to Synaps.", row.agent.name)),
            Err(error) => Notice::Error(format!("Could not connect {}: {error}", row.agent.name)),
        };
        self.rows = loadrows();
        cx.notify();
    }

    fn openinstructions(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.opendocument(kind, instructionspath, "instructions", window, cx);
    }

    fn opensettings(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.opendocument(kind, settingspath, "configuration", window, cx);
    }

    fn opendocument(
        &mut self,
        kind: Kind,
        select: fn(&agent::Agent) -> PathBuf,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.rows.iter().find(|row| row.agent.kind == kind) else {
            self.notice = Notice::Error("That connection is no longer available.".to_owned());
            cx.notify();
            return;
        };
        let path = select(&row.agent);
        let tool = row.agent.name.to_owned();
        match Self::loaddocument(tool, path, cx) {
            Ok(document) => {
                let focus = buffer::focus(&document.editor, cx);
                self.document = Some(document);
                window.focus(&focus);
                cx.notify();
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not open {label}: {error}"));
                cx.notify();
            }
        }
    }

    fn loaddocument(
        tool: String,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Document> {
        let content = files::read(&path)?;
        let format = buffer::format(&path);
        let editor = match format {
            Format::Markdown => {
                let editor = cx.new(|cx| buffer::markdown(&content, cx));
                let watched = path.clone();
                cx.subscribe(&editor, move |this, _editor, event, cx| {
                    if let MarkdownEditorEvent::Change(content) = event {
                        changedocument(this, &watched, content, cx);
                    }
                })
                .detach();
                Buffer::Markdown(editor)
            }
            _ => {
                let editor = cx.new(|cx| buffer::code(&content, format, cx));
                let watched = path.clone();
                cx.subscribe(&editor, move |this, _editor, event, cx| {
                    if let EditorEvent::Change(content) = event {
                        changedocument(this, &watched, content, cx);
                    }
                })
                .detach();
                Buffer::Code(editor)
            }
        };
        Ok(Document {
            tool,
            path,
            editor,
            format,
            saved: content.clone(),
            current: content,
            error: None,
        })
    }

    fn savedocument(&mut self, cx: &mut Context<Self>) {
        let refreshscope = {
            let Some(document) = self.document.as_mut() else {
                return;
            };
            let refreshscope = document.path.file_name().and_then(|name| name.to_str())
                == Some(crate::vault::CONFIG);
            match files::write(&document.path, &document.current) {
                Ok(()) => {
                    document.saved = document.current.clone();
                    document.error = None;
                }
                Err(error) => {
                    document.error = Some(format!("Save failed: {error}"));
                }
            }
            refreshscope
        };
        if refreshscope {
            self.refreshscope(cx);
        }
        cx.notify();
    }

    fn closedocument(&mut self, cx: &mut Context<Self>) {
        if self.document.as_ref().is_some_and(document::dirty) {
            return;
        }
        self.document = None;
        cx.notify();
    }

    fn discarddocument(&mut self, cx: &mut Context<Self>) {
        self.document = None;
        cx.notify();
    }

    pub fn preparequit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(document) = self
            .document
            .as_mut()
            .filter(|document| document::dirty(document))
        else {
            return true;
        };
        document.error = Some("Save or discard these changes before quitting Synaps.".to_owned());
        window.activate_window();
        cx.notify();
        false
    }

    fn opendata(&mut self, cx: &mut Context<Self>) {
        let result = self
            .database
            .parent()
            .map(files::reveal)
            .unwrap_or_else(|| Err(anyhow::anyhow!("the data directory is unavailable")));
        self.notice = match result {
            Ok(()) => Notice::Success("Opened the local Synaps data folder.".to_owned()),
            Err(error) => Notice::Error(format!("Could not open the data folder: {error}")),
        };
        cx.notify();
    }

    fn connected(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.detection.configured)
            .count()
    }
}

impl Render for Dashboard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = guise::theme(cx);
        let border = theme.border().hsla();
        let surface = theme.surface().hsla();
        let rows = self.rows.clone();

        if let Some(document) = self.document.clone() {
            let save = Box::new(cx.listener(|this, _, _, cx| this.savedocument(cx)));
            let close = Box::new(cx.listener(|this, _, _, cx| this.closedocument(cx)));
            let discard = Box::new(cx.listener(|this, _, _, cx| this.discarddocument(cx)));
            return div()
                .size_full()
                .flex()
                .flex_col()
                .bg(theme.body().hsla())
                .text_color(theme.text().hsla())
                .on_action(cx.listener(|this, _: &SaveDocument, _window, cx| this.savedocument(cx)))
                .child(header::render(
                    self.page,
                    self.appmenu.clone(),
                    Box::new(cx.listener(|this, _, _, cx| this.showconnections(cx))),
                    Box::new(cx.listener(|this, _, _, cx| this.showmemories(cx))),
                    Box::new(cx.listener(|this, _, _, cx| this.showvaults(cx))),
                    Box::new(cx.listener(|this, _, _, cx| this.showsettings(cx))),
                    cx,
                ))
                .child(document::render(document, save, close, discard, cx))
                .into_any_element();
        }

        let shell = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.body().hsla())
            .text_color(theme.text().hsla())
            .child(header::render(
                self.page,
                self.appmenu.clone(),
                Box::new(cx.listener(|this, _, _, cx| this.showconnections(cx))),
                Box::new(cx.listener(|this, _, _, cx| this.showmemories(cx))),
                Box::new(cx.listener(|this, _, _, cx| this.showvaults(cx))),
                Box::new(cx.listener(|this, _, _, cx| this.showsettings(cx))),
                cx,
            ));

        if self.page == Page::Memories {
            let selecthost = cx.entity().downgrade();
            let selectmemory = move |id| -> memories::Click {
                let host = selecthost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.selectmemory(id, cx)).ok();
                })
            };
            return shell
                .child(memories::render(
                    memories::View {
                        memories: self.memories.clone(),
                        selected: self.selectedmemory,
                        query: self.memoryquery.clone(),
                        body: self.memorybody.clone(),
                        source: self.memorysource.clone(),
                        pendingdelete: self.pendingmemory,
                        pendingwipe: self.pendingwipe,
                        notice: self.notice.clone(),
                    },
                    memories::Actions {
                        search: Box::new(cx.listener(|this, _, _, cx| this.refreshmemories(cx))),
                        select: Box::new(selectmemory),
                        save: Box::new(cx.listener(|this, _, _, cx| this.savememory(cx))),
                        delete: Box::new(cx.listener(|this, _, _, cx| this.deletememory(cx))),
                        wipe: Box::new(cx.listener(|this, _, _, cx| this.wipememories(cx))),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(Text::new("SQLite FTS5 · editable").size(Size::Xs))
                        .right(Text::new("Original changes save immediately").size(Size::Xs)),
                )
                .into_any_element();
        }

        if self.page == Page::Settings {
            let clistatus = crate::cli::status().unwrap_or(crate::cli::InstallStatus::Missing);
            let clipath = crate::cli::destination()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| error.to_string());
            let (shellintegration, shellerror) = match crate::cli::destination()
                .and_then(|command| crate::shellsetup::status(&command))
            {
                Ok(integration) => (Some(integration), None),
                Err(error) => (None, Some(error.to_string())),
            };
            return shell
                .child(settings::render(
                    settings::View {
                        optimization: self.optimization,
                        thememode: crate::ui::theme::mode(cx),
                        clistatus,
                        clipath,
                        shellintegration,
                        shellerror,
                        message: crate::ui::menu::message(cx),
                    },
                    settings::Actions {
                        full: Box::new(cx.listener(|this, _, _, cx| {
                            this.setoptimization(Optimization::Full, cx)
                        })),
                        balanced: Box::new(cx.listener(|this, _, _, cx| {
                            this.setoptimization(Optimization::Balanced, cx)
                        })),
                        lean: Box::new(cx.listener(|this, _, _, cx| {
                            this.setoptimization(Optimization::Lean, cx)
                        })),
                        system: Box::new(cx.listener(|_, _, _, cx| {
                            crate::ui::theme::set(crate::ui::theme::Mode::System, cx)
                        })),
                        light: Box::new(cx.listener(|_, _, _, cx| {
                            crate::ui::theme::set(crate::ui::theme::Mode::Light, cx)
                        })),
                        dark: Box::new(cx.listener(|_, _, _, cx| {
                            crate::ui::theme::set(crate::ui::theme::Mode::Dark, cx)
                        })),
                        install: Box::new(
                            cx.listener(|_, _, _, cx| crate::ui::menu::installcli(cx)),
                        ),
                        shellinstall: Box::new(
                            cx.listener(|_, _, _, cx| crate::ui::menu::installshell(cx)),
                        ),
                        shellremove: Box::new(
                            cx.listener(|_, _, _, cx| crate::ui::menu::removeshell(cx)),
                        ),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(Text::new("Settings · stored locally").size(Size::Xs))
                        .right(
                            Text::new("Two shell modes · values stay in Keychain").size(Size::Xs),
                        ),
                )
                .into_any_element();
        }

        if self.page == Page::Vaults {
            let selecthost = cx.entity().downgrade();
            let selectvault = move |id| -> vaults::Click {
                let host = selecthost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.selectvault(id, cx)).ok();
                })
            };
            let globalhost = cx.entity().downgrade();
            let toggleglobal = move |id| -> vaults::Click {
                let host = globalhost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.toggleglobal(id, cx)).ok();
                })
            };
            let replacehost = cx.entity().downgrade();
            let replacesecret = move |id| -> vaults::Click {
                let host = replacehost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.replacesecret(id, cx)).ok();
                })
            };
            let forgethost = cx.entity().downgrade();
            let forgetsecret = move |id| -> vaults::Click {
                let host = forgethost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.forgetsecret(id, cx)).ok();
                })
            };
            return shell
                .child(vaults::render(
                    vaults::View {
                        vaults: self.vaults.clone(),
                        selected: self.selectedvault,
                        secrets: self.secrets.clone(),
                        vaultname: self.vaultname.clone(),
                        secretname: self.secretname.clone(),
                        secretenv: self.secretenv.clone(),
                        secretvalue: self.secretvalue.clone(),
                        folderinput: self.folderinput.clone(),
                        folder: self.scopefolder.clone(),
                        scope: self.scopestate.clone(),
                        addglobal: self.addglobal,
                        pendingforget: self.pendingforget,
                        pendingvault: self.pendingvault,
                        notice: self.notice.clone(),
                    },
                    vaults::Actions {
                        createvault: Box::new(cx.listener(|this, _, _, cx| this.createvault(cx))),
                        selectvault: Box::new(selectvault),
                        addsecret: Box::new(cx.listener(|this, _, _, cx| this.addsecret(cx))),
                        togglenewglobal: Box::new(
                            cx.listener(|this, _, _, cx| this.togglenewglobal(cx)),
                        ),
                        toggleglobal: Box::new(toggleglobal),
                        replacesecret: Box::new(replacesecret),
                        forgetsecret: Box::new(forgetsecret),
                        deletevault: Box::new(cx.listener(|this, _, _, cx| this.deletevault(cx))),
                        openscope: Box::new(
                            cx.listener(|this, _, window, cx| this.openscope(window, cx)),
                        ),
                        trustscope: Box::new(cx.listener(|this, _, _, cx| this.trustscope(cx))),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(Text::new("Keychain · values protected").size(Size::Xs))
                        .right(Text::new("synaps run -- <command>").size(Size::Xs)),
                )
                .into_any_element();
        }

        shell
            .child(
                div()
                    .id("synapsmain")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(980.0))
                            .mx_auto()
                            .px(px(34.0))
                            .py(px(30.0))
                            .flex()
                            .flex_col()
                            .gap(px(24.0))
                            .child(summary::render(
                                &self.stats,
                                self.connected(),
                                self.rows.len(),
                                &self.notice,
                                cx,
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .rounded(px(14.0))
                                    .border_1()
                                    .border_color(border)
                                    .bg(surface)
                                    .overflow_hidden()
                                    .children(rows.into_iter().enumerate().flat_map(
                                        |(index, row)| {
                                            let kind = row.agent.kind;
                                            let setup =
                                                Box::new(cx.listener(move |this, _, _, cx| {
                                                    this.setup(kind, cx);
                                                }));
                                            let kind = row.agent.kind;
                                            let instructions = Box::new(cx.listener(
                                                move |this, _, window, cx| {
                                                    this.openinstructions(kind, window, cx);
                                                },
                                            ));
                                            let kind = row.agent.kind;
                                            let settings = Box::new(cx.listener(
                                                move |this, _, window, cx| {
                                                    this.opensettings(kind, window, cx);
                                                },
                                            ));
                                            let mut items = Vec::new();
                                            if index > 0 {
                                                items.push(
                                                    div()
                                                        .h(px(1.0))
                                                        .mx(px(22.0))
                                                        .bg(border)
                                                        .into_any_element(),
                                                );
                                            }
                                            items.push(agentrow::render(
                                                index,
                                                row,
                                                setup,
                                                instructions,
                                                settings,
                                            ));
                                            items
                                        },
                                    )),
                            ),
                    ),
            )
            .child(
                StatusBar::new()
                    .height(36.0)
                    .left(Text::new("SQLite · on device").size(Size::Xs))
                    .right(
                        Button::new("opendata", "Open data folder")
                            .variant(Variant::Subtle)
                            .size(Size::Xs)
                            .left_section(Icon::new(IconName::FolderOpen).size(Size::Xs))
                            .on_click(cx.listener(|this, _, _, cx| this.opendata(cx))),
                    ),
            )
            .into_any_element()
    }
}

fn loadrows() -> Vec<Row> {
    let server = connectionserver();
    files::home()
        .map(|home| {
            agent::agents(&home)
                .into_iter()
                .map(|agent| Row {
                    detection: agent::detect(&agent, server.as_deref()),
                    agent,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn connectionserver() -> Option<PathBuf> {
    match crate::cli::status() {
        Ok(crate::cli::InstallStatus::Installed(path)) => Some(path),
        _ => std::env::current_exe().ok(),
    }
}

fn initialpage() -> Page {
    match std::env::var("SYNAPS_PAGE").as_deref() {
        Ok("memory") => Page::Memories,
        Ok("vaults") => Page::Vaults,
        Ok("settings") => Page::Settings,
        _ => Page::Connections,
    }
}

fn instructionspath(agent: &agent::Agent) -> PathBuf {
    agent.instructions.clone()
}

fn settingspath(agent: &agent::Agent) -> PathBuf {
    agent.settings.clone()
}

fn changedocument(
    dashboard: &mut Dashboard,
    path: &std::path::Path,
    content: &str,
    cx: &mut Context<Dashboard>,
) {
    let Some(document) = dashboard
        .document
        .as_mut()
        .filter(|document| document.path == path)
    else {
        return;
    };
    document.current = content.to_owned();
    document.error = None;
    cx.notify();
}

fn loadstats(database: &std::path::Path) -> anyhow::Result<Stats> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let brain = crate::brain::Brain::open(database).await?;
        brain.stats().await
    })
}

fn loadoptimization(database: &std::path::Path) -> anyhow::Result<Optimization> {
    block(async {
        let brain = crate::brain::Brain::open(database).await?;
        Ok(brain.settings().await?.optimization)
    })
}

fn loadvaults(database: &std::path::Path) -> anyhow::Result<VaultData> {
    block(async {
        let store = VaultStore::open(database).await?;
        let vaults = store.vaults().await?;
        let selected = vaults.first().map(|vault| vault.id);
        let secrets = match selected {
            Some(id) => store.secrets(id).await?,
            None => Vec::new(),
        };
        Ok(VaultData {
            store,
            vaults,
            selected,
            secrets,
        })
    })
}

fn loadmemories(database: &std::path::Path) -> anyhow::Result<MemoryData> {
    block(async {
        let brain = Brain::open(database).await?;
        let memories = brain.search("", 100).await?;
        Ok(MemoryData { brain, memories })
    })
}

fn block<T>(future: impl std::future::Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    tokio::runtime::Runtime::new()?.block_on(future)
}
