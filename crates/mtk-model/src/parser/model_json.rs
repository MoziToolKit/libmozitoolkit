use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::ModelError;

const MAX_PARENT_DEPTH: usize = 32;

pub(crate) fn default_scale_one() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

/// Viewport/render lighting mode in Minecraft models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiLight {
    Front,
    Side,
}

/// Canonical display context enum matching vanilla Minecraft's `ItemDisplayContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemDisplayContext {
    ThirdpersonRighthand,
    ThirdpersonLefthand,
    FirstpersonRighthand,
    FirstpersonLefthand,
    Head,
    Gui,
    Ground,
    Fixed,
    OnShelf,
}

/// Raw JSON viewport 3D display transform (translation specified in pixels).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ItemTransformJson {
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default)]
    pub translation: [f32; 3],
    #[serde(default = "default_scale_one")]
    pub scale: [f32; 3],
}

impl Default for ItemTransformJson {
    fn default() -> Self {
        Self {
            rotation: [0.0, 0.0, 0.0],
            translation: [0.0, 0.0, 0.0],
            scale: default_scale_one(),
        }
    }
}

impl ItemTransformJson {
    /// Converts raw JSON transform (translation in pixels) into normalized `ModelTransform`
    /// where translation is in standard block units (scaled by 1/16 = 0.0625).
    #[inline]
    pub fn to_model_transform(self) -> ModelTransform {
        ModelTransform {
            rotation: self.rotation,
            translation: [
                self.translation[0] * 0.0625,
                self.translation[1] * 0.0625,
                self.translation[2] * 0.0625,
            ],
            scale: self.scale,
        }
    }
}

/// Raw JSON display transform dictionary across standard contexts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ItemTransformsJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thirdperson_righthand: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thirdperson_lefthand: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firstperson_righthand: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firstperson_lefthand: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gui: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<ItemTransformJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_shelf: Option<ItemTransformJson>,
}

/// Standardized 3D viewport transformation with translation normalized to block space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelTransform {
    /// Rotation Euler XYZ in degrees.
    #[serde(default)]
    pub rotation: [f32; 3],
    /// Translation offset in block units [tx, ty, tz].
    #[serde(default)]
    pub translation: [f32; 3],
    /// Scale factor [sx, sy, sz].
    #[serde(default = "default_scale_one")]
    pub scale: [f32; 3],
}

impl Default for ModelTransform {
    fn default() -> Self {
        Self {
            rotation: [0.0, 0.0, 0.0],
            translation: [0.0, 0.0, 0.0],
            scale: default_scale_one(),
        }
    }
}

/// Fully resolved display transforms across standard viewport contexts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ItemTransforms {
    #[serde(default)]
    pub thirdperson_righthand: Option<ModelTransform>,
    #[serde(default)]
    pub thirdperson_lefthand: Option<ModelTransform>,
    #[serde(default)]
    pub firstperson_righthand: Option<ModelTransform>,
    #[serde(default)]
    pub firstperson_lefthand: Option<ModelTransform>,
    #[serde(default)]
    pub head: Option<ModelTransform>,
    #[serde(default)]
    pub gui: Option<ModelTransform>,
    #[serde(default)]
    pub ground: Option<ModelTransform>,
    #[serde(default)]
    pub fixed: Option<ModelTransform>,
    #[serde(default)]
    pub on_shelf: Option<ModelTransform>,
}

impl ItemTransforms {
    /// Returns the transform for a given display context.
    pub fn get(&self, context: ItemDisplayContext) -> Option<ModelTransform> {
        match context {
            ItemDisplayContext::ThirdpersonRighthand => self.thirdperson_righthand,
            ItemDisplayContext::ThirdpersonLefthand => self.thirdperson_lefthand,
            ItemDisplayContext::FirstpersonRighthand => self.firstperson_righthand,
            ItemDisplayContext::FirstpersonLefthand => self.firstperson_lefthand,
            ItemDisplayContext::Head => self.head,
            ItemDisplayContext::Gui => self.gui,
            ItemDisplayContext::Ground => self.ground,
            ItemDisplayContext::Fixed => self.fixed,
            ItemDisplayContext::OnShelf => self.on_shelf,
        }
    }

