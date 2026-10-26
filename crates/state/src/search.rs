use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use gpui::{App, Context, Entity, Task};
use music::{Album, ArtistRef, MusicApi, Playlist, SavedArtist, Track};
use ui::Sort;

use crate::{Io, Library, Network, Session, SessionEvent, Shelf, join};

const DEBOUNCE: Duration = Duration::from_millis(250);
const LIMIT: usize = 20;
/// How many hits of one kind a lane keeps when the lens shows only that kind.
const FOCUSED: usize = 50;
const EXACT: u32 = 100;
const PREFIX: u32 = 80;
const WORD: u32 = 60;
const INSIDE: u32 = 40;
const TITLE: u32 = 3;
const ARTIST: u32 = 2;
const ALBUM: u32 = 1;
const NAME: u32 = 1;
const TOP: u32 = 95;
const TAIL: u32 = 35;
const DERIVED: u32 = 80;
const MINE: u32 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Song,
    Artist,
    Album,
    Playlist,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Song, Kind::Artist, Kind::Album, Kind::Playlist];
}

/// What the search screen narrows its hits to. The default shows every kind from every
/// signed-in provider, ranked by relevance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lens {
    pub kind: Option<Kind>,
    /// Providers the user switched off. A deny list, so a provider that signs in later takes
    /// part without touching the lens.
    pub hidden: Vec<&'static str>,
    pub mine_only: bool,
    /// `None` is relevance.
    pub order: Option<(Axis, Sort)>,
}

/// Which value of a hit the search screen orders by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Title,
    Artist,
    Duration,
    Year,
}

impl Axis {
    /// The axes that mean something for `kind`; `None` is the mixed All tab, where an axis only
    /// some kinds carry orders those hits and leaves the rest in relevance order.
    pub fn of(kind: Option<Kind>) -> &'static [Axis] {
        match kind {
            None => &[Axis::Title, Axis::Artist, Axis::Duration],
            Some(Kind::Song) => &[Axis::Title, Axis::Artist, Axis::Duration],
            Some(Kind::Album) => &[Axis::Title, Axis::Artist, Axis::Year],
            Some(Kind::Artist | Kind::Playlist) => &[Axis::Title],
        }
    }
}

impl Lens {
    /// Shows `kind` only, dropping a sort axis that kind has no value for.
    pub fn with_kind(mut self, kind: Option<Kind>) -> Self {
        self.kind = kind;
        self.order = self.order.filter(|(axis, _)| Axis::of(kind).contains(axis));
        self
    }

    /// Lets `slug` take part (`on`) or hides it.
    pub fn with_source(self, slug: &'static str, on: bool) -> Self {
        let mut lens = self.forget(slug);
        if !on {
            lens.hidden.push(slug);
        }
        lens
    }

    /// Steps `axis` through ascending, descending and back to relevance.
    pub fn cycled(mut self, axis: Axis) -> Self {
        self.order = match self.order {
            Some((current, Sort::Ascending)) if current == axis => Some((axis, Sort::Descending)),
            Some((current, Sort::Descending)) if current == axis => None,
            _ => Some((axis, Sort::Ascending)),
        };
        self
    }

    /// This lens with the provider and library filters dropped, keeping the tab and the order.
    pub fn unfiltered(self) -> Self {
        Self {
            kind: self.kind,
            order: self.order,
            ..Self::default()
        }
    }

    /// Drops `slug` from the hidden sources, for a provider that signed out.
    pub fn forget(mut self, slug: &str) -> Self {
        self.hidden.retain(|known| *known != slug);
        self
    }
}

/// Which of a provider's search calls a lens needs. Artists are also derived from tracks and
/// albums, so a provider without a direct artist search still lists them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Parts {
    songs: bool,
    albums: bool,
    playlists: bool,
    artists: bool,
}

impl Parts {
    fn of(kind: Option<Kind>) -> Self {
        match kind {
            None => Self {
                songs: true,
                albums: true,
                playlists: true,
                artists: true,
            },
            Some(Kind::Song) => Self {
                songs: true,
                ..Self::default()
            },
            Some(Kind::Artist) => Self {
                songs: true,
                albums: true,
                artists: true,
                ..Self::default()
            },
            Some(Kind::Album) => Self {
                albums: true,
                ..Self::default()
            },
            Some(Kind::Playlist) => Self {
                playlists: true,
                ..Self::default()
            },
        }
    }

    fn covers(self, other: Self) -> bool {
        (self.songs || !other.songs)
            && (self.albums || !other.albums)
            && (self.playlists || !other.playlists)
            && (self.artists || !other.artists)
    }
}

/// What one search fetched: the query, the calls and the providers it asked.
#[derive(Clone, Debug, PartialEq)]
struct Served {
    query: String,
    parts: Parts,
    slugs: Vec<&'static str>,
}

impl Served {
    /// Whether this answer already holds everything `wanted` asks for, so no fetch is needed.
    fn covers(&self, wanted: &Served) -> bool {
        self.query == wanted.query
            && self.parts.covers(wanted.parts)
            && wanted.slugs.iter().all(|slug| self.slugs.contains(slug))
    }
}

#[derive(Clone)]
pub struct ArtistHit {
    pub name: String,
    pub id: Option<String>,
    pub cover: Option<String>,
    pub saved: usize,
}

#[derive(Clone)]
pub struct AlbumHit {
    pub id: String,
    pub name: String,
    pub artists: String,
    pub artist_refs: Vec<ArtistRef>,
    pub cover: Option<String>,
    pub year: i32,
}

#[derive(Clone)]
pub struct PlaylistHit {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub cover: Option<String>,
}

