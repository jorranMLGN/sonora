use std::collections::HashMap;

use gpui::{Context, Entity, Task};

use crate::{Io, Playback, Session, SessionEvent, join};

#[derive(Clone)]
struct Held {
    large: Option<String>,
    max: Option<String>,
}

pub struct Cover {
    session: Entity<Session>,
    playback: Entity<Playback>,
    album: Option<String>,
    large: Option<String>,
    max: Option<String>,
    cache: HashMap<String, Held>,
    io: Io,
    task: Option<Task<()>>,
}

impl Cover {
    pub fn new(
        session: Entity<Session>,
        playback: Entity<Playback>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&playback, |this, _, cx| this.follow(cx))
            .detach();
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
            album: None,
            large: None,
            max: None,
            cache: HashMap::new(),
            io,
            task: None,
        }
    }

    pub fn large(&self) -> Option<&str> {
        self.large.as_deref()
    }

    pub fn max(&self) -> Option<&str> {
        self.max.as_deref().or(self.large.as_deref())
    }

    /// Returns the large artwork only when it belongs to `album`.
    pub(crate) fn large_for(&self, album: &str) -> Option<&str> {
        self.large
            .as_deref()
            .filter(|_| self.album.as_deref() == Some(album))
    }

    fn forget(&mut self, slug: &str, cx: &mut Context<Self>) {
        self.cache
            .retain(|id, _| music::tag::slug_of(id) != Some(slug));
        if self.album.as_deref().and_then(music::tag::slug_of) != Some(slug) {
            return;
        }
        self.task = None;
        self.album = None;
        self.large = None;
        self.max = None;
        cx.notify();
    }

    fn follow(&mut self, cx: &mut Context<Self>) {
        let album = self
            .playback
            .read(cx)
            .track()
            .and_then(|track| track.album_id.clone());
        if album == self.album {
            return;
        }
        self.task = None;
        self.album = album.clone();
        let held = album.as_ref().and_then(|id| self.cache.get(id)).cloned();
        self.large = held.as_ref().and_then(|held| held.large.clone());
        self.max = held.and_then(|held| held.max);
        cx.notify();

        let Some(id) = album else {
            return;
        };
        if self.large.is_some() {
            return;
        }
        self.load(id, cx);
    }

    fn load(&mut self, id: String, cx: &mut Context<Self>) {
        let session = self.session.read(cx);
        let client = session
            .slug_for(&id)
            .and_then(|slug| session.client_for_slug(slug));
        let Some(client) = client else {
            return;
        };

        let io = self.io.clone();
        let wanted = id.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let found = join(io.spawn(async move { client.album(&wanted).await })).await;

            this.update(cx, |this, cx| {
                this.task = None;
                match found {
                    Ok(detail) => {
                        let held = Held {
                            large: detail.album.cover_large,
                            max: detail.cover_max,
                        };
                        if held.large.is_none() && held.max.is_none() {
                            return;
                        }
                        this.cache.insert(id.clone(), held.clone());
                        if this.album.as_deref() == Some(id.as_str()) {
                            this.large = held.large;
                            this.max = held.max;
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        log::warn!("cover: cannot load {id}: {error:#}");
                        crate::noted(&error, cx);
                    }
                }
            })
            .ok();
        }));
    }
}
