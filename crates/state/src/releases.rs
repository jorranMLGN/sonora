//! The New releases page: what came out in the last few weeks from the artists the user
//! follows or plays most, merged across every signed-in provider.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Context, Entity, Task};
use music::{Album, ArtistRef, MusicApi, ReleaseType, Shape, Track};

use crate::{History, Io, Library, LibraryPart, Session, SessionEvent, Shelf, join};

/// How many days old a release may be and still show on the page.
pub(crate) const WINDOW: i64 = 28;
/// Words that mark a bracket as an edition of a release rather than another release.
const EDITION_WORDS: [&str; 9] = [
    "deluxe",
    "edition",
    "remaster",
    "remastered",
    "expanded",
    "anniversary",
    "explicit",
    "clean",
    "bonus",
];
/// How many plays in the history make an artist the user did not follow part of their taste.
const LISTENED: usize = 2;
/// What following an artist counts for, in plays, when the taste is ranked.
const FOLLOWED: usize = 3;
/// How many artists are asked for their releases, across every provider.
const ARTISTS: usize = 40;
/// How many artist lookups are in flight at once.
const IN_FLIGHT: usize = 4;
/// How old, in seconds, the list may be before opening the page fetches it again.
const STALE: i64 = 6 * 60 * 60;

/// An artist the user follows on one provider, with its tagged id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Followed {
    pub id: String,
    pub added_at: Option<i64>,
}

/// Where the New releases page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleasesState {
    /// Nothing asked yet, or no provider signed in.
    Idle,
    /// Waiting for the followed artists to arrive, or for the artists to answer.
    Loading,
    Ready,
    /// Every artist asked failed to answer.
    Failed,
}

/// The New releases page's list: recent albums, singles and EPs by the artists the user follows
/// or plays most, asked of every signed-in provider and merged into one list. It loads when the
/// page opens and again once it is older than `STALE`.
pub struct Releases {
    session: Entity<Session>,
    library: Entity<Library>,
    history: Entity<History>,
    io: Io,
    state: ReleasesState,
    albums: Rc<Vec<Album>>,
    /// When the list was last fetched, seconds since the epoch; 0 for never.
    loaded_at: i64,
    /// Why the last fetch failed, while the state is `Failed`.
    reason: Option<String>,
    /// Whether the page is on screen, which is the only time a fetch starts.
    visible: bool,
    task: Option<Task<()>>,
}

