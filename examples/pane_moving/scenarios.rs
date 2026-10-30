use super::{Host, Point, check, drag, pointer};
use iced::keyboard::{self, Key, Modifiers, key};
use iced::widget::pane_grid::{Edge, Region, Target};
use iced::{Event, mouse};
use pike::app::{CloseAction, Control, Direction, Message};
use pike::identity::SessionId;
use pike::session::Lifecycle;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[path = "joint.rs"]
mod joint;

pub(super) fn joint_dialogs(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
) -> io::Result<()> {
    joint::verify(host, output, first, second)
}

fn text(host: &Host, id: SessionId) -> String {
    host.app
        .terminal(id)
        .and_then(|t| t.renderable_content())
        .map(|c| c.viewport.cells().iter().map(|cell| cell.c).collect())
        .unwrap_or_default()
}

fn input(host: &mut Host, byte: char) -> io::Result<()> {
    let value = byte.to_string();
    host.event(
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character(value.clone().into()),
            modified_key: Key::Character(value.clone().into()),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: Some(value.into()),
            repeat: false,
        }),
        mouse::Cursor::Unavailable,
    );
    host.drain()
}

pub(super) fn ime_pointer(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
) -> io::Result<()> {
    use iced::advanced::input_method::Event as Ime;
    owner_file(host, output, first)?;
    owner_file(host, output, second)?;
    host.redraw();
    host.redraw();
    host.drain()?;
    host.event(Event::InputMethod(Ime::Opened), mouse::Cursor::Unavailable);
    host.event(
        Event::InputMethod(Ime::Preedit("ni".into(), Some(0..2))),
        mouse::Cursor::Unavailable,
    );
    check(
        host.app.terminal(first).unwrap().is_composing(),
        "IME did not enter preedit",
    )?;
    let bytes = files(output);
    let admission = admitted(host);
    let bounds = host.bounds(first).unwrap();
    drag(
        host,
        Point::new(bounds.x + 3.0, bounds.y + 3.0),
        Point::new(bounds.x + 100.0, bounds.y + 3.0),
    )?;
    check(
        host.app
            .terminal(first)
            .unwrap()
            .copy_selection(|s| s)
            .is_some(),
        "composition blocked ordinary terminal content selection",
    )?;
    let point = header(host, second);
    host.click(point);
    host.drain()?;
    check(
        host.app.active_session() == Some(second) && host.focused(second),
        "composition blocked a fresh pane header focus click",
    )?;
    check(
        !host.app.terminal(first).unwrap().is_composing(),
        "header owner transfer retained old composition",
    )?;
    host.event(
        Event::InputMethod(Ime::Commit("stale".into())),
        mouse::Cursor::Unavailable,
    );
    host.drain()?;
    check(
        files(output) == bytes,
        "pane pointer/IME transfer wrote preedit or stale commit bytes",
    )?;
    check(
        admitted(host) == admission,
        "IME pointer transfer admitted asynchronous stale input",
    )?;
    host.evidence.push("native preedit preserved content selection and fresh header owner transfer; stale commit admitted zero bytes".into());
    Ok(())
}

