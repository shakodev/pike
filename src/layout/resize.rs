//! Logical-pixel constraints for the pinned pane_grid rounding algorithm.
use std::collections::BTreeMap;

use super::{Axis, Node, Pane, PaneState, Rectangle, Size, Split, pane_grid};

/// Hard logical-pixel bound for a panning canvas, independent of ratios/depth.
/// Unrepresentable constraints retain the app's last good presentation instead.
pub const MAX_CANVAS_EXTENT: f32 = 16_384.0;

#[derive(Clone, Debug)]
pub struct ResizeGeometry {
    pub area: Size,
    /// Per-leaf cell minimum plus the actual header, border and status footer.
    pub minimum: BTreeMap<Pane, Size>,
    pub spacing: f32,
}

impl ResizeGeometry {
    fn valid(&self) -> bool {
        valid(self.area)
            && self.spacing.is_finite()
            && self.spacing >= 0.0
            && self.minimum.values().copied().all(valid)
    }

    /// Intrinsic canvas grows linearly with the pane count, never exponentially
    /// with nested ratios. Presentation clamps are separate from stored ratios.
    pub fn canvas(&self, node: &Node) -> Option<Size> {
        if !self.valid() {
            return None;
        }
        let required = self.required(node)?;
        let size = Size::new(
            self.area.width.max(required.width),
            self.area.height.max(required.height),
        );
        (valid(size) && size.width <= MAX_CANVAS_EXTENT && size.height <= MAX_CANVAS_EXTENT)
            .then_some(size)
    }

    fn required(&self, node: &Node) -> Option<Size> {
        match node {
            Node::Pane(pane) => self
                .minimum
                .get(pane)
                .map(|size| Size::new(size.width.ceil(), size.height.ceil())),
            Node::Split {
                axis, ratio, a, b, ..
            } => {
                if !ratio.is_finite() || *ratio <= 0.0 || *ratio >= 1.0 {
                    return None;
                }
                let a = self.required(a)?;
                let b = self.required(b)?;
                Some(match axis {
                    Axis::Vertical => {
                        Size::new(a.width + self.spacing + b.width, a.height.max(b.height))
                    }
                    Axis::Horizontal => {
                        Size::new(a.width.max(b.width), a.height + self.spacing + b.height)
                    }
                })
            }
        }
    }

    /// Clamp against both whole subtrees, including every descendant leaf.
    /// Return None for stale, invalid, or impossible geometry without mutation.
    pub(crate) fn clamp(&self, node: &Node, split: Split, ratio: f32) -> Option<f32> {
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return None;
        }
        let canvas = self.canvas(node)?;
        let target = find(node, split)?;
        let Node::Split { axis, a, b, .. } = target else {
            return None;
        };
        let mut presented = node.clone();
        self.fit(&mut presented, Rectangle::with_size(canvas))?;
        let (_, bounds, _) = presented
            .split_regions(self.spacing, 0.0, canvas)
            .remove(&split)?;
        let a = self.required(a)?;
        let b = self.required(b)?;
        let (extent, minimum_a, minimum_b) = match axis {
            Axis::Vertical => (bounds.width, a.width, b.width),
            Axis::Horizontal => (bounds.height, a.height, b.height),
        };
        let low = minimum_a.ceil();
        let high = (extent - self.spacing - minimum_b).floor();
        if low > high || extent <= 0.0 {
            return None;
        }
        let first = (extent * ratio - self.spacing / 2.0)
            .round()
            .clamp(low, high);
        Some((first + self.spacing / 2.0) / extent)
    }

    /// A lightweight presentation clone owns no sessions. At constrained sizes
    /// only its ratios change; restoring the viewport restores canonical ratios.
    pub fn presentation(
        &self,
        grid: &pane_grid::State<PaneState>,
    ) -> Option<(Size, pane_grid::State<PaneState>)> {
        let size = self.canvas(grid.layout())?;
        let mut node = grid.layout().clone();
        self.fit(&mut node, Rectangle::with_size(size))?;
        let mut grid = grid.clone();
        for (split, (_, _, ratio)) in node.split_regions(self.spacing, 0.0, size) {
            grid.resize(split, ratio);
        }
        Some((size, grid))
    }

    fn fit(&self, node: &mut Node, bounds: Rectangle) -> Option<()> {
        if let Node::Split {
            axis, ratio, a, b, ..
        } = node
        {
            let min_a = self.required(a)?;
            let min_b = self.required(b)?;
            let (extent, low, high) = match axis {
                Axis::Vertical => (
                    bounds.width,
                    min_a.width,
                    bounds.width - self.spacing - min_b.width,
                ),
                Axis::Horizontal => (
                    bounds.height,
                    min_a.height,
                    bounds.height - self.spacing - min_b.height,
                ),
            };
            let low = low.ceil();
            let high = high.floor();
            if low > high || !extent.is_finite() || extent <= 0.0 {
                return None;
            }
            let first = (extent * *ratio - self.spacing / 2.0)
                .round()
                .clamp(low, high);
            *ratio = (first + self.spacing / 2.0) / extent;
            let (left, right, _) = axis.split(&bounds, *ratio, self.spacing, 0.0, 0.0);
            self.fit(a, left)?;
            self.fit(b, right)?;
        }
        Some(())
    }
}

fn valid(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

fn find(node: &Node, split: Split) -> Option<&Node> {
    match node {
        Node::Pane(_) => None,
        Node::Split { id, a, b, .. } => {
            if *id == split {
                Some(node)
            } else {
                find(a, split).or_else(|| find(b, split))
            }
        }
    }
}

/// Nearest ancestor with the requested axis; keyboard resizing never changes
/// focus or guesses from the traversal order of unrelated split handles.
pub(crate) fn nearest(node: &Node, pane: Pane, axis: Axis) -> Option<Split> {
    fn visit(node: &Node, pane: Pane, axis: Axis) -> (bool, Option<Split>) {
        match node {
            Node::Pane(id) => (*id == pane, None),
            Node::Split {
                id,
                axis: current,
                a,
                b,
                ..
            } => {
                let left = visit(a, pane, axis);
                let (contains, found) = if left.0 { left } else { visit(b, pane, axis) };
                (
                    contains,
                    found.or_else(|| (contains && *current == axis).then_some(*id)),
                )
            }
        }
    }
    visit(node, pane, axis).1
}

#[cfg(test)]
mod tests;