#[derive(Clone)]
pub enum Hit {
    Song(Track),
    Artist(ArtistHit),
    Album(AlbumHit),
    Playlist(PlaylistHit),
}

#[derive(Clone)]
pub struct Found {
    pub slug: &'static str,
    pub hit: Hit,
    /// Whether the hit is in the user's library rather than only in the catalog.
    pub mine: bool,
}

impl Found {
    pub fn kind(&self) -> Kind {
        self.hit.kind()
    }
}

impl Hit {
    pub fn kind(&self) -> Kind {
        match self {
            Hit::Song(_) => Kind::Song,
            Hit::Artist(_) => Kind::Artist,
            Hit::Album(_) => Kind::Album,
            Hit::Playlist(_) => Kind::Playlist,
        }
    }
}

#[derive(Default)]
struct Catalog {
    tracks: Vec<Track>,
    albums: Vec<Album>,
    playlists: Vec<Playlist>,
    artists: Vec<SavedArtist>,
}

impl Catalog {
    fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.albums.is_empty()
            && self.playlists.is_empty()
            && self.artists.is_empty()
    }
}

struct Query {
    terms: Vec<String>,
    whole: String,
}

impl Query {
    fn new(query: &str) -> Self {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        Self {
            whole: terms.join(" "),
            terms,
        }
    }
}

struct Scored {
    score: u32,
    popularity: u32,
    mine: bool,
    hit: Hit,
}

pub struct Search {
    query: String,
    served: Option<Served>,
    lens: Lens,
    catalogs: Vec<(&'static str, Catalog)>,
    portraits: HashMap<String, String>,
    hits: Vec<Found>,
    loading: bool,
    error: Option<String>,
    session: Entity<Session>,
    library: Entity<Library>,
    io: Io,
    task: Option<Task<()>>,
    faces: Option<Task<()>>,
}

impl Search {
    pub fn new(
        session: Entity<Session>,
        library: Entity<Library>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::WentOffline => {}
            SessionEvent::SignedIn(_) | SessionEvent::SignedOut(_) => {
                if let SessionEvent::SignedOut(slug) = event {
                    this.lens = std::mem::take(&mut this.lens).forget(slug);
                    this.portraits.clear();
                }
                let pending = this.query.clone();
                this.query.clear();
                this.catalogs.clear();
                this.ask(&pending, cx);
            }
            SessionEvent::Reconnected(_) | SessionEvent::LocalChanged => {}
        })
        .detach();

        cx.observe(&library, |this, _, cx| this.rank(cx)).detach();

        Self {
            query: String::new(),
            served: None,
            lens: Lens::default(),
            catalogs: Vec::new(),
            portraits: HashMap::new(),
            hits: Vec::new(),
            loading: false,
            error: None,
            session,
            library,
            io,
            task: None,
            faces: None,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Every signed-in streaming provider, whether the lens hides it or not.
    pub fn sources(&self, cx: &App) -> Vec<&'static str> {
        self.session
            .read(cx)
            .active_slugs()
            .into_iter()
            .filter(|slug| *slug != music::tag::LOCAL)
            .collect()
    }

