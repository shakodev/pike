//! Narrow actual-main boundary: settings share the established pane and modal
//! owners. Drive the real view; retain process/widget identities throughout.
use super::*;
use iced::{
    Point,
    widget::pane_grid::{Region, Target},
};
use pike::app::{DialogAction, commands::Command};

fn layout(host: &Host, tab: pike::identity::TabId) -> String {
    format!(
        "{:?}",
        host.app
            .model()
            .workspace()
            .tab(tab)
            .unwrap()
            .grid()
            .layout()
    )
}

fn pointer(host: &mut Host, event: mouse::Event, point: Point) {
    host.event(Event::Mouse(event), mouse::Cursor::Available(point));
}

fn header(host: &mut Host, child: &Child) -> io::Result<Point> {
    let bounds = host
        .bounds(child.session)
        .ok_or_else(|| io::Error::other("joint pane missing"))?;
    Ok(Point::new(bounds.x + 40.0, bounds.y - 14.0))
}

fn press(host: &mut Host, point: Point) -> io::Result<()> {
    pointer(host, mouse::Event::CursorMoved { position: point }, point);
    pointer(
        host,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    host.drain()
}

fn release(host: &mut Host, point: Point) -> io::Result<()> {
    pointer(host, mouse::Event::CursorMoved { position: point }, point);
    pointer(
        host,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        point,
    );
    host.drain()
}

fn zoom(host: &mut Host, child: &Child) -> io::Result<()> {
    crate::chrome_actions::click_id(host, Id::from(format!("pane-zoom-{}", child.session.get())))
}

pub(super) fn verify(
    host: &mut Host,
    output: &Path,
    first: &Child,
    second: &Child,
) -> io::Result<()> {
    let tab = first.target.tab;
    check(
        tab == second.target.tab,
        "joint fixture requires split owners",
    )?;
    let before = layout(host, tab);
    let first_input = fs::read(first.directory(output).join("input.bin")).ok();
    let second_input = fs::read(second.directory(output).join("input.bin")).ok();
    // This is a real control emitted by the previous view. Its captured owner
    // must not reload after a later accepted selection.
    let (_, emitted) = host.held_event(
        widget::key(
            Key::Named(Named::F5),
            Modifiers::CTRL | Modifiers::SHIFT,
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    let stale_reload = emitted
        .into_iter()
        .find(|message| {
            matches!(
                message,
                Message::Control(Control::MappedCommand(_, _, Command::ReloadSettings))
            )
        })
        .ok_or_else(|| io::Error::other("joint actual reload shortcut not emitted"))?;
    host.update(Message::Control(Control::Focus(first.target)));
    host.until(|host| host.focused(first.session))?;
    host.update(stale_reload);
    check(
        !host.app.settings_pending(),
        "stale reload owner was accepted",
    )?;

    // Begin a real header gesture, then give ownership to the existing rename
    // dialog. Reload stays blocked and the old release cannot move a pane.
    let start = header(host, first)?;
    press(host, start)?;
    key(
        host,
        Key::Character("r".into()),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    let token = host
        .app
        .tab_dialog_token()
        .ok_or_else(|| io::Error::other("joint rename missing"))?;
    fs::write(
        output.join("settings.toml"),
        configuration(output, true, false)?
            .replace("size = 20", "size = 18")
            .replace("history = 7", "history = 0"),
    )?;
    key(
        host,
        Key::Named(Named::F5),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    check(
        !host.app.settings_pending() && host.app.active_settings().font().size == 20.0,
        "tab dialog leaked reload",
    )?;
    key(host, Key::Named(Named::Escape), Modifiers::empty())?;
    host.until(|host| host.focused(first.session))?;
    let end = host
        .bounds(second.session)
        .ok_or_else(|| io::Error::other("joint second pane missing"))?
        .center();
    release(host, end)?;
    check(
        layout(host, tab) == before,
        "dialog dismissal admitted stale pane release",
    )?;

    // A new modal gets a new token; a queued old edit/dismissal cannot take it.
    key(
        host,
        Key::Character("r".into()),
        Modifiers::CTRL | Modifiers::SHIFT,
    )?;
    let current = host
        .app
        .tab_dialog_token()
        .ok_or_else(|| io::Error::other("joint second rename missing"))?;
    check(current != token, "joint dialog token reused")?;
    host.update(Message::TabName(token, "stale".into()));
    host.update(Message::Control(Control::TabDialog(
        token,
        DialogAction::Cancel,
    )));
    check(
        host.app.tab_dialog_token() == Some(current),
        "stale dialog receipt dismissed current owner",
    )?;
    host.update(Message::TabName(current, "Settings joint".into()));
    click(host, "Save")?;
    host.until(|host| host.focused(first.session))?;
    check(
        host.app.tab_title(tab).unwrap().as_str() == "Settings joint",
        "joint rename lost metadata",
    )?;

    // Fresh drag still follows the pinned grid semantics with stable owners.
    let mut expected = host
        .app
        .model()
        .workspace()
        .tab(tab)
        .unwrap()
        .grid()
        .clone();
    expected.drop(
        first.target.pane,
        Target::Pane(second.target.pane, Region::Center),
    );
    let start = header(host, first)?;
    press(host, start)?;
    release(host, end)?;
    check(
        layout(host, tab) == format!("{:?}", expected.layout()),
        "joint fresh header move failed",
    )?;
    host.until(|host| host.focused(first.session))?;

    // Reload a zero history cap and new font while only one pane is presented;
    // the hidden owner must get the same live settings and restore unchanged.
    zoom(host, first)?;
    check(
        host.bounds(second.session).is_none(),
        "joint zoom retained hidden pane",
    )?;
    reload(host)?;
    check(
        host.app.active_settings().font().size == 18.0
            && host.app.active_settings().history().lines() == 0,
        "joint zoom reload not committed",
    )?;
    for child in [first, second] {
        check(
            child.intact(host)
                && host
                    .app
                    .terminal(child.session)
                    .unwrap()
                    .snapshot_stats()
                    .history_limit
                    .lines()
                    == 0,
            "joint reload lost live or hidden owner/history",
        )?;
    }
    check(
        host.app
            .terminal(second.session)
            .unwrap()
            .snapshot_stats()
            .viewport_cells
            == 0,
        "joint reload allocated hidden snapshot",
    )?;
    zoom(host, first)?;
    host.redraw();
    host.drain()?;
    for child in [first, second] {
        let content = host
            .app
            .terminal(child.session)
            .unwrap()
            .renderable_content()
            .ok_or_else(|| io::Error::other("joint restored content missing"))?;
        check(
            content.terminal_size.cell_height == 18.0 * 1.3 && host.bounds(child.session).is_some(),
            "joint restored font/geometry not updated",
        )?;
    }
    check(
        fs::read(first.directory(output).join("input.bin")).ok() == first_input
            && fs::read(second.directory(output).join("input.bin")).ok() == second_input,
        "joint pane/dialog/reload emitted terminal input",
    )?;
    fs::write(
        output.join("settings.toml"),
        configuration(output, true, false)?,
    )?;
    reload(host)?;
    check(
        first.intact(host) && second.intact(host) && host.focused(first.session),
        "joint restore changed owners/focus",
    )?;
    host.evidence.push("joint actual-main: stale reload owner rejected; rename blocked reload and cancelled armed drag; stale dialog receipts rejected; fresh move/max/restore; hidden live font/history=0; unchanged identities/PIDs/input".into());
    Ok(())
}
