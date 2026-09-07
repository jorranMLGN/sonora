use crate::Destination;

const SCHEME: &str = "spotify:";
const HOST: &str = "open.spotify.com";
const REGION: &str = "intl-";

pub fn destination(link: &str) -> Option<Destination> {
    let link = link.trim();
    match link.strip_prefix(SCHEME) {
        Some(rest) => from_uri(rest),
        None => from_url(link),
    }
}

fn from_uri(rest: &str) -> Option<Destination> {
    let mut parts = rest.split(':');
    match parts.next()? {
        "user" => {
            let user = parts.next()?;
            match (parts.next(), parts.next()) {
                (Some(kind), Some(id)) => route(kind, id),
                _ => route("user", user),
            }
        }
        kind => route(kind, parts.next()?),
    }
}

fn from_url(link: &str) -> Option<Destination> {
    let rest = link
        .strip_prefix("https://")
        .or_else(|| link.strip_prefix("http://"))
        .unwrap_or(link);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = rest.strip_prefix(HOST)?.strip_prefix('/')?;

    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let kind = match parts.next()? {
        region if region.starts_with(REGION) => parts.next()?,
        kind => kind,
    };
    route(kind, parts.next()?)
}

fn route(kind: &str, id: &str) -> Option<Destination> {
    let id = id.split(['?', '#']).next().filter(|id| !id.is_empty())?;
    let id = music::tag::tag("spotify", id);
    match kind {
        "track" => Some(Destination::Song(id.into())),
        "album" => Some(Destination::Album(id.into())),
        "playlist" => Some(Destination::Playlist(id.into())),
        "artist" => Some(Destination::Artist(id.into())),
        "user" => Some(Destination::User(id.into())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_uri_kind() {
        assert_eq!(
            destination("spotify:track:6rqhFgbbKwnb9MLmUQDhG6"),
            Some(Destination::Song("spotify:6rqhFgbbKwnb9MLmUQDhG6".into()))
        );
        assert_eq!(
            destination("spotify:album:1DFixLWuPkv3KT3TnV35m3"),
            Some(Destination::Album("spotify:1DFixLWuPkv3KT3TnV35m3".into()))
        );
        assert_eq!(
            destination("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
            Some(Destination::Playlist(
                "spotify:37i9dQZF1DXcBWIGoYBM5M".into()
            ))
        );
        assert_eq!(
            destination("spotify:artist:0TnOYISbd1XYRBk9myaseg"),
            Some(Destination::Artist("spotify:0TnOYISbd1XYRBk9myaseg".into()))
        );
    }

    #[test]
    fn a_link_produces_a_tagged_id() {
        assert_eq!(
            destination("spotify:track:6rqhFgbbKwnb9MLmUQDhG6"),
            Some(Destination::Song("spotify:6rqhFgbbKwnb9MLmUQDhG6".into()))
        );
    }

    #[test]
    fn reads_web_links() {
        assert_eq!(
            destination("https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6"),
            Some(Destination::Song("spotify:6rqhFgbbKwnb9MLmUQDhG6".into()))
        );
        assert_eq!(
            destination("http://www.open.spotify.com/album/1DFixLWuPkv3KT3TnV35m3/"),
            Some(Destination::Album("spotify:1DFixLWuPkv3KT3TnV35m3".into()))
        );
    }

    #[test]
    fn drops_the_share_query() {
        assert_eq!(
            destination("https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6?si=abc123"),
            Some(Destination::Song("spotify:6rqhFgbbKwnb9MLmUQDhG6".into()))
        );
    }

    #[test]
    fn skips_the_locale_segment() {
        assert_eq!(
            destination("https://open.spotify.com/intl-de/artist/0TnOYISbd1XYRBk9myaseg"),
            Some(Destination::Artist("spotify:0TnOYISbd1XYRBk9myaseg".into()))
        );
    }

    #[test]
    fn reads_the_legacy_user_playlist_uri() {
        assert_eq!(
            destination("spotify:user:spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
            Some(Destination::Playlist(
                "spotify:37i9dQZF1DXcBWIGoYBM5M".into()
            ))
        );
    }

    #[test]
    fn refuses_what_it_cannot_route() {
        assert_eq!(destination("spotify:show:4rOoJ6Egrf8K2IrywzwOMk"), None);
        assert_eq!(destination("spotify:track:"), None);
        assert_eq!(destination("spotify:track"), None);
        assert_eq!(destination("https://example.com/track/abc"), None);
        assert_eq!(destination("nonsense"), None);
        assert_eq!(destination(""), None);
    }
}
