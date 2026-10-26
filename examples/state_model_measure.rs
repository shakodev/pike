//! Synthetic pure-state measurements. Never executes the returned effects.
//! Run: cargo run --locked --release --example state_model_measure
use std::hint::black_box;
use std::num::NonZeroU16;
use std::time::Instant;

use iced::widget::pane_grid::Axis;
use pike::layout::PaneState;
use pike::session::{Effect, SessionEntry};
use pike::workspace::Model;

const SAMPLES: usize = 20_000;

fn measure(name: &str, count: u16, mut operation: impl FnMut()) {
    let mut times = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        operation();
        times.push(start.elapsed().as_nanos());
    }
    times.sort_unstable();
    println!(
        "{count},{name},{SAMPLES},{},{},{}",
        times[SAMPLES / 2],
        times[SAMPLES * 99 / 100],
        times[SAMPLES - 1]
    );
}

fn starting(model: &mut Model, effect: Effect) {
    let Effect::Spawn(ids) = effect else {
        panic!("spawn expected");
    };
    let _token = model.claim_launch(ids).unwrap();
    black_box(model.spawn_succeeded(ids));
}

fn main() {
    println!(
        "inline_bytes,PaneState={},SessionEntry={},Model={}",
        size_of::<PaneState>(),
        size_of::<SessionEntry>(),
        size_of::<Model>()
    );
    println!("sessions,operation,samples,p50_ns,p99_ns,max_ns");
    for count in [16, 128, 1024] {
        let mut model = Model::new(NonZeroU16::new(count + 1).unwrap());
        let (a, effect) = model.create_tab().unwrap();
        starting(&mut model, effect);
        let (b, effect) = model.create_tab().unwrap();
        starting(&mut model, effect);
        let mut handles = vec![a, b];
        for i in 2..count as usize {
            let destination = handles[(i - 2) / 2];
            let (pane, effect) = model.split(destination, Axis::Vertical).unwrap();
            starting(&mut model, effect);
            handles.push(pane);
        }
        let source = handles[2];
        let mut moved = source;
        measure("move_cross_tab", count, || {
            let destination = if moved.tab == a.tab { b } else { a };
            moved = model
                .move_to_tab(moved, destination, Axis::Horizontal)
                .unwrap();
            black_box(moved);
        });
        let split = *model
            .workspace()
            .tab(a.tab)
            .unwrap()
            .grid()
            .layout()
            .splits()
            .next()
            .unwrap();
        assert_eq!(moved.tab, a.tab);
        measure("zoom_focus_other_resize", count, || {
            model.focus(a).unwrap();
            model.zoom(a).unwrap();
            model.focus(moved).unwrap();
            model.resize(a.tab, split, 0.6).unwrap();
            black_box(model.workspace().active());
        });
        measure("split_claim_close_ack", count, || {
            let (target, effect) = model.split(a, Axis::Vertical).unwrap();
            let Effect::Spawn(ids) = effect else {
                panic!("spawn expected");
            };
            let _token = model.claim_launch(ids).unwrap();
            assert_eq!(model.close_pane(target).unwrap(), Some(Effect::Close(ids)));
            assert!(model.closed(ids.session));
        });
        black_box(model);
    }
}
