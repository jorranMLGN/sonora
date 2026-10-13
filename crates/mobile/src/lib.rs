//! The Rust core of the iPhone app, exported to Swift through UniFFI.
//!
//! Everything the phone does with music lives here: the providers and their sessions, search,
//! the queue and the one active player. Swift owns only the audio session and hands every
//! intent straight to these functions; what happens next comes back through the
//! `EventListener` it registered with `init`.

mod account;
mod core;
mod library;
mod player;
mod potoken;
mod queue;
mod store;

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use account::{Problem, SignInOption, WebSignIn, answer, cancel_sign_in, sign_in, sign_out};
pub use library::{
    AlbumDetail, ArtistDetail, HomeSection, LibraryPage, Part, PlaylistDetail, PlaylistMore,
    SavedIds, SectionKind,
};
pub use potoken::{PoTokenAsk, po_token, po_token_wanted};

use crate::player::Command;

uniffi::setup_scaffolding!();

/// One track, album, playlist or artist as the phone draws it. Every id carries its provider,
/// as `soundcloud:123`, so it can travel anywhere and still say where it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct Item {
    pub id: String,
    pub title: String,
    /// The artists of a track or an album, the owner of a playlist.
    pub subtitle: String,
    /// The album a track is on, for Now Playing. Empty for anything else, or when unknown.
    #[serde(default)]
    pub album: String,
    pub artwork: Option<String>,
    /// Zero for anything that is not a track.
    pub duration_ms: u64,
    pub explicit: bool,
    /// The provider's slug, such as `soundcloud`.
    pub provider: String,
    pub playable: bool,
}

/// What `search` answers, from every provider with a session, one provider after another.
#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct SearchResults {
    pub tracks: Vec<Item>,
    pub albums: Vec<Item>,
    pub artists: Vec<Item>,
    pub playlists: Vec<Item>,
}

/// One provider the app knows, whether or not it has a session.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ProviderInfo {
    pub slug: String,
    pub name: String,
    pub signed_in: bool,
    /// The account's display name, when signed in to one rather than as a guest.
    pub account: Option<String>,
    pub session: SessionState,
    /// The ways the listener may sign in to this provider, in the order to offer them.
    pub options: Vec<SignInOption>,
}

/// Where a provider's session stands.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SessionState {
    SignedOut,
    /// A stored session is being restored, or a sign-in is under way.
    Connecting,
    /// Usable without an account, as SoundCloud and YouTube are.
    Guest,
    SignedIn {
        account: String,
    },
    /// The stored account was refused and the provider fell back to a guest session, or to none.
    Expired,
    Failed {
        message: String,
        /// What went wrong, when the provider said, so the app can explain it in its own words.
        problem: Option<Problem>,
    },
}

/// What the player is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Status {
    /// Nothing in the queue.
    Idle,
    Loading,
    Playing,
    Paused,
}

/// What happens at the end of the queue, or of the track for `One`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

/// Everything the now-playing bar needs, sent on every change but position ticks.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PlayerState {
    pub track: Option<Item>,
    pub status: Status,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub shuffle: bool,
    pub repeat: Repeat,
}

/// The queue in play order, with the index of the track playing or ready to play.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct QueueSnapshot {
    pub items: Vec<Item>,
    pub current: Option<u32>,
    /// Where the list was started from, as the caller named it in `play`.
    pub origin: Option<String>,
    pub shuffle: bool,
    pub repeat: Repeat,
}

/// One account a provider asks the listener to choose between.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AccountChoice {
    pub id: String,
    pub name: String,
    pub detail: Option<String>,
}

/// A question a sign-in puts to the listener; the answer goes back through `answer`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Prompt {
    /// Open this url in a browser and wait.
    Url {
        url: String,
    },
    /// Enter `code` at `url`.
    Code {
        code: String,
        url: String,
    },
    /// Send back the secret the sign-in asks for. With `web`, the app runs that cookie sign-in
    /// in its own browser and sends the cookie header; without, the user pastes the value.
    Secret {
        web: Option<WebSignIn>,
    },
    Accounts {
        choices: Vec<AccountChoice>,
    },
}

