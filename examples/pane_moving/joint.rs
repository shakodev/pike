//! Only the R27/R30 integration boundary; complete feature evidence is reused.
use super::*;
use pike::app::DialogAction;
use pike::tab_strip::TabAction;

pub(super) fn verify(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
) -> io::Result<()> {
    let first_file = owner_file(host, output, first)?;
    owner_file(host, output, second)?;
    geometry(host, &[first, second])?;
    let tab = host.app.model().target(first).unwrap().tab;
    let owners: Vec<_> = [first, second]
        .into_iter()
        .map(|id| {
            (
                id,
                host.app.model().target(id).unwrap(),
                host.app.model().sessions().get(id).unwrap().reservation(),
                host.app.terminal(id).unwrap().widget_id().clone(),
            )
        })
        .collect();
    for mouse_mode in [false, true] {
        if mouse_mode {
            input(host, 'm')?;
            host.until(|h| {
                h.app
                    .terminal(first)
                    .unwrap()
                    .renderable_content()
                    .unwrap()
                    .terminal_mode
                    .contains(
                        iced_term::TermMode::MOUSE_REPORT_CLICK | iced_term::TermMode::SGR_MOUSE,
                    )
            })?;
        }
        for rename in [true, false] {
            for blur in [false, true] {
                let before = layout(host);
                let admission = admitted(host);
                let point = host.bounds(second).unwrap().center();
                press_header(host, first)?;
                host.update(Message::Control(Control::TabAction(if rename {
                    TabAction::BeginRename(tab)
                } else {
                    TabAction::OpenOverflow
                })));
                host.drain()?;
                let token = host
                    .app
                    .tab_dialog_token()
                    .ok_or_else(|| io::Error::other("joint dialog did not open"))?;
                check(
                    host.app.interaction_blocked()
                        && host.bounds(first).is_none()
                        && host.bounds(second).is_none(),
                    "tab dialog did not suspend pane geometry/interaction",
                )?;
                for id in [first, second] {
                    check(
                        host.app
                            .terminal(id)
                            .unwrap()
                            .snapshot_stats()
                            .viewport_cells
                            == 0,
                        "dialog kept a hidden terminal snapshot",
                    )?;
                }
                // Delivery while the dialog owns the actual root cannot drop
                // the pane, even though its press began in the previous tree.
                pointer(host, mouse::Event::CursorMoved { position: point }, point);
                host.event(
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    mouse::Cursor::Unavailable,
                );
                let current = if blur {
                    host.event(
                        Event::Window(iced::window::Event::Unfocused),
                        mouse::Cursor::Unavailable,
                    );
                    // Let the actual subscription consume raw blur once
                    // before issuing focus. Duplicating a queued App blur
                    // could otherwise retire a second token after dismissal.
                    host.until(|h| h.app.tab_dialog_token() != Some(token))?;
                    host.update(Message::WindowFocused(true));
                    host.event(
                        Event::Window(iced::window::Event::Focused),
                        mouse::Cursor::Unavailable,
                    );
                    host.drain()?;
                    let next = host.app.tab_dialog_token().unwrap();
                    check(next != token, "App blur failed to retire the dialog token")?;
                    host.update(Message::TabName(token, "stale".into()));
                    host.update(Message::Control(Control::TabDialog(
                        token,
                        DialogAction::Cancel,
                    )));
                    check(
                        host.app.tab_dialog_token() == Some(next),
                        "stale dialog receipt dismissed the current owner",
                    )?;
                    next
                } else {
                    token
                };
                if rename && !blur {
                    host.update(Message::TabName(current, "R27 joint".into()));
                    host.update(Message::Control(Control::TabDialog(
                        current,
                        DialogAction::Save,
                    )));
                    check(
                        host.app.tab_title(tab).unwrap().as_str() == "R27 joint",
                        "rename metadata was lost",
                    )?;
                } else {
                    host.update(Message::Control(Control::TabDialog(
                        current,
                        if rename {
                            DialogAction::Cancel
                        } else {
                            DialogAction::Choose(tab)
                        },
                    )));
                }
                host.drain()?;
                check(
                    !host.app.interaction_blocked() && host.app.active_session() == Some(first),
                    "dialog dismissal failed to restore the pane owner",
                )?;
                host.until(|h| {
                    h.focused(first)
                        && h.app
                            .terminal(first)
                            .unwrap()
                            .renderable_content()
                            .is_some_and(|c| c.focused)
                })?;
                host.update(Message::Control(Control::TabDialog(
                    current,
                    DialogAction::Close(tab),
                )));
                finish_drag(host, point)?;
                check(
                    layout(host) == before && admitted(host) == admission,
                    "dialog/stale release changed layout or admitted terminal input",
                )?;
                // Fresh content must remain live after the armed pointer was
                // removed with the modal tree. Mouse mode uses a real child.
                let bounds = host.bounds(first).unwrap();
                let cell = host
                    .app
                    .terminal(first)
                    .unwrap()
                    .renderable_content()
                    .unwrap()
                    .terminal_size;
                let from = Point::new(
                    bounds.x + 3.5 * cell.cell_width,
                    bounds.y + 2.5 * cell.cell_height,
                );
                let to = Point::new(
                    bounds.x + 5.5 * cell.cell_width,
                    bounds.y + 2.5 * cell.cell_height,
                );
                let bytes = fs::read(&first_file)?;
                drag(host, from, to)?;
                if mouse_mode {
                    let mut expected = bytes;
                    expected.extend_from_slice(b"\x1b[<0;4;3M\x1b[<0;6;3m");
                    host.until(|_| fs::read(&first_file).is_ok_and(|value| value == expected))?;
                } else {
                    check(
                        host.app
                            .terminal(first)
                            .unwrap()
                            .copy_selection(|s| s)
                            .is_some(),
                        "dialog left fresh content selection blocked",
                    )?;
                    check(
                        admitted(host) == admission,
                        "fresh selection admitted unexpected bytes",
                    )?;
                }
                host.evidence.push(format!("joint rename={rename} blur={blur} mouse={mouse_mode}: armed drag cancelled, stale tokens rejected, focus/content restored"));
            }
        }
        // A new header press is required and must still produce one layout
        // effect after every modal/blur cycle, using the same stable owners.
        let target = host.app.model().target(second).unwrap();
        let point = host.bounds(second).unwrap().center();
        move_to(
            host,
            first,
            point,
            Target::Pane(target.pane, Region::Center),
            output,
        )?;
        geometry(host, &[first, second])?;
        zoom_button(host, first)?;
        geometry(host, &[first])?;
        check(
            host.bounds(second).is_none(),
            "joint zoom retained hidden pane geometry",
        )?;
        zoom_button(host, first)?;
        geometry(host, &[first, second])?;
        check(host.focused(first), "joint restore lost pane focus")?;
    }
    for (id, target, reservation, widget) in owners {
        check(
            host.app.model().target(id) == Some(target)
                && host.app.model().sessions().get(id).unwrap().reservation() == reservation
                && host.app.terminal(id).unwrap().widget_id() == &widget,
            "tab dialog integration replaced a pane/session/widget owner",
        )?;
    }
    check(
        host.app.tab_title(tab).unwrap().as_str() == "R27 joint",
        "pane moves/zoom lost tab metadata",
    )?;
    host.evidence.push(
        "actual R30 tab metadata, exact R27 owners, fresh drop and zoom/restore geometry preserved"
            .into(),
    );
    Ok(())
}
