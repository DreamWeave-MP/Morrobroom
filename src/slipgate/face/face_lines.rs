use usage::Usage;

use super::FaceId;
use crate::slipgate::{DenseStorage, line::LineId};

pub enum FaceLinesTag {}
pub type FaceLines = Usage<FaceLinesTag, DenseStorage<FaceId, Vec<LineId>>>;