/// Why a call failed, in words fit for a log or a toast.
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    Failed {
        message: String,
    },
    /// A sign-in the provider turned down, with the reason it gave.
    SignIn {
        problem: Problem,
        message: String,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed { message } | Self::SignIn { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<anyhow::Error> for CoreError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed {
            message: format!("{error:#}"),
        }
    }
}

/// What the core tells the app. Every method is called from a Rust thread, never the main
/// one, and must return quickly.
#[uniffi::export(callback_interface)]
pub trait EventListener: Send + Sync {
    fn on_state(&self, state: PlayerState);
    fn on_position(&self, position_ms: u64);
    fn on_queue(&self, queue: QueueSnapshot);
    fn on_session(&self, slug: String, state: SessionState);
    fn on_prompt(&self, slug: String, prompt: Prompt);
    fn on_error(&self, message: String);
}

type Answer<T> = Result<T, CoreError>;

/// Starts the core with its files under `data_dir`, or hands an already running core a new
/// listener. It restores the queue paused where it was and every stored session in the
/// background, and emits the providers, the queue and the player state before any network
/// answer.
#[uniffi::export]
pub fn init(data_dir: String, listener: Box<dyn EventListener>) -> Answer<()> {
    Ok(core::start(data_dir.into(), listener)?)
}

/// Every provider the app knows, in the order the app lists them.
#[uniffi::export]
pub fn providers() -> Answer<Vec<ProviderInfo>> {
    Ok(core::get()?.providers())
}

/// Searches every provider with a session at once.
#[uniffi::export]
pub async fn search(query: String) -> Answer<SearchResults> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { worker.search(&query).await })
        .await?)
}

/// Replaces the queue with `ids` and plays the one at `index`. `origin` names where the list
/// came from, such as a search, and comes back on the queue.
#[uniffi::export]
pub async fn play(ids: Vec<String>, index: u32, origin: Option<String>) -> Answer<()> {
    let core = core::get()?;
    let worker = core.clone();
    let items = core.spawn(async move { worker.items(ids).await }).await?;
    core.send(Command::Play {
        items,
        index: index as usize,
        origin,
    })?;
    Ok(())
}

/// Pauses what plays, or plays what is paused, from where it stopped.
#[uniffi::export]
pub fn toggle() -> Answer<()> {
    Ok(core::get()?.send(Command::Toggle)?)
}

/// Plays what is paused, from where it stopped. Does nothing while playing.
#[uniffi::export]
pub fn resume() -> Answer<()> {
    Ok(core::get()?.send(Command::Resume)?)
}

/// Pauses what plays. Does nothing while paused.
#[uniffi::export]
pub fn pause() -> Answer<()> {
    Ok(core::get()?.send(Command::Pause)?)
}

/// The system interrupted the audio session (a call, an alarm, Siri): playback pauses.
#[uniffi::export]
pub fn audio_interrupted() -> Answer<()> {
    Ok(core::get()?.send(Command::Interrupted)?)
}

/// The interruption ended. Playback resumes when it was playing before and the system says
/// `should_resume`.
#[uniffi::export]
pub fn audio_resumed(should_resume: bool) -> Answer<()> {
    Ok(core::get()?.send(Command::Resumed(should_resume))?)
}

/// The output device went away, such as headphones pulled out, or the media services were
/// reset. Playback pauses and the engines are rebuilt on the next play.
#[uniffi::export]
pub fn route_changed() -> Answer<()> {
    Ok(core::get()?.send(Command::RouteLost)?)
}

#[uniffi::export]
pub fn next() -> Answer<()> {
    Ok(core::get()?.send(Command::Next)?)
}

/// Goes back a track, or to the start of this one once a few seconds of it have played.
#[uniffi::export]
pub fn previous() -> Answer<()> {
    Ok(core::get()?.send(Command::Previous)?)
}

#[uniffi::export]
pub fn seek(position_ms: u64) -> Answer<()> {
    let at = Duration::from_millis(position_ms);
    Ok(core::get()?.send(Command::Seek(at))?)
}

/// Shuffles what is still to come, or puts the queue back in its own order.
#[uniffi::export]
pub fn set_shuffle(on: bool) -> Answer<()> {
    Ok(core::get()?.send(Command::Shuffle(on))?)
}