pub(super) fn outside_release(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
) -> io::Result<()> {
    use iced::touch;
    let path = owner_file(host, output, first)?;
    owner_file(host, output, second)?;
    host.redraw();
    host.drain()?;
    let bounds = host.bounds(first).unwrap();
    let content = host
        .app
        .terminal(first)
        .unwrap()
        .renderable_content()
        .unwrap();
    let cell = content.terminal_size;
    let selected_row = content
        .viewport
        .cells()
        .chunks(content.viewport.columns())
        .position(|row| row.iter().filter(|cell| !cell.c.is_whitespace()).count() >= 15)
        .ok_or_else(|| io::Error::other("no controlled text row for content recovery"))?;
    let row = bounds.y + (selected_row as f32 + 0.5) * cell.cell_height;
    for cancellation in 0..5 {
        drag(
            host,
            Point::new(bounds.x + 2.1 * cell.cell_width, row),
            Point::new(bounds.x + 12.9 * cell.cell_width, row),
        )?;
        let baseline = host
            .app
            .terminal(first)
            .unwrap()
            .copy_selection(|s| s)
            .ok_or_else(|| io::Error::other("missing baseline selection"))?;
        let before = layout(host);
        let input = admitted(host);
        press_header(host, first)?;
        if matches!(cancellation, 0 | 4) {
            host.event(
                Event::Mouse(mouse::Event::CursorLeft),
                mouse::Cursor::Unavailable,
            );
        }
        match cancellation {
            0 | 1 => {
                host.event(
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    mouse::Cursor::Unavailable,
                );
            }
            2 => {
                let position = host.bounds(second).unwrap().center();
                host.event(
                    Event::Touch(touch::Event::FingerLost {
                        id: touch::Finger(1),
                        position,
                    }),
                    mouse::Cursor::Available(position),
                );
            }
            3 => {
                host.event(
                    Event::Touch(touch::Event::FingerLifted {
                        id: touch::Finger(1),
                        position: Point::ORIGIN,
                    }),
                    mouse::Cursor::Unavailable,
                );
            }
            4 => {} // CursorLeft must cancel before any later release arrives.
            _ => unreachable!(),
        }
        host.drain()?;
        check(
            layout(host) == before && admitted(host) == input,
            "outside/lost release changed layout or admitted header input",
        )?;
        host.event(
            Event::Mouse(mouse::Event::CursorEntered),
            mouse::Cursor::Available(bounds.center()),
        );
        drag(
            host,
            Point::new(bounds.x + 4.1 * cell.cell_width, row),
            Point::new(bounds.x + 6.9 * cell.cell_width, row),
        )?;
        let selected = host
            .app
            .terminal(first)
            .unwrap()
            .copy_selection(|s| s)
            .ok_or_else(|| io::Error::other("missing fresh content selection"))?;
        check(
            selected != baseline,
            "unavailable outside release left content pointer input blocked",
        )?;
        host.evidence.push(format!("cancel kind={cancellation}: unchanged layout, zero header admission and fresh content selection"));
    }
    input(host, 'm')?;
    host.until(|h| {
        h.app
            .terminal(first)
            .unwrap()
            .renderable_content()
            .unwrap()
            .terminal_mode
            .contains(iced_term::TermMode::MOUSE_REPORT_CLICK | iced_term::TermMode::SGR_MOUSE)
    })?;
    let before = fs::read(&path)?;
    let admission = admitted(host);
    press_header(host, first)?;
    host.event(
        Event::Mouse(mouse::Event::CursorLeft),
        mouse::Cursor::Unavailable,
    );
    host.event(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Unavailable,
    );
    host.drain()?;
    check(
        admitted(host) == admission,
        "mouse-mode unavailable header release admitted input",
    )?;
    drag(
        host,
        Point::new(bounds.x + 2.5 * cell.cell_width, row),
        Point::new(bounds.x + 4.5 * cell.cell_width, row),
    )?;
    let mut expected = before;
    expected.extend_from_slice(
        format!(
            "\x1b[<0;3;{}M\x1b[<0;5;{}m",
            selected_row + 1,
            selected_row + 1
        )
        .as_bytes(),
    );
    host.until(|_| fs::read(&path).is_ok_and(|bytes| bytes == expected))?;
    host.evidence.push("unavailable outside release in mouse mode: zero header admission; fresh content drag emitted exact SGR bytes".into());
    let target = host.app.model().target(second).unwrap();
    let point = host.bounds(second).unwrap().center();
    move_to(
        host,
        first,
        point,
        Target::Pane(target.pane, Region::Center),
        output,
    )?;
    host.evidence.push("fresh header press after cancellation emitted exactly one layout effect and zero PTY admission".into());
    Ok(())
}

fn owner_file(host: &mut Host, output: &Path, id: SessionId) -> io::Result<PathBuf> {
    host.until(|h| text(h, id).contains("PIKE27_READY="))?;
    let screen = text(host, id);
    let rest = screen
        .split("PIKE27_READY=")
        .nth(1)
        .ok_or_else(|| io::Error::other("missing child PID"))?;
    let pid: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let path = output.join(format!("{pid}.bin"));
    host.until(|_| path.exists())?;
    Ok(path)
}

fn files(output: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut values: Vec<_> = fs::read_dir(output)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bin"))
        .map(|p| {
            let data = fs::read(&p).unwrap();
            (p, data)
        })
        .collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    values
}

