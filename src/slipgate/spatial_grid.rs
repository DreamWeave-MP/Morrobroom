use std::collections::HashMap;

use super::{brush::BrushId, csg::CsgVector};

const DEFAULT_CELL_SIZE: f64 = 128.0;
const MAX_CELLS_PER_BRUSH: u128 = 4096;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Aabb3 {
    pub min: CsgVector,
    pub max: CsgVector,
}

impl Aabb3 {
    #[must_use]
    pub fn from_points(points: impl IntoIterator<Item = CsgVector>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let mut aabb = Self {
            min: first,
            max: first,
        };
        for point in points {
            aabb.min = aabb.min.inf(&point);
            aabb.max = aabb.max.sup(&point);
        }
        Some(aabb)
    }

    #[must_use]
    pub fn expanded(self, padding: f64) -> Self {
        let padding = CsgVector::repeat(padding);
        Self {
            min: self.min - padding,
            max: self.max + padding,
        }
    }

    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= other.max[axis] && self.max[axis] >= other.min[axis])
    }
}

#[derive(Debug, Default)]
pub(crate) struct BrushAabbGrid {
    cell_size: f64,
    cells: HashMap<[i64; 3], Vec<BrushId>>,
    overflow: Vec<(BrushId, Aabb3)>,
}

impl BrushAabbGrid {
    #[must_use]
    pub fn new(brush_aabbs: &[Aabb3]) -> Self {
        let mut grid = Self {
            cell_size: DEFAULT_CELL_SIZE,
            ..Self::default()
        };

        for (index, aabb) in brush_aabbs.iter().copied().enumerate() {
            let brush_id = BrushId(index);
            let min = grid.cell_for(aabb.min);
            let max = grid.cell_for(aabb.max);
            let span = [
                (i128::from(max[0]) - i128::from(min[0]) + 1).cast_unsigned(),
                (i128::from(max[1]) - i128::from(min[1]) + 1).cast_unsigned(),
                (i128::from(max[2]) - i128::from(min[2]) + 1).cast_unsigned(),
            ];
            let cell_count = span[0].saturating_mul(span[1]).saturating_mul(span[2]);

            if cell_count > MAX_CELLS_PER_BRUSH {
                grid.overflow.push((brush_id, aabb));
                continue;
            }

            for x in min[0]..=max[0] {
                for y in min[1]..=max[1] {
                    for z in min[2]..=max[2] {
                        grid.cells.entry([x, y, z]).or_default().push(brush_id);
                    }
                }
            }
        }

        grid
    }

    #[must_use]
    pub fn query(&self, aabb: Aabb3, padding: f64) -> Vec<BrushId> {
        let query = aabb.expanded(padding);
        let min = self.cell_for(query.min);
        let max = self.cell_for(query.max);
        let mut candidates = self
            .overflow
            .iter()
            .filter_map(|(brush_id, brush_aabb)| brush_aabb.intersects(query).then_some(*brush_id))
            .collect::<Vec<_>>();

        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    candidates.extend(self.cells.get(&[x, y, z]).into_iter().flatten().copied());
                }
            }
        }

        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    #[must_use]
    fn cell_for(&self, point: CsgVector) -> [i64; 3] {
        [
            cell_coordinate(point.x, self.cell_size),
            cell_coordinate(point.y, self.cell_size),
            cell_coordinate(point.z, self.cell_size),
        ]
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "The grid is addressed by signed integer cells; finite world coordinates are intentionally quantized."
)]
fn cell_coordinate(coordinate: f64, cell_size: f64) -> i64 {
    (coordinate / cell_size).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb(min: [f64; 3], max: [f64; 3]) -> Aabb3 {
        Aabb3 {
            min: CsgVector::from(min),
            max: CsgVector::from(max),
        }
    }

    #[test]
    fn query_includes_touching_bounds() {
        let grid = BrushAabbGrid::new(&[aabb([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])]);
        assert_eq!(
            grid.query(aabb([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]), 0.0),
            vec![BrushId(0)]
        );
    }

    #[test]
    fn query_handles_negative_coordinates_and_deduplicates_cells() {
        let grid = BrushAabbGrid::new(&[aabb([-200.0, -200.0, -1.0], [200.0, 200.0, 1.0])]);
        assert_eq!(
            grid.query(aabb([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]), 0.0),
            vec![BrushId(0)]
        );
    }

    #[test]
    fn very_large_brushes_use_the_overflow_list() {
        let grid = BrushAabbGrid::new(&[aabb([-100_000.0; 3], [100_000.0; 3])]);
        assert_eq!(
            grid.query(aabb([10_000.0; 3], [10_001.0; 3]), 0.0),
            vec![BrushId(0)]
        );
    }
}
