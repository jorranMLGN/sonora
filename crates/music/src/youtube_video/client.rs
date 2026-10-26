use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use futures::FutureExt as _;
use futures::future::{BoxFuture, Shared};
use ytmusic::YtMusic;
use ytmusic::web::{Video, WebPlaylist, WebSearch};

use crate::youtube::pages;
use crate::youtube_video::{VIDEO, wire};
use crate::{
    Album, AlbumDetail, Artist, ArtistCatalogue, ArtistProfile, HomeFeed, MediaKind, MusicApi,
    Playlist, PlaylistDetail, SavedArtist, Track, UserProfile, escape,
};

/// How long one search answer serves every kind the search page asks for.
const SEARCH_TTL: Duration = Duration::from_secs(30);
/// The most continuation pages a whole-list read follows, a stop for a token that never ends.
const MAX_PAGES: usize = 200;
const WATCH_LATER: &str = "WL";
const LIKED: &str = "LL";

/// The plain youtube.com client: videos are tracks, channels are artists, and playlists are
/// playlists. It shares its `YtMusic`, and so its session and playback, with YouTube Music.
pub struct YouTubeVideoClient {
    api: Arc<YtMusic>,
    /// The signed-in account's display name, empty for a guest. A library playlist whose
    /// byline names it is the account's own.
    account: String,
    searches: Memo<Arc<WebSearch>>,
}

/// A fetch every waiting caller can await and clone the answer of. The error sits behind an
/// `Arc`, since `anyhow::Error` cannot be cloned.
type Answer<T> = Shared<BoxFuture<'static, Result<T, Arc<anyhow::Error>>>>;

/// The last answer for one key, shared for `ttl` by every caller that asks for the same key,
/// including callers that ask while the fetch is still running. A failed answer is not reused
/// once it has settled, so the next caller tries again.
struct Memo<T> {
    ttl: Duration,
    last: Mutex<Option<Entry<T>>>,
}

struct Entry<T> {
    key: String,
    at: Instant,
    answer: Answer<T>,
}

impl YouTubeVideoClient {
    /// A guest's client over `api`, which owns no playlist until
    /// [`owned_by`](Self::owned_by) names the account.
    pub fn new(api: Arc<YtMusic>) -> Self {
        Self {
            api,
            account: String::new(),
            searches: Memo::new(SEARCH_TTL),
        }
    }

    /// The client for the signed-in account called `account`. Ownership is judged against this
    /// name, so without it the client owns no playlist, not even Watch later.
    pub fn owned_by(mut self, account: impl Into<String>) -> Self {
        self.account = account.into();
        self
    }

    /// One youtube.com search for `query`, shared by the videos, channels and playlists parts
    /// of the search page, which all ask for it on the same keystroke.
    async fn web_search(&self, query: &str) -> Result<Arc<WebSearch>> {
        let api = self.api.clone();
        let owned = query.to_owned();
        self.searches
            .get(query, move || async move {
                api.web_search(&owned).await.map(Arc::new)
            })
            .await
    }

    /// Every video of a playlist, from its first page through each continuation.
    async fn all_videos(&self, playlist_id: &str) -> Result<Vec<Video>> {
        let page = self.api.web_playlist(playlist_id).await?;
        let mut videos = page.videos;
        let mut next = page.continuation;
        for _ in 0..MAX_PAGES {
            let Some(token) = next else {
                break;
            };
            let (more, after) = self.api.web_continuation(&token).await?;
            videos.extend(more);
            next = after;
        }
        Ok(videos)
    }

    /// Whether the account owns `playlist`. A guest owns nothing. A signed-in account owns
    /// Watch later always, Liked videos never, and otherwise a card whose byline names it. A
    /// card without a byline is the account's own only when `in_library`, since the library
    /// listing leaves the byline off exactly the account's own playlists; elsewhere a missing
    /// byline says nothing.
    fn owns(&self, playlist: &WebPlaylist, in_library: bool) -> bool {
        if self.account.is_empty() {
            return false;
        }
        match playlist.id.as_str() {
            WATCH_LATER => true,
            LIKED => false,
            _ => match &playlist.owner {
                Some(owner) => owner.name == self.account,
                None => in_library,
            },
        }
    }

    /// Whether a playlist page whose header names no owner is the account's own, which only
    /// the library listing can tell. A listing that cannot be read leaves it not owned, so the
    /// page offers no edit that would then fail.
    async fn owns_bylineless(&self, playlist_id: &str) -> bool {
        match self.api.web_library_playlists().await {
            Ok(listing) => self.listed_as_own(playlist_id, &listing),
            Err(error) => {
                log::warn!("youtube: cannot read the library to judge {playlist_id}: {error:#}");
                false
            }
        }
    }

