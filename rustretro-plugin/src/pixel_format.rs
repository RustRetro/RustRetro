use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PixelFormat {
    RGBA,
}

impl Default for PixelFormat {
    fn default() -> Self {
        Self::RGBA
    }
}