    /// Sets the transform for a given display context.
    pub fn set(&mut self, context: ItemDisplayContext, transform: Option<ModelTransform>) {
        match context {
            ItemDisplayContext::ThirdpersonRighthand => self.thirdperson_righthand = transform,
            ItemDisplayContext::ThirdpersonLefthand => self.thirdperson_lefthand = transform,
            ItemDisplayContext::FirstpersonRighthand => self.firstperson_righthand = transform,
            ItemDisplayContext::FirstpersonLefthand => self.firstperson_lefthand = transform,
            ItemDisplayContext::Head => self.head = transform,
            ItemDisplayContext::Gui => self.gui = transform,
            ItemDisplayContext::Ground => self.ground = transform,
            ItemDisplayContext::Fixed => self.fixed = transform,
            ItemDisplayContext::OnShelf => self.on_shelf = transform,
        }
    }
}

/// Model override rule based on item predicates (e.g. pulling bow, damage, clock time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelOverride {
    pub predicate: HashMap<String, f32>,
    pub model: String,
}

/// High-level representation of a Minecraft Block Model JSON.
///
/// Follows Minecraft 1.21+ format, supporting parent template inheritance,
/// `#texture` variable resolutions, and custom elements.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockModelJson {
    /// Optional parent model path (e.g. "minecraft:block/cube_all" or "block/cube").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Ambient occlusion flag (defaults to true in Minecraft).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambientocclusion: Option<bool>,

    /// Viewport/render lighting mode: "front" (items) or "side" (blocks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gui_light: Option<GuiLight>,

    /// Texture mappings dictionary (e.g. {"all": "minecraft:block/stone", "particle": "#all"}).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub textures: Option<HashMap<String, TextureValue>>,

    /// 3D box elements definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<Vec<ElementJson>>,

    /// Viewport 3D display transformations across standard contexts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<ItemTransformsJson>,

    /// Item predicate-based model overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<Vec<ModelOverride>>,
}

impl BlockModelJson {
    /// Fully resolves the inheritance tree using a parent loader callback.
    ///
    /// Merges textures (child overrides parent) and inherits elements (first model in the chain providing elements wins).
    /// Resolves all `#var` texture variables to final canonical texture identifiers.
    pub fn resolve_hierarchy<F>(
        &self,
        current_model_id: &str,
        mut parent_loader: F,
    ) -> Result<ResolvedBlockModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let mut visited = HashSet::new();
        visited.insert(current_model_id.to_string());

        let mut raw_textures = self.textures.clone().unwrap_or_default();
        let mut elements = self.elements.clone();
        let mut ambientocclusion = self.ambientocclusion;
        let mut gui_light = self.gui_light;
        let mut overrides = self.overrides.clone();

        let mut display_chain: Vec<ItemTransformsJson> = Vec::new();
        if let Some(ref d) = self.display {
            display_chain.push(d.clone());
        }

        let mut curr_parent = self.parent.clone();
        let mut depth = 0;

