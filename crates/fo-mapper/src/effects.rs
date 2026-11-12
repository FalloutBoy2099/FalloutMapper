//! Les mélanges de palette dépendent du décor derrière le sprite. Seuls les petits
//! rectangles des effets sont recomposés sur CPU, puis conservés en texture.
use eframe::egui::{self, TextureHandle, TextureOptions};
use fo_scene::{Blend, DrawItem, LayerSet, Scene};
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub(crate) struct EffectsCache {
    textures: HashMap<u64, (TextureHandle, u64)>,
}

impl EffectsCache {
    pub(crate) fn prune(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        self.textures
            .retain(|_, (_, used)| frame.saturating_sub(*used) <= 2);
    }
}

pub(crate) struct EffectsFrame {
    scene: Scene,
    light: Vec<f32>,
    hash: DefaultHasher,
    background: [u8; 4],
}

impl EffectsFrame {
    pub(crate) fn new(scene: &Scene, background: [u8; 4]) -> Option<Self> {
        if !scene
            .items
            .iter()
            .any(|i| matches!(i.blend, Blend::Palette(_)))
        {
            return None;
        }
        let mut hash = DefaultHasher::new();
        // Identity is only a seed: all sprite identities, positions, effects and
        // lighting below are hashed too, including everything behind each effect.
        (std::sync::Arc::as_ptr(scene.palette.as_ref()?) as usize).hash(&mut hash);
        background.hash(&mut hash);
        Some(Self {
            scene: Scene {
                palette: scene.palette.clone(),
                ..Scene::default()
            },
            light: Vec::new(),
            hash,
            background,
        })
    }

    pub(crate) fn push(&mut self, item: DrawItem, light: f32, anim_key: u64) {
        (
            item.sprite.id,
            item.x,
            item.y,
            item.alpha,
            item.blend,
            light.to_bits(),
            anim_key,
        )
            .hash(&mut self.hash);
        self.scene.items.push(item);
        self.light.push(light);
    }

    pub(crate) fn patch(
        &self,
        cache: &mut EffectsCache,
        ctx: &egui::Context,
        anim: fo_scene::AnimAt<'_>,
    ) -> egui::TextureId {
        let key = self.hash.finish();
        if let Some((tex, used)) = cache.textures.get_mut(&key) {
            *used = ctx.cumulative_frame_nr();
            return tex.id();
        }
        let item = self.scene.items.last().unwrap();
        let s = &item.sprite;
        let mut n = 0;
        let mut image = fo_scene::render_shaded(
            &self.scene,
            LayerSet::all(),
            (item.x, item.y, s.width as i32, s.height as i32),
            1,
            anim,
            self.background,
            &mut |_| {
                let light = self.light[n];
                n += 1;
                Some(light)
            },
        );
        // Outside the sprite's actual silhouette retain the existing GPU image.
        for (pixel, mask) in image.data.chunks_exact_mut(4).zip(s.rgba.chunks_exact(4)) {
            pixel[3] = mask[3];
        }
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [s.width as usize, s.height as usize],
            &image.data,
        );
        let tex = ctx.load_texture(
            format!("palette-effect-{key}"),
            image,
            TextureOptions::NEAREST,
        );
        let id = tex.id();
        cache.textures.insert(key, (tex, ctx.cumulative_frame_nr()));
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fo_core::{Palette, Sprite, palette_blend::Effect};
    use fo_scene::Layer;
    use std::sync::Arc;

    fn floor() -> DrawItem {
        DrawItem {
            layer: Layer::Floor,
            sprite: Arc::new(Sprite {
                id: 1,
                width: 1,
                height: 1,
                origin_x: 0,
                origin_y: 0,
                rgba: vec![50, 50, 50, 255],
                indices: vec![50],
                anim_mask: 0,
            }),
            x: 0,
            y: 0,
            alpha: 255,
            blend: Blend::Normal,
            object: None,
            tile: 0,
        }
    }

    #[test]
    fn composite_cache_accounts_for_background_layers_light_position_and_animation() {
        let mut effect = floor();
        effect.blend = Blend::Palette(Effect::Glass);
        let scene = Scene {
            palette: Some(Arc::new(Palette::grayscale())),
            items: vec![effect.clone()],
            ..Scene::default()
        };
        let key = |background: Option<DrawItem>, light, animation| {
            let mut frame = EffectsFrame::new(&scene, [14, 14, 16, 255]).unwrap();
            if let Some(item) = background {
                frame.push(item, 1.0, animation);
            }
            frame.push(effect.clone(), light, 0);
            frame.hash.finish()
        };
        let normal = key(Some(floor()), 1.0, 0);
        assert_eq!(normal, key(Some(floor()), 1.0, 0));
        assert_ne!(normal, key(None, 1.0, 0));
        assert_ne!(normal, key(Some(floor()), 0.5, 0));
        assert_ne!(normal, key(Some(floor()), 1.0, 1));
        let mut moved = floor();
        moved.x = 1;
        assert_ne!(normal, key(Some(moved), 1.0, 0));
    }
}