    /// Whether `listing`, the library's playlists, holds `playlist_id` as the account's own.
    fn listed_as_own(&self, playlist_id: &str, listing: &[WebPlaylist]) -> bool {
        listing
            .iter()
            .any(|playlist| playlist.id == playlist_id && self.owns(playlist, true))
    }
}

impl<T: Clone + Send + Sync + 'static> Memo<T> {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            last: Mutex::new(None),
        }
    }

    /// The answer for `key`, from the running or recent fetch for it when there is one, or
    /// from `fetch` otherwise.
    async fn get<F, Fut>(&self, key: &str, fetch: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>> + Send + 'static,
    {
        let answer = {
            let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
            let reusable = last.as_ref().filter(|entry| {
                entry.key == key
                    && entry.at.elapsed() < self.ttl
                    && !matches!(entry.answer.peek(), Some(Err(_)))
            });
            match reusable {
                Some(entry) => entry.answer.clone(),
                None => {
                    let answer = fetch()
                        .map(|answer| answer.map_err(Arc::new))
                        .boxed()
                        .shared();
                    *last = Some(Entry {
                        key: key.to_owned(),
                        at: Instant::now(),
                        answer: answer.clone(),
                    });
                    answer
                }
            }
        };
        answer.await.map_err(|error| anyhow::anyhow!("{error:#}"))
    }
}

#[async_trait]
impl MusicApi for YouTubeVideoClient {
    async fn video(&self, track_id: &str, max_height: u32) -> Result<Option<crate::VideoFeed>> {
        crate::youtube::playback::open_video_feed(&self.api, track_id, max_height)
            .await
            .map(Some)
    }

    async fn has_video(&self, _track_id: &str) -> Result<bool> {
        Ok(true)
    }

    fn share_url(&self, kind: MediaKind, id: &str) -> Option<String> {
        let id = escape::component(id);
        match kind {
            MediaKind::Track => Some(format!("https://www.youtube.com/watch?v={id}")),
            MediaKind::Artist => Some(format!("https://www.youtube.com/channel/{id}")),
            MediaKind::Playlist => Some(format!("https://www.youtube.com/playlist?list={id}")),
            MediaKind::Album => None,
        }
    }

    async fn profile(&self) -> Result<UserProfile> {
        Ok(crate::youtube::user_profile(self.api.profile().await?))
    }

    async fn artist(&self, artist_id: &str) -> Result<Artist> {
        Ok(wire::artist_page(self.api.web_channel(artist_id).await?))
    }

    /// Every upload of the channel that is a track, read on past its first page, behind the
    /// `known` ones the page already lists.
    async fn artist_catalogue(&self, artist_id: &str, known: &[Track]) -> Result<ArtistCatalogue> {
        let channel = self
            .api
            .web_channel(artist_id)
            .await
            .context("cannot read the youtube channel")?;
        let api = self.api.clone();
        let uploads = pages::follow(
            wire::tracks(channel.uploads),
            channel.continuation,
            ArtistCatalogue::TRACKS,
            |token| {
                let api = api.clone();
                async move {
                    let (videos, next) = api.web_continuation(&token).await?;
                    Ok((wire::tracks(videos), next))
                }
            },
        )
        .await;
        Ok(ArtistCatalogue::every_track(known, wire::numbered(uploads)))
    }

    async fn artist_profile(&self, artist_id: &str) -> Result<ArtistProfile> {
        Ok(wire::artist_profile(self.api.web_channel(artist_id).await?))
    }

