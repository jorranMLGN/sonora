//! The one `Core` the app runs: its tokio runtime, the providers and their sessions, the event
//! listener, and the channel into the player.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context as _, Result, anyhow};
use music::soundcloud::SoundCloudProvider;
use music::{
    Album, MusicApi, MusicProvider, PlaybackFactory, Playlist, ProviderSession, SignIn,
    SignInFailure, SignInProblem, Track, tag, tagged,
};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::player::{self, Command};
use crate::store::Store;
use crate::{
    EventListener, Item, PlayerState, Prompt, ProviderInfo, QueueSnapshot, SearchResults,
    SessionState,
};

const WORKERS: usize = 3;
const STORE: &str = "mobile.sqlite";

static CORE: Mutex<Option<Arc<Core>>> = Mutex::new(None);

/// The core every exported function works through.
pub struct Core {
    runtime: Runtime,
    events: Arc<Events>,
    providers: Vec<Arc<dyn MusicProvider>>,
    sessions: Arc<Sessions>,
    /// Every track a search or a listing has handed out, so `play` can queue it by id alone.
    known: Mutex<HashMap<String, Item>>,
    commands: mpsc::UnboundedSender<Command>,
    published: Arc<Mutex<(PlayerState, QueueSnapshot)>>,
}

/// A provider's live session, its api already tagging every id with the provider's slug.
#[derive(Clone)]
pub struct Session {
    pub api: Arc<dyn MusicApi>,
    pub playback: Arc<dyn PlaybackFactory>,
}

/// The sessions and where each provider's stands, shared by the core and the player.
#[derive(Default)]
pub struct Sessions {
    live: RwLock<HashMap<&'static str, Session>>,
    states: RwLock<HashMap<&'static str, SessionState>>,
}

/// The listener Swift registered, swapped when `init` is called again after a reload.
pub struct Events {
    listener: RwLock<Box<dyn EventListener>>,
}

/// Starts the core once, or gives a running one the new listener and repeats what it last
/// said, so a reloaded interface catches up.
pub fn start(dir: PathBuf, listener: Box<dyn EventListener>) -> Result<()> {
    let mut slot = CORE
        .lock()
        .map_err(|_| anyhow!("the core lock is poisoned"))?;
    if let Some(core) = slot.as_ref() {
        *core
            .events
            .listener
            .write()
            .map_err(|_| anyhow!("poisoned"))? = listener;
        core.replay();
        return Ok(());
    }
    let _ = env_logger::Builder::new()
        .parse_filters("warn,music=info,mobile=debug")
        .try_init();
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKERS)
        .thread_name("sonora-core")
        .enable_all()
        .build()
        .context("cannot build the runtime")?;
    let store = Store::open(&dir.join(STORE))?;
    let events = Arc::new(Events {
        listener: RwLock::new(listener),
    });
    let sessions = Arc::new(Sessions::default());
    let providers = providers(&dir);
    for provider in &providers {
        sessions.set(provider.slug(), SessionState::SignedOut);
    }
    let (commands, receiver) = mpsc::unbounded_channel();
    let deck = player::Deck::restore(store, sessions.clone(), events.clone(), commands.clone());
    let published = deck.published();
    runtime.spawn(deck.run(receiver));
    let core = Arc::new(Core {
        runtime,
        events,
        providers,
        sessions,
        known: Mutex::new(HashMap::new()),
        commands,
        published,
    });
    core.replay();
    core.restore();
    *slot = Some(core);
    Ok(())
}

/// The running core, or an error when `init` has not succeeded yet.
pub fn get() -> Result<Arc<Core>> {
    CORE.lock()
        .map_err(|_| anyhow!("the core lock is poisoned"))?
        .clone()
        .context("init has not succeeded yet")
}