    /// The providers that get a results column: the sources the lens does not hide.
    pub fn lanes(&self, cx: &App) -> Vec<&'static str> {
        self.sources(cx)
            .into_iter()
            .filter(|slug| !self.lens.hidden.contains(slug))
            .collect()
    }

    pub fn hits(&self) -> &[Found] {
        &self.hits
    }

    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Found> {
        self.hits.iter().filter(move |found| found.kind() == kind)
    }

    /// The top hit, shown only on the All tab in relevance order, where "top" means something.
    pub fn best(&self) -> Option<&Found> {
        match self.lens.kind.is_none() && self.lens.order.is_none() {
            true => self.hits.first(),
            false => None,
        }
    }

    pub fn lens(&self) -> &Lens {
        &self.lens
    }

    /// Applies a new lens. A narrower lens reranks what is already fetched; a wider one drops
    /// any fetch in flight and asks again.
    pub fn set_lens(&mut self, lens: Lens, cx: &mut Context<Self>) {
        if self.lens == lens {
            return;
        }
        self.lens = lens;
        let query = self.query.clone();
        let wanted = self.wanted(&query, cx);
        match self
            .served
            .as_ref()
            .is_some_and(|served| served.covers(&wanted))
        {
            true => self.rank(cx),
            false => {
                self.task = None;
                self.loading = false;
                self.query.clear();
                self.ask(&query, cx);
            }
        }
    }

    /// What answering `query` under the current lens asks for.
    fn wanted(&self, query: &str, cx: &Context<Self>) -> Served {
        Served {
            query: query.to_owned(),
            parts: Parts::of(self.lens.kind),
            slugs: self
                .providers(cx)
                .into_iter()
                .map(|(slug, _, _)| slug)
                .collect(),
        }
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn ask(&mut self, query: &str, cx: &mut Context<Self>) {
        let query = query.trim().to_owned();
        let wanted = self.wanted(&query, cx);
        let answered = self.loading
            || self
                .served
                .as_ref()
                .is_some_and(|served| served.covers(&wanted));
        if query == self.query && answered {
            return;
        }
        self.query = query.clone();
        self.error = None;

        if query.is_empty() {
            self.task = None;
            self.catalogs.clear();
            self.served = Some(wanted);
            self.loading = false;
            self.rank(cx);
            return;
        }

        self.rank(cx);

        let asking = self.providers(cx);
        if asking.is_empty() {
            self.task = None;
            self.loading = false;
            cx.notify();
            return;
        }

        self.loading = true;
        cx.notify();

        let io = self.io.clone();
        let parts = wanted.parts;
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;

            let sent: Vec<_> = asking
                .into_iter()
                .map(|(slug, name, client)| {
                    let songs = parts.songs.then(|| {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search(&asked).await })
                    });
                    let albums = parts.albums.then(|| {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search_albums(&asked).await })
                    });
                    let playlists = parts.playlists.then(|| {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search_playlists(&asked).await })
                    });
                    let artists = parts.artists.then(|| {
                        let asked = query.clone();
                        io.spawn(async move { client.search_artists(&asked).await })
                    });
                    (slug, name, songs, albums, playlists, artists)
                })
                .collect();

            let mut answered = Vec::new();
            for (slug, name, songs, albums, playlists, artists) in sent {
                answered.push((
                    slug,
                    name,
                    awaited(songs).await,
                    awaited(albums).await,
                    awaited(playlists).await,
                    awaited(artists).await,
                ));
            }

            this.update(cx, |this, cx| {
                if this.query != query {
                    return;
                }
                this.loading = false;

                // a part that failed keeps what it showed before, so a refused request
                // never blanks the page; the query counts as served only once every part
                // answered, so asking it again fetches the rest
                let mut trouble = Vec::new();
                let mut held: Vec<(&'static str, Catalog)> = std::mem::take(&mut this.catalogs);
                this.catalogs = answered
                    .into_iter()
                    .map(|(slug, name, songs, albums, playlists, artists)| {
                        let kept = held
                            .iter()
                            .position(|(known, _)| *known == slug)
                            .map(|at| held.remove(at).1)
                            .unwrap_or_default();
                        let catalog = Catalog {
                            tracks: salvaged(songs, name, kept.tracks, &mut trouble),
                            albums: salvaged(albums, name, kept.albums, &mut trouble),
                            playlists: salvaged(playlists, name, kept.playlists, &mut trouble),
                            artists: salvaged(artists, name, kept.artists, &mut trouble),
                        };
                        (slug, catalog)
                    })
                    .collect();
                let bare = this.catalogs.iter().all(|(_, catalog)| catalog.is_empty());
                match trouble.first() {
                    Some(reason) => Network::failed(reason, cx),
                    None => {
                        Network::reached(cx);
                        this.served = Some(wanted);
                    }
                }
                this.error = (!trouble.is_empty() && bare).then(|| trouble.join(" · "));
                this.rank(cx);
            })
            .ok();
        }));
    }

    fn providers(
        &self,
        cx: &Context<Self>,
    ) -> Vec<(&'static str, &'static str, Arc<dyn MusicApi>)> {
        let session = self.session.read(cx);

        session
            .active_slugs()
            .into_iter()
            .filter(|slug| !self.lens.hidden.contains(slug))
            .filter_map(|slug| {
                let client = session.client_for_slug(slug)?;
                Some((
                    slug,
                    session.provider_name_for(slug).unwrap_or(slug),
                    client,
                ))
            })
            .collect()
    }

    fn fetch_portraits(&mut self, cx: &mut Context<Self>) {
        let mut wanted: Vec<(&'static str, Vec<String>)> = Vec::new();
        for found in &self.hits {
            let Hit::Artist(artist) = &found.hit else {
                continue;
            };
            let Some(id) = artist.id.clone() else {
                continue;
            };
            if self.portraits.contains_key(&id) {
                continue;
            }
            match wanted.iter_mut().find(|(slug, _)| *slug == found.slug) {
                Some((_, ids)) if ids.contains(&id) => {}
                Some((_, ids)) => ids.push(id),
                None => wanted.push((found.slug, vec![id])),
            }
        }

        if wanted.is_empty() {
            return;
        }

        let sending: Vec<(Vec<String>, Arc<dyn MusicApi>)> = {
            let session = self.session.read(cx);
            wanted
                .into_iter()
                .filter_map(|(slug, ids)| Some((ids, session.client_for_slug(slug)?)))
                .collect()
        };
        if sending.is_empty() {
            return;
        }

        let io = self.io.clone();
        let asked: Vec<String> = sending.iter().flat_map(|(ids, _)| ids.clone()).collect();
        self.faces = Some(cx.spawn(async move |this, cx| {
            let sent: Vec<_> = sending
                .into_iter()
                .map(|(ids, client)| io.spawn(async move { client.artist_images(ids).await }))
                .collect();

            let mut found = HashMap::new();
            for handle in sent {
                if let Ok(part) = join(handle).await {
                    found.extend(part);
                }
            }

            this.update(cx, |this, cx| {
                for id in asked {
                    this.portraits.entry(id).or_default();
                }
                this.portraits.extend(found);
                this.rank(cx);
            })
            .ok();
        }));
    }

    fn rank(&mut self, cx: &mut Context<Self>) {
        let query = self.query.trim();
        if query.is_empty() {
            self.hits.clear();
            cx.notify();
            return;
        }

        let found: Vec<Found> = {
            let held = self.library.read(cx);
            let state = held.state(Shelf::Streaming);
            let (tracks, albums, playlists) = (state.tracks(), state.albums(), state.playlists());

            let limit = match self.lens.kind {
                None => LIMIT,
                Some(_) => FOCUSED,
            };
            let empty = Catalog::default();
            let mut all: Vec<(Scored, &'static str)> = Vec::new();
            for slug in self.lanes(cx) {
                let catalog = self
                    .catalogs
                    .iter()
                    .find(|(known, _)| *known == slug)
                    .map_or(&empty, |(_, catalog)| catalog);
                let mine: Vec<Track> = tracks
                    .iter()
                    .filter(|track| owns(slug, track.id.as_deref()))
                    .cloned()
                    .collect();
                let sleeves: Vec<Album> = albums
                    .iter()
                    .filter(|album| owns(slug, Some(&album.id)))
                    .cloned()
                    .collect();
                let lists: Vec<Playlist> = playlists
                    .iter()
                    .filter(|list| owns(slug, Some(&list.id)))
                    .cloned()
                    .collect();

                all.extend(
                    rank(
                        &mine,
                        &sleeves,
                        &lists,
                        catalog,
                        &self.portraits,
                        query,
                        limit,
                    )
                    .into_iter()
                    .map(|scored| (scored, slug)),
                );
            }
            all.sort_by(|(left, _), (right, _)| {
                right
                    .score
                    .cmp(&left.score)
                    .then(right.popularity.cmp(&left.popularity))
            });

            all.into_iter()
                .map(|(scored, slug)| Found {
                    slug,
                    mine: scored.mine,
                    hit: scored.hit,
                })
                .collect()
        };
        self.hits = narrowed(found, &self.lens);
        self.fetch_portraits(cx);
        cx.notify();
    }
}

fn rank(
    library: &[Track],
    albums: &[Album],
    playlists: &[Playlist],
    catalog: &Catalog,
    portraits: &HashMap<String, String>,
    asked: &str,
    limit: usize,
) -> Vec<Scored> {
    let query = Query::new(asked);
    if query.terms.is_empty() {
        return Vec::new();
    }

    let mut all = songs(library, &catalog.tracks, &query, limit);
    all.extend(artists(
        library,
        &catalog.tracks,
        albums,
        &catalog.artists,
        portraits,
        &query,
        limit,
    ));
    all.extend(albums_of(albums, library, catalog, &query, limit));
    all.extend(playlists_of(playlists, &catalog.playlists, &query, limit));
    order(&mut all);

    all
}

fn owns(slug: &str, id: Option<&str>) -> bool {
    id.and_then(music::tag::slug_of) == Some(slug)
}

fn songs(library: &[Track], catalog: &[Track], query: &Query, limit: usize) -> Vec<Scored> {
    let mut scored: Vec<Scored> = Vec::new();

    for (track, rank) in sources(library, catalog) {
        if rank.is_some() && kept(library, track) {
            continue;
        }

        let fields = [
            (TITLE, track.name.as_str()),
            (ARTIST, track.artists.as_str()),
            (ALBUM, track.album.as_str()),
        ];
        let Some(score) = favored(fit(&fields, query), rank).max(rank) else {
            continue;
        };

        scored.push(Scored {
            score,
            popularity: track.popularity,
            mine: rank.is_none(),
            hit: Hit::Song(track.clone()),
        });
    }

    capped(scored, limit)
}

fn albums_of(
    albums: &[Album],
    library: &[Track],
    catalog: &Catalog,
    query: &Query,
    limit: usize,
) -> Vec<Scored> {
    let saved = albums.iter().map(|album| {
        (
            &album.id,
            &album.name,
            &album.artists,
            &album.artist_refs,
            &album.cover,
            None,
            0,
            album.year,
        )
    });
    let total = catalog.albums.len();
    let found = catalog.albums.iter().enumerate().map(move |(at, album)| {
        (
            &album.id,
            &album.name,
            &album.artists,
            &album.artist_refs,
            &album.cover,
            Some(placed(at, total)),
            0,
            album.year,
        )
    });
    let derived = sources(library, &catalog.tracks).filter_map(|(track, rank)| {
        let id = track.album_id.as_ref()?;
        Some((
            id,
            &track.album,
            &track.artists,
            &track.artist_refs,
            &track.cover,
            rank,
            track.popularity,
            0,
        ))
    });

    let mut scored: Vec<Scored> = Vec::new();
    let mut seen: Vec<&String> = Vec::new();
    for (id, name, artists, artist_refs, cover, rank, popularity, year) in
        saved.chain(found).chain(derived)
    {
        let fields = [(TITLE, name.as_str()), (ARTIST, artists.as_str())];
        let Some(score) = favored(fit(&fields, query), rank).max(inherited(rank)) else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);

        scored.push(Scored {
            score,
            popularity,
            mine: rank.is_none(),
            hit: Hit::Album(AlbumHit {
                id: id.clone(),
                name: name.clone(),
                artists: artists.clone(),
                artist_refs: artist_refs.clone(),
                cover: cover.clone(),
                year,
            }),
        });
    }

    capped(scored, limit)
}

