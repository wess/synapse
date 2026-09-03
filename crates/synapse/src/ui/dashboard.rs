use crate::ui::buffer::{self, Buffer, Format};
use crate::ui::{
    Document, Notice, Page, Row, SaveDocument, agentrow, clibanner, console, document, memories,
    mesh, settings, sidebar, skills, summary, vaults,
};
use gpui::prelude::*;
use gpui::{Context, Entity, IntoElement, Window, div, px};
use guise::editor::EditorEvent;
use guise::input::FileInputEvent;
use guise::markdown::MarkdownEditorEvent;
use guise::prelude::*;
use std::path::PathBuf;
use synapsecore::agent::{self, GuidanceState};
use synapsecore::brain::{Brain, Memory, MemoryScope, Optimization, Stats};
use synapsecore::files;
use synapsecore::imports::{ImportBatch, ImportProvider, ImportSummary};
use synapsecore::vault::{Backend, ScopeState, Secret, Vault, VaultStore};

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
    /// The name of a tool somebody is describing. It becomes the descriptor's
    /// file name, so it is taken here rather than guessed from the file.
    toolname: Entity<TextInput>,
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
    /// Where values are kept, and whether a move has been asked for once.
    backend: Backend,
    pendingbackend: bool,
    appmenu: Option<Entity<MenuBar>>,
    optimization: Optimization,
    meshenabled: bool,
    learnenabled: bool,
    meshagents: Vec<synapsecore::relay::AgentView>,
    meshworkers: Vec<synapsecore::relay::WorkerView>,
    meshfeed: Vec<synapsecore::relay::Message>,
    /// The name this window is on the roster under, or why it is not on it.
    /// `Err` is a state the console has to be able to draw: a mesh that is off,
    /// or a name an agent already answers to.
    consoleidentity: Result<String, String>,
    consolefocus: Option<String>,
    consoleinput: Entity<TextInput>,
    /// Most workers one session may run, read with the rest of the mesh.
    consolelimit: usize,
    /// Whether the console draws its reactor, as the settings have it.
    reactorwanted: bool,
    /// Dictation into the composer, when the build has a microphone.
    #[cfg(all(feature = "voice", target_os = "macos"))]
    dictation: crate::voice::Dictation,
    /// Monotonic seconds since the console opened. The one value the reactor
    /// draws from that is not measured off the mesh — it turns the sweep.
    consoleclock: Option<std::time::Instant>,
    /// The poll. A conversation nobody refreshed is a conversation that looks
    /// finished, so this is the one page that reloads on its own — and it only
    /// exists while that page is open.
    consoletick: Option<gpui::Task<()>>,
    mesherror: Option<String>,
    skillrows: Vec<skills::Row>,
    skillunmanaged: Vec<(String, String)>,
    skillproblems: Vec<String>,
    guidance: GuidanceState,
    pendingguidance: bool,
    clistatus: synapsecore::cli::InstallStatus,
    showclibanner: bool,
}

struct VaultData {
    store: VaultStore,
    vaults: Vec<Vault>,
    selected: Option<i64>,
    secrets: Vec<Secret>,
    backend: Backend,
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
        let soulerror = synapsecore::instructions::ensure(&soul).err();
        let guidance = agent::guidancestate(&home, &soul);
        let clistatus =
            synapsecore::cli::status().unwrap_or(synapsecore::cli::InstallStatus::Missing);
        let showclibanner = promptforcli(&clistatus, clidismissed(brain.as_ref()));
        let document = std::env::var_os("SYNAPSE_DOCUMENT")
            .map(PathBuf::from)
            .and_then(|path| Self::loaddocument("Synapse".to_owned(), path, cx).ok());
        let appmenu = crate::ui::menu::bar(cx);
        let toolname = cx.new(|cx| {
            TextInput::new(cx)
                .label("Add a connection")
                .placeholder("hermes")
        });
        let vaultname = cx.new(|cx| TextInput::new(cx).label("New vault").placeholder("work"));
        let consoleinput = cx.new(|cx| {
            TextInput::new(cx)
                .label("Say something to the mesh")
                .placeholder("@overseer get the release notes written")
        });
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
        let (vaultstore, vaults, selectedvault, secrets, backend, mut notice) =
            match loadvaults(&database) {
                Ok(data) => (
                    Some(data.store),
                    data.vaults,
                    data.selected,
                    data.secrets,
                    data.backend,
                    Notice::Ready,
                ),
                Err(error) => (
                    None,
                    Vec::new(),
                    None,
                    Vec::new(),
                    Backend::Encrypted,
                    Notice::Error(format!("Could not open vaults: {error}")),
                ),
            };
        if let Some(error) = memoryerror {
            notice = Notice::Error(format!("Could not open memories: {error}"));
        }
        if let Some(error) = soulerror {
            notice = Notice::Error(format!("Could not create SOUL.md: {error}"));
        }
        let learnenabled = loadlearn(&database).unwrap_or(false);
        let consolelimit = loadworkers(&database).unwrap_or(synapsecore::relay::DEFAULTWORKERS);
        let reactorwanted = loadreactor(&database).unwrap_or(true);
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
            toolname,
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
            backend,
            pendingbackend: false,
            appmenu,
            optimization,
            meshenabled: meshdata.enabled,
            learnenabled,
            meshagents: meshdata.agents,
            meshworkers: meshdata.workers,
            meshfeed: meshdata.feed,
            consoleidentity: Err(
                "The agent mesh is off. Turn it on in Settings to reach agents from here."
                    .to_owned(),
            ),
            consolefocus: None,
            consoleinput,
            consolelimit,
            reactorwanted,
            #[cfg(all(feature = "voice", target_os = "macos"))]
            dictation: crate::voice::Dictation::default(),
            consoleclock: None,
            consoletick: None,
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
        self.notice = match synapsecore::cli::install() {
            Ok(path) => Notice::Success(format!("CLI installed at {}.", path.display())),
            Err(error) => Notice::Error(format!("Could not install the CLI: {error}")),
        };
        self.clistatus =
            synapsecore::cli::status().unwrap_or(synapsecore::cli::InstallStatus::Missing);
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

