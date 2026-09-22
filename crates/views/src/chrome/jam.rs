use gpui::prelude::*;
use gpui::{App, ClipboardItem, Context, Entity, Render, SharedString, Window, div, px};
use i18n::t;
use state::{Jam, JamRole, Listener, Outcome, Sonora, Toasts};
use ui::{ActiveTheme as _, Button, Input, Skeleton, Text, Vacancy, eyebrow};

const ROW: f32 = 34.;

pub(crate) struct JamPanel {
    jam: Entity<Jam>,
    address: Entity<Input>,
    code: Entity<Input>,
}

impl JamPanel {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let jam = Sonora::global(cx).jam.clone();
        cx.observe(&jam, |_, _, cx| cx.notify()).detach();

        Self {
            jam,
            address: cx.new(|cx| Input::new("jam-join-hint", cx).compact()),
            code: cx.new(|cx| Input::new("jam-code-hint", cx).compact()),
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.jam.update(cx, |jam, cx| jam.start(cx));
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.jam.update(cx, |jam, cx| jam.stop(cx));
    }

    fn join(&mut self, cx: &mut Context<Self>) {
        let at = self.address.read(cx).text().trim().to_owned();
        let code = self.code.read(cx).text().trim().to_owned();
        if at.is_empty() || code.is_empty() {
            return;
        }

        self.jam.update(cx, |jam, cx| jam.join(at, code, cx));
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        self.jam.update(cx, |jam, cx| jam.leave(cx));
    }

    fn idle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                Vacancy::new(t!("jam-empty"))
                    .icon("icons/radio-tower.svg")
                    .action(
                        Button::new("jam-start")
                            .outline()
                            .label(t!("jam-start"))
                            .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                    ),
            )
            .child(eyebrow(t!("jam-join"), cx))
            .child(self.address.clone())
            .child(self.code.clone())
            .child(
                Button::new("jam-join")
                    .label(t!("jam-join"))
                    .on_click(cx.listener(|this, _, _, cx| this.join(cx))),
            )
    }

    fn hosting(
        &self,
        room: &str,
        code: &str,
        addresses: &[String],
        listeners: &[Listener],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = *cx.theme();
        let lead = self.jam.read(cx).lead();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(
                div()
                    .text_size(theme.text(Text::Large))
                    .child(room.to_owned()),
            )
            .child(eyebrow(t!("jam-open-on"), cx))
            .children(addresses.iter().enumerate().map(|(index, address)| {
                let url = format!("http://{address}/c/{code}");
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme.text(Text::Small))
                            .child(SharedString::from(url.clone())),
                    )
                    .child(
                        Button::new(("jam-copy", index))
                            .ghost()
                            .small()
                            .icon("icons/copy.svg")
                            .tooltip_above("jam-copy")
                            .on_click(move |_, _, cx| copy(&url, cx)),
                    )
            }))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .text_size(theme.text(Text::Small))
                    .child(t!("jam-code", code = code)),
            )
            .child(eyebrow(
                match listeners.is_empty() {
                    true => t!("jam-no-listeners"),
                    false => t!("jam-listeners", count = listeners.len()),
                },
                cx,
            ))
            .children(listeners.iter().enumerate().map(|(index, listener)| {
                let at = listener.at.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(ROW))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme.text(Text::Small))
                            .child(SharedString::from(listener.name.clone())),
                    )
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .text_size(theme.text(Text::Small))
                            .child(t!("jam-need", need = listener.need as i64)),
                    )
                    .child(
                        Button::new(("jam-kick", index))
                            .ghost()
                            .small()
                            .icon("icons/x.svg")
                            .tooltip_above("jam-kick")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.jam.update(cx, |jam, cx| jam.kick(&at, cx));
                            })),
                    )
            }))
            .child(eyebrow(t!("jam-lead"), cx))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .text_size(theme.text(Text::Small))
                    .child(t!("jam-lead-auto", lead = lead as i64)),
            )
            .child(
                Button::new("jam-stop")
                    .outline()
                    .label(t!("jam-stop"))
                    .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
            )
    }

    fn waiting(&self, caption: SharedString, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(eyebrow(caption, cx))
            .child(Skeleton::new().h(px(ROW)))
            .child(
                Button::new("jam-cancel")
                    .outline()
                    .label(t!("jam-cancel"))
                    .on_click(cx.listener(|this, _, _, cx| this.leave(cx))),
            )
    }

    fn listening(&self, room: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(eyebrow(t!("jam-listening", room = room), cx))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .text_size(theme.text(Text::Small))
                    .child(t!("jam-title")),
            )
            .child(
                Button::new("jam-leave")
                    .outline()
                    .label(t!("jam-leave"))
                    .on_click(cx.listener(|this, _, _, cx| this.leave(cx))),
            )
    }
}

impl Render for JamPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let role = self.jam.read(cx).role().clone();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(match role {
                JamRole::Idle => self.idle(cx).into_any_element(),
                JamRole::Hosting {
                    room,
                    code,
                    addresses,
                    listeners,
                    ..
                } => self
                    .hosting(&room, &code, &addresses, &listeners, cx)
                    .into_any_element(),
                JamRole::Joining { at } => self
                    .waiting(t!("jam-joining", at = at), cx)
                    .into_any_element(),
                JamRole::Lost { .. } => self.waiting(t!("jam-lost"), cx).into_any_element(),
                JamRole::Listening { room, .. } => self.listening(&room, cx).into_any_element(),
            })
    }
}

fn copy(url: &str, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(url.to_owned()));
    Toasts::show(Outcome::Done, "toast-jam-copied", cx);
}

pub(crate) fn panel(cx: &mut App) -> Entity<JamPanel> {
    cx.new(JamPanel::new)
}