    async fn artist_images(&self, _ids: Vec<String>) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    /// The account's Liked videos. A guest has none, and asks for nothing.
    async fn saved_tracks(&self) -> Result<Vec<Track>> {
        if !self.api.is_authenticated() {
            return Ok(Vec::new());
        }
        Ok(wire::tracks(self.all_videos(LIKED).await?))
    }

    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        self.api.rate_track(track_id, saved).await
    }

    /// The video's details, read through the guest player, so a login- or age-gated video
    /// fails here even for a signed-in account.
    async fn track(&self, track_id: &str) -> Result<Track> {
        wire::track(self.api.web_video(track_id).await?)
            .with_context(|| format!("video {track_id} has no length"))
    }

    async fn track_playcount(&self, _track_id: &str) -> Result<Option<u64>> {
        Ok(None)
    }

    async fn playlists(&self) -> Result<Vec<Playlist>> {
        let playlists = self
            .api
            .web_library_playlists()
            .await?
            .into_iter()
            .map(|source| {
                let owned = self.owns(&source, true);
                let mut playlist = wire::playlist(source);
                playlist.owned = owned;
                if owned && playlist.owner.is_empty() {
                    playlist.owner = self.account.clone();
                }
                playlist
            })
            .collect();
        Ok(ordered(playlists))
    }

    async fn create_playlist(&self, name: &str) -> Result<String> {
        self.api.create_playlist(name).await
    }

    async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<()> {
        self.api.rename_playlist(playlist_id, name).await
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<()> {
        self.api.delete_playlist(playlist_id).await
    }

    async fn remove_playlist_from_library(&self, playlist_id: &str) -> Result<()> {
        self.api.rate_playlist(playlist_id, false).await
    }

    async fn add_playlist_to_library(&self, playlist_id: &str) -> Result<()> {
        self.api.rate_playlist(playlist_id, true).await
    }

    async fn set_playlist_public(&self, playlist_id: &str, public: bool) -> Result<()> {
        self.api.set_playlist_privacy(playlist_id, public).await
    }

    async fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        self.api.add_playlist_track(playlist_id, track_id).await
    }

    /// Removal needs the entry's `setVideoId`, which only the YouTube Music listing of the
    /// same playlist carries, so the entry is looked up there.
    async fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        let tracks = self.api.playlist(playlist_id).await?.tracks;
        let track = tracks
            .iter()
            .find(|track| track.video_id.as_deref() == Some(track_id))
            .context("track is not in the playlist")?;
        let set_video_id = track
            .set_video_id
            .as_deref()
            .context("track cannot be removed from this playlist")?;
        self.api
            .remove_playlist_track(playlist_id, track_id, set_video_id)
            .await
    }

    async fn saved_albums(&self) -> Result<Vec<Album>> {
        Ok(Vec::new())
    }

    async fn set_album_saved(&self, _album_id: &str, _saved: bool) -> Result<()> {
        bail!("youtube has no albums")
    }

    async fn saved_artists(&self) -> Result<Vec<SavedArtist>> {
        Ok(self
            .api
            .web_subscriptions()
            .await?
            .into_iter()
            .map(wire::artist)
            .collect())
    }

    async fn set_artist_saved(&self, artist_id: &str, saved: bool) -> Result<()> {
        self.api.set_subscribed(artist_id, saved).await
    }

    async fn album(&self, _album_id: &str) -> Result<AlbumDetail> {
        bail!("youtube has no albums")
    }

    async fn album_tracks(&self, _album_id: &str) -> Result<Vec<Track>> {
        Ok(Vec::new())
    }

    async fn playlist(&self, playlist_id: &str) -> Result<PlaylistDetail> {
        let page = self.api.web_playlist(playlist_id).await?;
        let unclaimed = !self.account.is_empty()
            && page.playlist.owner.is_none()
            && ![WATCH_LATER, LIKED].contains(&playlist_id);
        let owned = match unclaimed {
            true => self.owns_bylineless(playlist_id).await,
            false => self.owns(&page.playlist, false),
        };
        let mut detail = wire::playlist_detail(page);
        detail.playlist.owned = owned;
        Ok(detail)
    }

    async fn playlist_continuation(
        &self,
        continuation: &str,
    ) -> Result<(Vec<Track>, Option<String>)> {
        let (videos, next) = self.api.web_continuation(continuation).await?;
        Ok((wire::tracks(videos), next))
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        Ok(wire::tracks(self.all_videos(playlist_id).await?))
    }

    async fn playlist_covers(&self, playlist_id: &str, wanted: usize) -> Result<Vec<String>> {
        let page = self.api.web_playlist(playlist_id).await?;
        Ok(crate::distinct_covers(&wire::tracks(page.videos), wanted))
    }

    /// The videos YouTube suggests beside `track_id`, in one go.
    async fn track_radio(
        &self,
        track_id: &str,
        _from: Option<&str>,
    ) -> Result<(Vec<Track>, Option<String>)> {
        Ok((wire::tracks(self.api.web_related(track_id).await?), None))
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>> {
        Ok(wire::tracks(self.web_search(query).await?.videos.clone()))
    }

    async fn search_albums(&self, _query: &str) -> Result<Vec<Album>> {
        Ok(Vec::new())
    }

    async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        let found = self.web_search(query).await?;
        Ok(found
            .playlists
            .iter()
            .cloned()
            .map(wire::playlist)
            .collect())
    }

    async fn search_artists(&self, query: &str) -> Result<Vec<SavedArtist>> {
        let found = self.web_search(query).await?;
        Ok(found.channels.iter().cloned().map(wire::artist).collect())
    }

    /// The signed-in home feed's shelves. A shelf YouTube left untitled is named after the
    /// site. A guest's feed is empty.
    async fn home(&self) -> Result<HomeFeed> {
        let sections = self
            .api
            .web_home()
            .await?
            .into_iter()
            .filter_map(|shelf| wire::section(shelf, VIDEO.name))
            .collect();
        Ok(HomeFeed {
            sections,
            ..HomeFeed::default()
        })
    }
}

