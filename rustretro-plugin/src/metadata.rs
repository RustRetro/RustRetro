use serde::{Deserialize, Serialize};

use crate::PixelFormat;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: alloc::string::String,
    pub width: u32,
    pub height: u32,

    pub pixel_format: PixelFormat,
    pub frames_per_seconds: f32,
}
