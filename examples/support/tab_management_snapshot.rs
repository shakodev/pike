//! Identity/state oracle for controlled children only; no user terminal logging.
use iced::advanced::widget::Id;
use iced_term::{ShutdownHandle, TermMode};
use pike::app::App;
use pike::identity::{SessionId, TabId};
use pike::layout::PaneTarget;
use pike::session::Lifecycle;

pub struct Snapshot {
    pub session: SessionId,
    pub target: PaneTarget,
    pub widget: Id,
    pub pid: u32,
    pub cleanup: ShutdownHandle,
    history: usize,
    mode: TermMode,
    selection: Option<String>,
    admitted: usize,
}

impl Snapshot {
    pub fn capture(app: &App, session: SessionId) -> Option<Self> {
        let terminal = app.terminal(session)?;
        let content = terminal.renderable_content()?;
        let text: String = content.viewport.cells().iter().map(|cell| cell.c).collect();
        let pid = text
            .split("PIKE30_PID=")
            .nth(1)?
            .split(|ch: char| !ch.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        let stats = terminal.snapshot_stats();
        if stats.history_lines == 0
            || stats.columns < 20
            || !content
                .terminal_mode
                .contains(TermMode::APP_CURSOR | TermMode::BRACKETED_PASTE)
        {
            return None;
        }
        let status = terminal.input_status();
        Some(Self {
            session,
            target: app.model().target(session)?,
            widget: terminal.widget_id().clone(),
            pid,
            cleanup: terminal.shutdown_handle(),
            history: stats.history_lines,
            mode: content.terminal_mode,
            selection: terminal.copy_selection(|selection| selection),
            admitted: status.outstanding_bytes + status.written_bytes + status.cancelled_bytes,
        })
    }

    pub fn tab(&self) -> TabId {
        self.target.tab
    }

    pub fn intact(&self, app: &App, input_unchanged: bool) -> bool {
        let Some(terminal) = app.terminal(self.session) else {
            return false;
        };
        let status = terminal.input_status();
        app.model().target(self.session) == Some(self.target)
            && app
                .model()
                .workspace()
                .pane(self.target)
                .is_some_and(|pane| pane.session == self.session)
            && app
                .model()
                .sessions()
                .get(self.session)
                .is_some_and(|entry| entry.lifecycle() == Lifecycle::Running)
            && terminal.widget_id() == &self.widget
            && terminal.snapshot_stats().history_lines >= self.history
            && terminal
                .renderable_content()
                .is_some_and(|content| content.terminal_mode == self.mode)
            && terminal.copy_selection(|selection| selection) == self.selection
            && self.cleanup.completion().is_none()
            && (!input_unchanged
                || status.outstanding_bytes + status.written_bytes + status.cancelled_bytes
                    == self.admitted)
    }

    pub fn cleaned_up(&self, app: &App) -> bool {
        app.model().target(self.session).is_none()
            && app.model().sessions().get(self.session).is_none()
            && self
                .cleanup
                .completion()
                .is_some_and(|completion| completion.output_drained)
    }
}
