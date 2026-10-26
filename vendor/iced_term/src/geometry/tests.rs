use super::*;

#[test]
fn restore_uses_font_changed_while_pane_was_too_small_for_one_cell() {
    let mut geometry = Geometry::default();
    assert!(geometry
        .update(Some(Size::new(10.0, 10.0)), Some(Size::new(8.0, 8.0)))
        .is_some());
    assert!(geometry.update(None, Some(Size::new(20.0, 30.0))).is_none());
    let restored = geometry
        .update(Some(Size::new(800.0, 600.0)), None)
        .unwrap();
    assert_eq!((restored.num_cols, restored.num_lines), (40, 20));
    assert_eq!((restored.cell_width, restored.cell_height), (20.0, 30.0));
}

#[test]
fn attachment_keeps_initial_rows_and_columns_without_float_product_rounding() {
    let mut geometry = Geometry::default();
    let cell = Size::new(8.433_333, 18.199_999);
    geometry.initialize(cell, TerminalSize::default());
    let size = geometry.update(None, Some(cell)).unwrap();
    assert_eq!((size.num_cols, size.num_lines), (80, 50));
}

#[test]
fn fractional_cells_and_subcell_drags_keep_the_same_grid() {
    let mut geometry = Geometry::default();
    let cell = Size::new(8.5, 18.25);
    let initial = geometry
        .update(Some(Size::new(935.0, 547.5)), Some(cell))
        .unwrap();
    assert_eq!((initial.num_cols, initial.num_lines), (110, 30));
    for delta in [0.1, 0.5, 1.0, 8.4] {
        assert_eq!(
            geometry.update(Some(Size::new(935.0 + delta, 548.0)), None),
            Some(initial)
        );
    }
    assert_eq!(
        geometry
            .update(Some(Size::new(943.5, 547.5)), None)
            .unwrap()
            .num_cols,
        111
    );
    let point =
        crate::backend::Backend::selection_point(17.0, 36.5, &initial, 7);
    assert_eq!((point.column.0, point.line.0), (2, -5));
}

#[test]
fn invalid_or_undersized_input_preserves_the_last_usable_layout_and_metrics() {
    let mut geometry = Geometry::default();
    let original = geometry
        .update(Some(Size::new(800.0, 480.0)), Some(Size::new(8.0, 16.0)))
        .unwrap();
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.01] {
        assert!(geometry
            .update(Some(Size::new(value, value)), None)
            .is_none());
        assert_eq!(geometry.update(None, None), Some(original));
    }
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(geometry
            .update(None, Some(Size::new(value, value)))
            .is_none());
        assert_eq!(geometry.update(None, None), Some(original));
    }
    // A font change still uses the last usable pane area after minimize.
    let zoomed = geometry.update(None, Some(Size::new(10.0, 20.0))).unwrap();
    assert_eq!((zoomed.num_cols, zoomed.num_lines), (80, 24));
}

#[test]
fn huge_and_subnormal_inputs_are_bounded_before_any_grid_allocation() {
    let mut geometry = Geometry::default();
    for cell in [f32::MIN_POSITIVE, f32::from_bits(1), 1.0] {
        let size = geometry
            .update(
                Some(Size::new(f32::MAX, f32::MAX)),
                Some(Size::new(cell, cell)),
            )
            .unwrap();
        assert_eq!((size.num_cols, size.num_lines), (MAX_COLUMNS, MAX_LINES));
        let native = WindowSize::from(size);
        assert!(native.num_cols > 0 && native.num_cols <= i16::MAX as u16);
        assert!(native.num_lines > 0 && native.num_lines <= i16::MAX as u16);
        assert!(native.cell_width > 0 && native.cell_height > 0);
    }
}

#[test]
fn dpi_affects_logical_layout_not_relative_line_height() {
    // Match Iced's physical-to-logical layout conversion for a fixed client area.
    let mut geometry = Geometry::default();
    for (dpi, cols, rows) in
        [(1.0, 120, 40), (1.25, 96, 32), (1.5, 80, 26), (2.0, 60, 20)]
    {
        let size = geometry
            .update(
                Some(Size::new(1020.0 / dpi, 730.0 / dpi)),
                Some(Size::new(8.5, 18.25)),
            )
            .unwrap();
        assert_eq!((size.num_cols, size.num_lines), (cols, rows));
        assert_eq!((size.cell_width, size.cell_height), (8.5, 18.25));
    }
}
