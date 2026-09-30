//! The Canvas of the playing track: its url from the provider, its bytes from the CDN, and the
//! `Player` that loops it. Only the current track ever has a player, so a late answer for a
//! track already skipped is dropped rather than shown.

use std::collections::HashMap;

use gpui::{App, Context, Entity, Task};
use music::video::{Player, download};

use crate::{AppSettings, Codec, Io, Playback, PlaybackState, Session, SessionEvent, join};

/// Follows the playing track and owns the one `Player` a canvas needs. Nothing here draws; a
/// view reads `player()` and `generation()`.
pub struct Canvas {
    session: Entity<Session>,
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    codec: Entity<Codec>,
    /// The tagged id of the track this entity is following.
    track: Option<String>,
    /// What each track answered, `None` meaning it has no canvas, so a replay asks nothing.
    urls: HashMap<String, Option<String>>,
    player: Option<Player>,
    /// Grows each time `player` is set or dropped, so a view can tell a new clip's first frame
    /// from the last one of the clip before it.
    generation: u64,
    /// The track whose lookup, download or start failed. It is not tried again until the
    /// track changes or a decoder arrives, since `Playback` notifies twice a second.
    failed: Option<String>,
    io: Io,
    task: Option<Task<()>>,
}

impl Canvas {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        settings: Entity<AppSettings>,
        codec: Entity<Codec>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&playback, |this, _, cx| this.follow(cx))
            .detach();
        cx.observe(&settings, |this, _, cx| this.follow(cx))
            .detach();
        cx.observe(&codec, |this, _, cx| this.retry(cx)).detach();
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedOut(slug) => this.forget(slug, cx),
            SessionEvent::SignedIn(_)
            | SessionEvent::Reconnected(_)
            | SessionEvent::LocalChanged => {}
        })
        .detach();
        Self {
            session,
            playback,
            settings,
            codec,
            track: None,
            urls: HashMap::new(),
            player: None,
            generation: 0,
            failed: None,
            io,
            task: None,
        }
    }

    /// The player for the current track, while the setting allows one and a decoder loaded.
    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// Which clip `player()` is; see the field.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether a canvas should play at all: the setting, and motion, since a user who asked for
    /// less of it is never asked to install a decoder for a video that will not move.
    fn wanted(&self, cx: &App) -> bool {
        self.settings.read(cx).canvas() && !cx.reduce_motion()
    }

    fn set_player(&mut self, player: Option<Player>, cx: &mut Context<Self>) {
        if player.is_none() && self.player.is_none() {
            return;
        }
        self.player = player;
        self.generation += 1;
        cx.notify();
    }

    /// Runs on every playback and settings change, so the common case (same track, nothing to
    /// do) has to return before any work.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let playback = self.playback.read(cx);
        let playing = *playback.state() == PlaybackState::Playing;
        let track = playback.track().and_then(|track| track.id.clone());
        let wanted = self.wanted(cx);
        if let Some(player) = &self.player {
            player.set_running(playing);
        }
        if track != self.track {
            self.task = None;
            self.failed = None;
            self.track = track.clone();
            self.set_player(None, cx);
            cx.notify();
        } else if !wanted {
            self.task = None;
            self.set_player(None, cx);
            return;
        }
        let Some(id) = track else { return };
        if wanted
            && self.player.is_none()
            && self.task.is_none()
            && self.failed.as_deref() != Some(id.as_str())
        {
            self.load(id, cx);
        }
    }

    /// The decoder arrived: try the current track again, a failed one included, since a new
    /// decoder is the one thing that could have changed its outcome.
    fn retry(&mut self, cx: &mut Context<Self>) {
        let ready = self.codec.update(cx, |codec, _| codec.ready());
        if ready
            && self.player.is_none()
            && self.task.is_none()
            && self.wanted(cx)
            && let Some(id) = self.track.clone()
        {
            self.failed = None;
            self.load(id, cx);
        }
    }

    fn forget(&mut self, slug: &str, cx: &mut Context<Self>) {
        self.urls
            .retain(|id, _| music::tag::slug_of(id) != Some(slug));
        if self.track.as_deref().and_then(music::tag::slug_of) == Some(slug) {
            self.task = None;
            self.set_player(None, cx);
        }
    }

    /// Resolves, downloads and starts. Returns at once when the track is known to have no
    /// canvas, or has one but no decoder is loaded yet (then it only raises the prompt, and
    /// `retry` comes back once the decoder arrives). Every way out of the task clears `task`,
    /// so `follow` and `retry` can always start the next one.
    fn load(&mut self, id: String, cx: &mut Context<Self>) {
        match self.urls.get(&id) {
            Some(None) => return,
            Some(Some(_)) if !self.codec.update(cx, |codec, _| codec.ready()) => {
                self.codec.update(cx, |codec, cx| codec.want(cx));
                return;
            }
            _ => {}
        }
        let session = self.session.read(cx);
        let Some(client) = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug))
        else {
            return;
        };
        let known = self.urls.get(&id).cloned().flatten();
        let io = self.io.clone();
        let wanted = id.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let url = match known {
                Some(url) => Ok(Some(url)),
                None => join(io.spawn(async move { client.canvas(&wanted).await })).await,
            };
            let url = this
                .update(cx, |this, cx| {
                    let url = match url {
                        Ok(url) => url,
                        Err(error) => {
                            log::warn!("canvas: cannot look up {id}: {error:#}");
                            this.task = None;
                            this.failed = Some(id.clone());
                            return None;
                        }
                    };
                    this.urls.insert(id.clone(), url.clone());
                    let ready = this.codec.update(cx, |codec, cx| {
                        let ready = codec.ready();
                        if !ready && url.is_some() {
                            codec.want(cx);
                        }
                        ready
                    });
                    let current = this.track.as_deref() == Some(id.as_str());
                    match (url, current && ready && this.wanted(cx)) {
                        (Some(url), true) => Some(url),
                        _ => {
                            this.task = None;
                            None
                        }
                    }
                })
                .ok()
                .flatten();
            let Some(url) = url else { return };
            let fetched = join(io.spawn(async move { download(&url).await })).await;
            // parsing the clip and hashing the decoder stay off the ui thread
            let started = match fetched {
                Ok(bytes) => join(io.spawn_blocking(move || Player::start(bytes))).await,
                Err(error) => Err(error),
            };
            this.update(cx, |this, cx| {
                this.task = None;
                if this.track.as_deref() != Some(id.as_str()) || !this.wanted(cx) {
                    return;
                }
                match started {
                    Ok(player) => {
                        let playing = *this.playback.read(cx).state() == PlaybackState::Playing;
                        player.set_running(playing);
                        this.set_player(Some(player), cx);
                    }
                    Err(error) => {
                        log::warn!("canvas: cannot play {id}: {error:#}");
                        this.failed = Some(id.clone());
                    }
                }
            })
            .ok();
        }));
    }
}
