use anyhow::{Context as _, Result};

use crate::soundcloud::http::Http;
use crate::soundcloud::wire::{self, Page};

const LIMIT: &str = "50";

pub async fn tracks(http: &Http, query: &str) -> Result<Vec<crate::Track>> {
    let page: Page<wire::Track> = http
        .get_json("/search/tracks", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud tracks")?;
    Ok(page.collection.into_iter().map(wire::track).collect())
}

pub async fn playlists(http: &Http, query: &str) -> Result<Vec<crate::Playlist>> {
    let page = raw_playlists(http, query).await?;
    Ok(split_playlists(page).0)
}

pub async fn albums(http: &Http, query: &str) -> Result<Vec<crate::Album>> {
    let page = raw_playlists(http, query).await?;
    Ok(split_playlists(page).1)
}

async fn raw_playlists(http: &Http, query: &str) -> Result<Page<wire::Playlist>> {
    http.get_json("/search/playlists", &[("q", query), ("limit", LIMIT)])
        .await
        .context("cannot search soundcloud playlists")
}

/// Splits a fetched page of sets into the playlists and the albums it holds.
pub(crate) fn split_playlists(
    page: Page<wire::Playlist>,
) -> (Vec<crate::Playlist>, Vec<crate::Album>) {
    let (albums, playlists): (Vec<_>, Vec<_>) =
        page.collection.into_iter().partition(wire::is_album);
    (
        playlists.into_iter().map(wire::playlist).collect(),
        albums.into_iter().map(wire::album).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::split_playlists;
    use crate::soundcloud::wire::{self, Page};

    #[test]
    fn splits_sets_into_playlists_and_albums() {
        let album: wire::Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_album.json")).unwrap();
        let set: wire::Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
        let album_title = album.title.clone();
        let set_title = set.title.clone();

        let page = Page {
            collection: vec![album, set],
            next_href: None,
        };
        let (playlists, albums) = split_playlists(page);

        assert_eq!(playlists.len(), 1, "the plain set belongs in playlists");
        assert_eq!(playlists[0].name, set_title);
        assert_eq!(albums.len(), 1, "the album fixture belongs in albums");
        assert_eq!(albums[0].name, album_title);
    }
}
