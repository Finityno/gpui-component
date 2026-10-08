use std::time::Duration;

use gpui::{Context, Pixels, Task, px};

static INTERVAL: Duration = Duration::from_millis(500);
static PAUSE_DELAY: Duration = Duration::from_millis(300);

// Keep caret width consistent across platforms.
#[cfg(not(target_os = "macos"))]
pub(super) const CURSOR_WIDTH: Pixels = px(1.5);
#[cfg(target_os = "macos")]
pub(super) const CURSOR_WIDTH: Pixels = px(1.5);

/// To manage the Input cursor blinking.
///
/// It will start blinking with a interval of 500ms.
/// Every loop will notify the view to update the `visible`, and Input will observe this update to touch repaint.
///
/// The input painter will check if this in visible state, then it will draw the cursor.
pub(crate) struct BlinkCursor {
    visible: bool,
    paused: bool,
    epoch: usize,

    _task: Task<()>,
}

impl BlinkCursor {
    pub fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            epoch: 0,
            _task: Task::ready(()),
        }
    }

    /// Start the blinking
    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.blink(self.epoch, cx);
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.epoch = 0;
        cx.notify();
    }

    /// Stop blinking and keep the cursor drawn until the next `start`.
    pub fn hold_visible(&mut self, cx: &mut Context<Self>) {
        self.next_epoch();
        self._task = Task::ready(());
        // Canceling the pause timer must not leave reactivation paused.
        self.paused = false;
        if !self.visible {
            self.visible = true;
            cx.notify();
        }
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if self.paused || epoch != self.epoch {
            self.visible = true;
            return;
        }

        self.visible = !self.visible;
        cx.notify();

        // Schedule the next blink
        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.blink(epoch, cx));
            }
        });
    }

    pub fn visible(&self) -> bool {
        // Keep showing the cursor if paused
        self.paused || self.visible
    }

    /// Pause the blinking, and delay to resume the blinking.
    ///
    /// After the delay the cursor stays visible for one full blink interval
    /// before toggling, so it doesn't flash off immediately after a text edit.
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.paused = true;
        self.visible = true;
        cx.notify();

        // Advance epoch to cancel any in-flight blink task.
        self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE_DELAY).await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.paused = false;
                    // Keep visible and schedule the first toggle after a full
                    // interval so the cursor doesn't disappear right away.
                    this.visible = true;
                    cx.notify();

                    let epoch = this.next_epoch();
                    this._task = cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(INTERVAL).await;
                        if let Some(this) = this.upgrade() {
                            this.update(cx, |this, cx| this.blink(epoch, cx));
                        }
                    });
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{BlinkCursor, INTERVAL};
    use gpui::{AppContext as _, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn holding_the_cursor_visible_stops_the_blink_timer(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update({
            let notifications = notifications.clone();
            |cx| {
                cx.observe(&cursor, move |_, _| {
                    notifications.set(notifications.get() + 1)
                })
            }
        });

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.executor().advance_clock(INTERVAL * 4);
        cx.run_until_parked();
        assert!(notifications.get() >= 4, "the cursor blinks while running");

        cursor.update(cx, |cursor, cx| cursor.hold_visible(cx));
        cx.run_until_parked();
        let held_at = notifications.get();
        cx.executor().advance_clock(INTERVAL * 20);
        cx.run_until_parked();
        assert_eq!(
            notifications.get(),
            held_at,
            "a held cursor schedules no blinks"
        );
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.executor().advance_clock(INTERVAL * 4);
        cx.run_until_parked();
        assert!(notifications.get() > held_at, "start resumes blinking");
    }

    #[gpui::test]
    fn holding_a_paused_cursor_resumes_after_reactivation(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update({
            let notifications = notifications.clone();
            |cx| {
                cx.observe(&cursor, move |_, _| {
                    notifications.set(notifications.get() + 1)
                })
            }
        });

        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cursor.update(cx, |cursor, cx| cursor.hold_visible(cx));
        cx.run_until_parked();
        let held_at = notifications.get();
        cx.executor().advance_clock(INTERVAL * 20);
        cx.run_until_parked();
        assert_eq!(notifications.get(), held_at);
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.executor().advance_clock(INTERVAL * 4);
        cx.run_until_parked();
        assert!(notifications.get() > held_at, "reactivation resumes blinking");
    }
}
