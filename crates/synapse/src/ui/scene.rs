mod art;
mod view;

use crate::ui::runtime::block;
use gpui::{Context, RenderImage};
use std::{collections::HashMap, sync::Arc, time::Duration};
use synapsecore::{
    brain::Brain,
    plugins::{self, Plugin},
    presence::{self, Session, State},
};

pub struct Scene {
    brain: Option<Brain>,
    pub active: bool,
    plugins: Vec<Plugin>,
    problems: Vec<String>,
    selectedplugin: usize,
    art: art::Art,
    sessions: Vec<Session>,
    selected: Option<String>,
    project: Option<String>,
    paused: bool,
    tick: u64,
    loading: bool,
    avatarrevision: u64,
    error: Option<String>,
    presenceerror: Option<String>,
    cache: HashMap<(usize, u8, u64, bool), Arc<RenderImage>>,
}

impl Scene {
    pub fn new(brain: Option<Brain>, cx: &mut Context<Self>) -> Self {
        let catalog = plugins::folder()
            .map(|path| plugins::catalog(&path))
            .unwrap_or_default();
        let mut plugins = catalog.plugins;
        if plugins.is_empty() {
            plugins.push(plugins::builtin());
        }
        let selectedid = brain
            .as_ref()
            .and_then(|brain| block(brain.preference("scene.plugin")).ok().flatten());
        let selectedplugin = plugins
            .iter()
            .position(|plugin| Some(&plugin.manifest.id) == selectedid.as_ref())
            .unwrap_or(0);
        let paused = brain
            .as_ref()
            .and_then(|brain| block(brain.preference("scene.paused")).ok().flatten())
            .map(|value| value == "true")
            .unwrap_or_else(reducedmotion);
        let mut problems = catalog.problems;
        if brain.is_none() {
            problems.push("Synapse could not open its store. Restart the app after resolving the database error.".into());
        }
        let art = match art::load(&plugins[selectedplugin]) {
            Ok(art) => art,
            Err(error) => {
                problems.push(format!("Could not load artwork: {error}"));
                art::Art {
                    characters: Vec::new(),
                    background: None,
                }
            }
        };
        cx.spawn(async move |this, cx| {
            let mut pulse = 0u64;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let Ok(read) = this.update(cx, |this, cx| {
                    if !this.active {
                        pulse = 0;
                        return None;
                    }
                    if !this.paused {
                        this.tick = this.tick.wrapping_add(1);
                        cx.notify();
                    }
                    pulse = pulse.wrapping_add(1);
                    if pulse % 10 != 1 || this.loading || this.brain.is_none() {
                        return None;
                    }
                    this.loading = true;
                    this.brain.clone().map(|brain| (brain, this.avatarrevision))
                }) else {
                    break;
                };
                if let Some((brain, revision)) = read {
                    let result = cx
                        .background_executor()
                        .spawn(async move { block(presence::sessions(brain.pool())) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.loading = false;
                        match result {
                            Ok(mut sessions) => {
                                if revision != this.avatarrevision {
                                    for session in &mut sessions {
                                        if let Some(current) =
                                            this.sessions.iter().find(|current| {
                                                current.identity() == session.identity()
                                            })
                                        {
                                            session.appearance = current.appearance;
                                        }
                                    }
                                }
                                this.sessions = sessions;
                                this.presenceerror = None;
                            }
                            Err(error) => {
                                this.presenceerror =
                                    Some(format!("Could not read connected sessions: {error}"))
                            }
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
        Self {
            brain,
            active: false,
            plugins,
            problems,
            selectedplugin,
            art,
            sessions: Vec::new(),
            selected: None,
            project: None,
            paused,
            tick: 0,
            loading: false,
            avatarrevision: 0,
            presenceerror: None,
            error: None,
            cache: HashMap::new(),
        }
    }

    fn save(&mut self, key: &'static str, value: String, _cx: &mut Context<Self>) {
        match &self.brain {
            Some(brain) => {
                if let Err(error) = block(brain.setpreference(key, &value)) {
                    self.error = Some(format!("Could not save scene settings: {error}"));
                }
            }
            None => self.error = Some("Synapse could not open its store.".into()),
        }
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        match art::load(&self.plugins[index]) {
            Ok(art) => {
                self.art = art;
                self.selectedplugin = index;
                self.cache.clear();
                self.error = None;
                self.save("scene.plugin", self.plugins[index].manifest.id.clone(), cx);
            }
            Err(error) => self.error = Some(format!("Could not load scene: {error}")),
        }
        cx.notify();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let id = self.plugins[self.selectedplugin].manifest.id.clone();
        match plugins::folder() {
            Ok(root) => {
                let catalog = plugins::catalog(&root);
                self.plugins = catalog.plugins;
                self.problems = catalog.problems;
                let index = self
                    .plugins
                    .iter()
                    .position(|plugin| plugin.manifest.id == id)
                    .unwrap_or(0);
                self.choose(index, cx);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn changeappearance(&mut self, delta: i64, cx: &mut Context<Self>) {
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| Some(&session.id) == self.selected.as_ref())
        else {
            return;
        };
        let Some(brain) = &self.brain else {
            return;
        };
        let appearance = (session.appearance + delta).rem_euclid(32);
        match block(presence::appearance(
            brain.pool(),
            &session.identity(),
            appearance,
        )) {
            Ok(()) => {
                session.appearance = appearance;
                self.avatarrevision = self.avatarrevision.wrapping_add(1);
                self.error = None;
            }
            Err(error) => self.error = Some(format!("Could not save character: {error}")),
        }
        cx.notify();
    }

    fn picture(&mut self, appearance: usize, state: State, inhabited: bool) -> Arc<RenderImage> {
        let frame = if self.paused || state == State::Disconnected {
            0
        } else {
            self.tick % 4
        };
        let key = (appearance, state as u8, frame, inhabited);
        if let Some(image) = self.cache.get(&key) {
            return image.clone();
        }
        if self.cache.len() >= 128 {
            self.cache.clear();
        }
        self.cache
            .entry(key)
            .or_insert_with(|| {
                art::room(
                    &self.plugins[self.selectedplugin],
                    &self.art,
                    appearance,
                    state,
                    frame,
                    inhabited,
                )
            })
            .clone()
    }
}

#[cfg(target_os = "macos")]
fn reducedmotion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(not(target_os = "macos"))]
fn reducedmotion() -> bool {
    false
}
