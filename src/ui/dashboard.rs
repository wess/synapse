use crate::agent::{self, GuidanceState, Kind};
use crate::brain::{Brain, Memory, MemoryScope, Optimization, Stats};
use crate::files;
use crate::imports::{ImportBatch, ImportProvider, ImportSummary};
use crate::ui::buffer::{self, Buffer, Format};
use crate::ui::{
    Document, Notice, Page, Row, SaveDocument, agentrow, clibanner, document, header, memories,
    mesh, settings, skills, summary, vaults,
};
use crate::vault::{ScopeState, Secret, Vault, VaultStore};
use gpui::prelude::*;
use gpui::{Context, Entity, IntoElement, Window, div, px};
use guise::editor::EditorEvent;
use guise::input::FileInputEvent;
use guise::markdown::MarkdownEditorEvent;
use guise::prelude::*;
use std::path::PathBuf;

/// Preference key recording that the CLI prompt was dismissed for good.
const CLIPROMPT: &str = "cliprompt";

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
    memoryproject: Entity<TextInput>,
    memoryscope: MemoryScope,
    pendingmemory: Option<i64>,
    pendingwipe: bool,
    imports: Vec<ImportSummary>,
    importbatches: Vec<ImportBatch>,
    pendingbatch: Option<i64>,
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
    meshenabled: bool,
    meshagents: Vec<crate::relay::AgentView>,
    meshworkers: Vec<crate::relay::WorkerView>,
    meshfeed: Vec<crate::relay::Message>,
    mesherror: Option<String>,
    skillrows: Vec<skills::Row>,
    skillunmanaged: Vec<(String, String)>,
    skillproblems: Vec<String>,
    guidance: GuidanceState,
    pendingguidance: bool,
    clistatus: crate::cli::InstallStatus,
    showclibanner: bool,
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
    imports: Vec<ImportSummary>,
    batches: Vec<ImportBatch>,
}

impl Dashboard {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let database = files::database().unwrap_or_else(|_| PathBuf::from("brain.db"));
        let stats = loadstats(&database).unwrap_or_default();
        let optimization = loadoptimization(&database).unwrap_or_default();
        let page = initialpage();
        // A page opened directly has to arrive with its data, not fill in only
        // once the user navigates away and back.
        let meshdata = match page {
            Page::Mesh => loadmeshdata(&database),
            _ => MeshData {
                enabled: loadmesh(&database).unwrap_or(false),
                ..MeshData::default()
            },
        };
        let skilldata = match page {
            Page::Skills => loadskills(),
            _ => SkillData {
                rows: Vec::new(),
                unmanaged: Vec::new(),
                problems: Vec::new(),
            },
        };
        let (brain, memories, imports, importbatches, memoryerror) = match loadmemories(&database) {
            Ok(data) => (
                Some(data.brain),
                data.memories,
                data.imports,
                data.batches,
                None,
            ),
            Err(error) => (None, Vec::new(), Vec::new(), Vec::new(), Some(error)),
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
        let memoryproject = cx.new(|cx| {
            TextInput::new(cx)
                .label("Project root")
                .placeholder("/path/to/project")
                .value(selected.map(|memory| memory.project.as_str()).unwrap_or(""))
        });
        let memoryscope = selected.map(|memory| memory.scope).unwrap_or_default();
        let home = files::home().unwrap_or_else(|_| PathBuf::from("."));
        let soul = files::soul().unwrap_or_else(|_| PathBuf::from("SOUL.md"));
        // Synapse owns the data folder and SOUL.md, so create them on every
        // launch when they are missing. Tool configuration is never touched
        // here; connecting an agent stays an explicit choice on Connections.
        let soulerror = crate::instructions::ensure(&soul).err();
        let guidance = agent::guidancestate(&home, &soul);
        let clistatus = crate::cli::status().unwrap_or(crate::cli::InstallStatus::Missing);
        let showclibanner = promptforcli(&clistatus, clidismissed(brain.as_ref()));
        let document = std::env::var_os("SYNAPSE_DOCUMENT")
            .map(PathBuf::from)
            .and_then(|path| Self::loaddocument("Synapse".to_owned(), path, cx).ok());
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
        if let Some(error) = soulerror {
            notice = Notice::Error(format!("Could not create SOUL.md: {error}"));
        }
        Self {
            rows: loadrows(),
            stats,
            database,
            notice,
            document,
            page,
            brain,
            memories,
            selectedmemory,
            memoryquery,
            memorybody,
            memorysource,
            memoryproject,
            memoryscope,
            pendingmemory: None,
            pendingwipe: false,
            imports,
            importbatches,
            pendingbatch: None,
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
            meshenabled: meshdata.enabled,
            meshagents: meshdata.agents,
            meshworkers: meshdata.workers,
            meshfeed: meshdata.feed,
            mesherror: meshdata.error,
            skillrows: skilldata.rows,
            skillunmanaged: skilldata.unmanaged,
            skillproblems: skilldata.problems,
            guidance,
            pendingguidance: false,
            clistatus,
            showclibanner,
        }
    }

