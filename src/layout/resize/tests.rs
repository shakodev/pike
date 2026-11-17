use super::*;
use crate::workspace::{Error, Model};

fn nested() -> (Model, crate::layout::PaneTarget, ResizeGeometry) {
    let mut model = Model::default();
    let (left, _) = model.create_tab().unwrap();
    let (right, _) = model.split(left, Axis::Vertical).unwrap();
    model.split(left, Axis::Horizontal).unwrap();
    model.split(right, Axis::Vertical).unwrap();
    let grid = model.workspace().tab(left.tab).unwrap().grid();
    let minimum = grid
        .iter()
        .enumerate()
        .map(|(n, (pane, _))| {
            (
                *pane,
                Size::new(163.25 + n as f32 * 9.5, 112.25 + n as f32 * 13.5),
            )
        })
        .collect();
    (
        model,
        left,
        ResizeGeometry {
            area: Size::new(1003.5, 607.25),
            minimum,
            spacing: 4.0,
        },
    )
}

fn fits(geometry: &ResizeGeometry, model: &Model, tab: crate::identity::TabId) {
    let canonical = model.workspace().tab(tab).unwrap().grid();
    let (canvas, presented) = geometry.presentation(canonical).unwrap();
    for (pane, bounds) in presented
        .layout()
        .pane_regions(geometry.spacing, 0.0, canvas)
    {
        let minimum = geometry.minimum[&pane];
        assert!(
            bounds.width >= minimum.width && bounds.height >= minimum.height,
            "{bounds:?} < {minimum:?}"
        );
    }
    // Intrinsic canvas is bounded by the sum of the leaves, irrespective of
    // depth, or the provided viewport when it is larger.
    assert!(
        canvas.width
            <= geometry.area.width.ceil().max(
                geometry
                    .minimum
                    .values()
                    .map(|s| s.width.ceil() + geometry.spacing)
                    .sum()
            )
    );
    assert!(
        canvas.height
            <= geometry.area.height.ceil().max(
                geometry
                    .minimum
                    .values()
                    .map(|s| s.height.ceil() + geometry.spacing)
                    .sum()
            )
    );
}

#[test]
fn nested_ratio_extremes_clamp_whole_subtrees_and_are_idempotent() {
    let (mut model, target, geometry) = nested();
    let ids: Vec<_> = model
        .sessions()
        .iter()
        .map(|(id, e)| (id, e.lifecycle()))
        .collect();
    let splits: Vec<_> = model
        .workspace()
        .tab(target.tab)
        .unwrap()
        .grid()
        .layout()
        .splits()
        .copied()
        .collect();
    for split in splits {
        for ratio in [0.0, 1.0, 0.1, 0.9, 0.5] {
            model
                .resize_with_geometry(target.tab, split, ratio, &geometry)
                .unwrap();
            fits(&geometry, &model, target.tab);
            let before = format!(
                "{:?}",
                model.workspace().tab(target.tab).unwrap().grid().layout()
            );
            model
                .resize_with_geometry(target.tab, split, ratio, &geometry)
                .unwrap();
            assert_eq!(
                before,
                format!(
                    "{:?}",
                    model.workspace().tab(target.tab).unwrap().grid().layout()
                )
            );
            assert_eq!(
                ids,
                model
                    .sessions()
                    .iter()
                    .map(|(id, e)| (id, e.lifecycle()))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn tiny_rounding_font_dpi_restore_preserve_canonical_ratios_and_sessions() {
    let (mut model, target, mut geometry) = nested();
    for split in model
        .workspace()
        .tab(target.tab)
        .unwrap()
        .grid()
        .layout()
        .splits()
        .copied()
        .collect::<Vec<_>>()
    {
        model.resize(target.tab, split, 0.17).unwrap();
    }
    let before = format!(
        "{:?}",
        model.workspace().tab(target.tab).unwrap().grid().layout()
    );
    for scale in [1.0, 1.25, 1.5, 2.0] {
        // A fixed physical window divided once by monitor scale. No monitor
        // scaling of the logical font metrics (line-height is independent).
        for area in [
            Size::new(1003.5 / scale, 607.25 / scale),
            Size::new(1.0, 1.0),
            Size::new(1920.0, 1080.0),
            Size::new(1003.5, 607.25),
        ] {
            geometry.area = area;
            fits(&geometry, &model, target.tab);
            assert_eq!(
                before,
                format!(
                    "{:?}",
                    model.workspace().tab(target.tab).unwrap().grid().layout()
                )
            );
        }
    }
    for size in geometry.minimum.values_mut() {
        size.width *= 1.7;
        size.height *= 1.7;
    }
    fits(&geometry, &model, target.tab);
    assert_eq!(
        before,
        format!(
            "{:?}",
            model.workspace().tab(target.tab).unwrap().grid().layout()
        )
    );
}

#[test]
fn invalid_and_minimized_geometry_is_atomic() {
    let (mut model, target, mut geometry) = nested();
    let split = *model
        .workspace()
        .tab(target.tab)
        .unwrap()
        .grid()
        .layout()
        .splits()
        .next()
        .unwrap();
    let before = format!(
        "{:?}",
        model.workspace().tab(target.tab).unwrap().grid().layout()
    );
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        geometry.area.width = bad;
        assert!(
            geometry
                .canvas(model.workspace().tab(target.tab).unwrap().grid().layout())
                .is_none()
        );
        assert_eq!(
            model.resize_with_geometry(target.tab, split, 0.5, &geometry),
            Err(Error::Geometry)
        );
    }
    geometry.area = Size::new(900.0, 600.0);
    for bad in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert_eq!(
            model.resize_with_geometry(target.tab, split, bad, &geometry),
            Err(Error::InvalidRatio)
        );
    }
    assert_eq!(
        before,
        format!(
            "{:?}",
            model.workspace().tab(target.tab).unwrap().grid().layout()
        )
    );
}

#[test]
fn deepest_matching_ancestor_is_the_keyboard_target() {
    let (model, target, _) = nested();
    let node = model.workspace().tab(target.tab).unwrap().grid().layout();
    let Node::Split { id: root, a, .. } = node else {
        panic!()
    };
    let Node::Split { id: inner, .. } = a.as_ref() else {
        panic!()
    };
    assert_eq!(nearest(node, target.pane, Axis::Vertical), Some(*root));
    assert_eq!(nearest(node, target.pane, Axis::Horizontal), Some(*inner));
}

#[test]
fn thirty_two_deep_panes_and_extreme_ratios_have_a_linear_finite_canvas() {
    let mut model = Model::default();
    let (mut target, _) = model.create_tab().unwrap();
    for _ in 1..32 {
        target = model.split(target, Axis::Vertical).unwrap().0;
    }
    for split in model
        .workspace()
        .tab(target.tab)
        .unwrap()
        .grid()
        .layout()
        .splits()
        .copied()
        .collect::<Vec<_>>()
    {
        model.resize(target.tab, split, 0.0).unwrap();
    }
    let grid = model.workspace().tab(target.tab).unwrap().grid();
    let mut geometry = ResizeGeometry {
        area: Size::new(1.0, 1.0),
        spacing: 4.0,
        minimum: grid
            .iter()
            .map(|(pane, _)| (*pane, Size::new(162.0, 130.0)))
            .collect(),
    };
    fits(&geometry, &model, target.tab);
    assert_eq!(
        geometry.canvas(grid.layout()).unwrap().width,
        32.0 * 162.0 + 31.0 * 4.0
    );
    for minimum in geometry.minimum.values_mut() {
        minimum.width = 10_000.0;
    }
    assert!(geometry.canvas(grid.layout()).is_none());
}
