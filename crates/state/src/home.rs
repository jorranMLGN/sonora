use std::collections::HashSet;
use std::rc::Rc;

use std::sync::Arc;

use gpui::{App, Context, Entity, Task};
use music::{GenreItem, GenreSection, MusicApi, Track};

use crate::{Io, Library, LibraryPart, LibraryState, Session, SessionEvent, join};

const GROUP_SIZE: usize = 10;
const LIMIT: usize = GROUP_SIZE * 3;

pub struct Home {
    library: Entity<Library>,
    session: Entity<Session>,
    io: Io,
    listen_again: Rc<Vec<Track>>,
    quick_picks: Rc<Vec<Track>>,
    quick_picks_seed: u64,
    sections: Rc<Vec<GenreSection>>,
    feeding: bool,
    task: Option<Task<()>>,
    naming: Option<Task<()>>,
}

impl Home {
    pub fn new(
        library: Entity<Library>,
        session: Entity<Session>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        let quick_picks_seed = fastrand::u64(..);
        let quick_picks = picks(&library, quick_picks_seed, cx);

        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedIn(_) | SessionEvent::SignedOut(_) => {
                this.task = None;
                this.naming = None;
                this.listen_again = Rc::new(Vec::new());
                this.quick_picks = Rc::new(Vec::new());
                this.sections = Rc::new(Vec::new());
                this.feeding = false;
                this.feed(cx);
            }
            SessionEvent::Reconnected(_) => {}
        })
        .detach();

        cx.observe(&library, |this, library, cx| {
            match library.read(cx).state(cx) {
                LibraryState::Ready { .. } if this.quick_picks.is_empty() => {
                    this.quick_picks = picks(&library, this.quick_picks_seed, cx);
                }
                LibraryState::Empty | LibraryState::Failed(_) => {
                    this.quick_picks = Rc::new(Vec::new());
                }
                _ => return,
            }
            cx.notify();
        })
        .detach();

        let mut home = Self {
            library,
            session,
            io,
            listen_again: Rc::new(Vec::new()),
            quick_picks,
            quick_picks_seed,
            sections: Rc::new(Vec::new()),
            feeding: false,
            task: None,
            naming: None,
        };
        home.feed(cx);
        home
    }

    pub fn sections(&self) -> Rc<Vec<GenreSection>> {
        self.sections.clone()
    }

    pub fn is_feeding(&self) -> bool {
        self.feeding
    }

    pub fn feed(&mut self, cx: &mut Context<Self>) {
        if self.feeding || !self.sections.is_empty() {
            return;
        }
        let asking = self.providers(cx);
        if asking.is_empty() {
            return;
        }

        self.feeding = true;
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let sent: Vec<_> = asking
                .into_iter()
                .map(|(name, client)| (name, io.spawn(async move { client.home().await })))
                .collect();

            let mut feeds = Vec::new();
            for (name, handle) in sent {
                match join(handle).await {
                    Ok(feed) => feeds.push((name, feed)),
                    Err(error) => log::warn!("home: cannot load the {name} feed: {error:#}"),
                }
            }

            this.update(cx, |this, cx| {
                this.feeding = false;
                let listen: Vec<Vec<Track>> = feeds
                    .iter()
                    .map(|(_, feed)| feed.listen_again.clone())
                    .collect();
                let picks: Vec<Vec<Track>> = feeds
                    .iter()
                    .filter_map(|(_, feed)| feed.quick_picks.clone())
                    .collect();
                let sections: Vec<Vec<GenreSection>> = feeds
                    .iter()
                    .map(|(name, feed)| credited(name, &feed.sections))
                    .collect();

                this.listen_again = Rc::new(woven(listen));
                if !picks.is_empty() {
                    this.quick_picks = Rc::new(woven(picks));
                }
                let sections = woven(sections);
                this.sections = Rc::new(pruned(&sections));
                this.name_playlists(sections, cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn providers(&self, cx: &Context<Self>) -> Vec<(&'static str, Arc<dyn MusicApi>)> {
        let session = self.session.read(cx);

        session
            .active_slugs()
            .into_iter()
            .filter_map(|slug| {
                let client = session.client_for_slug(slug)?;
                Some((session.provider_name_for(slug).unwrap_or(slug), client))
            })
            .collect()
    }

    fn name_playlists(&mut self, sections: Vec<GenreSection>, cx: &mut Context<Self>) {
        if !sections
            .iter()
            .any(|section| section.items.iter().any(blank))
        {
            return;
        }
        let asking = self.providers(cx);
        if asking.is_empty() {
            return;
        }

        let io = self.io.clone();
        self.naming = Some(cx.spawn(async move |this, cx| {
            let sent: Vec<_> = asking
                .into_iter()
                .map(|(name, client)| {
                    let mine: Vec<GenreSection> = sections
                        .iter()
                        .filter(|section| section.provider.as_deref() == Some(name))
                        .cloned()
                        .collect();
                    (
                        name,
                        io.spawn(async move { client.name_home_playlists(mine).await }),
                    )
                })
                .collect();

            let mut named = Vec::new();
            for (name, handle) in sent {
                let Ok(part) = handle.await else {
                    continue;
                };
                named.push(credited(name, &part));
            }

            this.update(cx, |this, cx| {
                this.sections = Rc::new(pruned(&woven(named)));
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn listen_again(&self) -> Rc<Vec<Track>> {
        self.listen_again.clone()
    }

    pub fn quick_picks(&self) -> Rc<Vec<Track>> {
        self.quick_picks.clone()
    }

    pub fn is_loading(&self, cx: &App) -> bool {
        self.library.read(cx).loading(LibraryPart::Tracks, cx)
    }
}

fn woven<T>(lanes: Vec<Vec<T>>) -> Vec<T> {
    let rounds = lanes.iter().map(Vec::len).max().unwrap_or(0);
    let mut lanes: Vec<_> = lanes.into_iter().map(Vec::into_iter).collect();
    let mut woven = Vec::new();

    for _ in 0..rounds {
        for lane in &mut lanes {
            woven.extend(lane.next());
        }
    }

    woven
}

fn credited(provider: &str, sections: &[GenreSection]) -> Vec<GenreSection> {
    sections
        .iter()
        .map(|section| GenreSection {
            provider: Some(provider.to_owned()),
            ..section.clone()
        })
        .collect()
}

fn blank(item: &GenreItem) -> bool {
    match item {
        GenreItem::Playlist(playlist) => playlist.name.is_empty(),
        _ => false,
    }
}

fn pruned(sections: &[GenreSection]) -> Vec<GenreSection> {
    sections
        .iter()
        .filter_map(|section| {
            let items: Vec<GenreItem> = section
                .items
                .iter()
                .filter(|item| !blank(item))
                .cloned()
                .collect();

            (!items.is_empty()).then(|| GenreSection {
                title: section.title.clone(),
                items,
                provider: section.provider.clone(),
            })
        })
        .collect()
}

fn picks(library: &Entity<Library>, seed: u64, cx: &App) -> Rc<Vec<Track>> {
    let tracks = match library.read(cx).state(cx) {
        LibraryState::Ready { tracks, .. } => mixed_tracks(tracks, seed),
        _ => Vec::new(),
    };
    Rc::new(tracks)
}

fn mixed_tracks(tracks: &[Track], seed: u64) -> Vec<Track> {
    let mut random = fastrand::Rng::with_seed(seed);
    let mut selected = tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.playable && track.id.is_some())
        .map(|(index, _)| index)
        .take(GROUP_SIZE)
        .collect::<Vec<_>>();
    let recent = selected.iter().copied().collect::<HashSet<_>>();
    let mut remaining = tracks
        .iter()
        .enumerate()
        .filter(|(index, track)| track.playable && track.id.is_some() && !recent.contains(index))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    random.shuffle(&mut remaining);

    let random_count = GROUP_SIZE.min(remaining.len());
    selected.extend(remaining.drain(..random_count));

    let mut artists = selected
        .iter()
        .map(|index| artist_key(&tracks[*index]))
        .collect::<HashSet<_>>();
    let mut fallback = Vec::new();
    let mut diverse_count = 0;
    for index in remaining {
        if diverse_count < GROUP_SIZE && artists.insert(artist_key(&tracks[index])) {
            selected.push(index);
            diverse_count += 1;
        } else {
            fallback.push(index);
        }
    }

    selected.extend(fallback.into_iter().take(LIMIT - selected.len()));
    let mut mixed = selected
        .into_iter()
        .map(|index| tracks[index].clone())
        .collect::<Vec<_>>();
    random.shuffle(&mut mixed);
    mixed
}

fn artist_key(track: &Track) -> &str {
    track
        .artist_refs
        .first()
        .map(|artist| artist.id.as_deref().unwrap_or(&artist.name))
        .unwrap_or(&track.artists)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use music::{ArtistRef, Track};

    use super::{GROUP_SIZE, LIMIT, mixed_tracks};

    fn track(index: usize, artist: usize, playable: bool) -> Track {
        Track {
            id: Some(format!("track-{index}")),
            name: format!("Track {index}"),
            playable,
            artists: format!("Artist {artist}"),
            artist_refs: vec![ArtistRef {
                name: format!("Artist {artist}"),
                id: Some(format!("artist-{artist}")),
            }],
            album: String::new(),
            album_id: None,
            cover: None,
            duration: Duration::from_secs(180),
            added_at: None,
            added_by: None,
            playcount: None,
            popularity: 0,
            explicit: false,
            track_number: 0,
            disc_number: 0,
            tags: Vec::new(),
            languages: Vec::new(),
            credits: Vec::new(),
        }
    }

    #[test]
    fn mixed_selection_is_stable_and_has_no_duplicates() {
        let tracks = (0..60)
            .map(|index| track(index, index, true))
            .collect::<Vec<_>>();

        let first = mixed_tracks(&tracks, 42);
        let second = mixed_tracks(&tracks, 42);
        let ids = first
            .iter()
            .filter_map(|track| track.id.as_ref())
            .collect::<HashSet<_>>();

        assert_eq!(first, second);
        assert_eq!(first.len(), LIMIT);
        assert_eq!(ids.len(), LIMIT);
        for index in 0..GROUP_SIZE {
            let expected = format!("track-{index}");
            assert!(
                first
                    .iter()
                    .any(|track| track.id.as_deref() == Some(expected.as_str()))
            );
        }
    }

    #[test]
    fn mixed_selection_excludes_unavailable_tracks() {
        let tracks = (0..50)
            .map(|index| track(index, index, index % 2 == 0))
            .collect::<Vec<_>>();

        let selected = mixed_tracks(&tracks, 7);

        assert_eq!(selected.len(), 25);
        assert!(selected.iter().all(|track| track.playable));
    }

    #[test]
    fn mixed_selection_adds_artist_variety() {
        let tracks = (0..64)
            .map(|index| track(index, index.saturating_sub(23), true))
            .collect::<Vec<_>>();

        let selected = mixed_tracks(&tracks, 99);
        let artists = selected
            .iter()
            .map(|track| &track.artists)
            .collect::<HashSet<_>>();

        assert!(artists.len() > GROUP_SIZE);
    }
}