/// The library's playlists with Watch later first and the rest in the order YouTube lists them.
fn ordered(mut playlists: Vec<Playlist>) -> Vec<Playlist> {
    if let Some(index) = playlists
        .iter()
        .position(|playlist| playlist.id == WATCH_LATER)
    {
        let watch_later = playlists.remove(index);
        playlists.insert(0, watch_later);
    }
    playlists
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::channel::oneshot;
    use futures::executor::block_on;

    use super::*;

    fn playlist(id: &str) -> Playlist {
        Playlist {
            id: id.to_owned(),
            name: id.to_owned(),
            owner: String::new(),
            owner_id: String::new(),
            owned: true,
            collaborative: false,
            blend: false,
            public: false,
            cover: None,
            track_count: 0,
            modified_at: None,
        }
    }

    fn card(id: &str, owner: Option<&str>) -> WebPlaylist {
        WebPlaylist {
            id: id.to_owned(),
            title: id.to_owned(),
            owner: owner.map(|name| ytmusic::web::ChannelRef {
                id: format!("UC{name}"),
                name: name.to_owned(),
            }),
            thumbnails: Vec::new(),
            count: None,
        }
    }

    fn client(account: &str) -> YouTubeVideoClient {
        let client = YouTubeVideoClient::new(Arc::new(YtMusic::anonymous()));
        match account.is_empty() {
            true => client,
            false => client.owned_by(account),
        }
    }

    #[test]
    fn a_guest_owns_nothing() {
        let guest = client("");
        for listed in [true, false] {
            assert!(!guest.owns(&card(WATCH_LATER, None), listed));
            assert!(!guest.owns(&card(LIKED, None), listed));
            assert!(!guest.owns(&card("PLa", None), listed));
            assert!(!guest.owns(&card("PLa", Some("Ada")), listed));
        }
    }

    #[test]
    fn a_byline_naming_the_account_is_its_own() {
        let ada = client("Ada");
        assert!(ada.owns(&card("PLa", Some("Ada")), true));
        assert!(ada.owns(&card("PLa", Some("Ada")), false));
        assert!(!ada.owns(&card("PLa", Some("Bob")), true));
        assert!(!ada.owns(&card("PLa", Some("Bob")), false));
    }

    #[test]
    fn no_byline_means_the_account_only_in_the_library() {
        let ada = client("Ada");
        assert!(ada.owns(&card("PLa", None), true));
        assert!(!ada.owns(&card("PLa", None), false));
        assert!(ada.owns(&card(WATCH_LATER, None), false));
        assert!(!ada.owns(&card(LIKED, None), true));
    }

    #[test]
    fn a_bylineless_page_is_judged_by_the_library() {
        let listing = [card("PLmine", None), card("PLtheirs", Some("Bob"))];
        let ada = client("Ada");
        assert!(ada.listed_as_own("PLmine", &listing));
        assert!(!ada.listed_as_own("PLtheirs", &listing));
        assert!(!ada.listed_as_own("PLelsewhere", &listing));
        assert!(!client("").listed_as_own("PLmine", &listing));
    }

    #[test]
    fn watch_later_comes_first() {
        let ids: Vec<String> = ordered(vec![playlist("PLa"), playlist("LL"), playlist("WL")])
            .into_iter()
            .map(|playlist| playlist.id)
            .collect();
        assert_eq!(ids, ["WL", "PLa", "LL"]);
    }

    #[test]
    fn one_search_serves_every_caller_of_the_same_query() {
        let memo = Memo::new(SEARCH_TTL);
        let fetches = AtomicUsize::new(0);
        let (send, receive) = oneshot::channel::<()>();
        let receive = Mutex::new(Some(receive));
        // Only the first caller's fetch runs; it waits until all three have asked.
        let call = || {
            memo.get("boiler room", || {
                fetches.fetch_add(1, Ordering::SeqCst);
                let receive = receive.lock().unwrap().take();
                async move {
                    if let Some(receive) = receive {
                        receive.await.ok();
                    }
                    Ok(7)
                }
            })
        };
        let (a, b, c, ()) = block_on(async {
            futures::join!(call(), call(), call(), async {
                send.send(()).ok();
            })
        });
        assert_eq!((a.unwrap(), b.unwrap(), c.unwrap()), (7, 7, 7));
        assert_eq!(fetches.load(Ordering::SeqCst), 1);

        block_on(memo.get("boiler room live", || async { Ok(8) })).unwrap();
        assert_eq!(
            block_on(memo.get("boiler room live", || async { Ok(9) })).unwrap(),
            8
        );
    }

    #[test]
    fn a_failed_search_is_tried_again() {
        let memo = Memo::new(SEARCH_TTL);
        let failed =
            block_on(memo.get("q", || async { Err::<u32, _>(anyhow::anyhow!("offline")) }));
        assert!(failed.is_err());
        assert_eq!(block_on(memo.get("q", || async { Ok(1) })).unwrap(), 1);
    }
}