/// Moves repeat from off to all to one and back to off.
#[uniffi::export]
pub fn cycle_repeat() -> Answer<()> {
    Ok(core::get()?.send(Command::CycleRepeat)?)
}

/// Puts tracks right after the one playing.
#[uniffi::export]
pub async fn enqueue_next(ids: Vec<String>) -> Answer<()> {
    let core = core::get()?;
    let worker = core.clone();
    let items = core.spawn(async move { worker.items(ids).await }).await?;
    Ok(core.send(Command::EnqueueNext(items))?)
}

/// Puts tracks at the end of the queue.
#[uniffi::export]
pub async fn enqueue_last(ids: Vec<String>) -> Answer<()> {
    let core = core::get()?;
    let worker = core.clone();
    let items = core.spawn(async move { worker.items(ids).await }).await?;
    Ok(core.send(Command::EnqueueLast(items))?)
}

/// Takes the track at `index` of the queue's play order out. The track playing stays.
#[uniffi::export]
pub fn remove(index: u32) -> Answer<()> {
    Ok(core::get()?.send(Command::Remove(index as usize))?)
}

/// The player as it was last reported through `on_state`.
#[uniffi::export]
pub fn player_state() -> Answer<PlayerState> {
    Ok(core::get()?.published().0)
}

/// The queue as it was last reported through `on_queue`.
#[uniffi::export]
pub fn queue() -> Answer<QueueSnapshot> {
    Ok(core::get()?.published().1)
}

// Home, the library and the pages.

/// The home of every provider with a session, woven a shelf at a time, each shelf naming its
/// provider.
#[uniffi::export]
pub async fn home() -> Answer<Vec<HomeSection>> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core.spawn(library::home(worker)).await?)
}

/// One page of a provider's library. Pages are numbered from zero; page zero reads the part
/// afresh, and `has_more` says whether another page follows. A guest session lists nothing.
#[uniffi::export]
pub async fn library(slug: String, part: Part, page: u32) -> Answer<LibraryPage> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::page(worker, &slug, part, page).await })
        .await?)
}

/// The first page of a library part as it was last listed, to draw before the network
/// answers, or `None` before the first listing.
#[uniffi::export]
pub fn cached_library(slug: String, part: Part) -> Answer<Option<LibraryPage>> {
    let core = core::get()?;
    Ok(library::snapshot(&core, &slug, part))
}

/// Every id in the signed-in providers' libraries, by kind, for the hearts.
#[uniffi::export]
pub async fn saved_ids() -> Answer<SavedIds> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core.spawn(library::saved(worker)).await?)
}

/// Puts the item `id`, of the kind `kind` names, into its provider's library or takes it out.
#[uniffi::export]
pub async fn set_saved(id: String, kind: Part, saved: bool) -> Answer<()> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::set_saved(worker, &id, kind, saved).await })
        .await?)
}

#[uniffi::export]
pub async fn album(id: String) -> Answer<AlbumDetail> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::album(worker, &id).await })
        .await?)
}

/// A playlist with its first tracks; `continuation`, when set, fetches the rest through
/// `playlist_more`.
#[uniffi::export]
pub async fn playlist(id: String) -> Answer<PlaylistDetail> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::playlist(worker, &id).await })
        .await?)
}

/// The tracks of the playlist `id` after `continuation`.
#[uniffi::export]
pub async fn playlist_more(id: String, continuation: String) -> Answer<PlaylistMore> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::playlist_more(worker, &id, &continuation).await })
        .await?)
}

/// The url of the Canvas of the track `id`, the short looping MP4 Spotify shows while it
/// plays, or `None` when it has none. Only Spotify tracks have one.
#[uniffi::export]
pub async fn canvas(id: String) -> Answer<Option<String>> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::canvas(worker, &id).await })
        .await?)
}

/// An artist with their top tracks and albums.
#[uniffi::export]
pub async fn artist(id: String) -> Answer<ArtistDetail> {
    let core = core::get()?;
    let worker = core.clone();
    Ok(core
        .spawn(async move { library::artist(worker, &id).await })
        .await?)
}
