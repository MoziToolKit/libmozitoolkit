use serde::{Deserialize, Serialize};

/// Detailed animation frame configuration from `.png.mcmeta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnimationFrame {
    Index(u32),
    Detailed {
        index: u32,
        #[serde(default = "default_frametime")]
        time: u32,
    },
}

impl AnimationFrame {
    pub fn index(&self) -> u32 {
        match self {
            AnimationFrame::Index(idx) => *idx,
            AnimationFrame::Detailed { index, .. } => *index,
        }
    }

    pub fn time(&self, default_time: u32) -> u32 {
        match self {
            AnimationFrame::Index(_) => default_time,
            AnimationFrame::Detailed { time, .. } => *time,
        }
    }
}

fn default_frametime() -> u32 {
    1
}

/// The `"animation"` block inside a `.png.mcmeta` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationMetadata {
    /// Ticks per frame if not explicitly specified by frame entry. Default is 1.
    #[serde(default = "default_frametime")]
    pub frametime: u32,

    /// Whether to linearly interpolate between frames. Default is false.
    #[serde(default)]
    pub interpolate: bool,

    /// Optional explicit frame width override.
    #[serde(default)]
    pub width: Option<u32>,

    /// Optional explicit frame height override.
    #[serde(default)]
    pub height: Option<u32>,

    /// Sequence of frames. If None, frames are played sequentially from 0 to N-1.
    #[serde(default)]
    pub frames: Option<Vec<AnimationFrame>>,
}

impl Default for AnimationMetadata {
    fn default() -> Self {
        Self {
            frametime: 1,
            interpolate: false,
            width: None,
            height: None,
            frames: None,
        }
    }
}

/// Root container for `.png.mcmeta` texture metadata.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct TextureMetadata {
    #[serde(default)]
    pub(crate) animation: Option<AnimationMetadata>,
}

impl TextureMetadata {
    pub(crate) fn parse_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_metadata_parse_animation() {
        let json_data = r#"{
            "animation": {
                "frametime": 2,
                "interpolate": true
            }
        }"#;
        let meta = TextureMetadata::parse_json(json_data).unwrap();
        assert!(meta.animation.is_some());
        let anim = meta.animation.unwrap();
        assert_eq!(anim.frametime, 2);
        assert!(anim.interpolate);
    }
}
