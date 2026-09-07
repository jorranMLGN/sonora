use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use gpui::{Context, Entity, EventEmitter, Task};
use music::{
    MusicApi, MusicProvider, PlaybackFactory, PromptSink, ProviderSession, SignIn, SignInFailure,
    SignInProblem, SignInPrompt, UserProfile,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::catalog::CatalogSource;
use crate::settings::AppSettings;
use crate::{Io, join};

const HEARTBEAT: Duration = Duration::from_secs(30);
const BACKOFF: [Duration; 5] = [
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(60),
    Duration::from_secs(300),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub problem: Option<SignInProblem>,
    pub summary: String,
    pub detail: Option<String>,
}

impl Failure {
    fn new(error: &Error) -> Self {
        let problem = error
            .downcast_ref::<SignInFailure>()
            .map(|failure| failure.0);
        let detail = error
            .chain()
            .skip(1)
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        Self {
            problem,
            summary: error.to_string(),
            detail: (!detail.is_empty()).then(|| detail.join(": ")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    SignedOut,
    Restoring,
    Authorizing(Option<SignInPrompt>),
    SignedIn,
    Failed(Failure),
}

pub enum SessionEvent {
    SignedIn(&'static str),
    SignedOut(&'static str),
    Reconnected(&'static str),
}

pub struct ProviderInfo {
    pub slug: &'static str,
    pub name: &'static str,
    pub options: Vec<SignIn>,
    pub stored: bool,
    pub active: bool,
    pub pending: bool,
    pub error: Option<Failure>,
}

pub struct Connected {
    pub client: Arc<dyn MusicApi>,
    pub(crate) catalog: Arc<CatalogSource>,
    pub playback: Arc<dyn PlaybackFactory>,
    pub profile: UserProfile,
    pub authenticated: bool,
}

pub struct Session {
    state: SessionState,
    providers: Vec<Arc<dyn MusicProvider>>,
    active: Option<usize>,
    awaiting: Option<usize>,
    resume: Option<(usize, UserProfile)>,
    error: Option<(usize, Failure)>,
    settings: Entity<AppSettings>,
    connected: HashMap<&'static str, Connected>,
    playcounts: bool,
    io: Io,
    tasks: HashMap<&'static str, Task<()>>,
    prompt_task: Option<Task<()>>,
    input: Option<UnboundedSender<String>>,
    local_provider: Arc<dyn MusicProvider>,
    watch: Option<Task<()>>,
    reconnect: Option<Task<()>>,
    reconnecting: bool,
    attempt: usize,
}

impl EventEmitter<SessionEvent> for Session {}

impl Session {
    pub fn new(
        providers: Vec<Arc<dyn MusicProvider>>,
        local_provider: Arc<dyn MusicProvider>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        let remembered = settings.read(cx).provider().to_string();
        let active = providers
            .iter()
            .position(|provider| provider.slug() == remembered);
        let mut session = Self {
            state: SessionState::SignedOut,
            providers,
            active,
            awaiting: None,
            resume: None,
            error: None,
            settings,
            connected: HashMap::new(),
            playcounts: false,
            io,
            tasks: HashMap::new(),
            prompt_task: None,
            input: None,
            local_provider,
            watch: None,
            reconnect: None,
            reconnecting: false,
            attempt: 0,
        };
        session.restore_local(cx);
        session
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn connected(&self, slug: &str) -> Option<&Connected> {
        self.connected.get(slug)
    }

    pub fn client_for_slug(&self, slug: &str) -> Option<Arc<dyn MusicApi>> {
        self.connected.get(slug).map(|entry| entry.client.clone())
    }

    pub fn client(&self) -> Option<Arc<dyn MusicApi>> {
        self.client_for_slug(self.provider_slug()?)
    }

    pub fn playback(&self) -> Option<Arc<dyn PlaybackFactory>> {
        self.playback_for_slug(self.provider_slug()?)
    }

    pub fn playback_for_slug(&self, slug: &str) -> Option<Arc<dyn PlaybackFactory>> {
        self.connected.get(slug).map(|entry| entry.playback.clone())
    }

    pub(crate) fn catalog(&self, id: &str) -> Option<Arc<CatalogSource>> {
        let slug = self.slug_for(id)?;
        self.connected.get(slug).map(|entry| entry.catalog.clone())
    }

    pub fn local_path(&self) -> Option<String> {
        self.local_provider.location()
    }

    pub fn providers(&self) -> impl Iterator<Item = ProviderInfo> + '_ {
        self.providers
            .iter()
            .enumerate()
            .map(|(index, provider)| ProviderInfo {
                slug: provider.slug(),
                name: provider.name(),
                options: provider.sign_in_options(),
                stored: provider.stored(),
                active: self.active == Some(index),
                pending: self.awaiting == Some(index),
                error: match &self.error {
                    Some((failed, failure)) if *failed == index => Some(failure.clone()),
                    _ => None,
                },
            })
    }

    pub fn forget(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(index) = self
            .providers
            .iter()
            .position(|provider| provider.slug() == slug)
        else {
            return;
        };
        if self.active == Some(index) {
            return self.sign_out(cx);
        }
        let slug = self.providers[index].slug();
        self.providers[index].sign_out();
        self.tasks.remove(slug);
        let dropped = self.connected.remove(slug).is_some();
        cx.notify();
        if dropped {
            cx.emit(SessionEvent::SignedOut(slug));
        }
    }

    pub fn switch(&mut self, slug: &str, cx: &mut Context<Self>) {
        if self.is_pending() {
            return;
        }
        let Some(index) = self
            .providers
            .iter()
            .position(|provider| provider.slug() == slug)
        else {
            return;
        };
        if self.active == Some(index) && matches!(self.state, SessionState::SignedIn) {
            return;
        }
        self.release(cx);
        self.active = Some(index);
        self.restore(cx);
    }

    pub fn provider_name(&self) -> Option<&'static str> {
        let provider = &self.providers[self.active?];
        Some(provider.name())
    }

    pub fn provider_name_for(&self, slug: &str) -> Option<&'static str> {
        self.providers
            .iter()
            .find(|provider| provider.slug() == slug)
            .map(|provider| provider.name())
    }

    pub fn profile(&self) -> Option<&UserProfile> {
        self.profile_for(self.provider_slug()?)
    }

    pub fn profile_for(&self, slug: &str) -> Option<&UserProfile> {
        self.connected.get(slug).map(|entry| &entry.profile)
    }

    pub fn provider_slug(&self) -> Option<&'static str> {
        let provider = &self.providers[self.active?];
        Some(provider.slug())
    }

    pub fn active_slugs(&self) -> Vec<&'static str> {
        let mut slugs: Vec<&'static str> = self.connected.keys().copied().collect();
        slugs.sort_unstable_by_key(|slug| self.order_of(slug));
        slugs
    }

    pub fn slug_for(&self, id: &str) -> Option<&'static str> {
        let slug = music::tag::slug_of(id)?;
        self.connected.keys().copied().find(|known| *known == slug)
    }

    fn order_of(&self, slug: &str) -> usize {
        self.providers
            .iter()
            .position(|provider| provider.slug() == slug)
            .unwrap_or(usize::MAX)
    }

    pub fn authenticated(&self) -> bool {
        self.provider_slug()
            .and_then(|slug| self.connected.get(slug))
            .is_some_and(|entry| entry.authenticated)
    }

    pub fn playcounts(&self) -> bool {
        self.playcounts
    }

    pub fn is_pending(&self) -> bool {
        matches!(
            self.state,
            SessionState::Restoring | SessionState::Authorizing(_)
        )
    }

    pub fn restore(&mut self, cx: &mut Context<Self>) {
        if self.is_pending() {
            return;
        }
        if self.active.is_none() {
            self.active = self
                .remaining()
                .or((!self.providers.is_empty()).then_some(0));
        }
        let Some(active) = self.active else {
            self.state = SessionState::SignedOut;
            cx.notify();
            return;
        };
        let others: Vec<usize> = (0..self.providers.len())
            .filter(|index| *index != active && self.providers[*index].stored())
            .collect();
        for index in others {
            self.rejoin(index, cx);
        }
        self.state = SessionState::Restoring;
        cx.notify();

        let slug = self.providers[active].slug();
        let provider = self.providers[active].clone();
        let io = self.io.clone();
        let task = cx.spawn(async move |this, cx| {
            let restored = join(io.spawn(async move { provider.restore().await })).await;

            this.update(cx, |this, cx| match restored {
                Ok(Some(session)) => this.signed_in(session, active, cx),
                Ok(None) => {
                    this.state = SessionState::SignedOut;
                    cx.notify();
                    cx.emit(SessionEvent::SignedOut(slug));
                }
                Err(error) => this.failed(&error, cx),
            })
            .ok();
        });
        self.tasks.insert(slug, task);
    }

    fn rejoin(&mut self, index: usize, cx: &mut Context<Self>) {
        let slug = self.providers[index].slug();
        if self.connected.contains_key(slug) {
            return;
        }
        let provider = self.providers[index].clone();
        let io = self.io.clone();
        let task = cx.spawn(async move |this, cx| {
            let restored = join(io.spawn(async move { provider.restore().await })).await;

            this.update(cx, |this, cx| match restored {
                Ok(Some(session)) => this.joined(slug, session, cx),
                Ok(None) => log::warn!("session: nothing stored for {slug}"),
                Err(error) => log::warn!("session: cannot restore {slug}: {error:#}"),
            })
            .ok();
        });
        self.tasks.insert(slug, task);
    }

    fn joined(&mut self, slug: &'static str, session: ProviderSession, cx: &mut Context<Self>) {
        self.connect(slug, session);
        cx.notify();
        cx.emit(SessionEvent::SignedIn(slug));
    }

    pub fn sign_in(&mut self, slug: &str, method: SignIn, cx: &mut Context<Self>) {
        if self.is_pending() {
            return;
        }
        let Some(index) = self
            .providers
            .iter()
            .position(|provider| provider.slug() == slug)
        else {
            return;
        };
        self.resume = match self.state {
            SessionState::SignedIn => self.active.and_then(|active| {
                let held = self.providers[active].slug();
                let profile = self.connected.get(held)?.profile.clone();
                Some((active, profile))
            }),
            _ => None,
        };
        self.error = None;
        self.awaiting = Some(index);
        self.state = SessionState::Authorizing(None);
        cx.notify();

        let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        self.input = Some(input_tx);
        let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<SignInPrompt>();
        self.prompt_task = Some(cx.spawn(async move |this, cx| {
            while let Some(prompt) = prompt_rx.recv().await {
                this.update(cx, |this, cx| {
                    if matches!(this.state, SessionState::Authorizing(_)) {
                        this.state = SessionState::Authorizing(Some(prompt));
                        cx.notify();
                    }
                })
                .ok();
            }
        }));
        let prompt: PromptSink = Arc::new(move |prompt| {
            prompt_tx.send(prompt).ok();
        });

        let awaited = self.providers[index].slug();
        let provider = self.providers[index].clone();
        let io = self.io.clone();
        let task = cx.spawn(async move |this, cx| {
            let authorized =
                join(io.spawn(async move { provider.sign_in(method, prompt, input_rx).await }))
                    .await;

            this.update(cx, |this, cx| {
                this.prompt_task = None;
                this.input = None;
                match authorized {
                    Ok(session) => this.signed_in(session, index, cx),
                    Err(error) => this.failed(&error, cx),
                }
            })
            .ok();
        });
        self.tasks.insert(awaited, task);
    }

    pub fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.state, SessionState::Authorizing(_)) {
            return;
        }
        let abandoned = self.awaiting.take();
        if let Some(index) = abandoned {
            self.tasks.remove(self.providers[index].slug());
            let provider = self.providers[index].clone();
            self.io.spawn(async move { provider.abandon() });
        }
        self.prompt_task = None;
        self.input = None;
        self.error = None;
        if let Some((index, _)) = self.resume.take() {
            self.active = Some(index);
            self.state = SessionState::SignedIn;
            cx.notify();
            return;
        }
        self.state = SessionState::SignedOut;
        cx.notify();
        if let Some(index) = abandoned.or(self.active) {
            cx.emit(SessionEvent::SignedOut(self.providers[index].slug()));
        }
    }

    pub fn submit_input(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(input) = &self.input {
            input.send(text).ok();
            if let SessionState::Authorizing(Some(
                SignInPrompt::Secret | SignInPrompt::Accounts(_),
            )) = &self.state
            {
                self.state = SessionState::Authorizing(None);
                cx.notify();
            }
        }
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = self.active {
            self.providers[active].sign_out();
        }
        self.release(cx);
        if let Some(index) = self.remaining() {
            self.active = Some(index);
            self.restore(cx);
        }
    }

    fn remaining(&self) -> Option<usize> {
        self.active
            .filter(|index| self.providers[*index].stored())
            .or_else(|| self.providers.iter().position(|provider| provider.stored()))
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        self.watch = None;
        self.reconnect = None;
        self.reconnecting = false;
        self.attempt = 0;
        self.prompt_task = None;
        self.input = None;
        if let Some(index) = self.awaiting.take() {
            self.tasks.remove(self.providers[index].slug());
        }
        self.resume = None;
        let released = self.provider_slug();
        if let Some(slug) = released {
            self.tasks.remove(slug);
            self.connected.remove(slug);
        }
        self.playcounts = false;
        self.state = SessionState::SignedOut;
        cx.notify();
        if let Some(slug) = released {
            cx.emit(SessionEvent::SignedOut(slug));
        }
    }

    fn connect(&mut self, slug: &'static str, mut session: ProviderSession) {
        session.profile.id = music::tag::tag(slug, &session.profile.id);
        let client = music::tagged::new(slug, session.api);
        self.connected.insert(
            slug,
            Connected {
                catalog: Arc::new(CatalogSource::new(client.clone())),
                client,
                playback: session.playback,
                profile: session.profile,
                authenticated: session.authenticated,
            },
        );
    }

    fn signed_in(&mut self, mut session: ProviderSession, index: usize, cx: &mut Context<Self>) {
        let slug = self.providers[index].slug();
        session.profile.id = music::tag::tag(slug, &session.profile.id);
        let replaced = self
            .resume
            .take()
            .filter(|(held, profile)| *held != index || profile.id != session.profile.id);
        if let Some((held, _)) = replaced {
            self.drop_previous_session(held, cx);
        }
        self.active = Some(index);
        self.awaiting = None;
        self.error = None;
        self.settings.update(cx, |settings, cx| {
            settings.set_provider(slug, cx);
        });
        self.playcounts = session.playcounts;
        self.connect(slug, session);
        self.state = SessionState::SignedIn;
        self.attempt = 0;
        self.start_heartbeat(cx);
        cx.notify();
        cx.emit(SessionEvent::SignedIn(slug));
    }

    fn drop_previous_session(&mut self, index: usize, cx: &mut Context<Self>) {
        let slug = self.providers[index].slug();
        self.connected.remove(slug);
        self.playcounts = false;
        self.watch = None;
        self.reconnect = None;
        self.reconnecting = false;
        self.attempt = 0;
        cx.emit(SessionEvent::SignedOut(slug));
    }

    fn start_heartbeat(&mut self, cx: &mut Context<Self>) {
        self.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(HEARTBEAT).await;
                if this
                    .update(cx, |this, cx| this.reconnect_if_stale(cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    pub fn reconnect_if_stale(&mut self, cx: &mut Context<Self>) -> bool {
        if self.reconnecting {
            return true;
        }
        if !matches!(self.state, SessionState::SignedIn) {
            return false;
        }
        let Some(active) = self.active else {
            return false;
        };
        let slug = self.providers[active].slug();
        let Some(entry) = self.connected.get(slug) else {
            return false;
        };
        if entry.client.alive() {
            self.attempt = 0;
            return false;
        }
        let wait = BACKOFF[self.attempt.min(BACKOFF.len() - 1)];
        self.attempt += 1;
        self.reconnecting = true;
        log::warn!(
            "session: the {} session went stale, reconnecting in {}s",
            self.providers[active].name(),
            wait.as_secs()
        );
        let provider = self.providers[active].clone();
        let io = self.io.clone();
        self.reconnect = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let restored = join(io.spawn(async move { provider.restore().await })).await;
            this.update(cx, |this, cx| {
                this.reconnecting = false;
                match restored {
                    Ok(Some(session)) => this.reconnected(slug, session, cx),
                    Ok(None) => log::warn!("session: nothing stored to reconnect with"),
                    Err(error) => log::warn!("session: cannot reconnect: {error:#}"),
                }
            })
            .ok();
        }));
        true
    }

    fn reconnected(
        &mut self,
        slug: &'static str,
        session: ProviderSession,
        cx: &mut Context<Self>,
    ) {
        self.attempt = 0;
        self.playcounts = session.playcounts;
        self.connect(slug, session);
        log::debug!("session: reconnected");
        cx.notify();
        cx.emit(SessionEvent::Reconnected(slug));
    }

    fn failed(&mut self, error: &Error, cx: &mut Context<Self>) {
        let failure = Failure::new(error);
        let failed = self.awaiting.take().or(self.active);
        if let Some(index) = failed {
            self.error = Some((index, failure.clone()));
        }
        if let Some((index, _)) = self.resume.take() {
            self.active = Some(index);
            self.state = SessionState::SignedIn;
            cx.notify();
            return;
        }
        if let Some(slug) = self.provider_slug() {
            self.connected.remove(slug);
        }
        self.state = SessionState::Failed(failure);
        cx.notify();
        if let Some(index) = failed {
            cx.emit(SessionEvent::SignedOut(self.providers[index].slug()));
        }
    }

    fn restore_local(&mut self, cx: &mut Context<Self>) {
        let slug = self.local_provider.slug();
        let provider = self.local_provider.clone();
        let io = self.io.clone();
        let task = cx.spawn(async move |this, cx| {
            let restored = join(io.spawn(async move { provider.restore().await })).await;
            this.update(cx, |this, cx| {
                if let Ok(Some(session)) = restored {
                    this.local_signed_in(session, cx);
                }
            })
            .ok();
        });
        self.tasks.insert(slug, task);
    }

    pub fn choose_local_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let slug = self.local_provider.slug();
        let provider = self.local_provider.clone();
        let io = self.io.clone();
        let task = cx.spawn(async move |this, cx| {
            let prompt: PromptSink = Arc::new(|_| {});
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let signed_in = join(
                io.spawn(async move { provider.sign_in(SignIn::Path(path), prompt, rx).await }),
            )
            .await;

            this.update(cx, |this, cx| match signed_in {
                Ok(session) => this.local_signed_in(session, cx),
                Err(error) => {
                    log::warn!("session: cannot set local music folder: {error:#}");
                }
            })
            .ok();
        });
        self.tasks.insert(slug, task);
    }

    pub fn clear_local_folder(&mut self, cx: &mut Context<Self>) {
        let slug = self.local_provider.slug();
        self.local_provider.sign_out();
        self.connected.remove(slug);
        self.tasks.remove(slug);
        cx.notify();
        cx.emit(SessionEvent::SignedOut(slug));
    }

    fn local_signed_in(&mut self, session: ProviderSession, cx: &mut Context<Self>) {
        let slug = self.local_provider.slug();
        self.connect(slug, session);
        cx.notify();
        cx.emit(SessionEvent::SignedIn(slug));
    }
}