    /// The handlers behind the navigation buttons, built once so the two places
    /// that draw the header cannot drift apart.
    fn navigation(&self, cx: &mut Context<Self>) -> sidebar::Navigation {
        sidebar::Navigation {
            connections: Box::new(cx.listener(|this, _, _, cx| this.showconnections(cx))),
            memories: Box::new(cx.listener(|this, _, _, cx| this.showmemories(cx))),
            mesh: Box::new(cx.listener(|this, _, _, cx| this.showmesh(cx))),
            console: Box::new(cx.listener(|this, _, _, cx| this.showconsole(cx))),
            skills: Box::new(cx.listener(|this, _, _, cx| this.showskills(cx))),
            vaults: Box::new(cx.listener(|this, _, _, cx| this.showvaults(cx))),
            settings: Box::new(cx.listener(|this, _, _, cx| this.showsettings(cx))),
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
                let candidates = synapsecore::imports::scan(&home, provider).await?;
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
            let receipts =
                synapsecore::skill::Receipts::open(synapsecore::files::database()?).await?;
            let (library, _) = synapsecore::skill::library::all()?;
            let waiting = receipts.proposals().await.unwrap_or_default();
            let mut done = 0_usize;
            let mut refused = Vec::new();
            for agent in agent::agents(&home) {
                for skill in &library {
                    if only.as_ref().is_some_and(|name| name != &skill.name) {
                        continue;
                    }
                    // Installing everything means everything approved. A
                    // proposal reaching a tool from this button is the one way
                    // the gate leaks.
                    if waiting
                        .iter()
                        .any(|item| item.skill == skill.name && item.shelf == skill.shelf.key())
                    {
                        continue;
                    }
                    if synapsecore::skill::target(&agent, &skill.shelf, &skill.name).is_none() {
                        continue;
                    }
                    match synapsecore::skill::install(&receipts, &agent, skill, false).await {
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
            let receipts =
                synapsecore::skill::Receipts::open(synapsecore::files::database()?).await?;
            let agent = agent::agents(&home)
                .into_iter()
                .find(|agent| agent.name == tool)
                .ok_or_else(|| anyhow::anyhow!("that tool is no longer connected"))?;
            synapsecore::skill::adopt(&receipts, &agent, &name).await
        });
        self.notice = match result {
            Ok(path) => Notice::Success(format!("Copied it into {}.", path.display())),
            Err(error) => Notice::Error(format!("Could not adopt it: {error}")),
        };
        self.refreshskills(cx);
    }

    /// Install a skill an agent wrote, everywhere it can go, and stop calling
    /// it proposed.
    fn approveskill(&mut self, name: String, project: String, cx: &mut Context<Self>) {
        let home = files::home().unwrap_or_else(|_| PathBuf::from("."));
        let result = block(async move {
            let receipts =
                synapsecore::skill::Receipts::open(synapsecore::files::database()?).await?;
            let skill = locateskill(&name, &project)?;
            let agents = agent::agents(&home);
            let reached = synapsecore::skill::approve(&receipts, &agents, &skill, false).await?;
            Ok::<_, anyhow::Error>(
                reached
                    .into_iter()
                    .filter_map(|(tool, outcome)| outcome.ok().map(|_| tool))
                    .collect::<Vec<_>>(),
            )
        });
        self.notice = match result {
            Ok(tools) if tools.is_empty() => {
                Notice::Error("No connected tool could take that skill.".to_owned())
            }
            Ok(tools) => Notice::Success(format!("Approved. Installed into {}.", tools.join(", "))),
            Err(error) => Notice::Error(format!("Could not approve it: {error}")),
        };
        self.refreshskills(cx);
    }

    fn rejectskill(&mut self, name: String, project: String, cx: &mut Context<Self>) {
        let result = block(async move {
            let receipts =
                synapsecore::skill::Receipts::open(synapsecore::files::database()?).await?;
            let skill = locateskill(&name, &project)?;
            synapsecore::skill::reject(&receipts, &skill).await
        });
        self.notice = match result {
            Ok(path) => Notice::Success(format!("Turned it down and removed {}.", path.display())),
            Err(error) => Notice::Error(format!("Could not turn it down: {error}")),
        };
        self.refreshskills(cx);
    }

    fn openskills(&mut self) {
        self.notice = match synapsecore::skill::library::directory()
            .and_then(|path| files::reveal(&path).map(|_| path))
        {
            Ok(_) => Notice::Ready,
            Err(error) => Notice::Error(format!("Could not open the library: {error}")),
        };
    }

    /// What clicking to the initial page would have done.
    ///
    /// `new` cannot do it: joining the mesh and starting the poll both want a
    /// handle to an entity that does not exist until it has returned. Without
    /// this, `SYNAPSE_PAGE=console` drew a console that had never joined —
    /// every column correct and empty, and nothing saying why.
    pub fn opened(&mut self, cx: &mut Context<Self>) {
        if self.page == Page::Console {
            self.showconsole(cx);
        }
    }

    /// Open the console, which means joining the mesh as yourself.
    ///
    /// Arriving here rather than at startup is deliberate. A roster row is a
    /// promise that somebody is reachable, and an app sitting in the dock with
    /// nobody in front of it is not — so the row appears when the page does.
    fn showconsole(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Console;
        self.consoleidentity = self.joinmesh();
        self.refreshconsole(cx);
        // One loop while the page is open, and none while it is not. It runs at
        // a frame rate rather than a poll rate because the reactor turns on it,
        // but it only goes back to the database every `RELOAD` frames — reading
        // the mesh ten times a second to animate a ring would be paying for the
        // wrong thing. `touch` rides along with the reload, so this window's
        // roster row goes quietly offline on its own if the app is closed
        // without leaving the page.
        // With a reactor there is something to animate, so the loop runs at a
        // frame rate and only reaches the database every `RELOAD` frames. With
        // none, there is nothing between reloads worth waking up for, so the
        // frame *is* the reload.
        const FRAME: std::time::Duration = match cfg!(feature = "reactor") {
            true => std::time::Duration::from_millis(100),
            false => std::time::Duration::from_millis(1200),
        };
        const RELOAD: u32 = match cfg!(feature = "reactor") {
            true => 12,
            false => 1,
        };
        if self.consoletick.is_none() {
            self.consoleclock = Some(std::time::Instant::now());
            self.consoletick = Some(cx.spawn(async move |this, cx| {
                let mut frame = 0_u32;
                loop {
                    cx.background_executor().timer(FRAME).await;
                    frame = frame.wrapping_add(1);
                    let alive = this
                        .update(cx, |this, cx| {
                            if this.page != Page::Console {
                                // The microphone belongs to this page. Walking
                                // away from it closes the device rather than
                                // leaving it open with nothing on screen to say
                                // so.
                                this.stopdictation();
                                this.consoletick = None;
                                this.consoleclock = None;
                                return false;
                            }
                            // A finished transcript is checked every frame:
                            // it arrives when the recogniser is done, not on
                            // the reload's schedule, and waiting a whole second
                            // to paste what you just said feels like a fault.
                            this.collectdictation(cx);
                            match frame.is_multiple_of(RELOAD) {
                                true => this.refreshconsole(cx),
                                // Nothing was read, but the clock moved, so the
                                // sweep does too.
                                false => cx.notify(),
                            }
                            true
                        })
                        .unwrap_or(false);
                    if !alive {
                        break;
                    }
                }
            }));
        }
    }

    /// Register this window as a person on the roster.
    ///
    /// `relay::console::arrive` is the only thing anywhere that sets `human`,
    /// and both a terminal `mux` and this call it — an agent is told to ask a
    /// human row questions and never delegate to one, so two ways of creating
    /// that row is two ways of getting it wrong.
    fn joinmesh(&self) -> Result<String, String> {
        if !self.meshenabled {
            return Err(
                "The agent mesh is off. Turn it on in Settings to reach agents from here."
                    .to_owned(),
            );
        }
        let name = synapsecore::relay::console::whoami();
        let database = self.database.clone();
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let joined = block(async {
            let mesh = synapsecore::relay::Mesh::open(&database).await?;
            synapsecore::relay::console::arrive(&mesh, &name, &root, "Synapse").await
        });
        match joined {
            Ok(()) => Ok(name),
            Err(error) => Err(format!("Could not join the mesh: {error:#}")),
        }
    }

    /// Reload what the console shows, and keep this window's roster row alive.
    fn refreshconsole(&mut self, cx: &mut Context<Self>) {
        self.refreshmesh(cx);
        let database = self.database.clone();
        let me = self.consoleidentity.clone().ok();
        self.consolelimit = block(async {
            let mesh = synapsecore::relay::Mesh::glance(&database).await?;
            if let Some(name) = me {
                let _ = mesh.touch(&name).await;
            }
            mesh.maxworkers().await
        })
        .unwrap_or(synapsecore::relay::DEFAULTWORKERS);
        cx.notify();
    }

    /// A frame of mesh, in the shape the reactor wants.
    ///
    /// Every field is measured: a ring for each message that has landed within
    /// its own lifetime, a band per agent carrying what that agent is doing, and
    /// a level that is the share of them working. Nothing is synthesised, so an
    /// idle mesh draws an idle reactor rather than a screensaver.
    fn consolemotion(&self) -> (console::Life, console::Pulse) {
        let phase = self
            .consoleclock
            .map(|start| start.elapsed().as_secs_f32())
            .unwrap_or_default();
        let agents: Vec<&synapsecore::relay::AgentView> = self
            .meshagents
            .iter()
            .filter(|agent| !agent.human)
            .collect();
        let working = agents
            .iter()
            .filter(|agent| agent.status == "working")
            .count();
        let bands: Vec<f32> = agents
            .iter()
            .map(|agent| match (agent.online, agent.status.as_str()) {
                (false, _) => 0.0,
                (true, "working") => 1.0,
                (true, "blocked") => 0.65,
                (true, _) => 0.3,
            })
            .collect();
        let level = match agents.is_empty() {
            true => 0.0,
            false => working as f32 / agents.len() as f32,
        };
        // A ring per recent message, aged in the same seconds `PULSE_LIFE` uses.
        // The age is the whole of the bookkeeping: a message rings while it is
        // younger than a ring's life and then stops on its own, so nothing has
        // to remember which ones have already been drawn. One that landed while
        // the window was on another page is born too old to ring at all.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or_default();
        let rings: Vec<f32> = self
            .meshfeed
            .iter()
            .map(|message| (now - message.created).max(0) as f32)
            .filter(|age| *age < console::RINGLIFE)
            .collect();
        // What the mesh is doing, in the four states the reactor draws. Each has
        // to be true of the mesh rather than of what the window would like:
        // `Idle` is nobody but you here, and it looks like it.
        let life = if agents.is_empty() {
            console::Life::Idle
        } else if !rings.is_empty() {
            console::Life::Talking
        } else if working > 0 {
            console::Life::Working
        } else if agents.iter().any(|agent| agent.online) {
            console::Life::Waiting
        } else {
            console::Life::Idle
        };
        (
            life,
            console::Pulse {
                phase,
                level,
                bands,
                rings,
            },
        )
    }

    /// What the microphone button should say, which is the only place the
    /// voice feature reaches the page.
    #[cfg(all(feature = "voice", target_os = "macos"))]
    fn micstate(&self) -> console::Mic {
        use crate::voice::Access;

        if !crate::voice::available() {
            return console::Mic::Refused(
                "This Mac cannot transcribe without sending audio to Apple, so Synapse will not."
                    .to_owned(),
            );
        }
        if self.dictation.transcribing() {
            return console::Mic::Transcribing;
        }
        if self.dictation.listening() {
            return console::Mic::Listening;
        }
        match crate::voice::access() {
            Access::Allowed => console::Mic::Ready,
            Access::Unknown => console::Mic::Ask,
            Access::Refused => console::Mic::Refused(
                "Synapse is not allowed to use the microphone. Turn it on in System Settings › Privacy & Security."
                    .to_owned(),
            ),
        }
    }

    #[cfg(not(all(feature = "voice", target_os = "macos")))]
    fn micstate(&self) -> console::Mic {
        console::Mic::Absent
    }

    /// Start dictating, or stop and transcribe. One button, because at any
    /// moment there is only one of these worth doing.
    #[cfg(all(feature = "voice", target_os = "macos"))]
    fn dictate(&mut self, cx: &mut Context<Self>) {
        use crate::voice::Access;

        if crate::voice::access() == Access::Unknown {
            crate::voice::ask();
            cx.notify();
            return;
        }
        let outcome = match self.dictation.listening() {
            true => self.dictation.stop(),
            false => self.dictation.start(),
        };
        if let Err(error) = outcome {
            self.notice = Notice::Error(format!("{error:#}"));
        }
        cx.notify();
    }

    #[cfg(not(all(feature = "voice", target_os = "macos")))]
    fn dictate(&mut self, _cx: &mut Context<Self>) {}

    /// Put a finished transcript into the composer, appended rather than
    /// replacing: dictation is another way to type, so it should behave like
    /// typing into whatever is already there.
    #[cfg(all(feature = "voice", target_os = "macos"))]
    fn collectdictation(&mut self, cx: &mut Context<Self>) {
        let Some(outcome) = self.dictation.poll() else {
            return;
        };
        match outcome {
            Ok(text) => {
                let existing = self.consoleinput.read(cx).text();
                let joined = match existing.trim().is_empty() {
                    true => text,
                    false => format!("{} {text}", existing.trim_end()),
                };
                self.consoleinput
                    .update(cx, |input, cx| input.set_text(&joined, cx));
                self.notice = Notice::Ready;
            }
            Err(error) => self.notice = Notice::Error(format!("{error:#}")),
        }
        cx.notify();
    }

    #[cfg(not(all(feature = "voice", target_os = "macos")))]
    fn collectdictation(&mut self, _cx: &mut Context<Self>) {}

    #[cfg(all(feature = "voice", target_os = "macos"))]
    fn stopdictation(&mut self) {
        self.dictation.cancel();
    }

    #[cfg(not(all(feature = "voice", target_os = "macos")))]
    fn stopdictation(&mut self) {}

    /// Aim a bare line at one agent. Nothing depends on it — `@name` always
    /// works — so this only ever saves typing.
    fn focusagent(&mut self, name: String, cx: &mut Context<Self>) {
        self.consolefocus = match self.consolefocus.as_deref() == Some(name.as_str()) {
            true => None,
            false => Some(name),
        };
        cx.notify();
    }

    /// Send what is in the composer, under the same grammar `synapse mux` uses.
    fn sendconsole(&mut self, cx: &mut Context<Self>) {
        use synapsecore::relay::console::Line;

        let Ok(me) = self.consoleidentity.clone() else {
            return;
        };
        let typed = self.consoleinput.read(cx).text();
        let (kind, target, body) = match synapsecore::relay::console::read(
            &typed,
            self.consolefocus.as_deref(),
        ) {
            Line::Blank => return,
            Line::Empty => {
                self.notice = Notice::Error("Nothing to send.".to_owned());
                cx.notify();
                return;
            }
            Line::Undirected => {
                self.notice = Notice::Error(synapsecore::relay::console::UNDIRECTED.to_owned());
                cx.notify();
                return;
            }
            // The console has no slash commands: everything they do in a
            // terminal is a button or a page here. Sending one as a message
            // would be worse than saying so.
            Line::Command(_) => {
                self.notice = Notice::Error(
                        "The console has no slash commands — use the roster, or `synapse mux` in a terminal.".to_owned(),
                    );
                cx.notify();
                return;
            }
            Line::Message { kind, target, body } => (kind, target, body),
        };

        let database = self.database.clone();
        let sent = block(async {
            let mesh = synapsecore::relay::Mesh::open(&database).await?;
            synapsecore::relay::deliver(&mesh, &me, kind, target.as_deref(), &body).await
        });
        self.notice = match sent {
            Ok(_) => {
                self.consoleinput
                    .update(cx, |input, cx| input.set_text("", cx));
                Notice::Ready
            }
            Err(error) => Notice::Error(format!("Could not send: {error:#}")),
        };
        self.refreshconsole(cx);
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

    /// What the settings page says about speech. The console's `Mic` is about
    /// what the button can do next; this is about what the build and the
    /// machine allow at all, which is a different question with its own answers.
    #[cfg(all(feature = "voice", target_os = "macos"))]
    fn voicestate(&self) -> settings::Voice {
        use crate::voice::Access;

        if !crate::voice::available() {
            return settings::Voice::Unsupported;
        }
        match crate::voice::access() {
            Access::Allowed => settings::Voice::Allowed,
            Access::Unknown => settings::Voice::Ask,
            Access::Refused => settings::Voice::Refused,
        }
    }

    #[cfg(not(all(feature = "voice", target_os = "macos")))]
    fn voicestate(&self) -> settings::Voice {
        settings::Voice::Absent
    }

    #[cfg(all(feature = "voice", target_os = "macos"))]
    fn askvoice(&mut self, cx: &mut Context<Self>) {
        crate::voice::ask();
        self.notice = Notice::Success(
            "macOS will ask. The answer shows here once you have given it.".to_owned(),
        );
        cx.notify();
    }

    #[cfg(not(all(feature = "voice", target_os = "macos")))]
    fn askvoice(&mut self, _cx: &mut Context<Self>) {}

    fn setreactor(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let database = self.database.clone();
        let result = block(async {
            let brain = synapsecore::brain::Brain::open(database).await?;
            brain.setreactor(enabled).await
        });
        self.notice = match result {
            Ok(()) => {
                self.reactorwanted = enabled;
                Notice::Success(match enabled {
                    true => "The console draws its reactor.".to_owned(),
                    false => "The console's reactor is off.".to_owned(),
                })
            }
            Err(error) => Notice::Error(format!("Could not change the reactor: {error}")),
        };
        cx.notify();
    }

    fn setworkers(&mut self, count: usize, cx: &mut Context<Self>) {
        let database = self.database.clone();
        let result = block(async {
            let brain = synapsecore::brain::Brain::open(database).await?;
            brain.setmaxworkers(count).await
        });
        self.notice = match result {
            Ok(()) => {
                self.consolelimit = count;
                Notice::Success(format!(
                    "One session may now run {count} background worker(s) at once."
                ))
            }
            Err(error) => Notice::Error(format!("Could not change the worker limit: {error}")),
        };
        cx.notify();
    }

    fn setlearn(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let database = self.database.clone();
        let result = block(async {
            let brain = synapsecore::brain::Brain::open(database).await?;
            brain.setlearn(enabled).await
        });
        match result {
            Ok(()) => {
                self.learnenabled = enabled;
                self.notice = Notice::Success(match enabled {
                    true => "Self-improvement on. A skill an agent writes waits on the Skills page until you approve it.".to_owned(),
                    false => "Self-improvement off. Skills already in the library stay where they are.".to_owned(),
                });
                self.refreshskills(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not change self-improvement: {error}"));
                cx.notify();
            }
        }
    }

    fn setmesh(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let database = self.database.clone();
        let result = block(async {
            let brain = synapsecore::brain::Brain::open(database).await?;
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
            synapsecore::instructions::ensure(&path)?;
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
            let brain = synapsecore::brain::Brain::open(database).await?;
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
            let backend = synapsecore::vault::backend().await?;
            Ok::<_, anyhow::Error>((vaults, selected, secrets, backend))
        }) {
            Ok((vaults, selected, secrets, backend)) => {
                self.vaults = vaults;
                self.selectedvault = selected;
                self.secrets = secrets;
                self.backend = backend;
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
            if let Err(error) = synapsecore::vault::setsecret(&secret.account, &value).await {
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

    /// Move every value into the other store and read from it afterwards.
    ///
    /// Two clicks, like every other move that is hard to watch happen. It is
    /// not destructive — values are copied and read back before the originals
    /// go — but it touches every credential on the machine at once, and that
    /// is worth being asked about.
    fn switchbackend(&mut self, cx: &mut Context<Self>) {
        let target = self.backend.other();
        if !self.pendingbackend {
            self.pendingbackend = true;
            self.notice = Notice::Error(format!(
                "Choose Confirm move to put every value in the {} vault.",
                target.name()
            ));
            cx.notify();
            return;
        }
        self.pendingbackend = false;
        match block(synapsecore::vault::migrate(target, false)) {
            Ok(migration) => {
                let moved = migration
                    .moved
                    .iter()
                    .filter(|(_, moved)| *moved == synapsecore::vault::Moved::Copied)
                    .count();
                self.notice = Notice::Success(format!(
                    "Moved {moved} value(s) into the {} vault.",
                    migration.target.name()
                ));
                self.refreshvaults(cx);
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not move the values: {error}"));
                cx.notify();
            }
        }
    }

    /// The value goes to the clipboard and nowhere else: it is not returned
    /// here, so it never reaches an element, a notice, or the crash log.
    fn copysecret(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(secret) = self.secrets.iter().find(|secret| secret.id == id) else {
            return;
        };
        self.notice = match block(synapsecore::vault::copysecret(&secret.account)) {
            Ok(()) => Notice::Success(format!(
                "Copied {}.{} to the clipboard.",
                secret.vault, secret.name
            )),
            Err(error) => Notice::Error(format!("Could not copy secret: {error}")),
        };
        cx.notify();
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
            self.notice = match block(synapsecore::vault::setsecret(&secret.account, &value)) {
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
        let result = block(async move {
            synapsecore::vault::deletesecret(&secret.account).await?;
            store.deletesecret(id).await.map(|_| ())
        });
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
        let path = folder.join(synapsecore::vault::CONFIG);
        if path.exists() {
            let target = path.canonicalize().unwrap_or(path);
            match block(synapsecore::vault::resolve(&store, &folder)) {
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
        let path = folder.join(synapsecore::vault::CONFIG);
        let result = if path.exists() {
            Ok(())
        } else {
            files::write(&path, synapsecore::vault::template())
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
        let path = folder.join(synapsecore::vault::CONFIG);
        let result = synapsecore::vault::readscope(&path)
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

    /// One card of tool rows.
    ///
    /// Called twice with two slices of the same vector: what is connected, then
    /// what could be. `offset` is where the slice starts, because the button
    /// ids are keyed on the row's position and two cards each numbering from
    /// zero would give the first row of each the same id.
    ///
    /// The row that adds a tool belongs to the lower card only — it is where
    /// you go to gain a connection, not to manage one.
    #[allow(clippy::too_many_arguments)]
    fn toolcard(
        &self,
        rows: Vec<Row>,
        offset: usize,
        heading: &'static str,
        border: gpui::Hsla,
        surface: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let supported = offset > 0 || rows.iter().all(|row| !row.connected());
        let count = rows.len();
        div()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(8.0))
                    .px(px(4.0))
                    .child(
                        Text::new(heading)
                            .size(Size::Sm)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .child(Text::new(count.to_string()).size(Size::Xs).dimmed()),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(border)
                    .bg(surface)
                    .overflow_hidden()
                    .when(count == 0, |element| {
                        element.child(
                            div().px(px(22.0)).py(px(18.0)).child(
                                Text::new(if supported {
                                    "Every tool this machine has is connected."
                                } else {
                                    "Nothing is connected yet. Set one up below."
                                })
                                .size(Size::Xs)
                                .dimmed(),
                            ),
                        )
                    })
                    .children(rows.into_iter().enumerate().flat_map(|(position, row)| {
                        let index = offset + position;
                        let slug = row.agent.slug.clone();
                        let set = Box::new(cx.listener(move |this, _, _, cx| {
                            this.setup(&slug, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let update = Box::new(cx.listener(move |this, _, _, cx| {
                            this.updatetool(&slug, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let reset = Box::new(cx.listener(move |this, _, _, cx| {
                            this.resettool(&slug, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let remove = Box::new(cx.listener(move |this, _, _, cx| {
                            this.removetool(&slug, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let instructions = Box::new(cx.listener(move |this, _, window, cx| {
                            this.openinstructions(&slug, window, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let settings = Box::new(cx.listener(move |this, _, window, cx| {
                            this.opensettings(&slug, window, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let notice = Box::new(cx.listener(move |this, _, _, cx| {
                            this.togglenotice(&slug, cx);
                        }));
                        let slug = row.agent.slug.clone();
                        let descriptor = Box::new(cx.listener(move |this, _, window, cx| {
                            this.opendescriptor(&slug, window, cx);
                        }));
                        let mut items = Vec::new();
                        if position > 0 {
                            items.push(div().h(px(1.0)).mx(px(22.0)).bg(border).into_any_element());
                        }
                        items.push(agentrow::render(
                            index,
                            row,
                            agentrow::Actions {
                                set,
                                update,
                                reset,
                                remove,
                                instructions,
                                settings,
                                notice,
                                descriptor,
                            },
                        ));
                        items
                    }))
                    // Past the end of the list. There will be more coding tools
                    // than Synapse ships descriptors for, and this is where a
                    // person says so.
                    .when(supported, |element| {
                        element
                            .when(count > 0, |element| {
                                element.child(div().h(px(1.0)).mx(px(22.0)).bg(border))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .justify_between()
                                    .gap(px(16.0))
                                    .px(px(22.0))
                                    .py(px(16.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .max_w(px(320.0))
                                            .child(self.toolname.clone()),
                                    )
                                    .child(
                                        Button::new("describetool", "Describe a tool")
                                            .variant(Variant::Light)
                                            .color(ColorName::Violet)
                                            .size(Size::Xs)
                                            .left_section(
                                                Icon::new(IconName::FileText).size(Size::Xs),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.describetool(window, cx);
                                            })),
                                    ),
                            )
                    }),
            )
            .into_any_element()
    }

    fn setup(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.slug == slug).cloned() else {
            return;
        };
        let result = connectionserver()
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synapse MCP executable"))
            .and_then(|server| {
                let soul = files::soul()?;
                let database = files::database()?;
                block(async {
                    agent::connect(&row.agent, &row.detection, &server, &soul, &database).await
                })
            });
        self.notice = match result {
            Ok(()) => Notice::Success(format!("{} is connected to Synapse.", row.agent.name)),
            Err(error) => Notice::Error(format!("Could not connect {}: {error}", row.agent.name)),
        };
        self.rows = loadrows();
        cx.notify();
    }

    /// Apply this release's descriptor to a tool that is already connected.
    ///
    /// The registration is written again rather than skipped: detection cannot
    /// tell a descriptor that moved from one that did not, so a tool set up
    /// under an older release would otherwise keep that release's answer for
    /// good. Nothing is removed, and the tool stays connected throughout.
    fn updatetool(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.slug == slug).cloned() else {
            return;
        };
        let result = connectionserver()
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synapse MCP executable"))
            .and_then(|server| {
                let soul = files::soul()?;
                let database = files::database()?;
                block(async {
                    agent::refresh(&row.agent, &row.detection, &server, &soul, &database).await
                })
            });
        self.notice = match result {
            Ok(()) => Notice::Success(format!(
                "Re-applied {}'s descriptor: registration rewritten, guidance pointer \
                 and startup notice refreshed. Nothing was removed.",
                row.agent.name
            )),
            Err(error) => Notice::Error(format!("Could not update {}: {error}", row.agent.name)),
        };
        self.rows = loadrows();
        cx.notify();
    }

    /// Take a connection out and make it again.
    ///
    /// Costs more than updating it: disconnecting removes the skills Synapse
    /// installed for this tool, and this puts the connection back rather than
    /// the library. It is here for the case updating cannot reach — an entry
    /// the tool's own CLI will not overwrite in place.
    fn resettool(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.slug == slug).cloned() else {
            return;
        };
        let result = connectionserver()
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synapse MCP executable"))
            .and_then(|server| {
                let soul = files::soul()?;
                let database = files::database()?;
                block(async { agent::reset(&row.agent, &server, &soul, &database).await })
            });
        self.notice = match result {
            // Connected again, but something would not come out on the way.
            // Reporting the success alone is how somebody ends up debugging a
            // hook nobody told them was left behind.
            Ok(removed) if !removed.problems.is_empty() => Notice::Error(format!(
                "{} was connected again, but {}",
                row.agent.name,
                removed.problems.join("; ")
            )),
            Ok(removed) => Notice::Success(format!(
                "{} was disconnected and connected again{}",
                row.agent.name,
                listing(&removed.done)
            )),
            Err(error) => Notice::Error(format!("Could not reset {}: {error}", row.agent.name)),
        };
        self.rows = loadrows();
        cx.notify();
    }

    /// Take a tool back out: its registration, the managed guidance block, the
    /// Claude Code notice, and the skills Synapse installed for it. A skill the
    /// user wrote, and their own words in an instruction file, stay where they
    /// are.
    fn removetool(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.slug == slug).cloned() else {
            return;
        };
        let result = connectionserver()
            .ok_or_else(|| anyhow::anyhow!("could not locate the Synapse MCP executable"))
            .and_then(|server| {
                let database = files::database()?;
                block(async { Ok(agent::remove(&row.agent, &server, &database).await) })
            });
        self.notice = match result {
            Ok(removed) if !removed.problems.is_empty() => Notice::Error(format!(
                "Disconnected {} with problems: {}",
                row.agent.name,
                removed.problems.join("; ")
            )),
            Ok(removed) if removed.done.is_empty() => Notice::Success(format!(
                "{} had nothing to disconnect — Synapse had written nothing into it.",
                row.agent.name
            )),
            Ok(removed) => Notice::Success(format!(
                "Disconnected {}{}",
                row.agent.name,
                listing(&removed.done)
            )),
            Err(error) => {
                Notice::Error(format!("Could not disconnect {}: {error}", row.agent.name))
            }
        };
        self.rows = loadrows();
        cx.notify();
    }

    /// Add or remove the startup notice for an already-connected tool, so
    /// gaining it never means disconnecting and connecting again. The tool's
    /// settings file is captured first and restored if the write fails.
    fn togglenotice(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(row) = self.rows.iter().find(|row| row.agent.slug == slug).cloned() else {
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

    fn openinstructions(&mut self, slug: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.opendocument(slug, instructionspath, "instructions", window, cx);
    }

    /// Open a tool's descriptor for editing. Editing one of the tools Synapse
    /// ships copies it into a layer the user owns first, so the shipped file
    /// stays as it was and can always be returned to by deleting the copy.
    fn opendescriptor(&mut self, slug: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .rows
            .iter()
            .find(|row| row.agent.slug == slug)
            .map_or_else(|| slug.to_owned(), |row| row.agent.name.clone());
        match agent::tool::draft(slug).and_then(|path| Self::loaddocument(name, path, cx)) {
            Ok(document) => {
                let focus = buffer::focus(&document.editor, cx);
                self.document = Some(document);
                window.focus(&focus);
                cx.notify();
            }
            Err(error) => {
                self.notice = Notice::Error(format!("Could not open that descriptor: {error:#}"));
                cx.notify();
            }
        }
    }

    /// Start describing a tool Synapse does not ship. The name becomes the
    /// descriptor's file name, so it is checked before anything is written —
    /// and the new file opens in the editor already filled with a template that
    /// explains each section.
    fn describetool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slug = self.toolname.read(cx).text().trim().to_lowercase();
        if let Err(error) = synapsecore::relay::validlayername(&slug) {
            self.notice = Notice::Error(format!("{error}"));
            cx.notify();
            return;
        }
        if self.rows.iter().any(|row| row.agent.slug == slug) {
            self.notice = Notice::Error(format!("`{slug}` is already a tool on this machine."));
            cx.notify();
            return;
        }
        self.toolname.update(cx, |input, cx| input.set_text("", cx));
        self.opendescriptor(&slug, window, cx);
        self.rows = loadrows();
    }

    fn opensettings(&mut self, slug: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.opendocument(slug, settingspath, "configuration", window, cx);
    }

    fn opendocument(
        &mut self,
        slug: &str,
        select: fn(&agent::Agent) -> PathBuf,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.rows.iter().find(|row| row.agent.slug == slug) else {
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
                == Some(synapsecore::vault::CONFIG);
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
    /// The window: the column of destinations, and whatever the open one drew.
    ///
    /// The wrapping happens here, once, rather than in each of the seven
    /// branches below — every one of them returns early, and a frame assembled
    /// seven times is a frame that is subtly different in one of them.
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The document editor takes the whole window: it is a mode, not a page,
        // and a sidebar beside unsaved edits invites leaving them.
        let framed = self.document.is_none();
        let body = self.body(cx);
        let navigation = self.navigation(cx);
        let page = self.page;
        let appmenu = self.appmenu.clone();
        let theme = guise::theme(cx);
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(theme.body().hsla())
            .text_color(theme.text().hsla())
            .when(framed, |element| {
                element.child(sidebar::render(page, appmenu, navigation, cx))
            })
            .child(body)
    }
}

impl Dashboard {
    fn body(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = guise::theme(cx);
        let border = theme.border().hsla();
        let surface = theme.surface().hsla();
        let rows = self.rows.clone();
        let connected = connectedcount(&rows);

        if let Some(document) = self.document.clone() {
            let save = Box::new(cx.listener(|this, _, _, cx| this.savedocument(cx)));
            let close = Box::new(cx.listener(|this, _, _, cx| this.closedocument(cx)));
            let discard = Box::new(cx.listener(|this, _, _, cx| this.discarddocument(cx)));
            // No sidebar: the editor is a mode, and `render` already leaves the
            // column off for it. One here as well put a full-height sidebar at
            // the top of a vertical stack and left the editor no room at all.
            return div()
                .size_full()
                .flex()
                .flex_col()
                .bg(theme.body().hsla())
                .text_color(theme.text().hsla())
                .on_action(cx.listener(|this, _: &SaveDocument, _window, cx| this.savedocument(cx)))
                .child(document::render(document, save, close, discard, cx))
                .into_any_element();
        }

        let banner = self.showclibanner.then(|| {
            let path = synapsecore::cli::destination()
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

        // The window is a row now: the column of destinations, then everything
        // that belongs to whichever one is open. Every page below still builds
        // its own body and status bar, so they go inside the second half rather
        // than beside the first.
        let shell = div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .flex()
            .flex_col()
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

        if self.page == Page::Console {
            let (life, pulse) = self.consolemotion();
            let aimhost = cx.entity().downgrade();
            let aim = move |name: String| -> console::Click {
                let host = aimhost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.focusagent(name.clone(), cx))
                        .ok();
                })
            };
            return shell
                .child(console::render(
                    console::View {
                        identity: self.consoleidentity.clone(),
                        focus: self.consolefocus.clone(),
                        feed: self.meshfeed.clone(),
                        agents: self.meshagents.clone(),
                        workers: self.meshworkers.clone(),
                        limit: self.consolelimit,
                        life,
                        pulse,
                        // A build with no reactor has none to draw whatever the
                        // setting says, so the two are `and`ed rather than the
                        // setting winning over a missing dependency.
                        reactor: self.reactorwanted && cfg!(feature = "reactor"),
                        composer: self.consoleinput.clone(),
                        mic: self.micstate(),
                        message: match &self.notice {
                            Notice::Ready => None,
                            Notice::Success(message) => Some((message.clone(), false)),
                            Notice::Error(message) => Some((message.clone(), true)),
                        },
                    },
                    console::Actions {
                        send: Box::new(cx.listener(|this, _, _, cx| this.sendconsole(cx))),
                        refresh: Box::new(cx.listener(|this, _, _, cx| this.refreshconsole(cx))),
                        dictate: Box::new(cx.listener(|this, _, _, cx| this.dictate(cx))),
                        focus: Box::new(aim),
                    },
                    cx,
                ))
                .child(
                    StatusBar::new()
                        .height(36.0)
                        .left(
                            Text::new(match &self.consoleidentity {
                                Ok(name) => format!("On the mesh as {name}"),
                                Err(_) => "Not on the mesh".to_owned(),
                            })
                            .size(Size::Xs),
                        )
                        .right(Text::new("@name · #channel · ! for everyone").size(Size::Xs)),
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
            let approvehost = cx.entity().downgrade();
            let approve = move |name: String, project: String| -> skills::Click {
                let host = approvehost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| {
                        this.approveskill(name.clone(), project.clone(), cx)
                    })
                    .ok();
                })
            };
            let rejecthost = cx.entity().downgrade();
            let reject = move |name: String, project: String| -> skills::Click {
                let host = rejecthost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| {
                        this.rejectskill(name.clone(), project.clone(), cx)
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
                        folder: synapsecore::skill::library::directory()
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
                        approve: Box::new(approve),
                        reject: Box::new(reject),
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
            let clistatus =
                synapsecore::cli::status().unwrap_or(synapsecore::cli::InstallStatus::Missing);
            let clipath = synapsecore::cli::destination()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| error.to_string());
            let (shellintegration, shellerror) = match synapsecore::cli::destination()
                .and_then(|command| synapsecore::shellsetup::status(&command))
            {
                Ok(integration) => (Some(integration), None),
                Err(error) => (None, Some(error.to_string())),
            };
            return shell
                .child(settings::render(
                    settings::View {
                        optimization: self.optimization,
                        mesh: self.meshenabled,
                        learn: self.learnenabled,
                        workers: self.consolelimit,
                        reactor: self.reactorwanted,
                        reactorbuilt: cfg!(feature = "reactor"),
                        voice: self.voicestate(),
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
                        reactoron: Box::new(
                            cx.listener(|this, _, _, cx| this.setreactor(true, cx)),
                        ),
                        reactoroff: Box::new(
                            cx.listener(|this, _, _, cx| this.setreactor(false, cx)),
                        ),
                        askvoice: Box::new(cx.listener(|this, _, _, cx| this.askvoice(cx))),
                        setworkers: {
                            let host = cx.entity().downgrade();
                            Box::new(move |count: usize| {
                                let host = host.clone();
                                Box::new(move |_, _, cx| {
                                    host.update(cx, |this, cx| this.setworkers(count, cx)).ok();
                                })
                            })
                        },
                        learnon: Box::new(cx.listener(|this, _, _, cx| this.setlearn(true, cx))),
                        learnoff: Box::new(cx.listener(|this, _, _, cx| this.setlearn(false, cx))),
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
            let backendhost = cx.entity().downgrade();
            let switchbackend: vaults::Click = Box::new(move |_, _, cx| {
                backendhost
                    .update(cx, |this, cx| this.switchbackend(cx))
                    .ok();
            });
            let copyhost = cx.entity().downgrade();
            let copysecret = move |id| -> vaults::Click {
                let host = copyhost.clone();
                Box::new(move |_, _, cx| {
                    host.update(cx, |this, cx| this.copysecret(id, cx)).ok();
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
                        backend: self.backend,
                        pendingbackend: self.pendingbackend,
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
                        switchbackend,
                        copysecret: Box::new(copysecret),
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
                // gpui measures a run of text at its full unwrapped width, so
                // every box between here and a label has to be told it may be
                // narrower than its contents. Without that the column silently
                // widens to the longest line on the page and the controls on
                // the right of each row go off the edge of the window.
                div()
                    .id("synapsemain")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
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
                            .child(self.toolcard(
                                rows[..connected].to_vec(),
                                0,
                                "Connected",
                                border,
                                surface,
                                cx,
                            ))
                            .child(self.toolcard(
                                rows[connected..].to_vec(),
                                connected,
                                "Supported",
                                border,
                                surface,
                                cx,
                            )),
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

/// What a teardown actually took out, as prose.
///
/// The count on its own ("4 thing(s) removed") is the shape of the information
/// without any of it: disconnecting a tool removes its registration, a block
/// from its instruction file, sometimes two entries from its settings, and
/// however many skills Synapse had installed into it. Which of those happened
/// is the whole question somebody has after pressing the button, and the
/// teardown already knows the answer — it was being thrown away.
fn listing(done: &[String]) -> String {
    match done {
        [] => ".".to_owned(),
        items => format!(" · removed {}.", items.join(", ").to_lowercase()),
    }
}

fn loadrows() -> Vec<Row> {
    let server = connectionserver();
    let Ok(home) = files::home() else {
        return Vec::new();
    };
    let Ok(database) = files::database() else {
        return Vec::new();
    };
    block(async { Ok(agent::connections(&home, server.as_deref(), &database).await) })
        .unwrap_or_default()
}

/// Where the connected half of the list ends. The rows arrive sorted with
/// connected tools first, so this is the only number the two cards need.
fn connectedcount(rows: &[Row]) -> usize {
    rows.iter().filter(|row| row.connected()).count()
}

fn connectionserver() -> Option<PathBuf> {
    match synapsecore::cli::status() {
        Ok(synapsecore::cli::InstallStatus::Installed(path)) => Some(path),
        _ => std::env::current_exe().ok(),
    }
}

fn initialpage() -> Page {
    match std::env::var("SYNAPSE_PAGE").as_deref() {
        Ok("memory") => Page::Memories,
        Ok("mesh") => Page::Mesh,
        Ok("console") => Page::Console,
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
        let brain = synapsecore::brain::Brain::open(database).await?;
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

/// The skill a page row names. The row carries the project rather than the
/// shelf, because a project root is the whole of what identifies one.
fn locateskill(name: &str, project: &str) -> anyhow::Result<synapsecore::skill::Skill> {
    let shelf = match project.is_empty() {
        true => synapsecore::skill::Shelf::Global,
        false => synapsecore::skill::Shelf::project(std::path::Path::new(project)),
    };
    synapsecore::skill::library::read(&shelf, name)
}

fn loadskills() -> SkillData {
    let home = files::home().unwrap_or_else(|_| PathBuf::from("."));
    let (statuses, mut problems) = match block(synapsecore::skill::survey(&home)) {
        Ok(surveyed) => surveyed,
        Err(error) => (
            Vec::new(),
            vec![format!("the library could not be read: {error}")],
        ),
    };
    let (library, listing) = synapsecore::skill::library::all().unwrap_or_default();
    problems.extend(listing);
    let known: Vec<String> = library.iter().map(|skill| skill.name.clone()).collect();
    let waiting = block(async {
        let receipts =
            synapsecore::skill::Receipts::glance(synapsecore::files::database()?).await?;
        receipts.proposals().await
    })
    .unwrap_or_default();
    SkillData {
        rows: library
            .into_iter()
            .map(|skill| {
                // Matched on the shelf as well as the name. The same name can
                // be a global skill and a project's — and two projects' — so a
                // match on the name and the word `project` would report one
                // repository's copy under another's heading.
                let shelf = skill.shelf.key();
                let root = skill.shelf.root().unwrap_or_default();
                let proposal = waiting
                    .iter()
                    .find(|item| item.skill == skill.name && item.shelf == shelf);
                skills::Row {
                    places: statuses
                        .iter()
                        .filter(|status| status.skill == skill.name && status.project == root)
                        .cloned()
                        .collect(),
                    name: skill.name,
                    description: skill.description,
                    files: skill.files.len(),
                    scope: skill.shelf.label().to_owned(),
                    project: skill.shelf.root().unwrap_or_default().to_owned(),
                    proposed: proposal.is_some(),
                    note: proposal.map(|item| item.note.clone()).unwrap_or_default(),
                }
            })
            .collect(),
        unmanaged: agent::agents(&home)
            .into_iter()
            .flat_map(|agent| {
                synapsecore::skill::unknown(&agent, &known)
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
    agents: Vec<synapsecore::relay::AgentView>,
    workers: Vec<synapsecore::relay::WorkerView>,
    feed: Vec<synapsecore::relay::Message>,
    error: Option<String>,
}

fn loadmeshdata(database: &std::path::Path) -> MeshData {
    let enabled = loadmesh(database).unwrap_or(false);
    if !enabled {
        return MeshData::default();
    }
    let path = database.to_path_buf();
    match block(async {
        let mesh = synapsecore::relay::Mesh::open(path).await?;
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
        let brain = synapsecore::brain::Brain::open(database).await?;
        brain.mesh().await
    })
}

fn loadreactor(database: &std::path::Path) -> anyhow::Result<bool> {
    block(async {
        let brain = synapsecore::brain::Brain::open(database).await?;
        brain.reactor().await
    })
}

fn loadworkers(database: &std::path::Path) -> anyhow::Result<usize> {
    block(async {
        let brain = synapsecore::brain::Brain::open(database).await?;
        brain.maxworkers().await
    })
}

fn loadlearn(database: &std::path::Path) -> anyhow::Result<bool> {
    block(async {
        let brain = synapsecore::brain::Brain::open(database).await?;
        brain.learn().await
    })
}

fn loadoptimization(database: &std::path::Path) -> anyhow::Result<Optimization> {
    block(async {
        let brain = synapsecore::brain::Brain::open(database).await?;
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
            backend: synapsecore::vault::backend().await?,
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
        let summary = match synapsecore::imports::scan(home, provider).await {
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
fn promptforcli(status: &synapsecore::cli::InstallStatus, dismissed: bool) -> bool {
    matches!(status, synapsecore::cli::InstallStatus::Missing) && !dismissed
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
    use synapsecore::cli::InstallStatus;

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
