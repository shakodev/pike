//! Public two-session regression using Iced's actual Row and shared Shell.
use super::*;

const SIZE: Size = Size::new(800.0, 200.0);
fn view(terms: &[Terminal]) -> iced::Element<'_, iced_term::Event> {
    iced::widget::row![TerminalView::show(&terms[0]), TerminalView::show(&terms[1])].into()
}
#[derive(Default)]
struct RowScene {
    tree: Option<Tree>,
}
impl RowScene {
    fn queue(
        &mut self,
        terms: &[Terminal],
        r: &iced::Renderer,
        event: Event,
        cursor: mouse::Cursor,
        consumed: bool,
    ) -> Vec<iced_term::Event> {
        let mut view = view(terms);
        let tree = self.tree.get_or_insert_with(Tree::empty);
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, r, &layout::Limits::new(SIZE, SIZE));
        let mut pending = Vec::new();
        let mut shell = Shell::new(&mut pending);
        if consumed {
            shell.capture_event();
        }
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            cursor,
            r,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(SIZE),
        );
        pending
    }
    fn focus(&mut self, terms: &[Terminal], r: &iced::Renderer, index: usize) {
        let mut view = view(terms);
        let tree = self.tree.get_or_insert_with(Tree::empty);
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, r, &layout::Limits::new(SIZE, SIZE));
        view.as_widget_mut().operate(
            tree,
            Layout::new(&node),
            r,
            &mut operation::focusable::focus::<()>(terms[index].widget_id().clone()),
        );
    }
    fn warm(&mut self, terms: &mut [Terminal], r: &iced::Renderer) -> io::Result<()> {
        let pending = self.queue(
            terms,
            r,
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            mouse::Cursor::Unavailable,
            false,
        );
        apply(terms, pending)
    }
}
fn apply(terms: &mut [Terminal], pending: Vec<iced_term::Event>) -> io::Result<()> {
    for iced_term::Event::BackendCall(id, command) in pending {
        let term = terms
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| io::Error::other("Row routed to unknown session"))?;
        check(
            !matches!(
                term.handle(Command::ProxyToBackend(command)),
                iced_term::actions::Action::InputRejected(_)
            ),
            "Row input rejected",
        )?;
    }
    Ok(())
}
fn key() -> Event {
    use iced::keyboard::{self, Key, Modifiers};
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("k".into()),
        modified_key: Key::Character("k".into()),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        text: Some("k".into()),
        repeat: false,
    })
}
fn targets(pending: &[iced_term::Event]) -> Vec<u64> {
    pending
        .iter()
        .filter_map(|iced_term::Event::BackendCall(id, command)| {
            matches!(command, B::Write(bytes) if bytes == b"k").then_some(*id)
        })
        .collect()
}
fn cursor(term: &Terminal, index: usize) -> mouse::Cursor {
    let cell = term.renderable_content().unwrap().terminal_size;
    mouse::Cursor::Available(Point::new(
        index as f32 * SIZE.width / 2.0 + cell.cell_width * 3.2,
        cell.cell_height * 2.2,
    ))
}
pub(super) fn run(ex: &SharedExecutor, r: &iced::Renderer, out: &Path) -> io::Result<()> {
    // Keep every successfully created session in this vector so even a later
    // spawn or assertion failure passes through the same acknowledged cleanup.
    let mut terms = Vec::new();
    let mut pids = Vec::new();
    let checked = (|| {
        for id in [171001, 171002] {
            terms.push(raw_terminal(id)?);
            pids.push(wait(ex, terms.last_mut().unwrap(), "pike:mouse:boot:")?);
        }
        let owner = iced_term::WindowFocus::default();
        owner.set_focused(true);
        for term in &mut terms {
            term.set_window_focus(owner.clone());
        }
        exercise(ex, r, out, &mut terms)
    })();
    let mut cleanup = fs::File::create(out.join("row-cleanup.csv"))?;
    writeln!(cleanup, "session,completion,boot_title")?;
    let mut clean = true;
    for (index, term) in terms.drain(..).enumerate() {
        let id = term.id;
        let handle = term.shutdown_handle();
        drop(term);
        let done = ex
            .block_on(async { tokio::time::timeout(Duration::from_secs(10), handle.wait()).await });
        clean &= done
            .as_ref()
            .is_ok_and(|done| done.reason == iced_term::ExitReason::Requested);
        writeln!(
            cleanup,
            "{id},{done:?},{}",
            pids.get(index).map(String::as_str).unwrap_or("unknown")
        )?;
    }
    checked?;
    check(clean, "Row fixture cleanup not acknowledged")
}
fn exercise(
    ex: &SharedExecutor,
    r: &iced::Renderer,
    out: &Path,
    terms: &mut [Terminal],
) -> io::Result<()> {
    let mut scene = RowScene::default();
    scene.warm(terms, r)?;
    let mut serials = [0, 0];
    let mut log = fs::File::create(out.join("row-checks.csv"))?;
    writeln!(log, "case,clicked_session,key_targets,result")?;
    for mode in ["history", "sgr"] {
        for index in 0..2 {
            control(
                ex,
                &mut terms[index],
                &mut serials[index],
                &format!("mode {mode}"),
            )?;
        }
        for index in 0..2 {
            let other = 1 - index;
            scene.focus(terms, r, other);
            scene.warm(terms, r)?;
            let expected = if mode == "sgr" {
                &b"\x1b[<0;4;3Mk\x1b[<0;4;3m"[..]
            } else {
                &b"k"[..]
            };
            let tag = expect(ex, &mut terms[index], &mut serials[index], expected)?;
            let cursor = cursor(&terms[index], index);
            let mut pending = scene.queue(
                terms,
                r,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                cursor,
                false,
            );
            // Preserve the production deferred FIFO: key follows the click
            // before either pane's backend sees the queued focus changes.
            let keys = scene.queue(terms, r, key(), mouse::Cursor::Unavailable, false);
            let key_targets = targets(&keys);
            writeln!(
                log,
                "{mode}-click,{},{key_targets:?},{}",
                terms[index].id,
                if key_targets == [terms[index].id] {
                    "passed"
                } else {
                    "FAILED"
                }
            )?;
            check(
                key_targets == [terms[index].id],
                "Row duplicated or misrouted key after click",
            )?;
            pending.extend(keys);
            pending.extend(scene.queue(
                terms,
                r,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                cursor,
                false,
            ));
            apply(terms, pending)?;
            check(
                terms[index].renderable_content().unwrap().focused
                    && !terms[other].renderable_content().unwrap().focused,
                "Row retained two focused sessions",
            )?;
            result(ex, &mut terms[index], tag)?;
            let tag = control(ex, &mut terms[other], &mut serials[other], "check")?;
            result(ex, &mut terms[other], tag)?;

            // A previously consumed inside click cannot acquire focus or start
            // selection/reporting; its outside sibling must still unfocus.
            scene.focus(terms, r, other);
            scene.warm(terms, r)?;
            let pending = scene.queue(
                terms,
                r,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                cursor,
                true,
            );
            check(
                pending
                    .iter()
                    .all(|iced_term::Event::BackendCall(_, c)| matches!(c, B::FocusChanged(false))),
                "consumed click acquired focus, selection or report",
            )?;
            apply(terms, pending)?;
            let keys = scene.queue(terms, r, key(), mouse::Cursor::Unavailable, false);
            check(
                targets(&keys).is_empty(),
                "consumed click acquired keyboard target",
            )?;
            writeln!(log, "{mode}-consumed-click,{},[],passed", terms[index].id)?;
        }
    }
    // Consumed outside clicks still cancel the old pane's owned gesture. Its
    // focus cleanup and queued UI release must produce only one peer release.
    for owner in 0..2 {
        scene.focus(terms, r, owner);
        scene.warm(terms, r)?;
        let tag = expect(
            ex,
            &mut terms[owner],
            &mut serials[owner],
            b"\x1b[<0;4;3M\x1b[<0;4;3m",
        )?;
        let pending = scene.queue(
            terms,
            r,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            cursor(&terms[owner], owner),
            false,
        );
        apply(terms, pending)?;
        let other = 1 - owner;
        let pending = scene.queue(
            terms,
            r,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            cursor(&terms[other], other),
            true,
        );
        check(
            pending.iter().all(|iced_term::Event::BackendCall(_, c)| {
                matches!(c, B::FocusChanged(false) | B::MouseReport(_, _, _, false))
            }),
            "consumed outside click started a new gesture",
        )?;
        apply(terms, pending)?;
        result(ex, &mut terms[owner], tag)?;
        let pending = scene.queue(
            terms,
            r,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            cursor(&terms[other], other),
            false,
        );
        apply(terms, pending)?;
        check(
            targets(&scene.queue(terms, r, key(), mouse::Cursor::Unavailable, false)).is_empty(),
            "consumed outside cleanup retained keyboard focus",
        )?;
        writeln!(
            log,
            "consumed-outside-cleanup,{},[],passed",
            terms[owner].id
        )?;
    }
    // Hovered wheel crosses the real shared Shell in both child directions.
    for index in 0..2 {
        let other = 1 - index;
        scene.focus(terms, r, other);
        scene.warm(terms, r)?;
        let tag = expect(ex, &mut terms[index], &mut serials[index], b"\x1b[<64;4;3M")?;
        let pending = scene.queue(
            terms,
            r,
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
            cursor(&terms[index], index),
            false,
        );
        apply(terms, pending)?;
        result(ex, &mut terms[index], tag)?;
        let tag = expect(ex, &mut terms[other], &mut serials[other], b"k")?;
        let keys = scene.queue(terms, r, key(), mouse::Cursor::Unavailable, false);
        check(
            targets(&keys) == [terms[other].id],
            "hovered wheel changed keyboard owner",
        )?;
        apply(terms, keys)?;
        result(ex, &mut terms[other], tag)?;
        writeln!(
            log,
            "hover-wheel,{},[{}],passed",
            terms[index].id, terms[other].id
        )?;
    }
    for index in 0..2 {
        let tag = control(ex, &mut terms[index], &mut serials[index], "check")?;
        result(ex, &mut terms[index], tag)?;
    }
    Ok(())
}