        while let Some(parent_id) = curr_parent {
            depth += 1;
            if depth > MAX_PARENT_DEPTH || visited.contains(&parent_id) {
                return Err(ModelError::CircularParentHierarchy(parent_id));
            }
            visited.insert(parent_id.clone());

            let parent_model = parent_loader(&parent_id).unwrap_or_default();

            // 1. Merge textures: child textures take priority; add parent's only if not present
            if let Some(ref p_textures) = parent_model.textures {
                for (k, v) in p_textures {
                    raw_textures.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }

            // 2. Inherit elements if not defined yet
            if elements.is_none() && parent_model.elements.is_some() {
                elements = parent_model.elements.clone();
            }

            // 3. Inherit ambient occlusion if not defined
            if ambientocclusion.is_none() && parent_model.ambientocclusion.is_some() {
                ambientocclusion = parent_model.ambientocclusion;
            }

            // 4. Inherit gui_light if not defined
            if gui_light.is_none() && parent_model.gui_light.is_some() {
                gui_light = parent_model.gui_light;
            }

            // 5. Inherit overrides if not defined
            if overrides.is_none() && parent_model.overrides.is_some() {
                overrides = parent_model.overrides.clone();
            }

            // 6. Collect display transforms up the parent chain
            if let Some(ref d) = parent_model.display {
                display_chain.push(d.clone());
            }

            curr_parent = parent_model.parent;
        }

        // Resolve display transforms per slot (top-wins in inheritance chain)
        let find_slot = |extractor: fn(&ItemTransformsJson) -> Option<ItemTransformJson>| -> Option<ModelTransform> {
            for d in &display_chain {
                if let Some(t) = extractor(d) {
                    return Some(t.to_model_transform());
                }
            }
            None
        };

        let thirdperson_righthand = find_slot(|d| d.thirdperson_righthand);
        let thirdperson_lefthand = find_slot(|d| d.thirdperson_lefthand).or(thirdperson_righthand);
        let firstperson_righthand = find_slot(|d| d.firstperson_righthand);
        let firstperson_lefthand = find_slot(|d| d.firstperson_lefthand).or(firstperson_righthand);
        let head = find_slot(|d| d.head);
        let gui = find_slot(|d| d.gui);
        let ground = find_slot(|d| d.ground);
        let fixed = find_slot(|d| d.fixed);
        let on_shelf = find_slot(|d| d.on_shelf);

        let resolved_transforms = ItemTransforms {
            thirdperson_righthand,
            thirdperson_lefthand,
            firstperson_righthand,
            firstperson_lefthand,
            head,
            gui,
            ground,
            fixed,
            on_shelf,
        };

        // Resolve all `#texture` variable references
        let mut resolved_textures = HashMap::new();
        for (k, val) in &raw_textures {
            let final_tex = Self::resolve_texture_value(val.as_str(), &raw_textures)?;
            resolved_textures.insert(k.clone(), final_tex);
        }

        // Bake texture values into elements
        let resolved_elements = if let Some(elems) = elements {
            let mut res_elems = Vec::with_capacity(elems.len());
            for elem in elems {
                let elem_emission = elem.light_emission.unwrap_or(0);
                let mut res_faces = HashMap::with_capacity(elem.faces.len());
                for (dir, face) in elem.faces {
                    let final_tex = Self::resolve_texture_value(&face.texture, &raw_textures)
                        .unwrap_or_else(|_| face.texture.trim_start_matches('#').to_string());
                    res_faces.insert(
                        dir,
                        ResolvedFace {
                            uv: face.uv,
                            texture: final_tex,
                            cullface: face.cullface,
                            rotation: face.rotation.unwrap_or(0),
                            tintindex: face.tintindex.unwrap_or(-1),
                        },
                    );
                }
                res_elems.push(ResolvedElement {
                    from: elem.from,
                    to: elem.to,
                    rotation: elem.rotation,
                    transform: elem.transform,
                    shade: elem.shade.unwrap_or(true),
                    light_emission: elem_emission,
                    faces: res_faces,
                });
            }
            res_elems
        } else {
            Vec::new()
        };

        Ok(ResolvedBlockModel {
            ambientocclusion: ambientocclusion.unwrap_or(true),
            gui_light,
            textures: resolved_textures,
            elements: resolved_elements,
            transforms: resolved_transforms,
            overrides: overrides.unwrap_or_default(),
        })
    }

    /// Recursively resolves a `#texture` variable to its terminal path.
    pub fn resolve_texture_value(
        target: &str,
        textures: &HashMap<String, TextureValue>,
    ) -> Result<String, ModelError> {
        let mut curr = target;
        let mut visited = HashSet::new();

        loop {
            let var_name = if let Some(v) = curr.strip_prefix('#') {
                v
            } else if textures.contains_key(curr) && !curr.contains('/') && !curr.contains(':') {
                curr
            } else {
                break;
            };

            if visited.contains(var_name) {
                return Err(ModelError::CircularParentHierarchy(format!(
                    "Cyclic texture variable '#{}'",
                    var_name
                )));
            }
            visited.insert(var_name.to_string());

            if let Some(next_val) = textures.get(var_name) {
                curr = next_val.as_str();
            } else {
                return Err(ModelError::UnresolvedTextureVariable(var_name.to_string()));
            }
        }

        let normalized = Self::normalize_texture(curr);
        Ok(normalized)
    }

