use std::collections::BTreeSet;

use usage::Usage;

use super::FaceId;
use crate::slipgate::{
    brush::BrushFaceContainment,
    face::{FaceDuplicates, FaceFaceContainment},
};

pub enum OccludedFacesTag {}

/// The set of faces that are completely occluded and will never be visible in-game.
///
/// A face is occluded if any of the following hold:
/// - It is a duplicate face: a brush boundary where two brushes touch and their
///   shared faces point toward each other — both sides are solid, so neither face
///   can be seen (detected by [`face_duplicates`](super::face_duplicates)).
/// - It lies entirely inside another brush's solid volume — all its vertices are
///   inside another brush's convex hull (detected by [`brush_face_containment`]).
/// - Its polygon is fully covered by a larger coplanar opposing face — it is
///   geometrically contained within another face on the same plane
///   (detected by [`face_face_containment`](super::face_face_containment)).
pub type OccludedFaces = Usage<OccludedFacesTag, BTreeSet<FaceId>>;

#[must_use]
pub fn occluded_faces(
    face_duplicates: &FaceDuplicates,
    brush_face_containment: &BrushFaceContainment,
    face_face_containment: &FaceFaceContainment,
) -> OccludedFaces {
    let mut occluded = BTreeSet::new();

    // Case 1: Both faces at a brush boundary are occluded — solid geometry
    // is immediately behind each face, making both invisible.
    for (a, b) in face_duplicates.iter() {
        occluded.insert(*a);
        occluded.insert(*b);
    }

    // Case 2: Faces entirely inside another brush's solid volume.
    for faces in brush_face_containment.as_slice() {
        occluded.extend(faces.iter().copied());
    }

    // Case 3: Faces whose polygon is fully covered by a larger coplanar opposing
    // face. The containing face is not necessarily occluded; only the contained one.
    for contained in face_face_containment.as_slice() {
        occluded.extend(contained.iter().copied());
    }

    occluded.into()
}
