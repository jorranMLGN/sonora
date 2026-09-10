use anyhow::{Context as _, Result};

use crate::soundcloud::http::Http;
use crate::soundcloud::search::split_playlists;
use crate::soundcloud::wire::{self, Page, PlaylistLike, TrackLike};

/// Fetches the tracks `user` has liked.
///
/// `TrackLike::created_at` is when the like happened, not when the track was
/// uploaded; `crates/music` has no date-parser dependency reachable from
/// here, so `added_at` is left `None` rather than parsing that timestamp by
/// hand or adding a dependency to do it.
pub async fn liked_tracks(http: &Http, user: &str, limit: u32) -> Result<Vec<crate::Track>> {
    let limit = limit.to_string();
    let page: Page<TrackLike> = http
        .get_json(&format!("/users/{user}/track_likes"), &[("limit", &limit)])
        .await
        .context("cannot fetch soundcloud liked tracks")?;
    http.remember_permalinks(
        page.collection
            .iter()
            .map(|like| (like.track.id, like.track.permalink_url.clone())),
    );
    Ok(page
        .collection
        .into_iter()
        .map(|like| wire::track(like.track))
        .collect())
}

/// Fetches the sets `user` owns or has liked, split into playlists and
/// albums.
///
/// This is one function behind two `MusicApi` methods (`playlists` and
/// `saved_albums`): it makes three requests — the user's own playlists,
/// their own albums, and their liked sets — and reuses
/// `search::split_playlists` to sort the liked half the same way a search
/// result page is sorted.
pub async fn saved_sets(
    http: &Http,
    user: &str,
    limit: u32,
) -> Result<(Vec<crate::Playlist>, Vec<crate::Album>)> {
    let limit = limit.to_string();

    let own_playlists: Page<wire::Playlist> = http
        .get_json(&format!("/users/{user}/playlists"), &[("limit", &limit)])
        .await
        .context("cannot fetch soundcloud playlists")?;
    let own_albums: Page<wire::Playlist> = http
        .get_json(&format!("/users/{user}/albums"), &[("limit", &limit)])
        .await
        .context("cannot fetch soundcloud albums")?;
    let likes: Page<PlaylistLike> = http
        .get_json(
            &format!("/users/{user}/playlist_likes"),
            &[("limit", &limit)],
        )
        .await
        .context("cannot fetch soundcloud liked sets")?;

    http.remember_permalinks(
        own_playlists
            .collection
            .iter()
            .map(|set| (set.id, set.permalink_url.clone())),
    );
    http.remember_permalinks(
        own_albums
            .collection
            .iter()
            .map(|set| (set.id, set.permalink_url.clone())),
    );
    http.remember_permalinks(
        likes
            .collection
            .iter()
            .map(|like| (like.playlist.id, like.playlist.permalink_url.clone())),
    );

    let liked_page = Page {
        collection: likes
            .collection
            .into_iter()
            .map(|like| like.playlist)
            .collect(),
        next_href: None,
    };
    let (mut playlists, mut albums) = split_playlists(liked_page);

    playlists.extend(own_playlists.collection.into_iter().map(wire::playlist));
    albums.extend(
        own_albums
            .collection
            .into_iter()
            .filter(wire::is_album)
            .map(wire::album),
    );

    Ok((playlists, albums))
}

/// Fetches the artists `user` follows.
pub async fn followed(http: &Http, user: &str, limit: u32) -> Result<Vec<crate::SavedArtist>> {
    let limit = limit.to_string();
    let page: Page<wire::User> = http
        .get_json(&format!("/users/{user}/followings"), &[("limit", &limit)])
        .await
        .context("cannot fetch soundcloud followings")?;
    http.remember_permalinks(
        page.collection
            .iter()
            .map(|user| (user.id, user.permalink_url.clone())),
    );
    Ok(page
        .collection
        .into_iter()
        .map(wire::saved_artist)
        .collect())
}

/// Likes or unlikes a track.
///
/// UNVERIFIED: no write endpoint on this API has been exercised against the
/// live service. This follows `PUT`/`DELETE /likes/tracks/{id}` as the
/// steps describe.
pub async fn set_track_liked(http: &Http, id: &str, liked: bool) -> Result<()> {
    require_authenticated(http)?;
    let path = format!("/likes/tracks/{id}");
    if liked {
        http.put_empty(&path).await
    } else {
        http.delete(&path).await
    }
}

/// Follows or unfollows an artist.
///
/// UNVERIFIED: this follows `PUT`/`DELETE /me/followings/{id}` as the steps
/// describe; nothing here has been exercised against the live service.
pub async fn set_followed(http: &Http, id: &str, followed: bool) -> Result<()> {
    require_authenticated(http)?;
    let path = format!("/me/followings/{id}");
    if followed {
        http.put_empty(&path).await
    } else {
        http.delete(&path).await
    }
}

/// Saves or unsaves a set (album or playlist) to the signed-in user's
/// library. SoundCloud has no album-save distinct from liking a set, so
/// this backs both `set_album_saved` and playlist-library membership.
///
/// UNVERIFIED: this follows `PUT`/`DELETE /me/library/albums_and_playlists/{id}`
/// as the steps describe; nothing here has been exercised against the live
/// service.
pub async fn set_saved(http: &Http, id: &str, saved: bool) -> Result<()> {
    require_authenticated(http)?;
    let path = format!("/me/library/albums_and_playlists/{id}");
    if saved {
        http.put_empty(&path).await
    } else {
        http.delete(&path).await
    }
}

fn require_authenticated(http: &Http) -> Result<()> {
    if !http.authenticated() {
        anyhow::bail!("signing in is required to change the library");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_a_liked_track_using_the_like_timestamp_not_the_upload_one() {
        let track_json = include_str!("fixtures/track.json");
        let like_json = format!(r#"{{"created_at":"2024-01-01T00:00:00Z","track":{track_json}}}"#);
        let like: TrackLike = serde_json::from_str(&like_json).unwrap();
        assert_eq!(like.created_at.as_deref(), Some("2024-01-01T00:00:00Z"));
        let converted = wire::track(like.track);
        // no date parser is reachable from this crate, so the like's
        // timestamp cannot be converted into `added_at` yet.
        assert!(converted.added_at.is_none());
    }

    #[test]
    fn splits_liked_sets_into_playlists_and_albums() {
        let album_json = include_str!("fixtures/playlist_album.json");
        let set_json = include_str!("fixtures/playlist_set.json");
        let likes_json = format!(
            r#"{{"collection":[{{"created_at":null,"playlist":{album_json}}},{{"created_at":null,"playlist":{set_json}}}]}}"#
        );
        let page: Page<PlaylistLike> = serde_json::from_str(&likes_json).unwrap();
        let liked_page = Page {
            collection: page.collection.into_iter().map(|l| l.playlist).collect(),
            next_href: None,
        };
        let (playlists, albums) = split_playlists(liked_page);
        assert_eq!(playlists.len(), 1, "the plain set belongs in playlists");
        assert_eq!(albums.len(), 1, "the album belongs in albums");
    }

    #[test]
    fn converts_a_followed_user_into_a_saved_artist() {
        let user_json = include_str!("fixtures/user.json");
        let user: wire::User = serde_json::from_str(user_json).unwrap();
        let name = user.username.clone();
        let artist = wire::saved_artist(user);
        assert_eq!(artist.name, name);
        assert!(artist.added_at.is_none());
    }
}