    /// Normalizes texture identifier into "namespace:block/name" form.
    pub fn normalize_texture(tex: &str) -> String {
        if tex.is_empty() || tex.starts_with('#') {
            return tex.to_string();
        }
        let (ns, path) = if let Some((ns, p)) = tex.split_once(':') {
            (ns, p)
        } else {
            ("minecraft", tex)
        };
        if !path.contains('/') {
            format!("{}:block/{}", ns, path)
        } else {
            format!("{}:{}", ns, path)
        }
    }
}

/// Fully canonical resolved Block Model with parent hierarchy and texture variables flattened.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolvedBlockModel {
    /// Ambient occlusion.
    pub ambientocclusion: bool,
    /// Viewport/render light mode: "front" (items) or "side" (blocks).
    pub gui_light: Option<GuiLight>,
    /// Resolved texture dictionary mapping variable names to canonical texture IDs.
    pub textures: HashMap<String, String>,
    /// Fully resolved elements.
    pub elements: Vec<ResolvedElement>,
    /// Resolved viewport 3D display transformations.
    pub transforms: ItemTransforms,
    /// Item predicate-based model overrides.
    pub overrides: Vec<ModelOverride>,
}

/// Resolved element with concrete textures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<RotationJson>,
    pub transform: Option<BuiltinTransform>,
    pub shade: bool,
    pub light_emission: u8,
    pub faces: HashMap<String, ResolvedFace>,
}

/// Resolved element face with concrete texture identifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedFace {
    pub uv: Option<[f32; 4]>,
    pub texture: String,
    pub cullface: Option<String>,
    pub rotation: u32,
    pub tintindex: i16,
}

/// Represents texture entries in Minecraft model JSON, which can be either a simple path
/// or a 1.21+ modern sprite object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextureValue {
    /// String path, e.g. "minecraft:block/oak_planks" or variable ref "#all"
    Path(String),
    /// Modern Minecraft 1.21+ object format with extra attributes
    SpriteObject {
        sprite: String,
        #[serde(default)]
        force_translucent: Option<bool>,
    },
}

impl TextureValue {
    /// Returns the target sprite string path or reference.
    pub fn as_str(&self) -> &str {
        match self {
            TextureValue::Path(s) => s.as_str(),
            TextureValue::SpriteObject { sprite, .. } => sprite.as_str(),
        }
    }
}

/// Optional 3D transformation for elements (e.g. from MiEx builtin models).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuiltinTransform {
    /// Euler rotation angles [rx, ry, rz] in degrees.
    pub rotate: [f32; 3],
    /// Pivot origin in [0, 16] space, default [8.0, 8.0, 8.0].
    pub pivot: [f32; 3],
}

impl Default for BuiltinTransform {
    fn default() -> Self {
        Self {
            rotate: [0.0, 0.0, 0.0],
            pivot: [8.0, 8.0, 8.0],
        }
    }
}

/// 3D box element within a model definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementJson {
    /// Starting coordinate in [0, 16] space.
    pub from: [f32; 3],

    /// Ending coordinate in [0, 16] space.
    pub to: [f32; 3],

    /// Optional local element rotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationJson>,

    /// Optional 3D transformation (from builtin MiEx models).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<BuiltinTransform>,

    /// Whether to render shadows/shading on this element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shade: Option<bool>,

    /// Optional light emission level in [0, 15] (1.21+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_emission: Option<u8>,

    /// Faces dictionary mapping direction ("down", "up", "north", "south", "west", "east") to face data.
    pub faces: HashMap<String, FaceJson>,
}

/// Rotation specification for an element around a fixed origin point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotationJson {
    /// Origin pivot point in [0, 16] space.
    pub origin: [f32; 3],

    /// Rotation value: either a single axis/angle pair or a three-axis Euler rotation.
    #[serde(flatten)]
    pub value: RotationValue,

    /// Whether to rescale the face to stretch to the cube bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rescale: Option<bool>,
}