    fn installcli(&mut self, cx: &mut Context<Self>) {
        self.notice = match crate::cli::install() {
            Ok(path) => Notice::Success(format!("CLI installed at {}.", path.display())),
            Err(error) => Notice::Error(format!("Could not install the CLI: {error}")),
        };
        self.clistatus = crate::cli::status().unwrap_or(crate::cli::InstallStatus::Missing);
        self.showclibanner = false;
        cx.notify();
    }

    fn dismisscli(&mut self, cx: &mut Context<Self>) {
        self.showclibanner = false;
        cx.notify();
    }

    fn nevershowcli(&mut self, cx: &mut Context<Self>) {
        self.showclibanner = false;
        self.notice = match self.brain.clone() {
            Some(brain) => {
                match block(async move { brain.setpreference(CLIPROMPT, "dismissed").await }) {
                    Ok(()) => Notice::Success(
                        "The CLI prompt is off. Install it any time from Settings.".to_owned(),
                    ),
                    Err(error) => Notice::Error(format!("Could not save that preference: {error}")),
                }
            }
            None => Notice::Error("Could not save that preference: no database.".to_owned()),
        };
        cx.notify();
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
        let project = memory.map(|memory| memory.project.as_str()).unwrap_or("");
        self.memoryscope = memory.map(|memory| memory.scope).unwrap_or_default();
        self.memorybody
            .update(cx, |input, cx| input.set_text(body, cx));
        self.memorysource
            .update(cx, |input, cx| input.set_text(source, cx));
        self.memoryproject
            .update(cx, |input, cx| input.set_text(project, cx));
    }

    fn setmemoryscope(&mut self, scope: MemoryScope, cx: &mut Context<Self>) {
        self.memoryscope = scope;
        cx.notify();
    }