fn playlists_of(
    playlists: &[Playlist],
    catalog: &[Playlist],
    query: &Query,
    limit: usize,
) -> Vec<Scored> {
    let total = catalog.len();
    let saved = playlists.iter().map(|playlist| (playlist, None));
    let found = catalog
        .iter()
        .enumerate()
        .map(move |(at, playlist)| (playlist, Some(placed(at, total))));

    let mut scored: Vec<Scored> = Vec::new();
    let mut seen: Vec<&String> = Vec::new();
    for (playlist, rank) in saved.chain(found) {
        let fields = [
            (TITLE, playlist.name.as_str()),
            (ARTIST, playlist.owner.as_str()),
        ];
        let Some(score) = favored(fit(&fields, query), rank).max(inherited(rank)) else {
            continue;
        };
        if seen.contains(&&playlist.id) {
            continue;
        }
        seen.push(&playlist.id);

        scored.push(Scored {
            score,
            popularity: 0,
            mine: rank.is_none(),
            hit: Hit::Playlist(PlaylistHit {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                owner: playlist.owner.clone(),
                cover: playlist.cover.clone(),
            }),
        });
    }

    capped(scored, limit)
}

fn artists(
    library: &[Track],
    catalog: &[Track],
    albums: &[Album],
    found: &[SavedArtist],
    portraits: &HashMap<String, String>,
    query: &Query,
    limit: usize,
) -> Vec<Scored> {
    let mut tallies: Vec<(u32, u32, ArtistHit)> = Vec::new();

    let mut record = |artist: &ArtistRef, own: Option<&String>, score, popularity, mine| {
        let name = &artist.name;
        let id = artist.id.clone();
        let cover = own.filter(|url| !url.is_empty()).cloned().or_else(|| {
            id.as_ref()
                .and_then(|id| portraits.get(id))
                .filter(|url| !url.is_empty())
                .cloned()
        });
        match tallies
            .iter_mut()
            .find(|(_, _, current)| match (&current.id, &id) {
                (Some(current), Some(incoming)) => current == incoming,
                (None, _) | (_, None) => current.name == *name,
            }) {
            Some((best, top, current)) => {
                *best = (*best).max(score);
                *top = (*top).max(popularity);
                current.saved += mine;
                current.id = current.id.take().or(id);
                current.cover = current.cover.take().or(cover);
            }
            None => tallies.push((
                score,
                popularity,
                ArtistHit {
                    name: name.clone(),
                    id,
                    cover,
                    saved: mine,
                },
            )),
        }
    };

    for (track, rank) in sources(library, catalog) {
        for artist in &track.artist_refs {
            let Some(score) = favored(named(&artist.name, query), rank).max(inherited(rank)) else {
                continue;
            };
            let mine = usize::from(rank.is_none());
            record(artist, None, score, track.popularity, mine);
        }
    }
    for album in albums {
        for artist in &album.artist_refs {
            let Some(score) = favored(named(&artist.name, query), None) else {
                continue;
            };
            record(artist, None, score, 0, 0);
        }
    }
    for artist in found {
        let Some(score) = favored(named(&artist.name, query), None) else {
            continue;
        };
        let named = ArtistRef {
            name: artist.name.clone(),
            id: Some(artist.id.clone()),
        };
        record(&named, artist.cover.as_ref(), score, 0, 0);
    }

    capped(
        tallies
            .into_iter()
            .map(|(score, popularity, artist)| Scored {
                score,
                popularity,
                mine: artist.saved > 0,
                hit: Hit::Artist(artist),
            })
            .collect(),
        limit,
    )
}