/// Element rotation payload.
///
/// Vanilla (`CuboidModelElement.getRotation`) accepts two shapes and picks by the presence of
/// `axis`/`angle`:
/// - `{ "axis": "y", "angle": 22.5, "origin": [..] }` -> [`RotationValue::SingleAxis`]
/// - `{ "x": 180, "y": -67.5, "z": -180, "origin": [..] }` -> [`RotationValue::Euler`]
///
/// Only supporting the first form made the whole model JSON fail to parse, which silently
/// dropped vanilla content such as `block/template_hanging_sign_rot_*`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RotationValue {
    /// Single axis rotation in degrees.
    SingleAxis {
        /// Rotation axis: "x", "y", or "z".
        axis: String,
        /// Rotation angle in degrees: typically multiples of 22.5.
        angle: f32,
    },
    /// Three-axis Euler rotation, applied as `Rz * Ry * Rx` (vanilla `rotationZYX(z, y, x)`).
    Euler {
        #[serde(default)]
        x: f32,
        #[serde(default)]
        y: f32,
        #[serde(default)]
        z: f32,
    },
}

impl RotationJson {
    /// Rotation as a single axis/angle pair, if this is the single-axis form.
    pub fn single_axis(&self) -> Option<(&str, f32)> {
        match &self.value {
            RotationValue::SingleAxis { axis, angle } => Some((axis.as_str(), *angle)),
            RotationValue::Euler { .. } => None,
        }
    }

    /// Euler angles in degrees, if this is the three-axis form.
    pub fn euler(&self) -> Option<(f32, f32, f32)> {
        match &self.value {
            RotationValue::Euler { x, y, z } => Some((*x, *y, *z)),
            RotationValue::SingleAxis { .. } => None,
        }
    }
}