    fn savememory(&mut self, cx: &mut Context<Self>) {
        let (Some(brain), Some(id)) = (self.brain.clone(), self.selectedmemory) else {
            return;
        };
        let body = self.memorybody.read(cx).text();
        let source = self.memorysource.read(cx).text();
        let project = self.memoryproject.read(cx).text();
        let project = (self.memoryscope == MemoryScope::Project).then(|| PathBuf::from(project));
        match block(brain.updatememoryscoped(
            id,
            &body,
            Some(&source),
            self.memoryscope,
            project.as_deref(),
        )) {
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

    fn refreshimports(&mut self, cx: &mut Context<Self>) {
        let Some(brain) = self.brain.clone() else {
            return;
        };
        match files::home().and_then(|home| loadimports(&brain, &home)) {
            Ok((imports, batches)) => {
                self.imports = imports;
                self.importbatches = batches;
                self.pendingbatch = None;
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not refresh imports: {error}"));
            }
        }
        cx.notify();
    }

    fn importmemories(&mut self, provider: ImportProvider, cx: &mut Context<Self>) {
        let Some(brain) = self.brain.clone() else {
            return;
        };
        let result = files::home().and_then(|home| {
            block(async {
                let candidates = crate::imports::scan(&home, provider).await?;
                let preview = brain.importpreview(provider, candidates).await?;
                brain.importmemories(preview, false).await
            })
        });
        match result {
            Ok(report) => {
                self.notice = Notice::Success(format!(
                    "Imported {} {} memories; {} flagged entries stayed untouched. Batch #{} can be undone.",
                    report.stored,
                    provider.name(),
                    report.flagged,
                    report.batch.id
                ));
                self.refreshmemories(cx);
                self.refreshmemorystats();
                self.refreshimports(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!(
                    "Could not import {} memories: {error}",
                    provider.name()
                ));
                cx.notify();
            }
        }
    }

    fn openimport(&mut self, provider: ImportProvider, cx: &mut Context<Self>) {
        let result = files::home().and_then(|home| {
            let folder = match provider {
                ImportProvider::Claude => home.join(".claude").join("projects"),
                ImportProvider::Codex => std::env::var_os("CODEX_HOME")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".codex")),
                ImportProvider::Markdown => home,
            };
            files::reveal(&folder)
        });
        self.notice = match result {
            Ok(()) => Notice::Success(format!("Opened the {} memory source.", provider.name())),
            Err(error) => Notice::Error(format!("Could not open the memory source: {error}")),
        };
        cx.notify();
    }