impl Releases {
    /// Follows the session and the library, so a provider signing in or out and the followed
    /// artists arriving reach the page.
    pub fn new(
        session: Entity<Session>,
        library: Entity<Library>,
        history: Entity<History>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| match event {
            SessionEvent::SignedIn(_) => this.reload(cx),
            SessionEvent::SignedOut(slug) => this.signed_out(slug, cx),
            SessionEvent::WentOffline
            | SessionEvent::Reconnected(_)
            | SessionEvent::LocalChanged => {}
        })
        .detach();
        cx.observe(&library, |this, _, cx| {
            if this.state == ReleasesState::Loading && this.task.is_none() {
                this.refresh(cx);
            }
        })
        .detach();
        Self {
            session,
            library,
            history,
            io,
            state: ReleasesState::Idle,
            albums: Rc::default(),
            loaded_at: 0,
            reason: None,
            visible: false,
            task: None,
        }
    }

    /// How many days back the page reaches.
    pub const DAYS: i64 = WINDOW;

    pub fn state(&self) -> ReleasesState {
        self.state
    }

    /// The releases, newest first.
    pub fn albums(&self) -> Rc<Vec<Album>> {
        self.albums.clone()
    }

    /// Why the last fetch failed, for the page's error state.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn is_loading(&self) -> bool {
        self.state == ReleasesState::Loading
    }

    /// Tells the list whether its page is on screen. Opening it fetches when the list is stale.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            self.refresh(cx);
        }
    }

    /// Asks every artist again, keeping what is shown until the answer lands.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.loaded_at = 0;
        self.task = None;
        self.refresh(cx);
    }

    /// Starts a fetch when the page is on screen, none is under way and the list is stale.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.visible || self.task.is_some() || now() - self.loaded_at <= STALE {
            return;
        }
        if self.session.read(cx).active_slugs().is_empty() {
            return self.reset(cx);
        }
        self.fetch(cx);
    }

    /// A provider left: drop what it contributed and ask the rest again. With none left,
    /// everything goes.
    fn signed_out(&mut self, slug: &str, cx: &mut Context<Self>) {
        if self.session.read(cx).active_slugs().is_empty() {
            return self.reset(cx);
        }
        self.task = None;
        let kept: Vec<Album> = self
            .albums
            .iter()
            .filter(|album| music::tag::slug_of(&album.id) != Some(slug))
            .cloned()
            .collect();
        self.albums = Rc::new(kept);
        if self.state == ReleasesState::Loading {
            self.state = ReleasesState::Idle;
        }
        self.loaded_at = 0;
        self.refresh(cx);
        cx.notify();
    }

    /// Forgets every release, for when no provider is signed in.
    fn reset(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.albums = Rc::default();
        self.state = ReleasesState::Idle;
        self.reason = None;
        self.loaded_at = 0;
        cx.notify();
    }

    /// The artists to ask, each with the client that owns it, or none while a provider's
    /// followed artists are still arriving. A provider whose library is its whole catalogue,
    /// such as local files or a self-hosted server, has no follows to go on and is left out.
    fn asked(&self, cx: &Context<Self>) -> Option<Vec<(String, Arc<dyn MusicApi>)>> {
        let session = self.session.read(cx);
        let library = self.library.read(cx);
        let slugs: Vec<&'static str> = session
            .active_slugs()
            .into_iter()
            .filter(|slug| !matches!(library.shape(Shelf::Of(slug)), Shape::Catalog(_)))
            .collect();
        if slugs
            .iter()
            .any(|slug| library.loading(Shelf::Of(slug), LibraryPart::Artists))
        {
            return None;
        }
        let followed: Vec<Followed> = slugs
            .iter()
            .flat_map(|slug| library.state(Shelf::Of(slug)).artists())
            .map(|artist| Followed {
                id: artist.id.clone(),
                added_at: artist.added_at,
            })
            .collect();
        let usable = |id: &str| music::tag::slug_of(id).is_some_and(|slug| slugs.contains(&slug));
        let artists = taste(&followed, self.history.read(cx).tracks(), usable, ARTISTS);
        Some(
            artists
                .into_iter()
                .filter_map(|id| {
                    let client = session.client_for_slug(music::tag::slug_of(&id)?)?;
                    Some((id, client))
                })
                .collect(),
        )
    }

    /// Asks every artist of the taste, a few at a time, for their releases and keeps the recent
    /// ones. Waits, in the loading state, until every provider's followed artists are in.
    fn fetch(&mut self, cx: &mut Context<Self>) {
        self.state = ReleasesState::Loading;
        self.reason = None;
        cx.notify();
        let Some(artists) = self.asked(cx) else {
            return;
        };
        let preferred = {
            let session = self.session.read(cx);
            preference(
                self.history.read(cx).tracks(),
                &session.active_slugs(),
                session.provider_slug(),
            )
        };
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let found = join(io.spawn(async move {
                let mut albums: Vec<Album> = Vec::new();
                let mut answered = artists.is_empty();
                for chunk in artists.chunks(IN_FLIGHT) {
                    let lookups = chunk
                        .iter()
                        .map(|(id, client)| releases_of(id.clone(), client.clone()));
                    for found in futures::future::join_all(lookups)
                        .await
                        .into_iter()
                        .flatten()
                    {
                        answered = true;
                        albums.extend(found);
                    }
                }
                anyhow::ensure!(answered, "no artist answered for new releases");
                anyhow::Ok(albums)
            }))
            .await;
            this.update(cx, |this, cx| {
                this.task = None;
                match found {
                    Ok(albums) => {
                        this.albums = Rc::new(merged(albums, now() / 86_400, &preferred));
                        this.state = ReleasesState::Ready;
                        this.loaded_at = now();
                    }
                    Err(error) => {
                        log::warn!("releases: cannot load new releases: {error:#}");
                        this.reason = Some(format!("{error:#}"));
                        this.state = ReleasesState::Failed;
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

/// The artists to ask for releases, best first, at most `limit`: every followed artist and every
/// artist played at least `LISTENED` times, ranked by plays plus `FOLLOWED` for a follow, then
/// by how recently they were followed. Only ids `usable` accepts are kept.
pub(crate) fn taste(
    followed: &[Followed],
    history: &[Track],
    usable: impl Fn(&str) -> bool,
    limit: usize,
) -> Vec<String> {
    let mut plays: HashMap<&str, usize> = HashMap::new();
    for artist in history.iter().flat_map(|track| &track.artist_refs) {
        if let Some(id) = artist.id.as_deref() {
            *plays.entry(id).or_default() += 1;
        }
    }
    let mut ranked: HashMap<&str, (usize, Option<i64>)> = HashMap::new();
    for artist in followed {
        let played = plays.get(artist.id.as_str()).copied().unwrap_or(0);
        let entry = ranked
            .entry(artist.id.as_str())
            .or_insert((played + FOLLOWED, None));
        entry.1 = entry.1.max(artist.added_at);
    }
    for (id, played) in plays {
        if played >= LISTENED {
            ranked.entry(id).or_insert((played, None));
        }
    }
    let mut ranked: Vec<(&str, (usize, Option<i64>))> =
        ranked.into_iter().filter(|(id, _)| usable(id)).collect();
    ranked.sort_by(
        |(left, (left_score, left_at)), (right, (right_score, right_at))| {
            right_score
                .cmp(left_score)
                .then(right_at.cmp(left_at))
                .then(left.cmp(right))
        },
    );
    ranked
        .into_iter()
        .take(limit)
        .map(|(id, _)| id.to_owned())
        .collect()
}

/// The signed-in providers in the order a release is preferred from: the most played first,
/// the one the app is showing breaking a tie, then the session's own order.
pub(crate) fn preference(
    history: &[Track],
    slugs: &[&'static str],
    active: Option<&str>,
) -> Vec<&'static str> {
    let plays = |slug: &str| {
        history
            .iter()
            .filter_map(|track| track.id.as_deref())
            .filter(|id| music::tag::slug_of(id) == Some(slug))
            .count()
    };
    let mut order: Vec<(usize, bool, usize, &'static str)> = slugs
        .iter()
        .enumerate()
        .map(|(place, slug)| (plays(slug), Some(*slug) == active, place, *slug))
        .collect();
    order.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(left.2.cmp(&right.2))
    });
    order.into_iter().map(|(.., slug)| slug).collect()
}

/// The releases out within `WINDOW` days of `today` (days since the epoch), newest first, one
/// copy of each, the copy from the provider earliest in `preferred`. Compilations, audiobooks
/// and podcasts are left out.
pub(crate) fn merged(
    albums: impl IntoIterator<Item = Album>,
    today: i64,
    preferred: &[&str],
) -> Vec<Album> {
    let rank = |album: &Album| {
        music::tag::slug_of(&album.id)
            .and_then(|slug| preferred.iter().position(|kept| *kept == slug))
            .unwrap_or(preferred.len())
    };
    let mut kept: Vec<Copies> = Vec::new();
    for album in albums {
        if !matches!(
            album.release_type,
            ReleaseType::Album | ReleaseType::Single | ReleaseType::Ep
        ) {
            continue;
        }
        let Some(day) = day_of(&album.release_date) else {
            continue;
        };
        if day > today || today - day > WINDOW {
            continue;
        }
        let place = rank(&album);
        let copy = Copies::of(day, place, album);
        match kept.iter_mut().find(|held| held.same_release(&copy)) {
            Some(held) => held.absorb(copy),
            None => kept.push(copy),
        }
    }
    kept.sort_by(|left, right| right.day.cmp(&left.day).then(left.title.cmp(&right.title)));
    kept.into_iter().map(Copies::into_album).collect()
}

/// Days since 1970-01-01 for a `YYYY-MM-DD` date, anything after the day ignored. A year or a
/// year and month alone is not a day, so it answers none.
pub(crate) fn day_of(date: &str) -> Option<i64> {
    let mut parts = date.splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts
        .next()?
        .split(|letter: char| !letter.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let year = match month <= 2 {
        true => year - 1,
        false => year,
    };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    Some(era * 146_097 + of_cycle - 719_468)
}

/// One artist's releases, each credited to the artist when the provider left that out, or none
/// when the provider cannot answer for them.
async fn releases_of(id: String, client: Arc<dyn MusicApi>) -> Option<Vec<Album>> {
    let artist = client
        .artist(&id)
        .await
        .inspect_err(|error| log::warn!("releases: cannot load artist {id}: {error:#}"))
        .ok()?;
    let credit = ArtistRef {
        name: artist.name,
        id: Some(id),
    };
    Some(
        artist
            .albums
            .into_iter()
            .map(|album| album.credit(&credit))
            .collect(),
    )
}

/// Seconds since the epoch.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Every copy of one release seen so far: the one to show, and every artist any copy credits.
/// A release turns up once per artist it was found through, and once per provider.
struct Copies {
    day: i64,
    /// Where the shown copy's provider ranks in `preferred`, lower is better.
    place: usize,
    album: Album,
    /// The normalised title, which every copy shares.
    title: String,
    ids: Vec<String>,
    artists: Vec<ArtistRef>,
    /// The normalised names in `artists`, to match another copy's against.
    names: Vec<String>,
}

impl Copies {
    fn of(day: i64, place: usize, album: Album) -> Self {
        let mut copies = Self {
            day,
            place,
            title: normalised(&album.name),
            ids: vec![album.id.clone()],
            artists: Vec::new(),
            names: Vec::new(),
            album,
        };
        copies.credit(credited(&copies.album));
        copies
    }

    /// Whether `other` is a copy of this release: the same album, or the same title with at
    /// least one artist in common, so a collaboration matches whichever artist comes first.
    fn same_release(&self, other: &Self) -> bool {
        other.ids.iter().any(|id| self.ids.contains(id))
            || (self.title == other.title
                && other.names.iter().any(|name| self.names.contains(name)))
    }

    /// Takes in another copy: its artists join the credits, and it becomes the one shown when
    /// its provider ranks better.
    fn absorb(&mut self, other: Self) {
        self.credit(other.artists);
        for id in other.ids {
            if !self.ids.contains(&id) {
                self.ids.push(id);
            }
        }
        if other.place < self.place {
            self.day = other.day;
            self.place = other.place;
            self.album = other.album;
        }
    }

    fn credit(&mut self, artists: Vec<ArtistRef>) {
        for artist in artists {
            let name = normalised(&artist.name);
            if name.is_empty() || self.names.contains(&name) {
                continue;
            }
            self.names.push(name);
            self.artists.push(artist);
        }
    }

    /// The copy to show, crediting every artist the copies did between them.
    fn into_album(self) -> Album {
        let mut album = self.album;
        album.artists = self
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        album.artist_refs = self.artists;
        album
    }
}

/// The artists `album` credits, from its links or else its comma-separated names.
fn credited(album: &Album) -> Vec<ArtistRef> {
    let linked: Vec<_> = album
        .artist_refs
        .iter()
        .filter(|artist| !artist.name.is_empty())
        .cloned()
        .collect();
    if !linked.is_empty() {
        return linked;
    }
    album
        .artists
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| ArtistRef {
            name: name.to_owned(),
            id: None,
        })
        .collect()
}

/// A name with case and punctuation left out, as well as a trailing "- Single" or "- EP" and
/// a bracketed edition such as "(Deluxe Edition)", so "Song - Single" and "song (Deluxe
/// Edition)" read the same. Any other bracket, such as "(Club Remix)", names another release
/// and is kept.
fn normalised(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let lower = [" - single", " - ep"]
        .iter()
        .find_map(|suffix| lower.strip_suffix(suffix))
        .unwrap_or(&lower);
    let mut kept = String::with_capacity(lower.len());
    let mut bracket: Option<String> = None;
    for letter in lower.chars() {
        match (letter, bracket.as_mut()) {
            ('(' | '[', None) => bracket = Some(String::new()),
            (')' | ']', Some(_)) => {
                let inside = bracket.take().unwrap_or_default();
                if !is_edition(&inside) {
                    kept.push(' ');
                    push_words(&mut kept, &inside);
                }
            }
            (_, Some(inside)) => inside.push(letter),
            (_, None) => push_words(&mut kept, &letter.to_string()),
        }
    }
    if let Some(inside) = bracket {
        push_words(&mut kept, &inside);
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a bracket only says which edition of the same release this is.
fn is_edition(inside: &str) -> bool {
    inside
        .split(|letter: char| !letter.is_alphanumeric())
        .any(|word| EDITION_WORDS.contains(&word))
}

/// Appends `text` with every letter that is not alphanumeric turned into a space.
fn push_words(kept: &mut String, text: &str) {
    kept.extend(text.chars().map(|letter| match letter.is_alphanumeric() {
        true => letter,
        false => ' ',
    }));
}

#[cfg(test)]
mod tests {
    use music::ArtistRef;

    use super::*;

    fn album(id: &str, name: &str, artist: &str, date: &str) -> Album {
        Album {
            id: id.to_owned(),
            name: name.to_owned(),
            artists: artist.to_owned(),
            artist_refs: vec![ArtistRef {
                name: artist.to_owned(),
                id: None,
            }],
            cover: None,
            cover_large: None,
            release_type: ReleaseType::Album,
            year: 0,
            track_count: 0,
            release_date: date.to_owned(),
            label: String::new(),
            copyrights: Vec::new(),
            added_at: None,
        }
    }

    fn track(id: &str, artists: &[&str]) -> Track {
        Track {
            id: Some(id.to_owned()),
            name: String::new(),
            playable: true,
            artists: String::new(),
            artist_refs: artists
                .iter()
                .map(|id| ArtistRef {
                    name: id.to_uppercase(),
                    id: Some((*id).to_owned()),
                })
                .collect(),
            album: String::new(),
            album_id: None,
            cover: None,
            duration: std::time::Duration::ZERO,
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

    fn play(artists: &[&str]) -> Track {
        track(&format!("spotify:{}", artists.join("+")), artists)
    }

    fn followed(id: &str, added_at: i64) -> Followed {
        Followed {
            id: id.to_owned(),
            added_at: Some(added_at),
        }
    }

    fn today() -> i64 {
        day_of("2026-10-30").unwrap()
    }

    fn ids(albums: &[Album]) -> Vec<&str> {
        albums.iter().map(|album| album.id.as_str()).collect()
    }

    #[test]
    fn a_day_counts_from_the_epoch() {
        assert_eq!(day_of("1970-01-01"), Some(0));
        assert_eq!(day_of("2000-03-01"), Some(11_017));
        assert_eq!(day_of("2026-10-30T00:00:00Z"), Some(20_756));
    }

    #[test]
    fn a_year_or_a_month_is_not_a_day() {
        assert_eq!(day_of("2026"), None);
        assert_eq!(day_of("2026-10"), None);
        assert_eq!(day_of(""), None);
        assert_eq!(day_of("2026-13-01"), None);
    }

    #[test]
    fn only_the_last_four_weeks_are_kept() {
        let albums = [
            album("spotify:today", "Today", "A", "2026-10-30"),
            album("spotify:edge", "Edge", "A", "2026-10-02"),
            album("spotify:old", "Old", "A", "2026-10-01"),
            album("spotify:future", "Future", "A", "2026-11-05"),
            album("spotify:year", "Year", "A", "2026"),
        ];

        let kept = merged(albums, today(), &["spotify"]);

        assert_eq!(ids(&kept), ["spotify:today", "spotify:edge"]);
    }

    #[test]
    fn newest_comes_first() {
        let albums = [
            album("spotify:b", "B", "A", "2026-10-20"),
            album("spotify:c", "C", "A", "2026-10-28"),
            album("spotify:a", "A", "A", "2026-10-10"),
        ];

        let kept = merged(albums, today(), &["spotify"]);

        assert_eq!(ids(&kept), ["spotify:c", "spotify:b", "spotify:a"]);
    }

    #[test]
    fn compilations_are_left_out() {
        let mut compilation = album("spotify:hits", "Hits", "A", "2026-10-20");
        compilation.release_type = ReleaseType::Compilation;
        let mut single = album("spotify:single", "Single", "A", "2026-10-20");
        single.release_type = ReleaseType::Single;
        let mut ep = album("spotify:ep", "Ep", "A", "2026-10-21");
        ep.release_type = ReleaseType::Ep;

        let kept = merged([compilation, single, ep], today(), &["spotify"]);

        assert_eq!(ids(&kept), ["spotify:ep", "spotify:single"]);
    }

    #[test]
    fn one_release_from_two_providers_is_listed_once_from_the_preferred() {
        let albums = [
            album("deezer:1", "Midnight Drive", "The Band", "2026-10-20"),
            album("spotify:1", "Midnight Drive", "the band", "2026-10-20"),
        ];

        let kept = merged(albums, today(), &["spotify", "deezer"]);

        assert_eq!(ids(&kept), ["spotify:1"]);
    }

    #[test]
    fn spelling_differences_do_not_split_a_release() {
        let albums = [
            album(
                "tidal:1",
                "Midnight Drive - Single",
                "Beyoncé",
                "2026-10-20",
            ),
            album("deezer:1", "midnight drive", "BEYONCÉ", "2026-10-21"),
            album(
                "spotify:1",
                "Midnight Drive (Deluxe Edition)",
                "Beyoncé",
                "2026-10-20",
            ),
        ];

        let kept = merged(albums, today(), &["deezer", "spotify", "tidal"]);

        assert_eq!(ids(&kept), ["deezer:1"]);
    }

    #[test]
    fn a_provider_nobody_ranked_loses_to_one_they_did() {
        let albums = [
            album("apple:1", "Song", "A", "2026-10-20"),
            album("tidal:1", "Song", "A", "2026-10-20"),
        ];

        let kept = merged(albums, today(), &["tidal"]);

        assert_eq!(ids(&kept), ["tidal:1"]);
    }

    #[test]
    fn one_release_found_through_each_of_its_artists_is_listed_once_with_all_of_them() {
        let albums = ["Incult", "KLOFAMA", "Revelation", "SLVL"].map(|artist| {
            album(
                "spotify:9",
                "S6TP (Incult & Revelation Remix)",
                artist,
                "2026-10-23",
            )
        });

        let kept = merged(albums, today(), &["spotify"]);

        assert_eq!(ids(&kept), ["spotify:9"]);
        let artists: Vec<_> = kept[0]
            .artist_refs
            .iter()
            .map(|artist| artist.name.as_str())
            .collect();
        assert_eq!(artists, ["Incult", "KLOFAMA", "Revelation", "SLVL"]);
        assert_eq!(kept[0].artists, "Incult, KLOFAMA, Revelation, SLVL");
    }

    #[test]
    fn a_collaboration_is_one_release_across_providers_whichever_artist_comes_first() {
        let mut deezer = album("deezer:1", "Together", "A", "2026-10-20");
        deezer.artist_refs.push(ArtistRef {
            name: "B".to_owned(),
            id: None,
        });
        let albums = [deezer, album("spotify:1", "Together", "b", "2026-10-20")];

        let kept = merged(albums, today(), &["spotify", "deezer"]);

        assert_eq!(ids(&kept), ["spotify:1"]);
        assert_eq!(kept[0].artist_refs.len(), 2);
    }

    #[test]
    fn a_remix_stays_apart_from_its_original() {
        let albums = [
            album("spotify:1", "S6TP", "SLVL", "2026-10-02"),
            album(
                "spotify:2",
                "S6TP (Incult & Revelation Remix)",
                "SLVL",
                "2026-10-23",
            ),
            album("spotify:3", "S6TP [Extended Mix]", "SLVL", "2026-10-23"),
            album("spotify:4", "S6TP (Remastered 2026)", "SLVL", "2026-10-02"),
        ];

        let merged = merged(albums, today(), &["spotify"]);
        let mut kept = ids(&merged);
        kept.sort_unstable();

        assert_eq!(kept, ["spotify:1", "spotify:2", "spotify:3"]);
    }

    #[test]
    fn same_title_by_other_artists_stays_apart() {
        let albums = [
            album("spotify:1", "Home", "A", "2026-10-20"),
            album("spotify:2", "Home", "B", "2026-10-20"),
        ];

        assert_eq!(merged(albums, today(), &["spotify"]).len(), 2);
    }

    #[test]
    fn the_most_played_provider_is_preferred() {
        let history = [
            track("deezer:1", &[]),
            track("deezer:2", &[]),
            track("spotify:1", &[]),
        ];

        let order = preference(&history, &["spotify", "deezer", "tidal"], Some("tidal"));

        assert_eq!(order, ["deezer", "spotify", "tidal"]);
    }

    #[test]
    fn without_plays_the_showing_provider_comes_first() {
        let order = preference(&[], &["spotify", "deezer", "tidal"], Some("deezer"));

        assert_eq!(order, ["deezer", "spotify", "tidal"]);
    }

    #[test]
    fn followed_and_played_artists_make_the_taste() {
        let history = [play(&["x"]), play(&["x"]), play(&["once"])];
        let follows = [followed("a", 10)];

        let artists = taste(&follows, &history, |_| true, 10);

        assert_eq!(artists, ["a", "x"]);
    }

    #[test]
    fn plays_rank_a_followed_artist_higher() {
        let history = [play(&["b"]), play(&["b"]), play(&["c"]), play(&["c"])];
        let follows = [followed("a", 30), followed("b", 10)];

        let artists = taste(&follows, &history, |_| true, 10);

        assert_eq!(artists, ["b", "a", "c"]);
    }

    #[test]
    fn the_most_recent_follow_breaks_a_tie() {
        let follows = [followed("old", 1), followed("new", 5)];

        assert_eq!(taste(&follows, &[], |_| true, 10), ["new", "old"]);
    }

    #[test]
    fn the_taste_is_capped_and_filtered() {
        let follows = [
            followed("spotify:a", 3),
            followed("local:b", 2),
            followed("spotify:c", 1),
        ];

        let artists = taste(&follows, &[], |id| id.starts_with("spotify:"), 1);

        assert_eq!(artists, ["spotify:a"]);
    }

    #[test]
    fn a_featured_artist_counts_too() {
        let history = [play(&["lead", "guest"]), play(&["other", "guest"])];

        assert_eq!(taste(&[], &history, |_| true, 10), ["guest"]);
    }
}
