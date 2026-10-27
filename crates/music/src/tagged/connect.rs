//! A provider's device network as the app sees it: every track, album and playlist id it sends
//! is untagged on the way out, and every one it receives is tagged on the way in.

use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::connect::{Collection, Command, Connect, Event, NowPlaying, Roster, Start};
use crate::tag;

/// A `Connect` that speaks tagged ids to the app and bare ids to the provider underneath.
pub(super) struct TaggedConnect {
    pub(super) slug: &'static str,
    pub(super) inner: Arc<dyn Connect>,
}

impl Connect for TaggedConnect {
    fn enable(&self, on: bool) {
        self.inner.enable(on);
    }

    fn rename(&self, naming: crate::connect::Naming) {
        self.inner.rename(naming);
    }

    /// Reports only this network's own tracks. Another provider's track playing is nothing the
    /// network can show, so it hears that nothing of its own plays.
    fn publish(&self, now: Option<NowPlaying>) {
        let own = |id: &str| tag::slug_of(id) == Some(self.slug);
        self.inner
            .publish(now.filter(|now| own(&now.track)).map(|mut now| {
                now.track = tag::untag(&now.track).to_owned();
                now.upcoming.retain(|id| own(id));
                untag_all(&mut now.upcoming);
                now.context = now.context.map(untag_collection);
                now
            }));
    }

    /// The provider's events, tagged by a thread of their own, since the receiver is read from
    /// outside any runtime.
    fn events(&self) -> Option<UnboundedReceiver<Event>> {
        let mut bare = self.inner.events()?;
        let (sender, tagged) = unbounded_channel();
        let slug = self.slug;
        let spawned = std::thread::Builder::new()
            .name(format!("{slug}-connect-events"))
            .spawn(move || {
                while let Some(event) = bare.blocking_recv() {
                    if sender.send(tag_event(slug, event)).is_err() {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            log::error!("connect: cannot spawn the {slug} event thread: {error}");
            return None;
        }
        Some(tagged)
    }

    fn device(&self) -> String {
        self.inner.device()
    }

    fn transfer(&self, to: &str) {
        self.inner.transfer(to);
    }

    fn control(&self, device: &str, command: Command) {
        self.inner.control(device, untag_command(command));
    }

    fn join(&self, link: &str) {
        self.inner.join(link);
    }

    fn start(&self) {
        self.inner.start();
    }

    fn peek(&self, link: &str) {
        self.inner.peek(link);
    }

    fn leave(&self) {
        self.inner.leave();
    }

    fn kick(&self, member: &str) {
        self.inner.kick(member);
    }
}

fn tag_event(slug: &str, event: Event) -> Event {
    let tagged = |id: &str| tag::tag(slug, id);
    match event {
        Event::Command(command) => Event::Command(map_command(command, &tagged)),
        Event::Roster(Roster { devices, elsewhere }) => Event::Roster(Roster {
            devices,
            elsewhere: elsewhere.map(|mut elsewhere| {
                elsewhere.track = elsewhere.track.as_deref().map(tagged);
                for listed in &mut elsewhere.upcoming {
                    listed.id = tagged(&listed.id);
                }
                elsewhere
            }),
        }),
        Event::Liked { track, liked } => Event::Liked {
            track: tagged(&track),
            liked,
        },
        other => other,
    }
}

fn untag_command(command: Command) -> Command {
    map_command(command, &|id: &str| tag::untag(id).to_owned())
}

/// `command` with every id it carries passed through `map`.
fn map_command(command: Command, map: &dyn Fn(&str) -> String) -> Command {
    match command {
        Command::Enqueue { id, added_by } => Command::Enqueue {
            id: map(&id),
            added_by,
        },
        Command::Start(start) => Command::Start(map_start(start, map)),
        other => other,
    }
}

fn map_start(start: Start, map: &dyn Fn(&str) -> String) -> Start {
    Start {
        collection: start
            .collection
            .map(|collection| map_collection(collection, map)),
        track: start.track.as_deref().map(map),
        upcoming: start.upcoming.iter().map(|id| map(id)).collect(),
        ..start
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

fn untag_collection(collection: Collection) -> Collection {
    map_collection(collection, &|id: &str| tag::untag(id).to_owned())
}

fn untag_all(ids: &mut [String]) {
    for id in ids {
        *id = tag::untag(id).to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device network that keeps what it was told is playing.
    #[derive(Default)]
    struct Told(std::sync::Mutex<Vec<Option<NowPlaying>>>);

    impl Connect for Told {
        fn enable(&self, _: bool) {}
        fn rename(&self, _: crate::connect::Naming) {}
        fn publish(&self, now: Option<NowPlaying>) {
            self.0.lock().unwrap().push(now);
        }
        fn events(&self) -> Option<UnboundedReceiver<Event>> {
            None
        }
        fn device(&self) -> String {
            String::new()
        }
        fn transfer(&self, _: &str) {}
        fn control(&self, _: &str, _: Command) {}
        fn join(&self, _: &str) {}
        fn start(&self) {}
        fn peek(&self, _: &str) {}
        fn leave(&self) {}
        fn kick(&self, _: &str) {}
    }

    fn playing(track: &str, upcoming: &[&str]) -> NowPlaying {
        NowPlaying {
            track: track.to_owned(),
            upcoming: upcoming.iter().map(|id| (*id).to_owned()).collect(),
            context: None,
            index: None,
            playing: true,
            position: std::time::Duration::ZERO,
            duration: std::time::Duration::ZERO,
            volume: 1.,
            shuffle: false,
            repeat: crate::connect::RepeatMode::Off,
        }
    }

    #[test]
    fn only_the_networks_own_tracks_are_reported_as_playing() {
        let told = Arc::new(Told::default());
        let connect = TaggedConnect {
            slug: "spotify",
            inner: told.clone(),
        };

        connect.publish(Some(playing(
            "spotify:a",
            &["youtube-video:b", "spotify:c"],
        )));
        connect.publish(Some(playing("youtube-video:b", &["spotify:c"])));

        let told = told.0.lock().unwrap();
        assert_eq!(told[0], Some(playing("a", &["c"])));
        assert_eq!(told[1], None);
    }

    #[test]
    fn a_start_from_the_network_comes_in_tagged_and_goes_out_bare() {
        let start = Start {
            collection: Some(Collection::Album("album".into())),
            track: Some("song".into()),
            upcoming: vec!["next".into()],
            ..Start::default()
        };
        let Event::Command(Command::Start(tagged)) =
            tag_event("spotify", Event::Command(Command::Start(start.clone())))
        else {
            panic!("a start stays a start");
        };
        assert_eq!(
            tagged.collection,
            Some(Collection::Album(tag::tag("spotify", "album")))
        );
        assert_eq!(tagged.track, Some(tag::tag("spotify", "song")));
        assert_eq!(tagged.upcoming, vec![tag::tag("spotify", "next")]);
        assert_eq!(untag_command(Command::Start(tagged)), Command::Start(start));
    }
}
