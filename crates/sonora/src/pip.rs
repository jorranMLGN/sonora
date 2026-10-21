//! The picture-in-picture window: the music video floating above other applications. While it
//! is open the video's place is `Place::Pip`, and the window lives only as long as that holds,
//! so whatever moves the video elsewhere (fullscreen, above all) closes it.

use gpui::{
    App, AppContext as _, Bounds, Global, Pixels, Size, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowHandle, WindowOptions, px, size,
};
use state::{MiniCorner, Place, Sonora};
use ui::ActiveTheme as _;
use views::PipView;

const PIP_SIZE: Size<Pixels> = size(px(480.), px(270.));
/// The smallest a remembered size may open the window, so a bad value never hides it.
const PIP_MIN: Size<Pixels> = size(px(160.), px(90.));

#[derive(Default)]
struct Pip(Option<WindowHandle<PipView>>);

impl Global for Pip {}

/// Reopens an open PiP when its corner changes, and closes it once the video's place is no
/// longer `Place::Pip`, the video setting is off, or the track is known to have no video.
pub fn watch(cx: &mut App) {
    let settings = Sonora::global(cx).settings.clone();
    let mut placed = settings.read(cx).pip_corner();
    cx.observe(&settings, move |settings, cx| {
        if is_open(cx) && !settings.read(cx).video() {
            close(cx);
            return;
        }
        let chosen = settings.read(cx).pip_corner();
        if chosen == placed {
            return;
        }
        placed = chosen;
        reposition(cx);
    })
    .detach();

    let video = Sonora::global(cx).video.clone();
    cx.observe(&video, |video, cx| {
        let on = Sonora::global(cx).settings.read(cx).video();
        let video = video.read(cx);
        if is_open(cx) && should_close(video.place(), on, video.lacks_video()) {
            close(cx);
        }
    })
    .detach();
}

pub fn is_open(cx: &App) -> bool {
    cx.try_global::<Pip>().is_some_and(|pip| pip.0.is_some())
}

pub fn toggle(cx: &mut App) {
    match is_open(cx) {
        true => close(cx),
        false => open(cx),
    }
}

/// Closes the window and hands the video back to the sidebar slot, which lets go of it in turn
/// when the sidebar is not showing.
pub fn close(cx: &mut App) {
    let Some(handle) = cx.try_global::<Pip>().and_then(|pip| pip.0) else {
        return;
    };
    cx.set_global(Pip(None));
    handle
        .update(cx, |_, window, _| window.remove_window())
        .ok();
    hand_back(cx);
}

/// Reopens the window, because gpui can size a window but not move one.
pub fn reposition(cx: &mut App) {
    if !is_open(cx) {
        return;
    }
    close(cx);
    open(cx);
}

/// Opens the window and moves the video into it. A track without a video opens nothing.
pub fn open(cx: &mut App) {
    if is_open(cx) {
        return;
    }
    let video = Sonora::global(cx).video.clone();
    if !video.read(cx).has_video() {
        return;
    }
    let settings = Sonora::global(cx).settings.read(cx);
    let screen = crate::mini::screen(cx);
    let placed = bounds(settings.pip_corner(), settings.pip_size(), screen);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(placed)),
            titlebar: Some(TitlebarOptions {
                title: Some("Sonora".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_decorations: Some(WindowDecorations::Client),
            kind: crate::mini::kind(true),
            is_movable: true,
            is_resizable: true,
            is_minimizable: false,
            focus: false,
            window_min_size: Some(PIP_MIN),
            app_id: Some("sonora".into()),
            ..Default::default()
        },
        |window, cx| {
            window.set_rem_size(cx.theme().font_size);
            cx.new(|cx| {
                PipView::new(PIP_MIN, screen.size, window, cx)
                    .on_shape(|window, width, height| crate::aspect::lock(window, width, height))
            })
        },
    );

    match opened {
        Ok(handle) => {
            // after the window is mapped, as for the mini player
            handle
                .update(cx, |view, window, cx| {
                    crate::above::raise(window);
                    view.lock_shape(window, cx);
                })
                .ok();
            cx.set_global(Pip(Some(handle)));
            video.update(cx, |video, cx| video.set_place(Place::Pip, cx));
            // the window manager's close button, or the view's own back button; a window
            // `close` already let go of, as in `reposition`, is no longer the open one
            let id = handle.window_id();
            cx.observe_release(&handle.entity(cx).unwrap(), move |_, cx| {
                let open = cx.try_global::<Pip>().and_then(|pip| pip.0);
                if open.is_some_and(|open| open.window_id() == id) {
                    cx.set_global(Pip(None));
                    hand_back(cx);
                }
            })
            .detach();
        }
        Err(error) => log::warn!("pip: cannot open the window: {error:#}"),
    }
}

/// Whether an open window has lost its reason: the video moved elsewhere, the setting went
/// off, or the provider said the track has no video.
fn should_close(place: Place, video_on: bool, lacks_video: bool) -> bool {
    place != Place::Pip || !video_on || lacks_video
}

/// Moves a video still placed in the PiP back to the slot. A place something else has taken
/// since, such as fullscreen, stays.
fn hand_back(cx: &mut App) {
    let video = Sonora::global(cx).video.clone();
    if video.read(cx).place() == Place::Pip {
        video.update(cx, |video, cx| video.set_place(Place::Slot, cx));
    }
}

/// Where the window opens: `PIP_SIZE`, or the size it was last left at, in `corner` of
/// `screen`. A remembered size is kept between `PIP_MIN` and the screen.
fn bounds(
    corner: MiniCorner,
    stored: Option<(f32, f32)>,
    screen: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let wanted = stored
        .map(|(width, height)| size(px(width), px(height)))
        .unwrap_or(PIP_SIZE);
    let fitted = size(
        wanted.width.min(screen.size.width).max(PIP_MIN.width),
        wanted.height.min(screen.size.height).max(PIP_MIN.height),
    );
    crate::mini::cornered(corner, fitted, screen)
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};
    use state::{MiniCorner, Place};

    use super::{bounds, should_close};

    #[test]
    fn pip_closes_when_it_has_nothing_to_show() {
        assert!(!should_close(Place::Pip, true, false));
        assert!(should_close(Place::Fullscreen, true, false));
        assert!(should_close(Place::Pip, false, false));
        assert!(should_close(Place::Pip, true, true));
    }

    #[test]
    fn pip_placement_uses_the_stored_size() {
        let screen = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1920.), px(1080.)),
        };

        let fresh = bounds(MiniCorner::BottomRight, None, screen);
        assert_eq!(fresh.size, size(px(480.), px(270.)));
        assert_eq!(
            fresh.origin,
            point(px(1920. - 480. - 24.), px(1080. - 270. - 24.))
        );

        let kept = bounds(MiniCorner::TopLeft, Some((640., 360.)), screen);
        assert_eq!(kept.size, size(px(640.), px(360.)));
        assert_eq!(kept.origin, point(px(24.), px(24.)));
    }
}