impl Core {
    /// Runs `work` on the core's runtime and waits for it from any executor.
    pub async fn spawn<T: Send + 'static>(
        &self,
        work: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        self.runtime
            .spawn(work)
            .await
            .context("a core task stopped")?
    }

    /// Hands the player an intent. Its outcome arrives through the listener.
    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .send(command)
            .map_err(|_| anyhow!("the player has stopped"))
    }

    pub fn published(&self) -> (PlayerState, QueueSnapshot) {
        self.published
            .lock()
            .map(|published| published.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    pub fn providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| {
                let session = self.sessions.state(provider.slug());
                let account = match &session {
                    SessionState::SignedIn { account } => Some(account.clone()),
                    _ => None,
                };
                ProviderInfo {
                    slug: provider.slug().to_owned(),
                    name: provider.name().to_owned(),
                    signed_in: self.sessions.session(provider.slug()).is_some(),
                    account,
                    session,
                }
            })
            .collect()
    }

    /// The session of the provider `slug` names, when it has one.
    pub fn session(&self, slug: &str) -> Option<Session> {
        self.sessions.session(slug)
    }

    /// Searches every provider with a session at once. One provider failing leaves the others'
    /// results; only all of them failing is an error.
    pub async fn search(&self, query: &str) -> Result<SearchResults> {
        let sessions = self.sessions.all();
        if sessions.is_empty() {
            anyhow::bail!("no provider is signed in yet");
        }
        let searches = sessions.iter().map(|(slug, session)| async move {
            let api = &session.api;
            let (tracks, albums, playlists) = tokio::join!(
                api.search(query),
                api.search_albums(query),
                api.search_playlists(query)
            );
            (*slug, tracks, albums, playlists)
        });
        let mut results = SearchResults::default();
        let mut failed = Vec::new();
        for (slug, tracks, albums, playlists) in futures::future::join_all(searches).await {
            match tracks {
                Ok(tracks) => results.tracks.extend(self.remember(&tracks)),
                Err(error) => {
                    log::warn!("search: {slug} failed: {error:#}");
                    failed.push(error);
                    continue;
                }
            }
            let albums = albums.unwrap_or_else(|error| {
                log::warn!("search: {slug} albums failed: {error:#}");
                Vec::new()
            });
            results.albums.extend(albums.iter().map(album));
            let playlists = playlists.unwrap_or_else(|error| {
                log::warn!("search: {slug} playlists failed: {error:#}");
                Vec::new()
            });
            results.playlists.extend(playlists.iter().map(playlist));
        }
        if failed.len() == sessions.len()
            && let Some(error) = failed.pop()
        {
            return Err(error.context("cannot search"));
        }
        Ok(results)
    }

    /// The items for `ids`, from what was handed out before or else fetched from the provider.
    /// An id no provider can answer for is left out.
    pub async fn items(&self, ids: Vec<String>) -> Result<Vec<Item>> {
        let known: Vec<Option<Item>> = {
            let known = self.known.lock().map_err(|_| anyhow!("poisoned"))?;
            ids.iter().map(|id| known.get(id).cloned()).collect()
        };
        let fetches = ids.iter().zip(known).map(|(id, item)| async move {
            if item.is_some() {
                return item;
            }
            let session = self.session(tag::slug_of(id)?)?;
            match session.api.track(id).await {
                Ok(track) => self.remember(std::slice::from_ref(&track)).pop(),
                Err(error) => {
                    log::warn!("mobile: cannot fetch {id}: {error:#}");
                    None
                }
            }
        });
        Ok(futures::future::join_all(fetches)
            .await
            .into_iter()
            .flatten()
            .collect())
    }

    /// Maps tracks to items and keeps them, so they can be queued by id later.
    pub fn remember(&self, tracks: &[Track]) -> Vec<Item> {
        let items: Vec<Item> = tracks.iter().filter_map(track).collect();
        if let Ok(mut known) = self.known.lock() {
            for item in &items {
                known.insert(item.id.clone(), item.clone());
            }
        }
        items
    }

    /// Says everything the interface needs to draw itself from nothing.
    fn replay(&self) {
        for provider in &self.providers {
            let slug = provider.slug();
            self.events.session(slug, self.sessions.state(slug));
        }
        let (state, queue) = self.published();
        self.events.queue(queue);
        self.events.state(state);
    }

    /// Restores every stored session in parallel. SoundCloud signs in as a guest when nothing
    /// is stored, so search works on a fresh install.
    fn restore(&self) {
        for provider in self.providers.clone() {
            let sessions = self.sessions.clone();
            let events = self.events.clone();
            self.runtime.spawn(async move {
                let slug = provider.slug();
                let guest = slug == "soundcloud";
                if !provider.stored() && !guest {
                    return;
                }
                sessions.set(slug, SessionState::Connecting);
                events.session(slug, SessionState::Connecting);
                let restored = match provider.restore().await {
                    Ok(None) if guest => {
                        let (_keep, input) = mpsc::unbounded_channel();
                        let prompts = prompt_sink(events.clone(), slug);
                        provider
                            .sign_in(SignIn::Anonymous, prompts, input)
                            .await
                            .map(Some)
                    }
                    other => other,
                };
                let state = match restored {
                    Ok(Some(session)) => sessions.install(slug, session),
                    Ok(None) => SessionState::SignedOut,
                    Err(error) => failure(slug, &error),
                };
                sessions.set(slug, state.clone());
                events.session(slug, state);
            });
        }
    }
}

impl Sessions {
    pub fn session(&self, slug: &str) -> Option<Session> {
        self.live.read().ok()?.get(slug).cloned()
    }

