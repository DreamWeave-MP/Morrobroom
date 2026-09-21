use std::collections::BTreeMap;

use usage::Usage;

use super::FaceId;
use crate::line::LineId;

pub enum FaceLinesTag {}
pub type FaceLines = Usage<FaceLinesTag, BTreeMap<FaceId, Vec<LineId>>>;
