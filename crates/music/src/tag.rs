//! The `<slug>:<id>` grammar every id in the app carries.
//!
//! `spotify:` is also the URI scheme sonora accepts for deep links
//! (`router::uri`), so the head alone cannot tell a tag from a link — the
//! colon count can. `spotify:track:abc` splits again; `spotify:abc` does not,
//! and no bare id ever contains a colon. `URI_KINDS` therefore rejects a tail
//! only when it splits and its first segment is a kind. Keying on the head
//! instead would strand every Spotify username spelled like one, and
//! `Playlist::owner_id` and `Contributor::id` are usernames. The guard applies
//! to spotify alone.
//!
//! Local ids carry their kind in the head — `local:` for a track, and one
//! `local-<kind>:` per other model. They all belong to the same provider, so
//! `LOCAL_HEADS` resolves every one of them to the `local` slug before the
//! spotify guard ever runs.

pub const SLUGS: [&str; 4] = ["spotify", "youtube", "soundcloud", "local"];

const LOCAL_HEADS: [&str; 4] = ["local", "local-album", "local-artist", "local-playlist"];

const URI_KINDS: [&str; 5] = ["track", "album", "playlist", "artist", "user"];

pub fn tag(slug: &str, id: &str) -> String {
    match split(id) {
        Some(_) => id.to_owned(),
        None => format!("{slug}:{id}"),
    }
}

pub fn split(id: &str) -> Option<(&str, &str)> {
    let (head, rest) = id.split_once(':')?;
    if LOCAL_HEADS.contains(&head) {
        return Some(("local", rest));
    }
    let slug = *SLUGS.iter().find(|known| **known == head)?;
    if slug == "spotify"
        && let Some((kind, _)) = rest.split_once(':')
        && URI_KINDS.contains(&kind)
    {
        return None;
    }
    Some((slug, rest))
}

pub fn slug_of(id: &str) -> Option<&str> {
    split(id).map(|(slug, _)| slug)
}

pub fn untag(id: &str) -> &str {
    match split(id) {
        Some(("local", _)) => id,
        Some((_, rest)) => rest,
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use super::{slug_of, split, tag, untag};

    #[test]
    fn tags_and_splits_a_plain_id() {
        assert_eq!(tag("soundcloud", "284873455"), "soundcloud:284873455");
        assert_eq!(
            split("soundcloud:284873455"),
            Some(("soundcloud", "284873455"))
        );
    }

    #[test]
    fn an_untagged_id_has_no_slug() {
        assert_eq!(split("284873455"), None);
        assert_eq!(slug_of("284873455"), None);
        assert_eq!(untag("284873455"), "284873455");
    }

    #[test]
    fn an_unknown_head_is_not_a_tag() {
        assert_eq!(split("track:abc"), None);
        assert_eq!(untag("track:abc"), "track:abc");
    }

    // A local id is a file path, so the tail carries both ':' and '/'.
    #[test]
    fn a_local_path_keeps_every_separator_after_the_first() {
        let id = "local:/home/j/My Music/a:b/x.mp3";
        assert_eq!(split(id), Some(("local", "/home/j/My Music/a:b/x.mp3")));
        assert_eq!(untag(id), id);
    }

    #[test]
    fn every_local_kind_resolves_to_one_provider() {
        assert_eq!(slug_of("local:/x/a.mp3"), Some("local"));
        assert_eq!(slug_of("local-album:/x"), Some("local"));
        assert_eq!(slug_of("local-artist:/x"), Some("local"));
        assert_eq!(slug_of("local-playlist:abc"), Some("local"));
    }

    #[test]
    fn a_local_id_is_never_stripped() {
        for id in [
            "local:/x/a.mp3",
            "local-album:/x",
            "local-artist:/x",
            "local-playlist:abc",
        ] {
            assert_eq!(untag(id), id);
        }
    }

    #[test]
    fn a_local_name_that_looks_like_a_uri_kind_still_resolves() {
        for name in ["track", "album", "playlist", "artist", "user"] {
            let id = format!("local-artist:{name}");
            assert_eq!(slug_of(&id), Some("local"), "{id}");
            assert_eq!(untag(&id), id, "{id}");
        }
    }

    // The hazard from the spec: `spotify:` is also a live URI scheme.
    #[test]
    fn a_spotify_deep_link_is_not_mistaken_for_a_tagged_id() {
        assert_eq!(split("spotify:track:6rqhFgbbKwnb9MLmUQDhG6"), None);
        assert_eq!(split("spotify:album:1DFixLWuPkv3KT3TnV35m3"), None);
        assert_eq!(split("spotify:user:x:playlist:y"), None);
    }

    #[test]
    fn a_tagged_spotify_id_still_splits() {
        assert_eq!(
            split("spotify:7etD5lFGaYcsKmFTmutVYO"),
            Some(("spotify", "7etD5lFGaYcsKmFTmutVYO"))
        );
    }

    #[test]
    fn a_user_named_like_a_uri_kind_still_tags() {
        for name in ["track", "album", "playlist", "artist", "user"] {
            let id = tag("spotify", name);
            assert_eq!(id, format!("spotify:{name}"));
            assert_eq!(slug_of(&id), Some("spotify"), "{id}");
            assert_eq!(untag(&id), name, "{id}");
        }
    }

    #[test]
    fn tagging_is_idempotent() {
        let once = tag("spotify", "abc");
        assert_eq!(tag("spotify", &once), once);
    }
}