fn admitted(host: &Host) -> Vec<(SessionId, usize)> {
    host.app
        .model()
        .sessions()
        .iter()
        .filter_map(|(id, _)| {
            let input = host.app.terminal(id)?.input_status();
            Some((
                id,
                input.written_bytes + input.outstanding_bytes + input.cancelled_bytes,
            ))
        })
        .collect()
}

fn header(host: &mut Host, id: SessionId) -> Point {
    let bounds = host.bounds(id).unwrap();
    Point::new(bounds.x + 40.0, bounds.y - 14.0) // the visible title itself
}

fn press_header(host: &mut Host, id: SessionId) -> io::Result<()> {
    let point = header(host, id);
    pointer(host, mouse::Event::CursorMoved { position: point }, point);
    pointer(
        host,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    host.drain()
}

fn finish_drag(host: &mut Host, point: Point) -> io::Result<()> {
    pointer(host, mouse::Event::CursorMoved { position: point }, point);
    pointer(
        host,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        point,
    );
    host.drain()
}

fn layout(host: &Host) -> String {
    let tab = host.app.model().workspace().active().unwrap();
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

fn move_to(
    host: &mut Host,
    source: SessionId,
    point: Point,
    target: Target,
    output: &Path,
) -> io::Result<()> {
    let source_target = host.app.model().target(source).unwrap();
    let mut expected = host
        .app
        .model()
        .workspace()
        .tab(source_target.tab)
        .unwrap()
        .grid()
        .clone();
    expected.drop(source_target.pane, target);
    let before = files(output);
    let admission = admitted(host);
    let actions = host.controls;
    let from = header(host, source);
    drag(host, from, point)?;
    check(
        layout(host) == format!("{:?}", expected.layout()),
        "header drop disagrees with pinned target semantics",
    )?;
    check(
        host.controls == actions + 2,
        "header drag did not emit exactly one focus and one layout action",
    )?;
    check(files(output) == before, "header drag emitted PTY input")?;
    check(
        admitted(host) == admission,
        "header drag admitted asynchronous PTY input",
    )?;
    check(
        host.app.active_session() == Some(source) && host.focused(source),
        "header drop lost exclusive source focus",
    )
}

fn zoom_button(host: &mut Host, id: SessionId) -> io::Result<()> {
    let bounds = host.bounds(id).unwrap();
    // Pinned TitleBar right-aligned controls: Close is 42 px; Max/Restore
    // immediately precedes it. The point lies in either zoom button.
    let point = Point::new(bounds.x + bounds.width - 65.0, bounds.y - 14.0);
    host.click(point);
    host.drain()
}

fn geometry(host: &mut Host, ids: &[SessionId]) -> io::Result<()> {
    host.redraw();
    host.drain()?;
    for id in ids {
        let bounds = host
            .bounds(*id)
            .ok_or_else(|| io::Error::other("missing restored terminal geometry"))?;
        let content = host
            .app
            .terminal(*id)
            .unwrap()
            .renderable_content()
            .unwrap();
        let cols = (bounds.width / content.terminal_size.cell_width).floor() as usize;
        let rows = (bounds.height / content.terminal_size.cell_height).floor() as usize;
        check(
            content.viewport.columns() == cols && content.viewport.lines() == rows,
            "restored cell dimensions do not match current widget bounds",
        )?;
        check(
            bounds.x >= 8.0
                && bounds.y >= 0.0
                && bounds.x + bounds.width <= crate::host_viewport::SIZE.width - 7.9
                && bounds.y + bounds.height <= crate::host_viewport::SIZE.height - 7.9,
            "restored pane escaped its viewport clip",
        )?;
        host.evidence.push(format!(
            "geometry session={} bounds={bounds:?} cols={cols} rows={rows}",
            id.get()
        ));
    }
    Ok(())
}

pub(super) fn verify(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
) -> io::Result<()> {
    let first_file = owner_file(host, output, first)?;
    let second_file = owner_file(host, output, second)?;
    let target = host.app.model().target(second).unwrap();
    host.update(Message::Control(Control::Split(target, Direction::Down)));
    host.until(|h| {
        h.app.active_session().is_some_and(|id| {
            id != first && id != second && h.app.terminal(id).is_some() && h.focused(id)
        })
    })?;
    let third = host.app.active_session().unwrap();
    let third_file = owner_file(host, output, third)?;
    host.redraw();
    host.drain()?;
    let ids = [first, second, third];
    let owners: Vec<_> = ids
        .iter()
        .map(|id| {
            (
                *id,
                host.app.model().target(*id).unwrap(),
                host.app.model().sessions().get(*id).unwrap().reservation(),
                host.app.terminal(*id).unwrap().widget_id().clone(),
            )
        })
        .collect();
    let before_layout = layout(host);
    let bounds = host.bounds(first).unwrap();
    let from = Point::new(bounds.x + 3.0, bounds.y + 3.0);
    let to = Point::new(bounds.x + 130.0, bounds.y + 3.0);
    let bytes = files(output);
    let admission = admitted(host);
    drag(host, from, to)?;
    check(
        layout(host) == before_layout,
        "content selection changed the layout",
    )?;
    let selection = host.app.terminal(first).unwrap().copy_selection(|s| s);
    check(
        selection.is_some(),
        "content dragging failed to select real terminal text",
    )?;
    check(
        files(output) == bytes,
        "ordinary content selection wrote PTY bytes",
    )?;
    check(
        admitted(host) == admission,
        "content selection admitted asynchronous PTY input",
    )?;
    let modes = format!(
        "{:?}",
        host.app
            .terminal(first)
            .unwrap()
            .renderable_content()
            .unwrap()
            .terminal_mode
    );
    let histories: Vec<_> = ids
        .iter()
        .map(|id| {
            host.app
                .terminal(*id)
                .unwrap()
                .snapshot_stats()
                .history_lines
        })
        .collect();
    let point = host.bounds(second).unwrap().center();
    move_to(
        host,
        first,
        point,
        Target::Pane(target.pane, Region::Center),
        output,
    )?;
    check(
        host.app.terminal(first).unwrap().copy_selection(|s| s) == selection,
        "swap lost selection",
    )?;
    let bounds = host.bounds(second).unwrap();
    move_to(
        host,
        first,
        Point::new(bounds.center_x(), bounds.y + 5.0),
        Target::Pane(target.pane, Region::Edge(Edge::Top)),
        output,
    )?;
    move_to(
        host,
        first,
        Point::new(9.0, 400.0),
        Target::Edge(Edge::Left),
        output,
    )?;
    geometry(host, &ids)?;
    let moved = layout(host);
    let columns = host.app.terminal(first).unwrap().snapshot_stats().columns;
    zoom_button(host, first)?;
    check(
        host.app
            .model()
            .workspace()
            .tab(target.tab)
            .unwrap()
            .grid()
            .maximized()
            == Some(host.app.model().target(first).unwrap().pane),
        "actual header Max control did not maximize",
    )?;
    geometry(host, &[first])?;
    check(
        host.app.terminal(first).unwrap().snapshot_stats().columns > columns,
        "zoom fixture did not exercise a real column-width change",
    )?;
    check(
        host.app
            .terminal(first)
            .unwrap()
            .copy_selection(|s| s)
            .is_none(),
        "pinned emulator width-resize selection behavior unexpectedly changed",
    )?;
    for hidden in [second, third] {
        check(
            host.bounds(hidden).is_none() && !host.app.terminal(hidden).unwrap().is_visible(),
            "maximized-away widget still has geometry",
        )?;
    }
    zoom_button(host, first)?;
    check(
        layout(host) == moved,
        "restore changed the stored moved layout",
    )?;
    geometry(host, &ids)?;
    check(
        host.app.active_session() == Some(first) && host.focused(first),
        "restore lost remembered focus",
    )?;
    // Upstream clears selection when columns change. Equal-column swap above
    // preserves the exact copy; restored content must support a fresh selection.
    let bounds = host.bounds(first).unwrap();
    drag(
        host,
        Point::new(bounds.x + 3.0, bounds.y + 3.0),
        Point::new(bounds.x + 130.0, bounds.y + 3.0),
    )?;
    let restored_selection = host
        .app
        .terminal(first)
        .unwrap()
        .copy_selection(|s| s)
        .ok_or_else(|| io::Error::other("restored content cannot select text"))?;
    host.update(Message::Control(Control::Copy(first)));
    host.drain()?;
    check(
        host.clipboard.0.as_deref() == Some(restored_selection.as_str()),
        "restored content selection/copy failed",
    )?;
    host.evidence.push("equal-column swap preserved exact selection; real column resize used pinned clearing; fresh restored selection/copy passed".into());
    check(
        format!(
            "{:?}",
            host.app
                .terminal(first)
                .unwrap()
                .renderable_content()
                .unwrap()
                .terminal_mode
        ) == modes,
        "move/zoom lost terminal modes",
    )?;
    for (id, target, reservation, widget) in &owners {
        check(
            host.app.model().target(*id) == Some(*target)
                && host.app.model().sessions().get(*id).unwrap().reservation() == *reservation
                && host.app.terminal(*id).unwrap().widget_id() == widget,
            "move/zoom replaced a session or widget identity",
        )?;
    }
    check(
        ids.iter().enumerate().all(|(i, id)| {
            host.app
                .terminal(*id)
                .unwrap()
                .snapshot_stats()
                .history_lines
                >= histories[i].saturating_sub(50)
        }),
        "move/zoom discarded scrollback",
    )?;
    host.evidence.push(format!(
        "stable owners={owners:?} child_files={:?}",
        [first_file.clone(), second_file.clone(), third_file.clone()]
    ));
    cancellation(host, output, first, second)?;
    mouse_and_clipping(host, output, first, second, &first_file)?;
    running_and_hidden(host, output, &ids, first_file, second_file)?;
    Ok(())
}

fn paint_clip(host: &mut Host, owner: SessionId) -> io::Result<()> {
    use iced::advanced::{
        Layout, layout,
        renderer::{Headless, Renderer as _, Style},
        widget::Tree,
    };
    use iced::{Color, Rectangle, Size, Theme};
    let bounds = host.bounds(owner).unwrap();
    let size = crate::host_viewport::SIZE;
    let mut view = host.app.view();
    let mut tree = Tree::new(view.as_widget());
    let node =
        view.as_widget_mut()
            .layout(&mut tree, &host.renderer, &layout::Limits::new(size, size));
    host.renderer.reset(Rectangle::with_size(size));
    view.as_widget().draw(
        &tree,
        &mut host.renderer,
        &Theme::Dark,
        &Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &Rectangle::with_size(size),
    );
    let rgba = Headless::screenshot(
        &mut host.renderer,
        Size::new(size.width as u32, size.height as u32),
        1.0,
        Color::BLACK,
    );
    let mut green = 0;
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        if pixel[1] > 220 && pixel[0] < 40 && pixel[2] < 40 {
            green += 1;
            let width = size.width as usize;
            let point = Point::new((index % width) as f32, (index / width) as f32);
            check(
                bounds.expand(1.0).contains(point),
                "moved terminal painted colored cells outside its current clip",
            )?;
        }
    }
    check(
        green > 1000,
        "native clipping oracle found no controlled colored terminal cells",
    )?;
    host.evidence.push(format!(
        "native renderer clip session={} green_pixels={green} bounds={bounds:?}",
        owner.get()
    ));
    Ok(())
}

fn mouse_and_clipping(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
    first_file: &Path,
) -> io::Result<()> {
    host.update(Message::Control(Control::Focus(
        host.app.model().target(first).unwrap(),
    )));
    host.drain()?;
    input(host, 'g')?;
    host.until(|h| text(h, first).contains("PIKE27_GREEN"))?;
    paint_clip(host, first)?;
    let target = host.app.model().target(second).unwrap();
    let point = host.bounds(second).unwrap().center();
    move_to(
        host,
        first,
        point,
        Target::Pane(target.pane, Region::Center),
        output,
    )?;
    host.redraw();
    host.drain()?;
    paint_clip(host, first)?;
    input(host, 'm')?;
    host.until(|h| {
        h.app
            .terminal(first)
            .unwrap()
            .renderable_content()
            .unwrap()
            .terminal_mode
            .contains(iced_term::TermMode::MOUSE_REPORT_CLICK | iced_term::TermMode::SGR_MOUSE)
    })?;
    let before = fs::read(first_file)?;
    let bounds = host.bounds(first).unwrap();
    let cell = host
        .app
        .terminal(first)
        .unwrap()
        .renderable_content()
        .unwrap()
        .terminal_size;
    let from = Point::new(
        bounds.x + 2.5 * cell.cell_width,
        bounds.y + 2.5 * cell.cell_height,
    );
    let to = Point::new(
        bounds.x + 4.5 * cell.cell_width,
        bounds.y + 2.5 * cell.cell_height,
    );
    let previous = layout(host);
    drag(host, from, to)?;
    let mut expected = before;
    expected.extend_from_slice(b"\x1b[<0;3;3M\x1b[<0;5;3m");
    host.until(|_| fs::read(first_file).is_ok_and(|bytes| bytes == expected))?;
    check(
        layout(host) == previous,
        "mouse-reporting content drag changed layout",
    )?;
    cancellation(host, output, first, second)?;
    let point = host.bounds(second).unwrap().center();
    move_to(
        host,
        first,
        point,
        Target::Pane(target.pane, Region::Center),
        output,
    )?;
    input(host, 'n')?;
    host.until(|h| {
        !h.app
            .terminal(first)
            .unwrap()
            .renderable_content()
            .unwrap()
            .terminal_mode
            .intersects(iced_term::TermMode::MOUSE_REPORT_CLICK)
    })?;
    host.evidence.push("content mouse drag delivered exact SGR press/release; header drag admitted zero bytes in mouse mode".into());
    Ok(())
}

fn cancellation(
    host: &mut Host,
    output: &Path,
    first: SessionId,
    second: SessionId,
) -> io::Result<()> {
    let before = layout(host);
    let bytes = files(output);
    let admission = admitted(host);
    // Raw native blur is delivered before the queued App focus message.
    press_header(host, first)?;
    host.event(
        Event::Window(iced::window::Event::Unfocused),
        mouse::Cursor::Unavailable,
    );
    host.event(
        Event::Window(iced::window::Event::Focused),
        mouse::Cursor::Unavailable,
    );
    host.update(Message::WindowFocused(true));
    let point = host.bounds(second).unwrap().center();
    finish_drag(host, point)?;
    check(layout(host) == before, "raw blur revived an armed drag")?;
    // App blur and owner round trips cancel even without an intervening event.
    for owner in [false, true] {
        press_header(host, first)?;
        if owner {
            host.update(Message::Control(Control::Focus(
                host.app.model().target(second).unwrap(),
            )));
            host.update(Message::Control(Control::Focus(
                host.app.model().target(first).unwrap(),
            )));
        } else {
            host.update(Message::WindowFocused(false));
            host.update(Message::WindowFocused(true));
        }
        finish_drag(host, point)?;
        check(
            layout(host) == before,
            "App owner/blur round trip revived a drag",
        )?;
    }
    press_header(host, first)?;
    host.update(Message::Control(Control::CloseSession(first)));
    let (token, _) = host
        .app
        .close_confirmation()
        .ok_or_else(|| io::Error::other("missing modal"))?;
    host.update(Message::Control(Control::CloseAction(CloseAction::Cancel(
        token,
    ))));
    finish_drag(host, point)?;
    check(layout(host) == before, "modal round trip revived a drag")?;
    press_header(host, first)?;
    finish_drag(host, Point::new(-50.0, -50.0))?;
    check(layout(host) == before, "outside release changed layout")?;
    press_header(host, first)?;
    for event in [
        keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            modified_key: Key::Named(key::Named::Escape),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: Some("\x1b".into()),
            repeat: false,
        },
        keyboard::Event::KeyReleased {
            key: Key::Named(key::Named::Escape),
            modified_key: Key::Named(key::Named::Escape),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
        },
    ] {
        host.event(Event::Keyboard(event), mouse::Cursor::Unavailable);
    }
    finish_drag(host, point)?;
    check(layout(host) == before, "Escape revived an armed drag")?;
    check(
        files(output) == bytes,
        "cancelled header gestures wrote PTY bytes",
    )?;
    check(
        admitted(host) == admission,
        "cancelled header gesture admitted asynchronous PTY input",
    )?;
    let target = host.app.model().target(second).unwrap();
    move_to(
        host,
        first,
        point,
        Target::Pane(target.pane, Region::Center),
        output,
    )?;
    host.evidence.push(
        "raw/App blur, owner/modal round trips, outside cancel and fresh restart passed".into(),
    );
    Ok(())
}

