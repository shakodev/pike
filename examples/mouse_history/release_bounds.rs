//! Raw-peer assertions for representable releases and unsupported coordinates.
use super::*;

pub(super) fn run(
    ex: &SharedExecutor,
    r: &iced::Renderer,
    term: &mut Terminal,
    serial: &mut usize,
    log: &mut fs::File,
) -> io::Result<()> {
    let b = resize(term, 300, 230);
    let mut scene = Scene::new();
    scene.focus(term, r, b, true);
    scene.queue(
        term,
        r,
        b,
        Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        mouse::Cursor::Unavailable,
    );
    scene.apply(term)?;
    let inside = point(term, b, 3.2, 2.2);
    let wide = point(term, b, 299.2, 229.2);
    let outside = point(term, b, 350.0, 250.0);
    for leave in [false, true] {
        control(ex, term, serial, "mode legacy")?;
        let tag = expect(ex, term, serial, b"\x1b[M $#\x1b[M#\xff\xff")?;
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            inside,
        );
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::CursorMoved { position: outside },
            outside,
        );
        if leave {
            scene.queue(
                term,
                r,
                b,
                Event::Mouse(mouse::Event::CursorLeft),
                mouse::Cursor::Unavailable,
            );
        } else {
            scene.mouse(
                term,
                r,
                b,
                mouse::Event::ButtonReleased(mouse::Button::Left),
                outside,
            );
        }
        scene.apply(term)?;
        result(ex, term, tag)?;
        let before = accepted_bytes(term);
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            outside,
        );
        scene.apply(term)?;
        term.handle(Command::ProxyToBackend(B::FocusChanged(false)));
        check(
            accepted_bytes(term) == before,
            "late duplicate legacy release",
        )?;
        let tag = control(ex, term, serial, "check")?;
        result(ex, term, tag)?;
        writeln!(
            log,
            "legacy-wide-{},12,passed",
            if leave { "leave" } else { "release" }
        )?;
    }
    // The UI may capture an unsupported press, but the backend must not report
    // it or manufacture a release; neither wheel nor motion may wrap or clamp.
    control(ex, term, serial, "mode legacy")?;
    let before = accepted_bytes(term);
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        wide,
    );
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::CursorMoved { position: outside },
        outside,
    );
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        },
        wide,
    );
    scene.mouse(
        term,
        r,
        b,
        mouse::Event::ButtonReleased(mouse::Button::Left),
        inside,
    );
    scene.apply(term)?;
    term.handle(Command::ProxyToBackend(B::FocusChanged(false)));
    check(
        accepted_bytes(term) == before,
        "unsupported legacy report or orphan release",
    )?;
    let tag = control(ex, term, serial, "check")?;
    result(ex, term, tag)?;
    writeln!(log, "legacy-unsupported-press-wheel,0,passed")?;

    // Test both UI release and backend-only hide/blur cleanup after the peer
    // changes the encoding while retaining mouse tracking and an accepted press.
    for blur in [false, true] {
        control(ex, term, serial, "mode sgr")?;
        let tag = expect(ex, term, serial, b"\x1b[<0;300;230M")?;
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            wide,
        );
        scene.apply(term)?;
        result(ex, term, tag)?;
        control(ex, term, serial, "mode legacy")?;
        let tag = expect(ex, term, serial, b"\x1b[M#\xff\xff")?;
        if blur {
            term.handle(Command::ProxyToBackend(B::FocusChanged(false)));
        } else {
            scene.mouse(
                term,
                r,
                b,
                mouse::Event::ButtonReleased(mouse::Button::Left),
                outside,
            );
            scene.apply(term)?;
        }
        result(ex, term, tag)?;
        let before = accepted_bytes(term);
        scene.mouse(
            term,
            r,
            b,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            outside,
        );
        scene.apply(term)?;
        term.handle(Command::ProxyToBackend(B::FocusChanged(false)));
        check(
            accepted_bytes(term) == before,
            "duplicate encoding-transition cleanup",
        )?;
        let tag = control(ex, term, serial, "check")?;
        result(ex, term, tag)?;
        writeln!(
            log,
            "sgr-to-legacy-{},19,passed",
            if blur { "blur" } else { "release" }
        )?;
    }
    Ok(())
}