/// Face properties of an element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceJson {
    /// UV coordinates [min_u, min_v, max_u, max_v] in [0, 16] space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv: Option<[f32; 4]>,

    /// Texture reference, e.g. "#side", "#all", or "minecraft:block/stone".
    pub texture: String,

    /// Optional cullface direction tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cullface: Option<String>,

    /// Clockwise UV rotation: 0, 90, 180, or 270 degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<u32>,

    /// Biome / colormap tint index (-1 if none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tintindex: Option<i16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_block_model_cube() {
        let json_data = r##"{
            "parent": "minecraft:block/block",
            "textures": {
                "particle": "#all",
                "all": "minecraft:block/stone"
            },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "down":  { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "down" },
                        "up":    { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "up" },
                        "north": { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "north" },
                        "south": { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "south" },
                        "west":  { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "west" },
                        "east":  { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "east" }
                    }
                }
            ]
        }"##;

        let model: BlockModelJson = serde_json::from_str(json_data).unwrap();
        assert_eq!(model.parent, Some("minecraft:block/block".to_string()));
        let textures = model.textures.as_ref().unwrap();
        assert_eq!(
            textures.get("all").unwrap().as_str(),
            "minecraft:block/stone"
        );
        assert_eq!(model.elements.as_ref().unwrap().len(), 1);

        // Test hierarchy and texture resolution
        let resolved = model.resolve_hierarchy("test:model", |_| None).unwrap();
        assert_eq!(
            resolved.textures.get("all").unwrap(),
            "minecraft:block/stone"
        );
        assert_eq!(
            resolved.textures.get("particle").unwrap(),
            "minecraft:block/stone"
        );
        assert_eq!(resolved.elements.len(), 1);
        assert_eq!(
            resolved.elements[0].faces.get("up").unwrap().texture,
            "minecraft:block/stone"
        );
    }

    #[test]
    fn test_parent_inheritance() {
        let parent_json = r##"{
            "textures": {
                "base": "minecraft:block/dirt"
            },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 8, 16],
                    "faces": {
                        "up": { "texture": "#base" }
                    }
                }
            ]
        }"##;

        let child_json = r##"{
            "parent": "minecraft:block/slab_base",
            "textures": {
                "base": "minecraft:block/oak_planks"
            }
        }"##;

        let child: BlockModelJson = serde_json::from_str(child_json).unwrap();
        let parent: BlockModelJson = serde_json::from_str(parent_json).unwrap();

        let resolved = child
            .resolve_hierarchy("minecraft:block/oak_slab", |id| {
                if id == "minecraft:block/slab_base" {
                    Some(parent.clone())
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(resolved.elements.len(), 1);
        assert_eq!(resolved.elements[0].to, [16.0, 8.0, 16.0]);
        // Child's texture should override parent's
        assert_eq!(
            resolved.elements[0].faces.get("up").unwrap().texture,
            "minecraft:block/oak_planks"
        );
    }

    #[test]
    fn test_display_transforms_deserialization_and_inheritance() {
        let parent_json = r##"{
            "gui_light": "side",
            "display": {
                "thirdperson_righthand": {
                    "rotation": [75, 45, 0],
                    "translation": [0, 2.5, 0],
                    "scale": [0.375, 0.375, 0.375]
                },
                "fixed": {
                    "rotation": [0, 180, 0],
                    "translation": [0, 0, 0],
                    "scale": [0.5, 0.5, 0.5]
                }
            }
        }"##;

        let child_json = r##"{
            "parent": "minecraft:item/parent_tool",
            "gui_light": "front",
            "display": {
                "gui": {
                    "rotation": [0, 0, 0],
                    "translation": [0, 0, 0],
                    "scale": [1.0, 1.0, 1.0]
                }
            },
            "overrides": [
                {
                    "predicate": { "damaged": 1.0, "damage": 0.5 },
                    "model": "minecraft:item/broken_tool"
                }
            ]
        }"##;

        let parent: BlockModelJson = serde_json::from_str(parent_json).unwrap();
        let child: BlockModelJson = serde_json::from_str(child_json).unwrap();

        assert_eq!(parent.gui_light, Some(GuiLight::Side));
        assert_eq!(child.gui_light, Some(GuiLight::Front));
        assert_eq!(child.overrides.as_ref().unwrap().len(), 1);

        let resolved = child
            .resolve_hierarchy("minecraft:item/sword", |id| {
                if id == "minecraft:item/parent_tool" {
                    Some(parent.clone())
                } else {
                    None
                }
            })
            .unwrap();

        // 1. gui_light: child overrides parent
        assert_eq!(resolved.gui_light, Some(GuiLight::Front));

        // 2. overrides: inherited from child
        assert_eq!(resolved.overrides.len(), 1);
        assert_eq!(resolved.overrides[0].model, "minecraft:item/broken_tool");
        assert_eq!(resolved.overrides[0].predicate.get("damaged"), Some(&1.0));

        // 3. display slots:
        // gui is defined on child
        let gui_tf = resolved.transforms.get(ItemDisplayContext::Gui).unwrap();
        assert_eq!(gui_tf.scale, [1.0, 1.0, 1.0]);

        // fixed is inherited from parent
        let fixed_tf = resolved.transforms.get(ItemDisplayContext::Fixed).unwrap();
        assert_eq!(fixed_tf.rotation, [0.0, 180.0, 0.0]);
        assert_eq!(fixed_tf.scale, [0.5, 0.5, 0.5]);

        // thirdperson_righthand is inherited from parent
        let tp_right = resolved
            .transforms
            .get(ItemDisplayContext::ThirdpersonRighthand)
            .unwrap();
        assert_eq!(tp_right.rotation, [75.0, 45.0, 0.0]);
        // translation in pixels [0, 2.5, 0] scaled by 0.0625 = [0, 0.15625, 0]
        assert_eq!(tp_right.translation, [0.0, 2.5 * 0.0625, 0.0]);

        // thirdperson_lefthand was not specified, so it falls back to thirdperson_righthand
        let tp_left = resolved
            .transforms
            .get(ItemDisplayContext::ThirdpersonLefthand)
            .unwrap();
        assert_eq!(tp_left, tp_right);

        // ground was not specified anywhere, so it is None
        assert!(resolved
            .transforms
            .get(ItemDisplayContext::Ground)
            .is_none());
    }

    #[test]
    fn test_light_emission_deserialization() {
        let json_data = r##"{
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "light_emission": 15,
                    "faces": {
                        "up": { "texture": "#all" }
                    }
                }
            ]
        }"##;

        let model: BlockModelJson = serde_json::from_str(json_data).unwrap();
        let resolved = model.resolve_hierarchy("test:light", |_| None).unwrap();
        assert_eq!(resolved.elements.len(), 1);
        assert_eq!(resolved.elements[0].light_emission, 15);
    }
}