    fn undoimport(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.pendingbatch != Some(id) {
            self.pendingbatch = Some(id);
            self.notice = Notice::Error(format!(
                "Choose Confirm undo to remove memories created only by import batch #{id}. Edited and shared memories stay."
            ));
            cx.notify();
            return;
        }
        let Some(brain) = self.brain.clone() else {
            return;
        };
        match block(brain.undoimport(id)) {
            Ok(deleted) => {
                self.notice = Notice::Success(format!(
                    "Undid import batch #{id}; removed {deleted} imported memories."
                ));
                self.pendingbatch = None;
                self.refreshmemories(cx);
                self.refreshmemorystats();
                self.refreshimports(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not undo import: {error}"));
                cx.notify();
            }
        }
    }

    fn showvaults(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Vaults;
        self.refreshvaults(cx);
    }

    fn showskills(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Skills;
        self.refreshskills(cx);
    }

    /// Re-read the library and ask every tool what it currently has.
    fn refreshskills(&mut self, cx: &mut Context<Self>) {
        let loaded = loadskills();
        self.skillrows = loaded.rows;
        self.skillunmanaged = loaded.unmanaged;
        self.skillproblems = loaded.problems;
        cx.notify();
    }

    /// Copy skills into the tools that read them. `only` narrows it to one.
    fn installskills(&mut self, only: Option<String>, cx: &mut Context<Self>) {
        let home = files::home().unwrap_or_else(|_| PathBuf::from("."));
        let result = block(async move {
            let receipts = crate::skill::Receipts::open(crate::files::database()?).await?;
            let (library, _) = crate::skill::library::all()?;
            let mut done = 0_usize;
            let mut refused = Vec::new();
            for agent in agent::agents(&home) {
                for skill in &library {
                    if only.as_ref().is_some_and(|name| name != &skill.name) {
                        continue;
                    }
                    match crate::skill::install(&receipts, &agent, skill, false).await {
                        Ok(_) => done += 1,
                        Err(error) => refused.push(format!("{}: {error}", agent.name)),
                    }
                }
            }
            Ok::<_, anyhow::Error>((done, refused))
        });
        self.notice = match result {
            Ok((done, refused)) if refused.is_empty() => {
                Notice::Success(format!("Installed {done} skill copies."))
            }
            // A skill Synapse does not own is left alone, and the page says so
            // rather than reporting a clean success it did not have.
            Ok((done, refused)) => Notice::Error(format!(
                "Installed {done}; left {} alone: {}",
                refused.len(),
                refused.join("; ")
            )),
            Err(error) => Notice::Error(format!("Could not install skills: {error}")),
        };
        self.refreshskills(cx);
    }

    fn adoptskill(&mut self, tool: String, name: String, cx: &mut Context<Self>) {
        let home = files::home().unwrap_or_else(|_| PathBuf::from("."));
        let result = block(async move {
            let receipts = crate::skill::Receipts::open(crate::files::database()?).await?;
            let agent = agent::agents(&home)
                .into_iter()
                .find(|agent| agent.name == tool)
                .ok_or_else(|| anyhow::anyhow!("that tool is no longer connected"))?;
            crate::skill::adopt(&receipts, &agent, &name).await
        });
        self.notice = match result {
            Ok(path) => Notice::Success(format!("Copied it into {}.", path.display())),
            Err(error) => Notice::Error(format!("Could not adopt it: {error}")),
        };
        self.refreshskills(cx);
    }

    fn openskills(&mut self) {
        self.notice = match crate::skill::library::directory()
            .and_then(|path| files::reveal(&path).map(|_| path))
        {
            Ok(_) => Notice::Ready,
            Err(error) => Notice::Error(format!("Could not open the library: {error}")),
        };
    }

    fn showmesh(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Mesh;
        self.refreshmesh(cx);
    }

    /// Read the roster, the workers, and the tail of the feed in one pass. The
    /// mesh has no push channel to subscribe to, so the page reloads when it is
    /// opened and when the refresh button is used.
    fn refreshmesh(&mut self, cx: &mut Context<Self>) {
        let loaded = loadmeshdata(&self.database);
        self.meshenabled = loaded.enabled;
        self.meshagents = loaded.agents;
        self.meshworkers = loaded.workers;
        self.meshfeed = loaded.feed;
        self.mesherror = loaded.error;
        cx.notify();
    }

    fn setmesh(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let database = self.database.clone();
        let result = block(async {
            let brain = crate::brain::Brain::open(database).await?;
            brain.setmesh(enabled).await
        });
        match result {
            Ok(()) => {
                self.meshenabled = enabled;
                self.notice = Notice::Success(format!(
                    "Agent mesh {}. Connected tools pick this up the next time they start.",
                    if enabled { "on" } else { "off" }
                ));
                self.refreshmesh(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not change the mesh: {error}"));
                cx.notify();
            }
        }
    }

    fn showsettings(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Settings;
        self.refreshguidance();
        cx.notify();
    }

    fn refreshguidance(&mut self) {
        if let (Ok(home), Ok(soul)) = (files::home(), files::soul()) {
            self.guidance = agent::guidancestate(&home, &soul);
        }
    }

    fn opensoul(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| {
            let path = files::soul()?;
            crate::instructions::ensure(&path)?;
            let document = Self::loaddocument("Shared guidance".to_owned(), path, cx)?;
            window.focus(&buffer::focus(&document.editor, cx));
            self.document = Some(document);
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(error) = result {
            self.notice = Notice::Error(format!("Could not open SOUL.md: {error}"));
        }
        self.refreshguidance();
        cx.notify();
    }

    fn syncguidance(&mut self, cx: &mut Context<Self>) {
        let result = files::home().and_then(|home| {
            let soul = files::soul()?;
            agent::sync(&home, &soul)
        });
        self.notice = match result {
            Ok(report) => Notice::Success(format!(
                "Shared guidance is ready; refreshed {} global pointers.",
                report.files.len()
            )),
            Err(error) => Notice::Error(format!("Could not sync shared guidance: {error}")),
        };
        self.pendingguidance = false;
        self.refreshguidance();
        cx.notify();
    }

    fn adoptguidance(&mut self, cx: &mut Context<Self>) {
        if !self.pendingguidance {
            self.pendingguidance = true;
            self.notice = Notice::Error(
                "Choose Confirm consolidation to move both global instruction files into SOUL.md. Backups will be kept."
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        let result = files::home().and_then(|home| {
            let soul = files::soul()?;
            agent::adopt(&home, &soul)
        });
        self.notice = match result {
            Ok(report) => Notice::Success(format!(
                "Consolidated {} guidance source(s) into SOUL.md; both global files now contain managed pointers.",
                report.moved
            )),
            Err(error) => Notice::Error(format!("Could not consolidate guidance: {error}")),
        };
        self.pendingguidance = false;
        self.refreshguidance();
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
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synapse MCP executable"))
            .and_then(|server| {
                let soul = files::soul()?;
                agent::setup(&row.agent, &row.detection, &server, &soul)
            });
        self.notice = match result {
            Ok(()) => Notice::Success(format!("{} is connected to Synapse.", row.agent.name)),
            Err(error) => Notice::Error(format!("Could not connect {}: {error}", row.agent.name)),
        };
        self.rows = loadrows();
        cx.notify();
    }

    /// Add or remove the startup notice for an already-connected tool, so
    /// gaining it never means disconnecting and connecting again. The tool's
    /// settings file is captured first and restored if the write fails.
    fn togglenotice(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.kind == kind).cloned() else {
            return;
        };
        let installed = row.detection.hooks.notice;
        let result = connectionserver()
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synapse executable"))
            .and_then(|server| {
                let snapshot = files::Snapshot::capture(&row.agent.settings)?;
                let applied = if installed {
                    agent::removenotice(&row.agent.settings, &server)
                        .map(|_| agent::HookState::default())
                } else {
                    agent::applynotice(&row.agent.settings, &server)
                };
                if applied.is_err() {
                    snapshot.restore()?;
                }
                applied
            });
        self.notice = match (result, installed) {
            (Ok(_), true) => Notice::Success(format!(
                "{} no longer announces Synapse at startup.",
                row.agent.name
            )),
            (Ok(state), false) if state.borrowed => Notice::Success(format!(
                "{} announces Synapse at startup. Your own status line was left alone.",
                row.agent.name
            )),
            (Ok(_), false) => Notice::Success(format!(
                "{} announces Synapse at startup. Open a new session to see it.",
                row.agent.name
            )),
            (Err(error), _) => {
                Notice::Error(format!("Could not update {}: {error}", row.agent.name))
            }
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
        document.error = Some("Save or discard these changes before quitting Synapse.".to_owned());
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
            Ok(()) => Notice::Success("Opened the local Synapse data folder.".to_owned()),
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
                    Box::new(cx.listener(|this, _, _, cx| this.showmesh(cx))),
                    Box::new(cx.listener(|this, _, _, cx| this.showskills(cx))),
                    Box::new(cx.listener(|this, _, _, cx| this.showvaults(cx))),
                    Box::new(cx.listener(|this, _, _, cx| this.showsettings(cx))),
                    cx,
                ))
                .child(document::render(document, save, close, discard, cx))
                .into_any_element();
        }

        let banner = self.showclibanner.then(|| {
            let path = crate::cli::destination()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "~/.local/bin".to_owned());
            clibanner::render(
                path,
                clibanner::Actions {
                    install: Box::new(cx.listener(|this, _, _, cx| this.installcli(cx))),
                    later: Box::new(cx.listener(|this, _, _, cx| this.dismisscli(cx))),
                    never: Box::new(cx.listener(|this, _, _, cx| this.nevershowcli(cx))),
                },
                cx,
            )
        });

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
                Box::new(cx.listener(|this, _, _, cx| this.showmesh(cx))),
                Box::new(cx.listener(|this, _, _, cx| this.showskills(cx))),
                Box::new(cx.listener(|this, _, _, cx| this.showvaults(cx))),
                Box::new(cx.listener(|this, _, _, cx| this.showsettings(cx))),
                cx,
            ))
            .children(banner);