fn running_and_hidden(
    host: &mut Host,
    output: &Path,
    ids: &[SessionId; 3],
    first_file: PathBuf,
    second_file: PathBuf,
) -> io::Result<()> {
    let [first, second, third] = *ids;
    host.update(Message::Control(Control::Focus(
        host.app.model().target(first).unwrap(),
    )));
    host.drain()?;
    input(host, 't')?;
    let ticks = first_file.with_extension("ticks");
    host.until(|_| {
        fs::read_to_string(&ticks)
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|n| n >= 2)
    })?;
    let tick_before: u32 = fs::read_to_string(&ticks)?
        .parse()
        .map_err(io::Error::other)?;
    let second_target = host.app.model().target(second).unwrap();
    let point = host.bounds(second).unwrap().center();
    move_to(
        host,
        first,
        point,
        Target::Pane(second_target.pane, Region::Center),
        output,
    )?;
    zoom_button(host, first)?;
    zoom_button(host, first)?;
    host.until(|_| {
        fs::read_to_string(&ticks)
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|n| n > tick_before)
    })?;
    host.update(Message::Control(Control::Focus(second_target)));
    host.drain()?;
    input(host, 'f')?;
    host.until(|_| fs::read(&second_file).is_ok_and(|bytes| bytes == b"f"))?;
    zoom_button(host, third)?;
    check(
        !host.app.terminal(second).unwrap().is_visible(),
        "flood source was not hidden",
    )?;
    let hidden = host.app.terminal(second).unwrap().snapshot_stats();
    let work = host.app.terminal(second).unwrap().work_stats();
    check(
        hidden.viewport_cells == 0 && host.bounds(second).is_none(),
        "hidden source retained snapshot or geometry",
    )?;
    fs::write(second_file.with_extension("go"), b"go")?;
    host.until(|h| {
        h.app
            .model()
            .sessions()
            .get(second)
            .is_some_and(|e| e.lifecycle() == Lifecycle::Exited { code: Some(37) })
    })?;
    let after = host.app.terminal(second).unwrap().snapshot_stats();
    check(
        after.snapshots == hidden.snapshots && after.viewport_cells == 0,
        "hidden flood captured render snapshots",
    )?;
    let after_work = host.app.terminal(second).unwrap().work_stats();
    check(
        after_work.grid_resizes == work.grid_resizes
            && after_work.pty_resizes == work.pty_resizes
            && after_work.cache_invalidations == work.cache_invalidations,
        "hidden flood built geometry or resized the PTY",
    )?;
    let query = fs::read(second_file.with_extension("query"))?;
    check(
        query.starts_with(b"\x1b[") && query.ends_with(b"R"),
        "hidden terminal query did not receive a real reply",
    )?;
    let completion = host
        .app
        .terminal(second)
        .unwrap()
        .shutdown_handle()
        .completion()
        .ok_or_else(|| io::Error::other("missing hidden native completion"))?;
    check(
        completion.output_drained,
        "hidden native exit did not drain output",
    )?;
    zoom_button(host, third)?;
    host.redraw();
    host.drain()?;
    check(
        text(host, second).contains("PIKE27_FINAL_SENTINEL="),
        "restored view lost final hidden output",
    )?;
    check(
        host.app.active_session() == Some(third) && host.focused(third),
        "hidden exit stole restored focus",
    )?;
    geometry(host, ids)?;
    let final_snapshots = host
        .app
        .terminal(second)
        .unwrap()
        .snapshot_stats()
        .snapshots;
    host.redraw();
    host.drain()?;
    host.redraw();
    host.drain()?;
    check(
        host.app
            .terminal(second)
            .unwrap()
            .snapshot_stats()
            .snapshots
            == final_snapshots,
        "restore left hidden output/event backlog",
    )?;
    host.evidence.push(format!("counter {tick_before}->{}; hidden snapshots={hidden:?}->{after:?}; query={query:?}; completion={completion:?}", fs::read_to_string(ticks)?));
    // A release from before restart cannot move the replacement pane.
    press_header(host, second)?;
    let before = layout(host);
    host.update(Message::Control(Control::Retry(second)));
    host.until(|h| {
        h.app.model().sessions().get(second).is_none()
            && h.app
                .active_session()
                .is_some_and(|id| id != second && h.app.terminal(id).is_some() && h.focused(id))
    })?;
    let replacement = host.app.active_session().unwrap();
    owner_file(host, output, replacement)?;
    let point = host.bounds(first).unwrap().center();
    finish_drag(host, point)?;
    check(
        layout(host) == before,
        "pre-restart release moved a replacement owner",
    )?;
    host.evidence.push(format!(
        "restart retired source={} replacement={} with unchanged layout",
        second.get(),
        replacement.get()
    ));
    Ok(())
}
