//! Whether Cisco's OpenH264 is here, and the one question that fetches it. Modelled on `Drm`:
//! nothing is looked for until a canvas actually wants playing, and "not now" lasts the run.

use gpui::{Context, Task};
use music::video::codec;

use crate::{Io, join};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecState {
    /// Not looked for yet.
    Looking,
    Ready,
    /// Looked for, not found, not asked about.
    Missing,
    /// The prompt is up.
    Wanted,
    Installing,
    /// The user said not now; asked again only from Settings.
    Declined,
    /// The download or the check failed; Settings can retry.
    Failed,
}

/// Where Cisco's decoder stands for this run, and the install in flight.
pub struct Codec {
    state: CodecState,
    /// Whether the install in flight came from the prompt, which is the only install the prompt
    /// stays up for; one started from Settings shows in Settings alone.
    prompted: bool,
    io: Io,
    task: Option<Task<()>>,
}

impl Codec {
    pub fn new(io: Io) -> Self {
        Self {
            state: CodecState::Looking,
            prompted: false,
            io,
            task: None,
        }
    }

    pub fn state(&self) -> CodecState {
        self.state
    }

    /// Whether the prompt should be up: asking, or installing what it was asked to.
    pub fn prompting(&self) -> bool {
        match self.state {
            CodecState::Wanted => true,
            CodecState::Installing => self.prompted,
            _ => false,
        }
    }

    /// Whether Cisco publishes a binary for this platform, which is when Settings shows a row.
    pub fn supported(&self) -> bool {
        codec::supported()
    }

    /// Looks once, lazily, and answers whether a decoder can be loaded now.
    pub fn ready(&mut self) -> bool {
        if self.state == CodecState::Looking {
            self.state = match codec::find() {
                Some(_) => CodecState::Ready,
                None => CodecState::Missing,
            };
        }
        self.state == CodecState::Ready
    }

    /// A canvas wants playing and there is no decoder: raise the prompt, once a run.
    pub fn want(&mut self, cx: &mut Context<Self>) {
        if !self.ready() && self.state == CodecState::Missing && codec::supported() {
            self.state = CodecState::Wanted;
            cx.notify();
        }
    }

    /// Downloads and installs the decoder, from the prompt or from Settings.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        if self.state == CodecState::Installing {
            return;
        }
        self.prompted = self.state == CodecState::Wanted;
        self.state = CodecState::Installing;
        cx.notify();
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = join(io.spawn(codec::install())).await;
            this.update(cx, |this, cx| {
                this.task = None;
                this.prompted = false;
                this.state = match result {
                    Ok(_) => CodecState::Ready,
                    Err(error) => {
                        log::warn!("codec: cannot install openh264: {error:#}");
                        CodecState::Failed
                    }
                };
                cx.notify();
            })
            .ok();
        }));
    }

    /// "Not now" on the prompt. Ignored while installing, so a dismiss cannot be overwritten by
    /// the download finishing a moment later.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.state == CodecState::Installing {
            return;
        }
        self.state = CodecState::Declined;
        cx.notify();
    }

    /// Removes the decoder from Settings. A canvas already playing keeps its open decoder.
    pub fn uninstall(&mut self, cx: &mut Context<Self>) {
        match codec::uninstall() {
            Ok(()) => self.state = CodecState::Missing,
            Err(error) => log::warn!("codec: cannot remove openh264: {error:#}"),
        }
        cx.notify();
    }
}