fn sources<'a>(
    library: &'a [Track],
    catalog: &'a [Track],
) -> impl Iterator<Item = (&'a Track, Option<u32>)> {
    let total = catalog.len();

    library.iter().map(|track| (track, None)).chain(
        catalog
            .iter()
            .enumerate()
            .map(move |(at, track)| (track, Some(placed(at, total)))),
    )
}

fn placed(at: usize, total: usize) -> u32 {
    if total <= 1 {
        return TOP;
    }
    let last = total - 1;
    TOP - (TOP - TAIL) * at.min(last) as u32 / last as u32
}

fn inherited(rank: Option<u32>) -> Option<u32> {
    rank.map(|score| score * DERIVED / 100)
}

fn favored(score: Option<u32>, rank: Option<u32>) -> Option<u32> {
    match rank {
        Some(_) => score,
        None => score.map(|score| score + MINE),
    }
}

fn order(scored: &mut [Scored]) {
    scored.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then(right.popularity.cmp(&left.popularity))
    });
}

fn capped(mut scored: Vec<Scored>, limit: usize) -> Vec<Scored> {
    order(&mut scored);
    scored.truncate(limit);
    scored
}

/// The rows a search part answered with, or the rows it showed before when the request
/// failed. The failure is logged and recorded in `trouble`.
fn salvaged<T>(
    found: Result<Vec<T>>,
    provider: &str,
    kept: Vec<T>,
    trouble: &mut Vec<String>,
) -> Vec<T> {
    match found {
        Ok(found) => found,
        Err(error) => {
            log::warn!("search: {provider}: {error:#}");
            trouble.push(format!("{provider}: {error:#}"));
            kept
        }
    }
}

fn kept(library: &[Track], track: &Track) -> bool {
    track
        .id
        .as_ref()
        .is_some_and(|id| library.iter().any(|kept| kept.id.as_ref() == Some(id)))
}

fn score(value: &str, term: &str) -> u32 {
    let value = value.trim().to_lowercase();
    if value == term {
        return EXACT;
    }
    if value.starts_with(term) {
        return PREFIX;
    }
    if value.split_whitespace().any(|word| word.starts_with(term)) {
        return WORD;
    }
    match value.contains(term) {
        true => INSIDE,
        false => 0,
    }
}

fn fit(fields: &[(u32, &str)], query: &Query) -> Option<u32> {
    let ceiling = fields.iter().map(|(weight, _)| *weight).max()? * EXACT;
    let best = |term: &str| {
        fields
            .iter()
            .map(|(weight, value)| weight * score(value, term))
            .max()
            .unwrap_or(0)
    };

    let spread = query.terms.iter().try_fold(0, |total, term| {
        let hit = best(term);
        (hit > 0).then_some(total + hit)
    })?;
    let mean = spread * 100 / (ceiling * query.terms.len() as u32);

    Some(mean.max(best(&query.whole) * 100 / ceiling).min(100))
}

fn named(value: &str, query: &Query) -> Option<u32> {
    fit(&[(NAME, value)], query)
}

