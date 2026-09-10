//! One walker per model that carries an id, tagging every id it reaches.
//!
//! The models nest — an `AlbumDetail` holds `Track`s, a `GenreSection` holds
//! `Album`s — so a walker recurses rather than touching only its own fields.
//! `tag::tag` is idempotent, which is why a walker never checks first.
//!
//! `Playlist::owner_id` is the one id allowed to be empty, and empty is a
//! sentinel meaning the playlist has no owner: `state::detail` and
//! `views::screens::library::playlists` both branch on `is_empty()` to decide
//! whether to render an owner link at all, so tagging an empty owner would
//! make it `"spotify:"` — not empty, and a link to nobody. Never tag it.
//! `bare` needs no matching guard: `untag("")` finds no colon to split on, so
//! the sentinel survives untouched. The asymmetry is deliberate — making the
//! two sides match is what reintroduces the bug.
//!
//! `ArtistProfile`, `TrackTags` and `Lyrics` carry no id, so they have no
//! walker.
//!
//! `bare` is the other direction, for the one method that takes models rather
//! than ids as arguments. It reaches only what a `GenreSection` nests.

use std::sync::Arc;

use crate::tag;
use crate::{
    Album, AlbumDetail, Artist, ArtistRef, Contributor, Credit, Genre, GenreDetail, GenreItem,
    GenreSection, HomeFeed, Playlist, PlaylistDetail, SavedArtist, Track, UserDetail, UserProfile,
};

pub(crate) fn track(slug: &str, value: &mut Track) {
    if let Some(id) = value.id.as_mut() {
        *id = tag::tag(slug, id);
    }
    if let Some(id) = value.album_id.as_mut() {
        *id = tag::tag(slug, id);
    }
    for reference in &mut value.artist_refs {
        artist_ref(slug, reference);
    }
    if let Some(contributor) = value.added_by.as_mut() {
        self::contributor(slug, Arc::make_mut(contributor));
    }
    for credit in &mut value.credits {
        self::credit(slug, credit);
    }
}

pub(crate) fn album(slug: &str, value: &mut Album) {
    value.id = tag::tag(slug, &value.id);
    for reference in &mut value.artist_refs {
        artist_ref(slug, reference);
    }
}

pub(crate) fn album_detail(slug: &str, value: &mut AlbumDetail) {
    album(slug, &mut value.album);
    for entry in &mut value.tracks {
        track(slug, entry);
    }
}

pub(crate) fn playlist(slug: &str, value: &mut Playlist) {
    value.id = tag::tag(slug, &value.id);
    if !value.owner_id.is_empty() {
        value.owner_id = tag::tag(slug, &value.owner_id);
    }
}

pub(crate) fn playlist_detail(slug: &str, value: &mut PlaylistDetail) {
    playlist(slug, &mut value.playlist);
    for entry in &mut value.tracks {
        track(slug, entry);
    }
}

pub(crate) fn artist_ref(slug: &str, value: &mut ArtistRef) {
    if let Some(id) = value.id.as_mut() {
        *id = tag::tag(slug, id);
    }
}

pub(crate) fn artist(slug: &str, value: &mut Artist) {
    for entry in &mut value.top_tracks {
        track(slug, entry);
    }
    for entry in &mut value.albums {
        album(slug, entry);
    }
}

pub(crate) fn saved_artist(slug: &str, value: &mut SavedArtist) {
    value.id = tag::tag(slug, &value.id);
}

pub(crate) fn contributor(slug: &str, value: &mut Contributor) {
    value.id = tag::tag(slug, &value.id);
}

pub(crate) fn credit(slug: &str, value: &mut Credit) {
    if let Some(id) = value.id.as_mut() {
        *id = tag::tag(slug, id);
    }
}

pub(crate) fn genre(slug: &str, value: &mut Genre) {
    value.id = tag::tag(slug, &value.id);
}

pub(crate) fn genre_item(slug: &str, value: &mut GenreItem) {
    match value {
        GenreItem::Playlist(entry) => playlist(slug, entry),
        GenreItem::Album(entry) => album(slug, entry),
        GenreItem::Genre(entry) => genre(slug, entry),
    }
}

pub(crate) fn genre_section(slug: &str, value: &mut GenreSection) {
    for item in &mut value.items {
        genre_item(slug, item);
    }
}

pub(crate) fn genre_detail(slug: &str, value: &mut GenreDetail) {
    for section in &mut value.sections {
        genre_section(slug, section);
    }
}

pub(crate) fn user_profile(slug: &str, value: &mut UserProfile) {
    value.id = tag::tag(slug, &value.id);
}

pub(crate) fn user_detail(slug: &str, value: &mut UserDetail) {
    value.id = tag::tag(slug, &value.id);
    for entry in &mut value.playlists {
        playlist(slug, entry);
    }
}

pub(crate) fn home_feed(slug: &str, value: &mut HomeFeed) {
    for entry in &mut value.listen_again {
        track(slug, entry);
    }
    for entry in value.quick_picks.iter_mut().flatten() {
        track(slug, entry);
    }
    for section in &mut value.sections {
        genre_section(slug, section);
    }
}

pub(crate) mod bare {
    use crate::tag;
    use crate::{Album, ArtistRef, Genre, GenreItem, GenreSection, Playlist};

    pub(crate) fn genre_section(value: &mut GenreSection) {
        for item in &mut value.items {
            genre_item(item);
        }
    }

    fn genre_item(value: &mut GenreItem) {
        match value {
            GenreItem::Playlist(entry) => playlist(entry),
            GenreItem::Album(entry) => album(entry),
            GenreItem::Genre(entry) => genre(entry),
        }
    }

    fn playlist(value: &mut Playlist) {
        value.id = tag::untag(&value.id).to_owned();
        value.owner_id = tag::untag(&value.owner_id).to_owned();
    }

    fn album(value: &mut Album) {
        value.id = tag::untag(&value.id).to_owned();
        for reference in &mut value.artist_refs {
            artist_ref(reference);
        }
    }

    fn genre(value: &mut Genre) {
        value.id = tag::untag(&value.id).to_owned();
    }

    fn artist_ref(value: &mut ArtistRef) {
        if let Some(id) = value.id.as_mut() {
            *id = tag::untag(id).to_owned();
        }
    }
}
