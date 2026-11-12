use crate::scene::{Blend, DrawItem, LayerSet, Scene};
use fo_core::{Palette, PaletteAnim};
use std::borrow::Cow;

/// Image RGBA 8 bits, lignes de haut en bas.
#[derive(Debug, Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, background: [u8; 4]) -> Self {
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..(width as usize * height as usize) {
            data.extend_from_slice(&background);
        }
        Self {
            width,
            height,
            data,
        }
    }
}

/// Animation de palette à appliquer au rendu : (animation, palette, instant en ms).
pub type AnimAt<'a> = Option<(&'a PaletteAnim, &'a Palette, u64)>;

/// Rend la zone `(x, y, w, h)` du monde à l'échelle 1:`downscale` (1 = taille réelle).
pub fn render(
    scene: &Scene,
    layers: LayerSet,
    region: (i32, i32, i32, i32),
    downscale: u32,
    anim: AnimAt,
) -> RgbaImage {
    render_shaded(
        scene,
        layers,
        region,
        downscale,
        anim,
        [12, 12, 14, 255],
        &mut |_| Some(1.0),
    )
}

/// Comme `render`, avec un fond au choix et un ombrage par élément : `shade` renvoie `None` pour sauter l'élément, ou la lumière reçue
/// (0 à 1). Les effets utilisent les tables de palette et d’intensité du jeu.
pub fn render_shaded(
    scene: &Scene,
    layers: LayerSet,
    region: (i32, i32, i32, i32),
    downscale: u32,
    anim: AnimAt,
    background: [u8; 4],
    shade: &mut dyn FnMut(&DrawItem) -> Option<f32>,
) -> RgbaImage {
    let ds = downscale.max(1) as i32;
    let (rx, ry, rw, rh) = region;
    let (w, h) = ((rw / ds).max(1) as u32, (rh / ds).max(1) as u32);
    let mut img = RgbaImage::new(w, h, background);

    let palette = scene.palette.as_deref();
    let tables = palette.map(Palette::tables);
    let mut indices = tables.map(|t| vec![t.nearest(background); w as usize * h as usize]);
    let display_palette = palette.map(|p| match anim {
        Some((a, _, t)) => a.render(&(0..=255).collect::<Vec<u8>>(), p, t),
        None => p.colors.iter().flatten().copied().collect(),
    });
    for item in scene.items.iter().filter(|i| layers.contains(i.layer)) {
        let Some(light) = shade(item) else { continue };
        let light = light.clamp(0.0, 1.0);
        let s = &item.sprite;
        let (ix, iy) = (item.x - rx, item.y - ry);
        if ix >= rw || iy >= rh || ix + s.width as i32 <= 0 || iy + s.height as i32 <= 0 {
            continue;
        }
        let pixels: Cow<[u8]> = match anim {
            Some((a, pal, t)) if s.anim_mask != 0 => Cow::Owned(a.render(&s.indices, pal, t)),
            _ => Cow::Borrowed(&s.rgba),
        };
        let alpha = item.alpha as u32;
        for sy in (0..s.height as i32).step_by(ds as usize) {
            let dy = (iy + sy) / ds;
            if iy + sy < 0 || dy >= h as i32 {
                continue;
            }
            for sx in (0..s.width as i32).step_by(ds as usize) {
                let dx = (ix + sx) / ds;
                if ix + sx < 0 || dx >= w as i32 {
                    continue;
                }
                let si = (sy as usize * s.width as usize + sx as usize) * 4;
                if pixels[si + 3] == 0 {
                    continue;
                }
                let di = (dy as usize * w as usize + dx as usize) * 4;
                if let (Some(tables), Some(indices), Some(colors)) =
                    (tables, indices.as_mut(), display_palette.as_ref())
                {
                    let pi = di / 4;
                    let source =
                        s.indices.get(si / 4).copied().unwrap_or_else(|| {
                            tables.nearest(pixels[si..si + 4].try_into().unwrap())
                        });
                    let index = match item.blend {
                        Blend::Palette(effect) => tables.mix(effect, source, indices[pi], light),
                        Blend::Normal => tables.shade(source, light),
                    };
                    if alpha == 255 || matches!(item.blend, Blend::Palette(_)) {
                        indices[pi] = index;
                        img.data[di..di + 3]
                            .copy_from_slice(&colors[index as usize * 4..index as usize * 4 + 3]);
                    } else {
                        for c in 0..3 {
                            img.data[di + c] = ((colors[index as usize * 4 + c] as u32 * alpha
                                + img.data[di + c] as u32 * (255 - alpha))
                                / 255) as u8;
                        }
                        indices[pi] = tables.nearest(img.data[di..di + 4].try_into().unwrap());
                    }
                } else {
                    for c in 0..3 {
                        let src = (pixels[si + c] as f32 * light) as u32;
                        let dst = img.data[di + c] as u32;
                        img.data[di + c] = ((src * alpha + dst * (255 - alpha)) / 255) as u8;
                    }
                }
                img.data[di + 3] =
                    (alpha + img.data[di + 3] as u32 * (255 - alpha) / 255).min(255) as u8;
            }
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Blend, Layer};
    use fo_core::{Sprite, palette_blend::Effect};
    use std::sync::Arc;

    fn item(pal: &Palette, pixels: Vec<u8>, layer: Layer, blend: Blend) -> DrawItem {
        let sprite = Sprite {
            id: layer as u64,
            width: pixels.len() as u32,
            height: 1,
            origin_x: 0,
            origin_y: 0,
            rgba: pixels
                .iter()
                .flat_map(|&i| pal.colors[i as usize])
                .collect(),
            indices: pixels,
            anim_mask: 0,
        };
        DrawItem {
            layer,
            sprite: Arc::new(sprite),
            x: 0,
            y: 0,
            alpha: 255,
            blend,
            object: None,
            tile: 0,
        }
    }

    #[test]
    fn palette_effects_keep_holes_foreground_order_and_layer_visibility() {
        let palette = Arc::new(Palette::grayscale());
        let floor = item(&palette, vec![50; 3], Layer::Floor, Blend::Normal);
        let glass = item(
            &palette,
            vec![0, 128, 128],
            Layer::Scenery,
            Blend::Palette(Effect::Glass),
        );
        let foreground = item(&palette, vec![0, 0, 100], Layer::Wall, Blend::Normal);
        let scene = Scene {
            palette: Some(palette),
            items: vec![floor, glass, foreground],
            ..Scene::default()
        };
        let mut layers = LayerSet::all();
        let full = render(&scene, layers, (0, 0, 3, 1), 1, None);
        layers.set(Layer::Scenery, false);
        let without = render(&scene, layers, (0, 0, 3, 1), 1, None);
        assert_eq!(
            full.data[..4],
            without.data[..4],
            "source index zero leaves the background intact"
        );
        assert_ne!(
            full.data[4..8],
            without.data[4..8],
            "glass blends with the floor"
        );
        assert_eq!(
            full.data[8..12],
            without.data[8..12],
            "foreground is drawn over glass"
        );
        let dark = render_shaded(
            &scene,
            LayerSet::all(),
            (0, 0, 3, 1),
            1,
            None,
            [0, 0, 0, 255],
            &mut |_| Some(0.5),
        );
        assert_ne!(dark.data, full.data, "effects receive the map lighting");
        assert!(full.data.chunks_exact(4).all(|p| p[3] == 255));
    }
}
