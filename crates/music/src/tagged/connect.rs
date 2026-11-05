//! A provider's device network as the app sees it: every track, album and playlist id it sends
//! is untagged on the way out, and every one it receives is tagged on the way in.

use std::sync::Arc;

use crate::connect::{
    Collection, Command, Connect, Event, QueuedTrack, Request, Roster, Snapshot, Start,
};
use crate::tag;

/// A `Connect` that speaks tagged ids to the app and bare ids to `inner`, the provider's own.
/// Two threads carry the traffic across, since both ends are read from outside any runtime.
/// Dropping the returned handle ends both threads, and with them the hold on `inner`.
pub(super) fn wrap(slug: &'static str, inner: Arc<Connect>) -> Option<Arc<Connect>> {
    let (outer, mut requests, events) = Connect::new(inner.device().to_owned());
    let bare = inner.events();
    let spawned = std::thread::Builder::new()
        .name(format!("{slug}-connect-requests"))
        .spawn(move || {
            while let Some(request) = requests.blocking_recv() {
                inner.send(untag_request(slug, request));
            }
        });
    if let Err(error) = spawned {
        log::error!("connect: cannot spawn the {slug} request thread: {error}");
        return None;
    }
    if let Some(mut bare) = bare {
        let spawned = std::thread::Builder::new()
            .name(format!("{slug}-connect-events"))
            .spawn(move || {
                while let Some(event) = bare.blocking_recv() {
                    if events.send(tag_event(slug, event)).is_err() {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            log::error!("connect: cannot spawn the {slug} event thread: {error}");
        }
    }
    Some(Arc::new(outer))
}

/// `request` as the provider underneath takes it. A report of what plays keeps only this
/// network's own tracks: another provider's track playing is nothing the network can show, so it
/// hears that nothing of its own plays.
fn untag_request(slug: &str, request: Request) -> Request {
    let own = |queued: &QueuedTrack| tag::slug_of(&queued.id) == Some(slug);
    match request {
        Request::Publish(Some(state)) => {
            if state.track.as_ref().is_some_and(|track| !own(track)) {
                return Request::Publish(None);
            }
            let mut state = (*state).clone();
            state.previous.retain(own);
            state.upcoming.retain(own);
            Request::Publish(Some(Arc::new(map_snapshot(state, &untag))))
        }
        Request::Control(device, command) => Request::Control(device, map_command(command, &untag)),
        other => other,
    }
}

fn tag_event(slug: &str, event: Event) -> Event {
    let tagged = |id: &str| tag::tag(slug, id);
    match event {
        Event::Command(command) => Event::Command(map_command(command, &tagged)),
        Event::Roster(Roster { devices, elsewhere }) => Event::Roster(Roster {
            devices,
            elsewhere: elsewhere.map(|mut elsewhere| {
                elsewhere.state = Arc::new(map_snapshot((*elsewhere.state).clone(), &tagged));
                elsewhere
            }),
        }),
        Event::Liked { track, liked } => Event::Liked {
            track: tagged(&track),
            liked,
        },
        Event::PlaylistChanged(id) => Event::PlaylistChanged(tagged(&id)),
        other => other,
    }
}

fn untag(id: &str) -> String {
    tag::untag(id).to_owned()
}

/// `command` with every id it carries passed through `map`.
fn map_command(command: Command, map: &dyn Fn(&str) -> String) -> Command {
    match command {
        Command::Enqueue { id, added_by } => Command::Enqueue {
            id: map(&id),
            added_by,
        },
        Command::Start(start) => Command::Start(Start {
            state: Arc::new(map_snapshot((*start.state).clone(), map)),
            ..start
        }),
        Command::SetQueue { previous, upcoming } => Command::SetQueue {
            previous: map_queued(previous, map),
            upcoming: map_queued(upcoming, map),
        },
        other => other,
    }
}

fn map_snapshot(state: Snapshot, map: &dyn Fn(&str) -> String) -> Snapshot {
    Snapshot {
        track: state.track.map(|track| map_track(track, map)),
        previous: map_queued(state.previous, map),
        upcoming: map_queued(state.upcoming, map),
        context: state
            .context
            .map(|collection| map_collection(collection, map)),
        ..state
    }
}

fn map_queued(tracks: Vec<QueuedTrack>, map: &dyn Fn(&str) -> String) -> Vec<QueuedTrack> {
    tracks
        .into_iter()
        .map(|track| map_track(track, map))
        .collect()
}

fn map_track(track: QueuedTrack, map: &dyn Fn(&str) -> String) -> QueuedTrack {
    QueuedTrack {
        id: map(&track.id),
        ..track
    }
}

fn map_collection(collection: Collection, map: &dyn Fn(&str) -> String) -> Collection {
    match collection {
        Collection::Album(id) => Collection::Album(map(&id)),
        Collection::Playlist(id) => Collection::Playlist(map(&id)),
        Collection::Radio(id) => Collection::Radio(map(&id)),
        Collection::Saved => Collection::Saved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(id: &str) -> QueuedTrack {
        QueuedTrack::new(id.to_owned(), false)
    }

    fn playing(track: &str, upcoming: &[&str]) -> Snapshot {
        Snapshot {
            track: Some(queued(track)),
            upcoming: upcoming.iter().map(|id| queued(id)).collect(),
            playing: true,
            volume: 1.,
            ..Snapshot::default()
        }
    }

    fn published(request: Request) -> Option<Snapshot> {
        match request {
            Request::Publish(state) => state.map(|state| (*state).clone()),
            _ => panic!("a publish stays a publish"),
        }
    }

    #[test]
    fn only_the_networks_own_tracks_are_reported_as_playing() {
        let own = untag_request(
            "spotify",
            Request::Publish(Some(Arc::new(playing(
                "spotify:a",
                &["youtube-video:b", "spotify:c"],
            )))),
        );
        let other = untag_request(
            "spotify",
            Request::Publish(Some(Arc::new(playing("youtube-video:b", &["spotify:c"])))),
        );

        assert_eq!(published(own), Some(playing("a", &["c"])));
        assert_eq!(published(other), None);
    }

    #[test]
    fn a_start_from_the_network_comes_in_tagged_and_goes_out_bare() {
        let start = Start {
            state: Arc::new(Snapshot {
                context: Some(Collection::Album("album".into())),
                ..playing("song", &["next"])
            }),
            ..Start::default()
        };
        let Event::Command(Command::Start(tagged)) =
            tag_event("spotify", Event::Command(Command::Start(start.clone())))
        else {
            panic!("a start stays a start");
        };
        assert_eq!(
            tagged.state.context,
            Some(Collection::Album(tag::tag("spotify", "album")))
        );
        assert_eq!(
            tagged.state.track.as_ref().map(|track| track.id.as_str()),
            Some(tag::tag("spotify", "song").as_str())
        );
        assert_eq!(tagged.state.upcoming[0].id, tag::tag("spotify", "next"));
        assert_eq!(
            map_command(Command::Start(tagged), &untag),
            Command::Start(start)
        );
    }
}
