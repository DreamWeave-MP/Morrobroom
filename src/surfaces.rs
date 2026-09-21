pub mod colors {}

pub enum NiBroomSurface {
    NoClip = 1,
    SmoothShading = 2,
    InvertFaces = 4,
}

impl std::fmt::Display for NiBroomSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NiBroomSurface::NoClip => write!(f, "No Clip"),
            NiBroomSurface::SmoothShading => write!(f, "Smooth Shading"),
            NiBroomSurface::InvertFaces => write!(f, "Invert Faces"),
        }
    }
}

#[allow(dead_code)] // Reserved for the content categories emitted by the NIF bridge.
pub enum NiBroomContent {}
