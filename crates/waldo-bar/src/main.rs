//! The Waldo bar: type a task, and Claude Code does it on this Mac.
//!
//! A small native window. Each task is one headless Claude Code run with
//! the `os` tools. The bar shows what the run says and
//! does, and it holds the two brakes: stop this run, and stop everything.

mod run;
mod theme;

use futures::channel::mpsc;
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    div, px, size, App, Application, Bounds, ClickEvent, Context, FocusHandle, FontWeight,
    KeyDownEvent, Render, ScrollHandle, SharedString, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
};
use waldo_core::state;
use run::Ev;
use std::time::Duration;
use theme::{theme, Theme, BODY_SMALL, HEADER_BAR, LABEL_SMALL, RADIUS_CONTROL, TOP_BAR_LEADING};

enum Item {
    User(String),
    Say(String),
    Tool(String),
    Error(String),
    Note(String),
}

struct Bar {
    items: Vec<Item>,
    input: String,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// Process id of the run in flight.
    running: Option<u32>,
    /// Lets the next task carry on the same Claude Code session.
    session: Option<String>,
    /// Scripted tasks still to run, one after the other.
    queue: std::collections::VecDeque<String>,
}

impl Bar {
    fn new(cx: &mut Context<Self>) -> Self {
        // The stop switch can be thrown from outside: the corner gesture, a
        // gate panel, or `waldo stop`. Redraw each second so the chip is true.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();

        // A smoke hook: WALDO_BAR_AUTOSEND runs tasks at startup with no typing.
        // Split several with "||". Each one starts when the last has ended.
        let queue: std::collections::VecDeque<String> = std::env::var("WALDO_BAR_AUTOSEND")
            .map(|all| all.split("||").map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
            .unwrap_or_default();
        let mut bar = Self {
            items: Vec::new(),
            input: String::new(),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            running: None,
            session: None,
            queue,
        };
        bar.next_queued(cx);
        bar
    }

    fn next_queued(&mut self, cx: &mut Context<Self>) {
        if self.running.is_none() {
            if let Some(task) = self.queue.pop_front() {
                self.submit(task, cx);
            }
        }
    }

    fn push(&mut self, item: Item, cx: &mut Context<Self>) {
        self.items.push(item);
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        if self.running.is_some() || text.is_empty() {
            return;
        }
        if state::stopped() {
            self.push(Item::Error("Everything is stopped. Press Resume first.".into()), cx);
            return;
        }
        self.push(Item::User(text.clone()), cx);
        let (tx, mut rx) = mpsc::unbounded::<Ev>();
        match run::start(&text, self.session.as_deref(), tx) {
            Ok(pid) => self.running = Some(pid),
            Err(e) => {
                self.push(Item::Error(e), cx);
                return;
            }
        }
        cx.spawn(async move |this, cx| {
            while let Some(ev) = rx.next().await {
                if this.update(cx, |bar: &mut Self, cx| bar.apply(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply(&mut self, ev: Ev, cx: &mut Context<Self>) {
        match ev {
            Ev::Session(id) => self.session = Some(id),
            Ev::Say(text) => self.push(Item::Say(text), cx),
            Ev::Tool(line) => self.push(Item::Tool(line), cx),
            Ev::ToolError(text) => self.push(Item::Error(text), cx),
            Ev::Done { ok: true, note } => self.push(Item::Note(note), cx),
            Ev::Done { ok: false, note } => self.push(Item::Error(note), cx),
            Ev::Exit(text) => {
                self.running = None;
                if !text.is_empty() {
                    self.push(Item::Error(text), cx);
                }
                self.next_queued(cx);
            }
        }
        cx.notify();
    }

    fn stop_run(&mut self, cx: &mut Context<Self>) {
        if let Some(pid) = self.running {
            run::stop(pid);
            self.push(Item::Note("You stopped this run.".into()), cx);
        }
    }

    fn toggle_stop_all(&mut self, cx: &mut Context<Self>) {
        if state::stopped() {
            state::resume();
            self.push(Item::Note("Resumed. The tools work again.".into()), cx);
        } else {
            state::stop("stopped from the bar");
            if let Some(pid) = self.running {
                run::stop(pid);
            }
            self.push(Item::Note("Everything is stopped. No session can use the tools.".into()), cx);
        }
    }

    fn new_session(&mut self, cx: &mut Context<Self>) {
        if self.running.is_none() {
            self.session = None;
            self.items.clear();
            cx.notify();
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if ks.modifiers.platform {
            match ks.key.as_str() {
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.input.push_str(text.trim_end());
                    }
                }
                "backspace" => self.input.clear(),
                _ => return,
            }
            cx.notify();
            return;
        }
        if ks.modifiers.control {
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                self.input.pop();
            }
            "enter" => {
                let text = std::mem::take(&mut self.input);
                self.submit(text, cx);
            }
            "space" => self.input.push(' '),
            _ => match ks.key_char.as_deref() {
                Some(ch) => self.input.push_str(ch),
                None => return,
            },
        }
        cx.notify();
    }

    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        t: &Theme,
        cx: &mut Context<Self>,
        on_click: fn(&mut Self, &mut Context<Self>),
    ) -> impl IntoElement {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded(RADIUS_CONTROL)
            .border_1()
            .border_color(t.border)
            .text_size(LABEL_SMALL)
            .text_color(t.text)
            .cursor_pointer()
            .hover(|s| s.bg(t.element_hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
            .child(label)
    }
}

impl Render for Bar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *theme(cx);
        let focused = self.focus.is_focused(window);
        let stopped = state::stopped();
        let (chip, chip_color) = if stopped {
            ("stopped", t.error)
        } else if self.running.is_some() {
            ("working", t.accent)
        } else {
            ("ready", t.success)
        };

        let mut buttons = div().flex().items_center().gap_2().pr_3();
        if self.running.is_some() && !stopped {
            buttons = buttons.child(self.button("stop-run", "Stop", &t, cx, Self::stop_run));
        }
        if self.running.is_none() && !self.items.is_empty() {
            buttons = buttons.child(self.button("new", "New", &t, cx, Self::new_session));
        }
        buttons = buttons.child(self.button(
            "stop-all",
            if stopped { "Resume" } else { "Stop everything" },
            &t,
            cx,
            Self::toggle_stop_all,
        ));

        div()
            .size_full()
            .bg(t.background)
            .text_color(t.text)
            .text_size(BODY_SMALL)
            .flex()
            .flex_col()
            .child(
                div()
                    .h(HEADER_BAR)
                    .flex_none()
                    .pl(TOP_BAR_LEADING)
                    .border_b_1()
                    .border_color(t.border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(LABEL_SMALL)
                                    .font_weight(FontWeight::BOLD)
                                    .child("WALDO"),
                            )
                            .child(div().text_size(LABEL_SMALL).text_color(chip_color).child(chip)),
                    )
                    .child(buttons),
            )
            .child(
                div()
                    .id("log")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .bg(t.surface)
                    .px_3()
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when(self.items.is_empty(), |el| {
                        el.child(div().text_color(t.text_muted).child(
                            "Say what to do on this Mac. Claude Code does it with the shell and the screen. \
                             Money, sending, passwords, and deletes always stop and ask you first.",
                        ))
                    })
                    .children(self.items.iter().map(|item| match item {
                        Item::User(msg) => div()
                            .p_2()
                            .rounded(RADIUS_CONTROL)
                            .bg(t.elevated_surface)
                            .child(SharedString::from(msg.clone())),
                        Item::Say(msg) => div().px_1().child(SharedString::from(msg.clone())),
                        Item::Tool(msg) => div()
                            .px_2()
                            .py_1()
                            .border_l_2()
                            .border_color(t.accent)
                            .text_size(LABEL_SMALL)
                            .font_family("Menlo")
                            .text_color(t.text_muted)
                            .child(SharedString::from(msg.clone())),
                        Item::Error(msg) => div()
                            .px_1()
                            .text_color(t.error)
                            .child(SharedString::from(msg.clone())),
                        Item::Note(msg) => div()
                            .px_1()
                            .text_size(LABEL_SMALL)
                            .text_color(t.text_muted)
                            .child(SharedString::from(msg.clone())),
                    }))
                    .when(self.running.is_some(), |el| {
                        el.child(
                            div()
                                .px_1()
                                .text_size(LABEL_SMALL)
                                .text_color(t.text_muted)
                                .child("working…"),
                        )
                    }),
            )
            .child(
                div()
                    .id("input")
                    .flex_none()
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(Self::on_key))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, _| {
                        window.focus(&this.focus)
                    }))
                    .border_t_1()
                    .border_color(if focused { t.accent } else { t.border })
                    .px_3()
                    .py_3()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .map(|el| {
                        if self.input.is_empty() {
                            el.child(div().text_color(t.text_muted).child(if stopped {
                                "Stopped. Press Resume to carry on."
                            } else if self.session.is_some() {
                                "What next?"
                            } else {
                                "What should Claude do on this Mac?"
                            }))
                        } else {
                            el.child(div().child(SharedString::from(self.input.clone())))
                        }
                    })
                    .when(focused, |el| el.child(div().ml_1().w(px(7.)).h(px(15.)).bg(t.accent))),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.set_global(Theme::dark());
        // WALDO_BAR_QUIET opens the bar without taking the focus from your work.
        let quiet = std::env::var_os("WALDO_BAR_QUIET").is_some();
        let bounds = Bounds::centered(None, size(px(560.), px(440.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Waldo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(12.), px(12.))),
                }),
                window_min_size: Some(size(px(420.), px(260.))),
                focus: !quiet,
                ..Default::default()
            },
            |window, cx| {
                let bar = cx.new(Bar::new);
                let focus = bar.read(cx).focus.clone();
                window.focus(&focus);
                bar
            },
        )
        .unwrap();
        if !quiet {
            cx.activate(true);
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
