//! The hero at the top of the home page: a new release from an artist the user follows, or
//! else a mix, over a wash of its cover's hue.

use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, Hsla, div, linear_color_stop, linear_gradient};
use i18n::t;
use router::{Destination, navigate};
use state::{Discover, Featured, Origin, Playback, Sonora};
use ui::{ActiveTheme as _, Button, Rising as _};

use crate::shared::cells;
use crate::shared::hero::{HeroMetaStrip, HeroPlayButton, PageHero, release_date_label};

/// How strongly the cover's hue shows at the top of the wash.
const WASH: f32 = 0.35;

/// The cover the hero shows, so the view can sample its hue before drawing it.
pub(crate) fn cover(featured: &Featured, discover: &Discover) -> Option<String> {
    match featured {
        Featured::Release(album) => album.cover_large.clone().or_else(|| album.cover.clone()),
        Featured::Mix(index) => discover.mixes().get(*index)?.seed.cover.clone(),
    }
}

/// The hero over a wash of the cover's hue, or `None` when a mix has gone from under it.
pub(crate) fn hero(
    featured: &Featured,
    discover: &Discover,
    tint: Option<Hsla>,
    playback: &Entity<Playback>,
    cx: &App,
) -> Option<AnyElement> {
    let theme = *cx.theme();
    let top = tint.map_or(theme.secondary, |tint| tint.opacity(WASH));
    let page = match featured {
        Featured::Release(album) => {
            let session = Sonora::global(cx).session.read(cx);
            let provider = music::tag::slug_of(&album.id)
                .and_then(|slug| session.provider_name_for(slug))
                .unwrap_or_default();
            let opened = album.id.clone();
            PageHero::new("home-featured", album.name.clone())
                .cover(cover(featured, discover))
                .eyebrow(t!("home-featured-release", provider = provider))
                .meta(
                    HeroMetaStrip::new()
                        .item(cells::artist_links(
                            "home-featured-artists",
                            album.artist_refs.clone(),
                            album.artists.clone(),
                            theme.muted_foreground,
                        ))
                        .text(release_date_label(&album.release_date)),
                )
                .actions(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            HeroPlayButton::new(
                                "home-featured-play",
                                t!("home-featured-play"),
                                discover.featured_tracks().as_ref().clone(),
                                playback.clone(),
                            )
                            .from(Some(Origin::album(album.id.clone()))),
                        )
                        .child(
                            Button::new("home-featured-open")
                                .outline()
                                .label(t!("home-featured-open"))
                                .on_click(move |_, _, cx| {
                                    navigate(Destination::Album(opened.clone().into()), cx)
                                }),
                        ),
                )
        }
        Featured::Mix(index) => {
            let mixes = discover.mixes();
            let mix = mixes.get(*index)?;
            let seed = mix.seed.id.clone().unwrap_or_default();
            PageHero::new(
                "home-featured",
                t!("home-because", artist = mix.seed.artists.as_str()),
            )
            .cover(cover(featured, discover))
            .eyebrow(t!("home-featured-mix"))
            .actions(
                HeroPlayButton::new(
                    "home-featured-play",
                    t!("home-featured-play"),
                    mix.tracks.clone(),
                    playback.clone(),
                )
                .from(Some(Origin::radio(seed))),
            )
        }
    };

    Some(
        div()
            .w_full()
            .p(theme.metrics.inset)
            .rounded(theme.radius * 2.)
            .bg(linear_gradient(
                180.,
                linear_color_stop(top, 0.),
                linear_color_stop(theme.background.opacity(0.), 1.),
            ))
            .child(page)
            .rising("home-featured")
            .into_any_element(),
    )
}
