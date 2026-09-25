use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Error;
use gpui::{App, Context, Entity, EventEmitter, Task};
use i18n::t;
use music::{
    Capabilities, MusicApi, MusicProvider, PlaybackFactory, PromptSink, ProviderSession, Shape,
    SignIn, SignInFailure, SignInProblem, SignInPrompt, UserProfile,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::Shelf;
use crate::catalog::CatalogSource;
use crate::settings::AppSettings;
use crate::{Io, Network, Outcome, Toasts, join};

const HEARTBEAT: Duration = Duration::from_secs(30);
/// How often the sign-in window is asked whether the user is through.
const WINDOW_POLL: Duration = Duration::from_millis(300);
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
        let reason = format!("{error:#}");
        let problem = error
            .downcast_ref::<SignInFailure>()
            .map(|failure| failure.0)
            .or_else(|| music::trouble::offline(&reason).then_some(SignInProblem::Network));
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

    /// Whether the sign-in failed because there was no network, rather than because the account
    /// was refused. A provider that was only unreachable is still the user's.
    pub fn offline(&self) -> bool {
        self.problem == Some(SignInProblem::Network)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    SignedOut,
    Restoring,
    Authorizing(Option<SignInPrompt>),
    SignedIn,
    /// The stored account could not be reached. It is still the user's, so nothing is signed
    /// out: the app stays on whatever the library kept and on the local files, and tries the
    /// account again by itself once the network is back.
    Offline(Failure),
    Failed(Failure),
}

pub enum SessionEvent {
    SignedIn(&'static str),
    SignedOut(&'static str),
    Reconnected(&'static str),
    LocalChanged,
}

#[derive(Clone, Copy)]
enum SecretInput {
    Browser,
    Manual,
}

pub struct ProviderInfo {
    pub slug: &'static str,
    pub name: &'static str,
    pub options: Vec<SignIn>,
    pub web_sign_in: bool,
    pub protected: bool,
    pub stored: bool,
    pub active: bool,
    /// Whether what is stored is an anonymous session rather than an account.
    pub guest: bool,
    pub pending: bool,
    pub error: Option<Failure>,
}

pub(crate) struct Connected {
    pub(crate) client: Arc<dyn MusicApi>,
    pub(crate) catalog: Arc<CatalogSource>,
    pub(crate) playback: Arc<dyn PlaybackFactory>,
    pub(crate) profile: UserProfile,
    pub(crate) shape: Shape,
    pub(crate) authenticated: bool,
    pub(crate) capabilities: Capabilities,
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
    io: Io,
    tasks: HashMap<&'static str, Task<()>>,
    prompt_task: Option<Task<()>>,
    input: Option<UnboundedSender<String>>,
    /// The browser window a `SignInPrompt::Secret` opened, while it is up.
    window: Option<webview::Login>,
    window_task: Option<Task<()>>,
    local_provider: Arc<dyn MusicProvider>,
    local_folders: Vec<PathBuf>,
    local_task: Option<Task<()>>,
    /// Whether a local scan is under way, so the UI can show its progress.
    scanning: bool,
    watch: Option<Task<()>>,
    reconnect: HashMap<&'static str, Task<()>>,
    reconnecting: HashSet<&'static str>,
    attempt: HashMap<&'static str, usize>,
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
        let local_folders = settings.read(cx).local_folders().to_vec();
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
            io,
            tasks: HashMap::new(),
            prompt_task: None,
            input: None,
            window: None,
            window_task: None,
            local_provider,
            local_folders,
            local_task: None,
            scanning: false,
            watch: None,
            reconnect: HashMap::new(),
            reconnecting: HashSet::new(),
            attempt: HashMap::new(),
        };
        session.restore_local(cx);
        session
    }

    pub fn state(&self) -> &SessionState {
        &self.state
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

    pub fn local_client(&self) -> Option<Arc<dyn MusicApi>> {
        self.client_for_slug(self.local_slug())
    }

    pub fn local_playback(&self) -> Option<Arc<dyn PlaybackFactory>> {
        self.playback_for_slug(self.local_slug())
    }

    /// Whether the active provider is behind an account rather than a guest session.
    pub fn authenticated(&self) -> bool {
        self.provider_slug()
            .is_some_and(|slug| self.authenticated_for(slug))
    }

    /// The client serving a shelf, if that shelf has a provider right now.
    pub fn client_of(&self, shelf: Shelf) -> Option<Arc<dyn MusicApi>> {
        match shelf {
            Shelf::Streaming => self.client(),
            Shelf::Local => self.local_client(),
            Shelf::Of(slug) => self.client_for_slug(slug),
        }
    }

    /// What a shelf's library is made of. The local shelf is always a catalog.
    pub fn shape_of(&self, shelf: Shelf) -> Shape {
        match shelf {
            Shelf::Local => Shape::Catalog,
            Shelf::Of(slug) if slug == music::tag::LOCAL => Shape::Catalog,
            Shelf::Streaming => self.shape(),
            Shelf::Of(slug) => self.shape_for(slug),
        }
    }

    /// What the active provider's library is made of.
    pub fn shape(&self) -> Shape {
        self.provider_slug()
            .map_or(Shape::Saved, |slug| self.shape_for(slug))
    }

    /// The same, for one signed-in provider by slug.
    pub fn shape_for(&self, slug: &str) -> Shape {
        self.connected
            .get(slug)
            .map_or(Shape::Saved, |entry| entry.shape)
    }

    pub(crate) fn catalog(&self, id: &str) -> Option<Arc<CatalogSource>> {
        let slug = self.slug_for(id)?;
        self.connected.get(slug).map(|entry| entry.catalog.clone())
    }

    pub fn local_paths(&self) -> Vec<String> {
        self.local_folders
            .iter()
            .map(|path| path.display().to_string())
            .collect()
    }

    pub fn providers(&self) -> impl Iterator<Item = ProviderInfo> + '_ {
        self.providers
            .iter()
            .enumerate()
            .map(|(index, provider)| ProviderInfo {
                slug: provider.slug(),
                name: provider.name(),
                options: provider.sign_in_options(),
                // Asked in this order because answering `supported` costs a library load on
                // Linux, and only a provider that signs in with cookies is worth it.
                web_sign_in: provider.web_sign_in().is_some() && webview::supported(),
                protected: provider.protected(),
                stored: provider.stored(),
                active: self.active == Some(index),
                guest: self.guest_for(provider.slug()),
                pending: self.awaiting == Some(index),
                error: match &self.error {
                    Some((failed, failure)) if *failed == index => Some(failure.clone()),
                    _ => None,
                },
            })
    }

    /// Every provider with something stored, whether or not it is the one being shown.
    pub fn stored_providers(&self) -> impl Iterator<Item = ProviderInfo> + '_ {
        self.providers().filter(|info| info.stored)
    }

    /// Whether any signed-in provider's tracks need the Widevine module. Several providers are
    /// live at once here, so a protected one the user is not looking at still needs it.
    pub fn wants_drm(&self) -> bool {
        self.providers
            .iter()
            .any(|provider| provider.protected() && provider.stored())
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
        self.stop_reconnect(self.providers[index].slug());
        self.active = Some(index);
        self.restore(cx);
    }

    pub fn provider_name(&self) -> Option<&'static str> {
        let provider = &self.providers[self.active?];
        Some(provider.name())
    }

    fn provider_for(&self, slug: &str) -> Option<&Arc<dyn MusicProvider>> {
        self.providers
            .iter()
            .find(|provider| provider.slug() == slug)
            .or_else(|| Some(&self.local_provider).filter(|local| local.slug() == slug))
    }

    /// The provider an id belongs to, whichever shelf it sits on.
    pub(crate) fn owner_of(&self, id: &str) -> Option<&dyn MusicProvider> {
        let slug = music::tag::slug_of(id)?;
        self.provider_for(slug).map(|provider| provider.as_ref())
    }

    pub fn provider_name_for(&self, slug: &str) -> Option<&'static str> {
        self.provider_for(slug).map(|provider| provider.name())
    }

    pub fn has_all_tracks(&self, slug: &str) -> bool {
        self.provider_for(slug)
            .is_some_and(|provider| provider.has_all_tracks())
    }

    pub fn registered_slugs(&self) -> Vec<&'static str> {
        self.providers
            .iter()
            .chain([&self.local_provider])
            .map(|provider| provider.slug())
            .collect()
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

    /// The host to try when checking whether the network is back. It is the active provider's
    /// own, so the check never touches a service the app is not already using.
    pub fn reach(&self) -> Option<String> {
        self.providers.get(self.active?)?.reach()
    }

    pub fn local_slug(&self) -> &'static str {
        self.local_provider.slug()
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

    pub fn authenticated_for(&self, slug: &str) -> bool {
        self.connected
            .get(slug)
            .is_some_and(|entry| entry.authenticated)
    }

    /// Whether the provider is connected, but with no account behind it.
    ///
    /// Not the inverse of [`Session::authenticated_for`]: a provider whose
    /// restore has not landed yet is neither. Reading the inverse would call
    /// every stored provider a guest for the first moments after launch.
    pub fn guest_for(&self, slug: &str) -> bool {
        self.connected
            .get(slug)
            .is_some_and(|entry| !entry.authenticated)
    }

    /// What the live streaming provider can do beyond listing and playing. Nothing is offered
    /// while signed out, which is what the empty set means.
    pub fn capabilities(&self) -> Capabilities {
        self.provider_slug()
            .map_or(Capabilities::NONE, |slug| self.capabilities_for(slug))
    }

    /// The same, for one signed-in provider by slug.
    pub fn capabilities_for(&self, slug: &str) -> Capabilities {
        self.connected
            .get(slug)
            .map_or(Capabilities::NONE, |entry| entry.capabilities)
    }

    /// The same, for whichever shelf a thing belongs to. Anything that routes by id asks this
    /// one, since the two shelves can differ: local files keep favorites but seed no station.
    pub fn capabilities_of(&self, shelf: Shelf) -> Capabilities {
        match shelf {
            Shelf::Streaming => self.capabilities(),
            Shelf::Local => self.capabilities_for(self.local_slug()),
            Shelf::Of(slug) => self.capabilities_for(slug),
        }
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
        let expired = session.expired;
        self.connect(slug, session);
        self.warn_expired(slug, expired, cx);
        cx.notify();
        cx.emit(SessionEvent::SignedIn(slug));
    }

    fn warn_expired(&self, slug: &'static str, expired: bool, cx: &mut App) {
        if !expired {
            return;
        }
        let name = self.provider_name_for(slug).unwrap_or(slug);
        Toasts::about(Outcome::Failed, "toast-session-expired", name, cx);
    }

    pub fn sign_in(&mut self, slug: &str, method: SignIn, cx: &mut Context<Self>) {
        self.start_sign_in(slug, method, SecretInput::Browser, cx);
    }

    pub fn sign_in_with_cookies(&mut self, slug: &str, cx: &mut Context<Self>) {
        self.start_sign_in(slug, SignIn::Secret, SecretInput::Manual, cx);
    }

    fn start_sign_in(
        &mut self,
        slug: &str,
        method: SignIn,
        secret_input: SecretInput,
        cx: &mut Context<Self>,
    ) {
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
                        let secret = matches!(prompt, SignInPrompt::Secret);
                        this.state = SessionState::Authorizing(Some(prompt));
                        cx.notify();
                        if secret && matches!(secret_input, SecretInput::Browser) {
                            this.open_window(cx);
                        }
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
                this.window = None;
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
        self.window = None;
        self.window_task = None;
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

    /// Opens the browser window for the secret prompt now showing. The window answers the prompt
    /// itself once the user is through; closing it cancels the sign-in.
    fn open_window(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.state,
            SessionState::Authorizing(Some(SignInPrompt::Secret))
        ) || self.window.is_some()
        {
            return;
        }
        let Some(index) = self.awaiting else {
            return;
        };
        let provider = &self.providers[index];
        let Some(sign_in) = provider.web_sign_in() else {
            return;
        };
        let target = webview::Target {
            url: sign_in.url.to_string(),
            landing: sign_in.landing.to_string(),
            domain: sign_in.domain.to_string(),
            proof: sign_in.proof.iter().map(ToString::to_string).collect(),
            title: t!("login-window-title", provider = provider.name()).to_string(),
            agent: sign_in.agent.map(str::to_owned),
        };
        match webview::Login::open(target) {
            Ok(login) => self.window = Some(login),
            Err(error) => {
                log::warn!("session: cannot open the sign-in window: {error:#}");
                // Dropping the provider's task closes its prompt channel, which ends the prompt
                // task on its own; this runs inside that task, so it must not drop it here.
                if let Some(index) = self.awaiting {
                    self.tasks.remove(self.providers[index].slug());
                }
                self.input = None;
                return self.failed(&error, cx);
            }
        }
        self.window_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WINDOW_POLL).await;
                let open = this.update(cx, |this, cx| {
                    let Some(login) = this.window.as_mut() else {
                        return false;
                    };
                    match login.poll() {
                        webview::Poll::Pending => true,
                        webview::Poll::Closed => {
                            this.window = None;
                            this.cancel_sign_in(cx);
                            false
                        }
                        webview::Poll::Cookies(header) => {
                            this.window = None;
                            this.submit_input(header, cx);
                            false
                        }
                    }
                });
                if !open.unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    pub fn submit_input(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(input) = &self.input {
            self.window = None;
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

    /// Whether the active provider has an account stored, which is what tells a failed restore
    /// from a user who signed out.
    fn stored_active(&self) -> bool {
        self.active
            .is_some_and(|index| self.providers[index].stored())
    }

    /// Tries the stored account again once the network is back, for a run that started without
    /// one. Anything but a run held up by the network is left alone.
    pub fn restore_if_offline(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state, SessionState::Offline(_)) {
            self.restore(cx);
        }
    }

    fn remaining(&self) -> Option<usize> {
        self.active
            .filter(|index| self.providers[*index].stored())
            .or_else(|| self.providers.iter().position(|provider| provider.stored()))
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        self.prompt_task = None;
        self.input = None;
        self.window = None;
        self.window_task = None;
        if let Some(index) = self.awaiting.take() {
            self.tasks.remove(self.providers[index].slug());
        }
        self.resume = None;
        let released = self.provider_slug();
        if let Some(slug) = released {
            self.tasks.remove(slug);
            self.connected.remove(slug);
            self.stop_reconnect(slug);
        }
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
                shape: session.shape,
                authenticated: session.authenticated,
                capabilities: session.capabilities,
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
        let expired = session.expired;
        self.connect(slug, session);
        self.warn_expired(slug, expired, cx);
        self.state = SessionState::SignedIn;
        self.attempt.remove(slug);
        self.start_heartbeat(cx);
        cx.notify();
        cx.emit(SessionEvent::SignedIn(slug));
    }

    fn drop_previous_session(&mut self, index: usize, cx: &mut Context<Self>) {
        let slug = self.providers[index].slug();
        self.connected.remove(slug);
        self.stop_reconnect(slug);
        self.attempt.remove(slug);
        cx.emit(SessionEvent::SignedOut(slug));
    }

    fn start_heartbeat(&mut self, cx: &mut Context<Self>) {
        self.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(HEARTBEAT).await;
                if this
                    .update(cx, |this, cx| this.reconnect_stale(cx))
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    /// Forgets any reconnect in flight for one provider.
    fn stop_reconnect(&mut self, slug: &str) {
        self.reconnect.remove(slug);
        self.reconnecting.remove(slug);
        self.attempt.remove(slug);
    }

    /// Reconnects every connected provider whose session has gone stale.
    ///
    /// Every one, not only the active one. Playback, search, the library and
    /// the feed all reach providers this session is not "on", so a session
    /// that dies in the background takes those with it — and nothing else
    /// looks. Checking the active provider alone left a dead one failing
    /// every track it owned until the app was restarted.
    fn reconnect_stale(&mut self, cx: &mut Context<Self>) {
        let connected: Vec<&'static str> = self.connected.keys().copied().collect();
        for slug in connected {
            self.reconnect_if_stale(slug, cx);
        }
    }

    /// Reconnects one provider if its session has gone stale, and reports
    /// whether a reconnect is now in flight for it.
    pub fn reconnect_if_stale(&mut self, slug: &str, cx: &mut Context<Self>) -> bool {
        if self.reconnecting.contains(slug) {
            return true;
        }
        let Some(index) = self
            .providers
            .iter()
            .position(|provider| provider.slug() == slug)
        else {
            return false;
        };
        // a provider being signed in is already on its way back
        if self.awaiting == Some(index) {
            return false;
        }
        let slug = self.providers[index].slug();
        let Some(entry) = self.connected.get(slug) else {
            return false;
        };
        if entry.client.alive() {
            self.attempt.remove(slug);
            return false;
        }
        let attempt = self.attempt.entry(slug).or_default();
        let wait = BACKOFF[(*attempt).min(BACKOFF.len() - 1)];
        *attempt += 1;
        self.reconnecting.insert(slug);
        log::warn!(
            "session: the {} session went stale, reconnecting in {}s",
            self.providers[index].name(),
            wait.as_secs()
        );
        let provider = self.providers[index].clone();
        let io = self.io.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let restored = join(io.spawn(async move { provider.restore().await })).await;
            this.update(cx, |this, cx| {
                this.reconnecting.remove(slug);
                match restored {
                    Ok(Some(session)) => this.reconnected(slug, session, cx),
                    Ok(None) => log::warn!("session: nothing stored to reconnect {slug} with"),
                    Err(error) => log::warn!("session: cannot reconnect {slug}: {error:#}"),
                }
            })
            .ok();
        });
        self.reconnect.insert(slug, task);
        true
    }

    fn reconnected(
        &mut self,
        slug: &'static str,
        session: ProviderSession,
        cx: &mut Context<Self>,
    ) {
        self.attempt.remove(slug);
        self.connect(slug, session);
        log::debug!("session: reconnected {slug}");
        cx.notify();
        cx.emit(SessionEvent::Reconnected(slug));
    }

    fn failed(&mut self, error: &Error, cx: &mut Context<Self>) {
        let failure = Failure::new(error);
        Network::failed(&format!("{error:#}"), cx);
        let failed = self.awaiting.or(self.active);
        if let Some(index) = failed {
            self.error = Some((index, failure.clone()));
        }
        // Only a restore may end up offline. A sign-in the user is watching says what went
        // wrong on the page they started it from.
        let restoring = self.awaiting.is_none();
        if let Some(index) = self.awaiting.take() {
            self.tasks.remove(self.providers[index].slug());
        }
        if let Some((index, _)) = self.resume.take() {
            self.active = Some(index);
            self.state = SessionState::SignedIn;
            cx.notify();
            return;
        }
        if restoring && failure.offline() && self.stored_active() {
            log::warn!("session: the account could not be reached, carrying on offline");
            self.state = SessionState::Offline(failure);
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
        if self.local_folders.is_empty() {
            return;
        }
        self.rescan_local(false, cx);
    }

    /// Adds a folder to the local library, then rescans every configured folder together so
    /// artists and albums that span more than one root merge into one, seamlessly.
    pub fn add_local_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.add_local_folders(vec![path], cx);
    }

    /// Same as [`Session::add_local_folder`], for a batch picked in one native dialog.
    pub fn add_local_folders(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut folders = self.local_folders.clone();
        for path in paths {
            if folders.iter().any(|existing| overlaps(existing, &path)) {
                log::warn!(
                    "session: {} overlaps an already-added local folder",
                    path.display()
                );
                continue;
            }
            folders.push(path);
        }
        if folders.len() == self.local_folders.len() {
            return;
        }
        self.set_local_folders(folders, cx);
    }

    pub fn remove_local_folder(&mut self, path: &Path, cx: &mut Context<Self>) {
        let mut folders = self.local_folders.clone();
        let before = folders.len();
        folders.retain(|existing| existing != path);
        if folders.len() == before {
            return;
        }
        self.set_local_folders(folders, cx);
    }

    /// Rescans every configured local folder without changing the list, e.g. after files
    /// changed on disk or a tag was edited. A `thorough` rescan is the one the user asked for:
    /// it forgets what the last scan recorded, so every folder is listed and every file stat'd
    /// again, which is the only way an edit made behind Sonora's back is noticed.
    pub fn rescan_local(&mut self, thorough: bool, cx: &mut Context<Self>) {
        if thorough {
            self.local_provider.forget_scan();
        }
        self.set_local_folders(self.local_folders.clone(), cx);
    }

    /// Points the local library at `folders` and scans them. A scan already under way is
    /// cancelled first: its folders may be the ones just removed, and two scans would only
    /// fight over the same disk.
    fn set_local_folders(&mut self, folders: Vec<PathBuf>, cx: &mut Context<Self>) {
        music::progress::cancel();
        let slug = self.local_provider.slug();
        if folders.is_empty() {
            self.scanning = false;
            self.local_provider.sign_out();
            self.local_folders = Vec::new();
            self.settings.update(cx, |settings, cx| {
                settings.set_local_folders(Vec::new(), cx)
            });
            self.connected.remove(slug);
            self.local_task = None;
            cx.notify();
            cx.emit(SessionEvent::LocalChanged);
            cx.emit(SessionEvent::SignedOut(slug));
            return;
        }

        let provider = self.local_provider.clone();
        let chosen = folders.clone();
        let io = self.io.clone();
        self.scanning = true;
        cx.notify();
        self.local_task = Some(cx.spawn(async move |this, cx| {
            let prompt: PromptSink = Arc::new(|_| {});
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let signed_in = join(
                io.spawn(async move { provider.sign_in(SignIn::Path(folders), prompt, rx).await }),
            )
            .await;

            this.update(cx, |this, cx| {
                this.scanning = false;
                match signed_in {
                    Ok(session) => {
                        this.local_folders = chosen.clone();
                        this.settings
                            .update(cx, |settings, cx| settings.set_local_folders(chosen, cx));
                        this.local_signed_in(session, cx);
                        cx.emit(SessionEvent::LocalChanged);
                    }
                    Err(error) => {
                        log::warn!("session: cannot update local music folders: {error:#}");
                        cx.notify();
                    }
                }
            })
            .ok();
        }));
    }

    /// Brings up the local engine even when no folder has ever been configured, so a
    /// file-association open can play a track without the user having visited Settings first.
    /// A no-op once a local client already exists or one is already being brought up; a
    /// configured library goes through the normal [`Session::rescan_local`] path instead.
    pub fn ensure_local_ready(&mut self, cx: &mut Context<Self>) {
        let slug = self.local_provider.slug();
        if self.connected.contains_key(slug) || self.local_task.is_some() {
            return;
        }
        if !self.local_folders.is_empty() {
            return self.rescan_local(false, cx);
        }

        let provider = self.local_provider.clone();
        let io = self.io.clone();
        self.local_task = Some(cx.spawn(async move |this, cx| {
            let prompt: PromptSink = Arc::new(|_| {});
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let signed_in = join(io.spawn(async move {
                provider.sign_in(SignIn::Path(Vec::new()), prompt, rx).await
            }))
            .await;

            this.update(cx, |this, cx| match signed_in {
                Ok(session) => this.local_signed_in(session, cx),
                Err(error) => {
                    log::warn!("session: cannot initialize local playback: {error:#}");
                }
            })
            .ok();
        }));
    }

    /// Whether a local scan is under way right now.
    pub fn scanning(&self) -> bool {
        self.scanning
    }

    fn local_signed_in(&mut self, session: ProviderSession, cx: &mut Context<Self>) {
        let slug = self.local_provider.slug();
        self.connect(slug, session);
        cx.notify();
        cx.emit(SessionEvent::SignedIn(slug));
    }
}

/// Whether `a` and `b` are the same directory, or one contains the other — either way, scanning
/// both would double-count the tracks they share.
fn overlaps(a: &Path, b: &Path) -> bool {
    let a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a == b || a.starts_with(&b) || b.starts_with(&a)
}