        if self.page == Page::Memories {
            let selecthost = cx.entity().downgrade();
            let selectmemory = move |id| -> memories::Click {
                let host = selecthost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.selectmemory(id, cx)).ok();
                })
            };
            let importhost = cx.entity().downgrade();
            let importmemories = move |provider| -> memories::Click {
                let host = importhost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.importmemories(provider, cx))
                        .ok();
                })
            };
            let reviewhost = cx.entity().downgrade();
            let reviewimport = move |provider| -> memories::Click {
                let host = reviewhost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.openimport(provider, cx))
                        .ok();
                })
            };
            let undohost = cx.entity().downgrade();
            let undoimport = move |id| -> memories::Click {
                let host = undohost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.undoimport(id, cx)).ok();
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
                        project: self.memoryproject.clone(),
                        scope: self.memoryscope,
                        pendingdelete: self.pendingmemory,
                        pendingwipe: self.pendingwipe,
                        imports: self.imports.clone(),
                        batches: self.importbatches.clone(),
                        pendingbatch: self.pendingbatch,
                        notice: self.notice.clone(),
                    },
                    memories::Actions {
                        search: Box::new(cx.listener(|this, _, _, cx| this.refreshmemories(cx))),
                        select: Box::new(selectmemory),
                        save: Box::new(cx.listener(|this, _, _, cx| this.savememory(cx))),
                        global: Box::new(cx.listener(|this, _, _, cx| {
                            this.setmemoryscope(MemoryScope::Global, cx)
                        })),
                        project: Box::new(cx.listener(|this, _, _, cx| {
                            this.setmemoryscope(MemoryScope::Project, cx)
                        })),
                        delete: Box::new(cx.listener(|this, _, _, cx| this.deletememory(cx))),
                        wipe: Box::new(cx.listener(|this, _, _, cx| this.wipememories(cx))),
                        import: Box::new(importmemories),
                        review: Box::new(reviewimport),
                        undo: Box::new(undoimport),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(Text::new("Scoped SQLite · editable and importable").size(Size::Xs))
                        .right(Text::new("Original changes save immediately").size(Size::Xs)),
                )
                .into_any_element();
        }

        if self.page == Page::Mesh {
            return shell
                .child(mesh::render(
                    mesh::View {
                        enabled: self.meshenabled,
                        agents: self.meshagents.clone(),
                        workers: self.meshworkers.clone(),
                        feed: self.meshfeed.clone(),
                        error: self.mesherror.clone(),
                    },
                    mesh::Actions {
                        enable: Box::new(cx.listener(|this, _, _, cx| this.setmesh(true, cx))),
                        refresh: Box::new(cx.listener(|this, _, _, cx| this.refreshmesh(cx))),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(Text::new("Agent mesh · local SQLite bus").size(Size::Xs))
                        .right(Text::new("Messages stay on this Mac").size(Size::Xs)),
                )
                .into_any_element();
        }

        if self.page == Page::Skills {
            let host = cx.entity().downgrade();
            let install = move |name: String| -> skills::Click {
                let host = host.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.installskills(Some(name.clone()), cx))
                        .ok();
                })
            };
            let adopthost = cx.entity().downgrade();
            let adopt = move |tool: String, name: String| -> skills::Click {
                let host = adopthost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| {
                        this.adoptskill(tool.clone(), name.clone(), cx)
                    })
                    .ok();
                })
            };
            return shell
                .child(skills::render(
                    skills::View {
                        rows: self.skillrows.clone(),
                        unmanaged: self.skillunmanaged.clone(),
                        problems: self.skillproblems.clone(),
                        folder: crate::skill::library::directory()
                            .map(|path| path.display().to_string())
                            .unwrap_or_default(),
                        message: match &self.notice {
                            Notice::Ready => None,
                            Notice::Success(message) => Some((message.clone(), false)),
                            Notice::Error(message) => Some((message.clone(), true)),
                        },
                    },
                    skills::Actions {
                        installall: Box::new(
                            cx.listener(|this, _, _, cx| this.installskills(None, cx)),
                        ),
                        refresh: Box::new(cx.listener(|this, _, _, cx| this.refreshskills(cx))),
                        openfolder: Box::new(cx.listener(|this, _, _, _| this.openskills())),
                        install: Box::new(install),
                        adopt: Box::new(adopt),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(Text::new("Skills · one library, every tool").size(Size::Xs))
                        .right(Text::new("Agent Skills open format").size(Size::Xs)),
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
                        mesh: self.meshenabled,
                        thememode: crate::ui::theme::mode(cx),
                        clistatus,
                        clipath,
                        shellintegration,
                        shellerror,
                        // Sync, adopt, and the recall budget all report through
                        // the dashboard notice. Without this the page showed
                        // only the app menu's message and every button on it
                        // looked like it had done nothing.
                        message: crate::ui::menu::message(cx).or_else(|| match &self.notice {
                            Notice::Ready => None,
                            Notice::Success(message) => Some((message.clone(), false)),
                            Notice::Error(message) => Some((message.clone(), true)),
                        }),
                        guidance: self.guidance.clone(),
                        pendingguidance: self.pendingguidance,
                    },
                    settings::Actions {
                        meshon: Box::new(cx.listener(|this, _, _, cx| this.setmesh(true, cx))),
                        meshoff: Box::new(cx.listener(|this, _, _, cx| this.setmesh(false, cx))),
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
                        opensoul: Box::new(
                            cx.listener(|this, _, window, cx| this.opensoul(window, cx)),
                        ),
                        syncguidance: Box::new(cx.listener(|this, _, _, cx| this.syncguidance(cx))),
                        adoptguidance: Box::new(
                            cx.listener(|this, _, _, cx| this.adoptguidance(cx)),
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
                        .right(Text::new("synapse run -- <command>").size(Size::Xs)),
                )
                .into_any_element();
        }

        shell
            .child(
                div()
                    .id("synapsemain")
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
                                            let kind = row.agent.kind;
                                            let notice =
                                                Box::new(cx.listener(move |this, _, _, cx| {
                                                    this.togglenotice(kind, cx);
                                                }));
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
                                                notice,
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
    match std::env::var("SYNAPSE_PAGE").as_deref() {
        Ok("memory") => Page::Memories,
        Ok("mesh") => Page::Mesh,
        Ok("skills") => Page::Skills,
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

/// Everything the Skills screen shows. Opening the app straight onto a page has
/// to fill it the same way navigating to it does, or the screen reports an
/// empty library that is not empty.
struct SkillData {
    rows: Vec<skills::Row>,
    unmanaged: Vec<(String, String)>,
    problems: Vec<String>,
}

fn loadskills() -> SkillData {
    let home = files::home().unwrap_or_else(|_| PathBuf::from("."));
    let (statuses, mut problems) = match block(crate::skill::survey(&home)) {
        Ok(surveyed) => surveyed,
        Err(error) => (
            Vec::new(),
            vec![format!("the library could not be read: {error}")],
        ),
    };
    let (library, listing) = crate::skill::library::all().unwrap_or_default();
    problems.extend(listing);
    let known: Vec<String> = library.iter().map(|skill| skill.name.clone()).collect();
    SkillData {
        rows: library
            .into_iter()
            .map(|skill| skills::Row {
                places: statuses
                    .iter()
                    .filter(|status| status.skill == skill.name)
                    .cloned()
                    .collect(),
                name: skill.name,
                description: skill.description,
                files: skill.files.len(),
            })
            .collect(),
        unmanaged: agent::agents(&home)
            .into_iter()
            .flat_map(|agent| {
                crate::skill::unknown(&agent, &known)
                    .into_iter()
                    .map(move |name| (agent.name.to_owned(), name))
            })
            .collect(),
        problems,
    }
}

/// Everything the Mesh screen shows, for the same reason.
#[derive(Default)]
struct MeshData {
    enabled: bool,
    agents: Vec<crate::relay::AgentView>,
    workers: Vec<crate::relay::WorkerView>,
    feed: Vec<crate::relay::Message>,
    error: Option<String>,
}

fn loadmeshdata(database: &std::path::Path) -> MeshData {
    let enabled = loadmesh(database).unwrap_or(false);
    if !enabled {
        return MeshData::default();
    }
    let path = database.to_path_buf();
    match block(async {
        let mesh = crate::relay::Mesh::open(path).await?;
        Ok((
            mesh.agents().await?,
            mesh.workers().await?,
            mesh.feed(0, 40).await?,
        ))
    }) {
        Ok((agents, workers, feed)) => MeshData {
            enabled,
            agents,
            workers,
            feed,
            error: None,
        },
        Err(error) => MeshData {
            enabled,
            error: Some(format!("Could not read the mesh: {error}")),
            ..MeshData::default()
        },
    }
}

fn loadmesh(database: &std::path::Path) -> anyhow::Result<bool> {
    block(async {
        let brain = crate::brain::Brain::open(database).await?;
        brain.mesh().await
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
        let home = files::home()?;
        let (imports, batches) = importdata(&brain, &home).await?;
        Ok(MemoryData {
            brain,
            memories,
            imports,
            batches,
        })
    })
}

fn loadimports(
    brain: &Brain,
    home: &std::path::Path,
) -> anyhow::Result<(Vec<ImportSummary>, Vec<ImportBatch>)> {
    block(importdata(brain, home))
}

async fn importdata(
    brain: &Brain,
    home: &std::path::Path,
) -> anyhow::Result<(Vec<ImportSummary>, Vec<ImportBatch>)> {
    let mut summaries = Vec::new();
    for provider in [ImportProvider::Claude, ImportProvider::Codex] {
        let summary = match crate::imports::scan(home, provider).await {
            Ok(candidates) => match brain.importpreview(provider, candidates).await {
                Ok(preview) => preview.summary(),
                Err(error) => ImportSummary::error(provider, error),
            },
            Err(error) => ImportSummary::error(provider, error),
        };
        summaries.push(summary);
    }
    let batches = brain.importbatches().await?;
    Ok((summaries, batches))
}

/// Offer the prompt for a first install only. A conflicting file at the
/// destination needs a decision the banner cannot offer, so Settings keeps
/// that case where the path and the conflict are both visible.
fn promptforcli(status: &crate::cli::InstallStatus, dismissed: bool) -> bool {
    matches!(status, crate::cli::InstallStatus::Missing) && !dismissed
}

fn clidismissed(brain: Option<&Brain>) -> bool {
    let Some(brain) = brain else {
        return false;
    };
    block(async { brain.preference(CLIPROMPT).await })
        .ok()
        .flatten()
        .as_deref()
        == Some("dismissed")
}

fn block<T>(future: impl std::future::Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    tokio::runtime::Runtime::new()?.block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::InstallStatus;

    #[test]
    fn cli_prompt_appears_only_for_a_fresh_install_the_user_has_not_dismissed() {
        assert!(promptforcli(&InstallStatus::Missing, false));
        assert!(!promptforcli(&InstallStatus::Missing, true));
        assert!(!promptforcli(
            &InstallStatus::Installed(PathBuf::from("/bin/synapse")),
            false
        ));
        assert!(!promptforcli(
            &InstallStatus::Conflict(PathBuf::from("/bin/synapse")),
            false
        ));
    }
}