    fn all(&self) -> Vec<(&'static str, Session)> {
        self.live
            .read()
            .map(|live| {
                live.iter()
                    .map(|(slug, session)| (*slug, session.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn state(&self, slug: &str) -> SessionState {
        self.states
            .read()
            .ok()
            .and_then(|states| states.get(slug).cloned())
            .unwrap_or(SessionState::SignedOut)
    }

    fn set(&self, slug: &'static str, state: SessionState) {
        if let Ok(mut states) = self.states.write() {
            states.insert(slug, state);
        }
    }

    /// Makes a fresh provider session the live one and says what kind it is.
    fn install(&self, slug: &'static str, session: ProviderSession) -> SessionState {
        let state = match (session.authenticated, session.expired) {
            (true, _) => SessionState::SignedIn {
                account: session.profile.display_name.clone(),
            },
            (false, true) => SessionState::Expired,
            (false, false) => SessionState::Guest,
        };
        let live = Session {
            api: tagged::new(slug, session.api),
            playback: session.playback,
        };
        if let Ok(mut sessions) = self.live.write() {
            sessions.insert(slug, live);
        }
        state
    }
}

impl Events {
    fn with(&self, call: impl FnOnce(&dyn EventListener)) {
        match self.listener.read() {
            Ok(listener) => call(listener.as_ref()),
            Err(poisoned) => call(poisoned.into_inner().as_ref()),
        }
    }

    pub fn state(&self, state: PlayerState) {
        self.with(|listener| listener.on_state(state));
    }

    pub fn position(&self, position_ms: u64) {
        self.with(|listener| listener.on_position(position_ms));
    }

    pub fn queue(&self, queue: QueueSnapshot) {
        self.with(|listener| listener.on_queue(queue));
    }

    pub fn session(&self, slug: &str, state: SessionState) {
        self.with(|listener| listener.on_session(slug.to_owned(), state));
    }

    pub fn prompt(&self, slug: &str, prompt: Prompt) {
        self.with(|listener| listener.on_prompt(slug.to_owned(), prompt));
    }

    pub fn error(&self, message: String) {
        log::warn!("mobile: {message}");
        self.with(|listener| listener.on_error(message));
    }
}

/// Every provider the phone carries, each keeping its files in its own folder under `dir`.
fn providers(dir: &Path) -> Vec<Arc<dyn MusicProvider>> {
    let soundcloud: Arc<dyn MusicProvider> =
        Arc::new(SoundCloudProvider::at(dir.join("soundcloud")));
    [spotify(dir), youtube(dir), Some(soundcloud)]
        .into_iter()
        .flatten()
        .collect()
}

#[cfg(feature = "spotify")]
fn spotify(dir: &Path) -> Option<Arc<dyn MusicProvider>> {
    let config = music::spotify::AuthConfig {
        cache_dir: dir.join("spotify"),
        ..music::spotify::AuthConfig::default()
    };
    Some(Arc::new(music::spotify::SpotifyProvider::new(config)))
}

#[cfg(not(feature = "spotify"))]
fn spotify(_dir: &Path) -> Option<Arc<dyn MusicProvider>> {
    None
}

#[cfg(feature = "youtube")]
fn youtube(dir: &Path) -> Option<Arc<dyn MusicProvider>> {
    Some(Arc::new(music::youtube::YouTubeProvider::at(
        dir.join("youtube"),
    )))
}

#[cfg(not(feature = "youtube"))]
fn youtube(_dir: &Path) -> Option<Arc<dyn MusicProvider>> {
    None
}

/// Forwards a provider's sign-in questions to the listener.
fn prompt_sink(events: Arc<Events>, slug: &'static str) -> music::PromptSink {
    Arc::new(move |prompt| {
        let prompt = match prompt {
            music::SignInPrompt::Url(url) => Prompt::Url { url },
            music::SignInPrompt::Code { code, url } => Prompt::Code { code, url },
            music::SignInPrompt::Secret => Prompt::Secret,
            music::SignInPrompt::Accounts(choices) => Prompt::Accounts {
                choices: choices
                    .into_iter()
                    .map(|choice| crate::AccountChoice {
                        id: choice.id,
                        name: choice.name,
                        detail: choice.detail,
                    })
                    .collect(),
            },
        };
        events.prompt(slug, prompt);
    })
}

/// The state a provider whose session could not be restored is left in.
fn failure(slug: &str, error: &anyhow::Error) -> SessionState {
    log::warn!("mobile: cannot restore {slug}: {error:#}");
    match error.downcast_ref::<SignInFailure>() {
        Some(SignInFailure(SignInProblem::Credentials)) => SessionState::Expired,
        _ => SessionState::Failed {
            message: format!("{error:#}"),
        },
    }
}

/// A tagged track as the phone draws it, or nothing for a track without an id.
pub fn track(track: &Track) -> Option<Item> {
    let id = track.id.clone()?;
    Some(Item {
        provider: provider(&id),
        id,
        title: track.name.clone(),
        subtitle: track.artists.clone(),
        album: track.album.clone(),
        artwork: track.cover.clone(),
        duration_ms: track.duration.as_millis() as u64,
        explicit: track.explicit,
        playable: track.playable,
    })
}

pub fn album(album: &Album) -> Item {
    Item {
        provider: provider(&album.id),
        id: album.id.clone(),
        title: album.name.clone(),
        subtitle: album.artists.clone(),
        album: String::new(),
        artwork: album.cover.clone(),
        duration_ms: 0,
        explicit: false,
        playable: true,
    }
}

pub fn playlist(playlist: &Playlist) -> Item {
    Item {
        provider: provider(&playlist.id),
        id: playlist.id.clone(),
        title: playlist.name.clone(),
        subtitle: playlist.owner.clone(),
        album: String::new(),
        artwork: playlist.cover.clone(),
        duration_ms: 0,
        explicit: false,
        playable: true,
    }
}

fn provider(id: &str) -> String {
    tag::slug_of(id).unwrap_or_default().to_owned()
}
