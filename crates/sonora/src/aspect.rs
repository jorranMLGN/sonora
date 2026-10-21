//! Locks a window's shape to a picture's while the user resizes it, on X11.

/// Asks the window manager to keep `window` at `width`:`height` while it is resized.
///
/// gpui exposes no aspect ratio, so this sets the ICCCM `WM_NORMAL_HINTS` aspect fields over a
/// short-lived connection of our own, as [`crate::above::raise`] does for the window level. The
/// window's current hints are read first and only the aspect is replaced, so gpui's own minimum
/// and maximum size survive. A window manager that honours the hint keeps the shape live during
/// the drag; one that ignores it, and Wayland, macOS and Windows, where this is a no-op, rely on
/// `PipView`'s snap after the resize settles.
#[cfg(target_os = "linux")]
pub fn lock(window: &gpui::Window, width: u32, height: u32) {
    use anyhow::Context as _;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection as _;
    use x11rb::properties::{AspectRatio, WmSizeHints};

    let Some((numerator, denominator)) = ratio(width, height) else {
        return;
    };
    let locked = || -> anyhow::Result<()> {
        let handle = HasWindowHandle::window_handle(window).context("this window has no handle")?;
        let RawWindowHandle::Xcb(handle) = handle.as_raw() else {
            return Ok(());
        };
        let (xcb, _) = x11rb::connect(None)?;
        let id = handle.window.get();
        let mut hints = WmSizeHints::get_normal_hints(&xcb, id)?
            .reply()?
            .unwrap_or_default();
        let shape = AspectRatio::new(numerator, denominator);
        hints.aspect = Some((shape, shape));
        hints.set_normal_hints(&xcb, id)?.check()?;
        xcb.flush()?;
        Ok(())
    };

    if let Err(error) = locked() {
        log::warn!("pip: cannot lock the window to the video's shape: {error:#}");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn lock(_window: &gpui::Window, _width: u32, _height: u32) {}

/// `width`:`height` in lowest terms, as the hint carries it, or `None` for an empty picture.
#[cfg(any(target_os = "linux", test))]
fn ratio(width: u32, height: u32) -> Option<(i32, i32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let (mut a, mut b) = (width, height);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Some((
        i32::try_from(width / a).ok()?,
        i32::try_from(height / a).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_size_locks_to_its_ratio_in_lowest_terms() {
        assert_eq!(ratio(1920, 1080), Some((16, 9)));
        assert_eq!(ratio(854, 480), Some((427, 240)));
        assert_eq!(ratio(1080, 1920), Some((9, 16)));
        assert_eq!(ratio(16, 9), Some((16, 9)));
        assert_eq!(ratio(0, 1080), None);
        assert_eq!(ratio(1920, 0), None);
    }
}
