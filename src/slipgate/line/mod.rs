mod line_duplicates;
mod line_face_connections;
mod line_faces;
mod line_id;
mod manifold_lines;

pub use line_duplicates::*;
pub use line_face_connections::*;
pub use line_faces::*;
pub use line_id::*;
pub use manifold_lines::*;
use usage::Usage;

use crate::slipgate::{
    DenseStorage, EPSILON, Vector3,
    face::{FaceId, FaceIndices, FaceLines},
};

#[derive(Debug, Copy, Clone)]
pub struct Line {
    pub i0: usize,
    pub i1: usize,
}

pub enum LinesTag {}
pub type Lines = Usage<LinesTag, DenseStorage<LineId, Line>>;

pub fn lines(face_indices: &FaceIndices) -> (Lines, FaceLines) {
    let mut face_lines = vec![Vec::new(); face_indices.len()];
    let mut lines = Vec::new();

    for (face_index, indices) in face_indices.iter().enumerate() {
        let face_id = FaceId(face_index);
        if indices.len() < 2 {
            continue;
        }

        for i in 0..indices.len() - 1 {
            let line_id = LineId(lines.len());

            lines.push(Line {
                i0: indices[i],
                i1: indices[i + 1],
            });
            face_lines[face_id.0].push(line_id);
        }

        let line_id = LineId(lines.len());

        lines.push(Line {
            i0: indices[indices.len() - 1],
            i1: indices[0],
        });
        face_lines[face_id.0].push(line_id);
    }

    (
        DenseStorage::from_vec(lines).into(),
        DenseStorage::from_vec(face_lines).into(),
    )
}

fn line_eq(a0: &Vector3, a1: &Vector3, b0: &Vector3, b1: &Vector3) -> bool {
    ((a0 - b0).magnitude() < EPSILON && (a1 - b1).magnitude() < EPSILON)
        || ((a0 - b1).magnitude() < EPSILON && (a1 - b0).magnitude() < EPSILON)
}

fn point_in_line(point: &Vector3, v0: &Vector3, v1: &Vector3) -> bool {
    if point == v0 {
        return true;
    }

    if point == v1 {
        return true;
    }

    if v0 == v1 {
        return false;
    }

    let dc = point - v0;
    let dl = v1 - v0;

    let cross = dc.cross(&dl);
    if cross.magnitude() > EPSILON {
        return false;
    }

    let norm = dc;
    let dp = norm.dot(point);
    let d0 = norm.dot(v0);
    let d1 = norm.dot(v1);

    dp >= d0 - EPSILON && dp <= d1 + EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_in_line() {
        let point = Vector3::new(0.0, 0.0, 0.0);
        let v0 = Vector3::new(1.0, 1.0, 1.0);
        let v1 = Vector3::new(-1.0, -1.0, -1.0);
        let contained = point_in_line(&point, &v0, &v1);
        assert!(contained);

        let point = Vector3::new(0.01, 0.0, 0.0);
        let contained = point_in_line(&point, &v0, &v1);
        assert!(!contained);
    }
}
