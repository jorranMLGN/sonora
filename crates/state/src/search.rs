use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use gpui::{App, Context, Entity, Task};
use music::{Album, ArtistRef, MusicApi, Playlist, Track};

use crate::{Io, Library, Network, Session, SessionEvent, Shelf, join};

const DEBOUNCE: Duration = Duration::from_millis(250);
const LIMIT: usize = 20;
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Song,
    Artist,
    Album,
    Playlist,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Song, Kind::Artist, Kind::Album, Kind::Playlist];
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
}

impl Catalog {
    fn is_empty(&self) -> bool {
        self.tracks.is_empty() && self.albums.is_empty() && self.playlists.is_empty()
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
    hit: Hit,
}

pub struct Search {
    query: String,
    served: Option<String>,
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
            SessionEvent::SignedIn(_) | SessionEvent::SignedOut(_) => {
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

    pub fn lanes(&self, cx: &App) -> Vec<&'static str> {
        self.session
            .read(cx)
            .active_slugs()
            .into_iter()
            .filter(|slug| *slug != music::tag::LOCAL)
            .collect()
    }

    pub fn hits(&self) -> &[Found] {
        &self.hits
    }

    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Found> {
        self.hits.iter().filter(move |found| found.kind() == kind)
    }

    pub fn best(&self) -> Option<&Found> {
        self.hits.first()
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn ask(&mut self, query: &str, cx: &mut Context<Self>) {
        let query = query.trim().to_owned();
        let answered = self.loading || self.served.as_deref() == Some(query.as_str());
        if query == self.query && answered {
            return;
        }
        self.query = query.clone();
        self.error = None;

        if query.is_empty() {
            self.task = None;
            self.catalogs.clear();
            self.served = Some(String::new());
            self.loading = false;
            self.rank(cx);
            return;
        }

        self.rank(cx);

        let asking = self.providers(cx);
        if asking.is_empty() {
            self.loading = false;
            cx.notify();
            return;
        }

        self.loading = true;
        cx.notify();

        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;

            let sent: Vec<_> = asking
                .into_iter()
                .map(|(slug, name, client)| {
                    let songs = {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search(&asked).await })
                    };
                    let albums = {
                        let client = client.clone();
                        let asked = query.clone();
                        io.spawn(async move { client.search_albums(&asked).await })
                    };
                    let asked = query.clone();
                    let playlists = io.spawn(async move { client.search_playlists(&asked).await });
                    (slug, name, songs, albums, playlists)
                })
                .collect();

            let mut answered = Vec::new();
            for (slug, name, songs, albums, playlists) in sent {
                answered.push((
                    slug,
                    name,
                    join(songs).await,
                    join(albums).await,
                    join(playlists).await,
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
                    .map(|(slug, name, songs, albums, playlists)| {
                        let kept = held
                            .iter()
                            .position(|(known, _)| *known == slug)
                            .map(|at| held.remove(at).1)
                            .unwrap_or_default();
                        let catalog = Catalog {
                            tracks: salvaged(songs, name, kept.tracks, &mut trouble),
                            albums: salvaged(albums, name, kept.albums, &mut trouble),
                            playlists: salvaged(playlists, name, kept.playlists, &mut trouble),
                        };
                        (slug, catalog)
                    })
                    .collect();
                let bare = this.catalogs.iter().all(|(_, catalog)| catalog.is_empty());
                match trouble.first() {
                    Some(reason) => Network::failed(reason, cx),
                    None => {
                        Network::reached(cx);
                        this.served = Some(query);
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

        self.hits = {
            let held = self.library.read(cx);
            let state = held.state(Shelf::Streaming);
            let (tracks, albums, playlists) = (state.tracks(), state.albums(), state.playlists());

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
                    rank(&mine, &sleeves, &lists, catalog, &self.portraits, query)
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
                    hit: scored.hit,
                })
                .collect()
        };
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
) -> Vec<Scored> {
    let query = Query::new(asked);
    if query.terms.is_empty() {
        return Vec::new();
    }

    let mut all = songs(library, &catalog.tracks, &query);
    all.extend(artists(library, &catalog.tracks, albums, portraits, &query));
    all.extend(albums_of(albums, library, catalog, &query));
    all.extend(playlists_of(playlists, &catalog.playlists, &query));
    order(&mut all);

    all
}

fn owns(slug: &str, id: Option<&str>) -> bool {
    id.and_then(music::tag::slug_of) == Some(slug)
}

fn songs(library: &[Track], catalog: &[Track], query: &Query) -> Vec<Scored> {
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
            hit: Hit::Song(track.clone()),
        });
    }

    capped(scored)
}

fn albums_of(albums: &[Album], library: &[Track], catalog: &Catalog, query: &Query) -> Vec<Scored> {
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

    capped(scored)
}

fn playlists_of(playlists: &[Playlist], catalog: &[Playlist], query: &Query) -> Vec<Scored> {
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
            hit: Hit::Playlist(PlaylistHit {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                owner: playlist.owner.clone(),
                cover: playlist.cover.clone(),
            }),
        });
    }

    capped(scored)
}

fn artists(
    library: &[Track],
    catalog: &[Track],
    albums: &[Album],
    portraits: &HashMap<String, String>,
    query: &Query,
) -> Vec<Scored> {
    let mut tallies: Vec<(u32, u32, ArtistHit)> = Vec::new();

    let mut record = |artist: &ArtistRef, score, popularity, mine| {
        let name = &artist.name;
        let id = artist.id.clone();
        let cover = id
            .as_ref()
            .and_then(|id| portraits.get(id))
            .filter(|url| !url.is_empty())
            .cloned();
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
            record(artist, score, track.popularity, mine);
        }
    }
    for album in albums {
        for artist in &album.artist_refs {
            let Some(score) = favored(named(&artist.name, query), None) else {
                continue;
            };
            record(artist, score, 0, 0);
        }
    }

    capped(
        tallies
            .into_iter()
            .map(|(score, popularity, artist)| Scored {
                score,
                popularity,
                hit: Hit::Artist(artist),
            })
            .collect(),
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

fn capped(mut scored: Vec<Scored>) -> Vec<Scored> {
    order(&mut scored);
    scored.truncate(LIMIT);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rank(library: &[Track], albums: &[Album], catalog: &[Track], asked: &str) -> Vec<Hit> {
        let catalog = Catalog {
            tracks: catalog.to_vec(),
            ..Catalog::default()
        };

        super::rank(library, albums, &[], &catalog, &HashMap::new(), asked)
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
}