/// The ranked hits the lens lets through, in the lens's order. Hidden providers are already
/// gone, since `lanes` leaves them out before ranking.
fn narrowed(mut hits: Vec<Found>, lens: &Lens) -> Vec<Found> {
    hits.retain(|found| {
        lens.kind.is_none_or(|kind| found.kind() == kind) && (!lens.mine_only || found.mine)
    });
    sorted(&mut hits, lens.order);
    hits
}

/// Orders `hits` by one axis. The sort is stable and hits without that value keep their
/// relevance order behind the ones that have it.
fn sorted(hits: &mut [Found], order: Option<(Axis, Sort)>) {
    let Some((axis, sort)) = order else {
        return;
    };
    hits.sort_by(
        |left, right| match (measure(&left.hit, axis), measure(&right.hit, axis)) {
            (Some(left), Some(right)) => match sort {
                Sort::Ascending => left.cmp(&right),
                Sort::Descending => right.cmp(&left),
            },
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
    );
}

/// A hit's value on one axis, comparable with another of the same axis.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Measure {
    Text(String),
    Number(u64),
}

/// `hit`'s value on `axis`, or `None` when that kind of hit has none.
fn measure(hit: &Hit, axis: Axis) -> Option<Measure> {
    let text = |value: &str| (!value.is_empty()).then(|| Measure::Text(value.to_lowercase()));
    match (axis, hit) {
        (Axis::Title, Hit::Song(track)) => text(&track.name),
        (Axis::Title, Hit::Artist(artist)) => text(&artist.name),
        (Axis::Title, Hit::Album(album)) => text(&album.name),
        (Axis::Title, Hit::Playlist(list)) => text(&list.name),
        (Axis::Artist, Hit::Song(track)) => text(&track.artists),
        (Axis::Artist, Hit::Album(album)) => text(&album.artists),
        (Axis::Artist, Hit::Artist(artist)) => text(&artist.name),
        (Axis::Artist, Hit::Playlist(list)) => text(&list.owner),
        (Axis::Duration, Hit::Song(track)) => (!track.duration.is_zero())
            .then_some(Measure::Number(track.duration.as_millis() as u64)),
        (Axis::Year, Hit::Album(album)) => {
            (album.year > 0).then_some(Measure::Number(album.year as u64))
        }
        (Axis::Duration | Axis::Year, _) => None,
    }
}

/// The rows a search part answered with, or none for a part the lens did not ask for.
async fn awaited<T>(handle: Option<tokio::task::JoinHandle<Result<Vec<T>>>>) -> Result<Vec<T>> {
    match handle {
        Some(handle) => join(handle).await,
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn served(query: &str, kind: Option<Kind>, slugs: &[&'static str]) -> Served {
        Served {
            query: query.to_owned(),
            parts: Parts::of(kind),
            slugs: slugs.to_vec(),
        }
    }

    #[test]
    fn artists_need_songs_and_albums() {
        assert_eq!(
            Parts::of(Some(Kind::Artist)),
            Parts {
                songs: true,
                albums: true,
                playlists: false,
                artists: true,
            }
        );
    }

    #[test]
    fn narrowing_is_already_served() {
        let all = served("queen", None, &["spotify", "youtube"]);
        assert!(all.covers(&served("queen", Some(Kind::Album), &["spotify"])));
    }

    #[test]
    fn widening_the_kind_needs_a_fetch() {
        let albums = served("queen", Some(Kind::Album), &["spotify"]);
        assert!(!albums.covers(&served("queen", None, &["spotify"])));
    }

    #[test]
    fn a_new_source_needs_a_fetch() {
        let one = served("queen", None, &["spotify"]);
        assert!(!one.covers(&served("queen", None, &["spotify", "youtube"])));
    }

    #[test]
    fn resetting_filters_keeps_the_tab_and_order() {
        let lens = Lens {
            kind: Some(Kind::Album),
            hidden: vec!["spotify"],
            mine_only: true,
            order: Some((Axis::Year, Sort::Descending)),
        };
        assert_eq!(
            lens.unfiltered(),
            Lens {
                kind: Some(Kind::Album),
                order: Some((Axis::Year, Sort::Descending)),
                ..Lens::default()
            }
        );
    }

    #[test]
    fn another_query_needs_a_fetch() {
        let asked = served("queen", None, &["spotify"]);
        assert!(!asked.covers(&served("queens", None, &["spotify"])));
    }

    fn rank(library: &[Track], albums: &[Album], catalog: &[Track], asked: &str) -> Vec<Hit> {
        let catalog = Catalog {
            tracks: catalog.to_vec(),
            ..Catalog::default()
        };

        super::rank(
            library,
            albums,
            &[],
            &catalog,
            &HashMap::new(),
            asked,
            LIMIT,
        )
        .into_iter()
        .map(|scored| scored.hit)
        .collect()
    }

    fn track(name: &str, artists: &str, album: &str) -> Track {
        Track {
            id: Some(format!("{name}:{artists}")),
            name: name.to_owned(),
            playable: true,
            artists: artists.to_owned(),
            artist_refs: artists
                .split(", ")
                .map(|name| ArtistRef {
                    name: name.to_owned(),
                    id: None,
                })
                .collect(),
            album: album.to_owned(),
            album_id: Some(album.to_owned()),
            cover: None,
            duration: Duration::ZERO,
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

    fn liked(name: &str, artists: &str, album: &str, popularity: u32) -> Track {
        Track {
            popularity,
            ..track(name, artists, album)
        }
    }

    fn rhapsody() -> Track {
        track("Bohemian Rhapsody", "Queen", "A Night at the Opera")
    }

    fn dora() -> Track {
        track("Дорога", "Дора", "Дорадура")
    }

    fn titles(hits: &[Hit]) -> Vec<&str> {
        hits.iter()
            .filter_map(|hit| match hit {
                Hit::Song(track) => Some(track.name.as_str()),
                _ => None,
            })
            .collect()
    }

    fn count(hits: &[Hit], kind: Kind) -> usize {
        hits.iter().filter(|hit| hit.kind() == kind).count()
    }

    #[test]
    fn whole_string_query_matches() {
        let hits = rank(&[rhapsody()], &[], &[], "bohemian rhapsody");
        assert_eq!(titles(&hits), ["Bohemian Rhapsody"]);
    }

    #[test]
    fn title_and_artist_terms_match_in_any_order() {
        let hits = rank(&[rhapsody()], &[], &[], "queen bohemian");
        assert_eq!(titles(&hits), ["Bohemian Rhapsody"]);
    }

    #[test]
    fn every_term_must_match() {
        let hits = rank(&[rhapsody()], &[], &[], "queen zeppelin");
        assert!(hits.is_empty());
    }

    #[test]
    fn unmatched_catalog_entries_survive() {
        let hits = rank(&[], &[], &[dora()], "dora");
        assert_eq!(titles(&hits), ["Дорога"]);
        assert_eq!(count(&hits, Kind::Artist), 1);
        assert_eq!(count(&hits, Kind::Album), 1);
    }

    #[test]
    fn saved_track_is_listed_once() {
        let saved = rhapsody();
        let library = [saved.clone()];
        let hits = rank(&library, &[], &[saved], "queen");
        assert_eq!(titles(&hits), ["Bohemian Rhapsody"]);
    }

    #[test]
    fn catalog_keeps_the_order_it_answered_with() {
        let catalog = [
            track("First", "Nobody", "One"),
            track("Second", "Nobody", "Two"),
            track("Third", "Nobody", "Three"),
        ];
        let hits = rank(&[], &[], &catalog, "lyric phrase");
        assert_eq!(titles(&hits), ["First", "Second", "Third"]);
    }

    #[test]
    fn library_exact_title_outranks_the_catalog_top() {
        let hits = rank(&[rhapsody()], &[], &[dora()], "bohemian rhapsody");
        assert_eq!(titles(&hits), ["Bohemian Rhapsody", "Дорога"]);
    }

    #[test]
    fn a_library_hit_holds_its_place_when_the_catalog_answers() {
        let library = [rhapsody()];
        assert_eq!(
            titles(&rank(&library, &[], &[], "bohe")),
            ["Bohemian Rhapsody"]
        );
        assert_eq!(
            titles(&rank(&library, &[], &[dora()], "bohe")),
            ["Bohemian Rhapsody", "Дорога"]
        );
    }

    #[test]
    fn a_weak_library_hit_still_beats_the_catalog_top() {
        let hits = rank(&[rhapsody()], &[], &[dora()], "hapsod");
        assert_eq!(titles(&hits), ["Bohemian Rhapsody", "Дорога"]);
    }

    #[test]
    fn popularity_breaks_ties() {
        let library = [
            liked("Fever", "Bullet For My Valentine", "Fever", 10),
            liked("Fever", "Peggy Lee", "Black Coffee", 80),
        ];
        let hits = rank(&library, &[], &[], "fever");
        let Some(Hit::Song(first)) = hits.first() else {
            panic!("expected a song first");
        };
        assert_eq!(first.artists, "Peggy Lee");
    }

    #[test]
    fn library_songs_are_counted_for_artists() {
        let hits = rank(&[rhapsody()], &[], &[rhapsody()], "queen");
        let Some(Hit::Artist(artist)) = hits.iter().find(|hit| hit.kind() == Kind::Artist) else {
            panic!("expected an artist hit");
        };
        assert_eq!(artist.saved, 1);
    }

    #[test]
    fn artists_with_the_same_name_keep_separate_ids() {
        let mut first = track("One", "Echo", "First");
        first.artist_refs[0].id = Some("artist-one".to_owned());
        let mut second = track("Two", "Echo", "Second");
        second.artist_refs[0].id = Some("artist-two".to_owned());

        let hits = rank(&[], &[], &[first, second], "echo");
        let ids: Vec<_> = hits
            .iter()
            .filter_map(|hit| match hit {
                Hit::Artist(artist) => artist.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"artist-one"));
        assert!(ids.contains(&"artist-two"));
    }

    #[test]
    fn artist_pictures_come_from_portraits() {
        let mut portrait = track("One", "Echo", "First");
        portrait.artist_refs[0].id = Some("artist-one".to_owned());
        portrait.cover = Some("https://album-cover".to_owned());

        let portraits = HashMap::from([("artist-one".to_owned(), "https://portrait".to_owned())]);
        let hits = super::rank(
            &[portrait],
            &[],
            &[],
            &Catalog::default(),
            &portraits,
            "echo",
            LIMIT,
        );

        let Some(Hit::Artist(artist)) = hits
            .iter()
            .map(|scored| &scored.hit)
            .find(|hit| hit.kind() == Kind::Artist)
        else {
            panic!("expected an artist hit");
        };
        assert_eq!(artist.cover.as_deref(), Some("https://portrait"));
    }

    #[test]
    fn one_kind_lifts_the_cap() {
        let catalog = Catalog {
            tracks: (0..30)
                .map(|at| track(&format!("Song {at}"), "Band", "Record"))
                .collect(),
            ..Catalog::default()
        };
        let songs = |limit| {
            super::rank(&[], &[], &[], &catalog, &HashMap::new(), "song", limit)
                .iter()
                .filter(|scored| scored.hit.kind() == Kind::Song)
                .count()
        };
        assert_eq!(songs(LIMIT), 20);
        assert_eq!(songs(FOCUSED), 30);
    }

    #[test]
    fn library_hits_are_marked_mine() {
        let catalog = Catalog {
            tracks: vec![track(
                "Bohemian Like You",
                "The Dandy Warhols",
                "Thirteen Tales",
            )],
            ..Catalog::default()
        };
        let scored = super::rank(
            &[rhapsody()],
            &[],
            &[],
            &catalog,
            &HashMap::new(),
            "bohemian",
            LIMIT,
        );
        let mine = |name: &str| {
            scored.iter().find_map(|scored| match &scored.hit {
                Hit::Song(track) if track.name == name => Some(scored.mine),
                _ => None,
            })
        };
        assert_eq!(mine("Bohemian Rhapsody"), Some(true));
        assert_eq!(mine("Bohemian Like You"), Some(false));
    }

    fn found(hit: Hit, mine: bool) -> Found {
        Found {
            slug: "spotify",
            hit,
            mine,
        }
    }

    fn timed(name: &str, seconds: u64) -> Hit {
        Hit::Song(Track {
            duration: Duration::from_secs(seconds),
            ..track(name, "Band", "Record")
        })
    }

    fn record(name: &str) -> Hit {
        Hit::Album(AlbumHit {
            id: name.to_owned(),
            name: name.to_owned(),
            artists: "Band".to_owned(),
            artist_refs: Vec::new(),
            cover: None,
            year: 2001,
        })
    }

    fn names(hits: &[Found]) -> Vec<&str> {
        hits.iter()
            .map(|found| match &found.hit {
                Hit::Song(track) => track.name.as_str(),
                Hit::Album(album) => album.name.as_str(),
                Hit::Artist(artist) => artist.name.as_str(),
                Hit::Playlist(list) => list.name.as_str(),
            })
            .collect()
    }

    #[test]
    fn one_kind_keeps_only_that_kind() {
        let lens = Lens::default().with_kind(Some(Kind::Album));
        let hits = narrowed(
            vec![
                found(timed("Song", 60), false),
                found(record("Record"), false),
            ],
            &lens,
        );
        assert_eq!(names(&hits), ["Record"]);
    }

    #[test]
    fn mine_only_drops_catalog_hits() {
        let lens = Lens {
            mine_only: true,
            ..Lens::default()
        };
        let hits = narrowed(
            vec![
                found(timed("Saved", 60), true),
                found(timed("Found", 60), false),
            ],
            &lens,
        );
        assert_eq!(names(&hits), ["Saved"]);
    }

    #[test]
    fn duration_sorts_both_ways_and_valueless_hits_trail() {
        let hits = || {
            vec![
                found(timed("Long", 300), false),
                found(record("Record"), false),
                found(timed("Short", 60), false),
            ]
        };
        let up = Lens::default().cycled(Axis::Duration);
        assert_eq!(names(&narrowed(hits(), &up)), ["Short", "Long", "Record"]);
        let down = up.cycled(Axis::Duration);
        assert_eq!(names(&narrowed(hits(), &down)), ["Long", "Short", "Record"]);
    }

    #[test]
    fn cycling_an_axis_returns_to_relevance() {
        let lens = Lens::default()
            .cycled(Axis::Title)
            .cycled(Axis::Title)
            .cycled(Axis::Title);
        assert_eq!(lens.order, None);
    }

    #[test]
    fn changing_kind_drops_an_axis_it_lacks() {
        let lens = Lens::default()
            .with_kind(Some(Kind::Song))
            .cycled(Axis::Duration)
            .with_kind(Some(Kind::Album));
        assert_eq!(lens.order, None);
    }

    #[test]
    fn the_all_tab_sorts_by_length_too() {
        assert!(Axis::of(None).contains(&Axis::Duration));
        let lens = Lens::default()
            .with_kind(Some(Kind::Song))
            .cycled(Axis::Duration)
            .with_kind(None);
        assert_eq!(lens.order, Some((Axis::Duration, Sort::Ascending)));
    }

    #[test]
    fn signing_out_forgets_a_hidden_source() {
        let lens = Lens::default()
            .with_source("youtube", false)
            .forget("youtube");
        assert!(lens.hidden.is_empty());
    }

    fn boiler_room() -> music::SavedArtist {
        music::SavedArtist {
            id: "youtube-video:UCx".to_owned(),
            name: "Boiler Room".to_owned(),
            cover: Some("c".to_owned()),
            added_at: None,
        }
    }

    fn artist_hits(tracks: &[Track], found: &[music::SavedArtist]) -> Vec<ArtistHit> {
        super::artists(
            &[],
            tracks,
            &[],
            found,
            &HashMap::new(),
            &Query::new("boiler"),
            LIMIT,
        )
        .into_iter()
        .filter_map(|scored| match scored.hit {
            Hit::Artist(artist) => Some(artist),
            _ => None,
        })
        .collect()
    }

    #[test]
    fn a_searched_artist_appears_without_any_of_its_tracks() {
        let hits = artist_hits(&[], &[boiler_room()]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_deref(), Some("youtube-video:UCx"));
        assert_eq!(hits[0].cover.as_deref(), Some("c"));
    }

    #[test]
    fn a_searched_artist_merges_with_the_same_artist_from_tracks() {
        let mut song = track("Set", "Boiler Room", "Live");
        song.artist_refs[0].id = Some("youtube-video:UCx".to_owned());
        let hits = artist_hits(&[song], &[boiler_room()]);
        assert_eq!(hits.len(), 1);
    }
}
