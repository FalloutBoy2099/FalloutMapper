//! Bibliothèque de motifs : tous les tampons de `data/stamps/` avec une miniature, des étiquettes (maison, entrepôt, bureau…),
//! un filtre, et le glisser-déposer d'une miniature sur la carte pour poser le motif.

#![allow(deprecated)]

use crate::app::MapperApp;
use crate::i18n::t;
use eframe::egui::{
    self, Color32, ColorImage, Pos2, Rect, Sense, TextureHandle, TextureOptions, Vec2,
};
use fo_core::pattern::{Pattern, PatternVariant};
use fo_core::{ArtLibrary, Fid, geom};
use std::collections::{BTreeSet, HashMap};

/// Boîte des miniatures, en pixels.
const THUMB: Vec2 = Vec2::new(132.0, 96.0);
/// Étiquettes proposées d'un clic.
const SUGGESTED_TAGS: [&str; 7] = [
    "maison",
    "entrepôt",
    "bureau",
    "bâtiment",
    "mur",
    "décor",
    "végétation",
];

#[derive(Default)]
pub(crate) struct LibraryState {
    pub open: bool,
    filter: String,
    /// Étiquette qui filtre la liste ; `None` : tous les motifs.
    tag: Option<String>,
    new_tag: String,
    thumbs: HashMap<String, Option<TextureHandle>>,
    /// Motif en cours de glissement (nom).
    drag: Option<String>,
}

/// Un motif correspond au filtre (nom ou étiquette contenant le texte) et à l'étiquette choisie.
pub(crate) fn matches_filter(p: &Pattern, filter: &str, tag: Option<&str>) -> bool {
    let f = filter.trim().to_lowercase();
    (f.is_empty()
        || p.name.to_lowercase().contains(&f)
        || p.tags.iter().any(|g| g.to_lowercase().contains(&f)))
        && tag.is_none_or(|t| p.tags.iter().any(|g| g.eq_ignore_ascii_case(t)))
}

/// Ajoute une étiquette (sans doublon, sans espaces autour) ; renvoie vrai si elle est nouvelle.
pub(crate) fn add_tag(p: &mut Pattern, tag: &str) -> bool {
    let tag = tag.trim();
    if tag.is_empty() || p.tags.iter().any(|g| g.eq_ignore_ascii_case(tag)) {
        return false;
    }
    p.tags.push(tag.to_string());
    true
}

/// Toutes les étiquettes de la bibliothèque, triées sans doublon.
pub(crate) fn all_tags(patterns: &[Pattern]) -> Vec<String> {
    let set: BTreeSet<String> = patterns
        .iter()
        .flat_map(|p| p.tags.iter().map(|g| g.to_lowercase()))
        .collect();
    set.into_iter().collect()
}

/// Clé du cache de miniature : change quand le contenu du motif change.
fn thumb_key(p: &Pattern) -> String {
    let v = &p.variants[0];
    format!(
        "{}|{}|{}|{}|{}",
        p.name,
        v.anchor_hex,
        v.objects.len(),
        v.floor.len(),
        v.roof.len()
    )
}

/// Dessine la première variante en vue de dessus (sols, objets, toits à demi transparents), réduite pour tenir dans `THUMB`.
fn render_thumbnail(art: &ArtLibrary, v: &PatternVariant) -> Option<ColorImage> {
    struct Piece {
        x: i32,
        y: i32,
        sprite: std::sync::Arc<fo_core::Sprite>,
        alpha: u32,
        order: (u8, i32, i32),
    }
    let plan = v.plan(v.anchor_hex);
    let mut pieces: Vec<Piece> = Vec::new();
    for t in &plan.tiles {
        let Some(sprite) = art.tile_sprite(t.tile_id as usize) else {
            continue;
        };
        let (x, y) = geom::square_to_screen(t.square);
        let dy = if t.roof { geom::ROOF_OFFSET_Y } else { 0 };
        pieces.push(Piece {
            x,
            y: y + dy,
            sprite,
            alpha: if t.roof { 140 } else { 255 },
            order: (if t.roof { 2 } else { 0 }, y, x),
        });
    }
    for o in &plan.objects {
        let Some((_, sprite)) = art.object_sprite(Fid(o.frm_pid), o.direction, 0) else {
            continue;
        };
        let (cx, cy) = geom::hex_to_screen(o.hex);
        pieces.push(Piece {
            x: cx + sprite.origin_x,
            y: cy + sprite.origin_y,
            sprite,
            alpha: 255,
            order: (1, cy, cx),
        });
    }
    if pieces.is_empty() {
        return None;
    }
    pieces.sort_by_key(|p| p.order);
    let min_x = pieces.iter().map(|p| p.x).min()?;
    let min_y = pieces.iter().map(|p| p.y).min()?;
    let max_x = pieces.iter().map(|p| p.x + p.sprite.width as i32).max()?;
    let max_y = pieces.iter().map(|p| p.y + p.sprite.height as i32).max()?;
    let (w, h) = ((max_x - min_x).max(1) as f32, (max_y - min_y).max(1) as f32);
    let s = (THUMB.x / w).min(THUMB.y / h).min(1.0);
    let (tw, th) = (
        ((w * s).ceil() as usize).max(1),
        ((h * s).ceil() as usize).max(1),
    );
    let mut rgba = vec![0u8; tw * th * 4];
    for p in &pieces {
        let (sw, sh) = (p.sprite.width as i32, p.sprite.height as i32);
        let (ox, oy) = (
            ((p.x - min_x) as f32 * s) as i32,
            ((p.y - min_y) as f32 * s) as i32,
        );
        let (dw, dh) = (
            ((sw as f32 * s).ceil() as i32).max(1),
            ((sh as f32 * s).ceil() as i32).max(1),
        );
        for dy in 0..dh {
            let ty = oy + dy;
            if ty < 0 || ty >= th as i32 {
                continue;
            }
            for dx in 0..dw {
                let tx = ox + dx;
                if tx < 0 || tx >= tw as i32 {
                    continue;
                }
                let (sx, sy) = (
                    ((dx as f32 + 0.5) / s) as i32,
                    ((dy as f32 + 0.5) / s) as i32,
                );
                if sx >= sw || sy >= sh {
                    continue;
                }
                let si = (sy as usize * sw as usize + sx as usize) * 4;
                let a = p.sprite.rgba[si + 3] as u32 * p.alpha / 255;
                if a == 0 {
                    continue;
                }
                let di = (ty as usize * tw + tx as usize) * 4;
                for c in 0..3 {
                    rgba[di + c] = ((p.sprite.rgba[si + c] as u32 * a
                        + rgba[di + c] as u32 * (255 - a))
                        / 255) as u8;
                }
                rgba[di + 3] = (a + rgba[di + 3] as u32 * (255 - a) / 255).min(255) as u8;
            }
        }
    }
    Some(ColorImage::from_rgba_unmultiplied([tw, th], &rgba))
}

impl MapperApp {
    fn thumbnail(&mut self, ctx: &egui::Context, index: usize) -> Option<egui::TextureId> {
        let p = self.edit.stamp.patterns.get(index)?;
        let key = thumb_key(p);
        if !self.library.thumbs.contains_key(&key) {
            let art = self.game.as_ref()?.art.clone();
            let tex = render_thumbnail(&art, &p.variants[0])
                .map(|img| ctx.load_texture(format!("thumb_{key}"), img, TextureOptions::LINEAR));
            self.library.thumbs.insert(key.clone(), tex);
        }
        self.library.thumbs.get(&key)?.as_ref().map(|t| t.id())
    }

    /// Hexagone de la carte sous un point de l'écran (hors fenêtres : seulement si le point est sur le canevas).
    fn drop_hex(&self, ctx: &egui::Context, p: Pos2) -> Option<u32> {
        let on_canvas = ctx
            .layer_id_at(p)
            .is_none_or(|l| l.order == egui::Order::Background)
            && self.canvas_rect.contains(p);
        let w = self.camera.screen_to_world(self.canvas_rect, p);
        on_canvas
            .then(|| geom::screen_to_hex(w.x as i32, w.y as i32))
            .flatten()
    }

    pub(crate) fn library_window(&mut self, ctx: &egui::Context) {
        if !self.library.open {
            return;
        }
        self.ensure_patterns();
        let mut open = true;
        let mut select = None;
        let mut save = None;
        egui::Window::new(t("Bibliothèque de motifs")).open(&mut open).default_size([560.0, 520.0]).default_pos([60.0, 80.0]).show(ctx, |ui| {
            let tags = all_tags(&self.edit.stamp.patterns);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.library.filter).hint_text(t("Filtrer par nom ou étiquette…")).desired_width(220.0));
                if ui.selectable_label(self.library.tag.is_none(), t("Tous")).clicked() {
                    self.library.tag = None;
                }
            });
            if !tags.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for g in &tags {
                        let on = self.library.tag.as_deref() == Some(g.as_str());
                        if ui.selectable_label(on, g).clicked() {
                            self.library.tag = if on { None } else { Some(g.clone()) };
                        }
                    }
                });
            }
            ui.separator();
            let selected = self.edit.stamp.selected;
            let visible: Vec<usize> = self.edit.stamp.patterns.iter().enumerate().filter(|(_, p)| matches_filter(p, &self.library.filter, self.library.tag.as_deref())).map(|(i, _)| i).collect();
            if visible.is_empty() {
                ui.weak(t("Aucun motif. Capturez une sélection avec l'outil Tampon, ou gardez une pièce construite."));
            }
            let list_height = (ui.available_height() - 120.0).max(160.0);
            egui::ScrollArea::vertical().max_height(list_height).auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for &i in &visible {
                        let tex = self.thumbnail(ctx, i);
                        let name = self.edit.stamp.patterns[i].name.clone();
                        ui.vertical(|ui| {
                            ui.set_width(THUMB.x + 8.0);
                            let (rect, resp) = ui.allocate_exact_size(THUMB + Vec2::splat(8.0), Sense::click_and_drag());
                            let bg = if selected == Some(i) { Color32::from_rgb(40, 80, 120) } else { ui.visuals().extreme_bg_color };
                            ui.painter().rect_filled(rect, 4.0, bg);
                            if let Some(tex) = tex {
                                ui.painter().image(tex, Rect::from_center_size(rect.center(), self.thumb_size(&name)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            } else {
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(24.0), Color32::GRAY);
                            }
                            ui.label(egui::RichText::new(&name).small());
                            if resp.clicked() {
                                select = Some(i);
                            }
                            if resp.drag_started() {
                                self.library.drag = Some(name.clone());
                                select = Some(i);
                            }
                            resp.on_hover_text(t("Glissez sur la carte pour poser le motif"));
                        });
                    }
                });
            });
            ui.separator();
            if let Some(i) = selected.filter(|&i| i < self.edit.stamp.patterns.len()) {
                ui.strong(&self.edit.stamp.patterns[i].name);
                let mut remove = None;
                ui.horizontal_wrapped(|ui| {
                    ui.label(t("Étiquettes"));
                    for (n, g) in self.edit.stamp.patterns[i].tags.iter().enumerate() {
                        if ui.small_button(format!("{g} ✕")).clicked() {
                            remove = Some(n);
                        }
                    }
                });
                let mut add: Option<String> = None;
                ui.horizontal_wrapped(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.library.new_tag).hint_text(t("nouvelle étiquette")).desired_width(130.0));
                    if (ui.button("+").clicked() || (r.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter)))) && !self.library.new_tag.trim().is_empty() {
                        add = Some(std::mem::take(&mut self.library.new_tag));
                    }
                    for s in SUGGESTED_TAGS {
                        if !self.edit.stamp.patterns[i].tags.iter().any(|g| g.eq_ignore_ascii_case(t(s))) && ui.small_button(t(s)).clicked() {
                            add = Some(t(s).to_string());
                        }
                    }
                });
                if let Some(n) = remove {
                    self.edit.stamp.patterns[i].tags.remove(n);
                    save = Some(i);
                }
                if let Some(tag) = add
                    && add_tag(&mut self.edit.stamp.patterns[i], &tag)
                {
                    save = Some(i);
                }
            }
        });
        if let Some(i) = select {
            self.edit.stamp.selected = Some(i);
            self.edit.stamp.variant = 0;
        }
        if let Some(i) = save {
            self.save_pattern_to_library(i);
        }
        self.library_drag(ctx);
        if !open {
            self.library.open = false;
        }
    }

    /// Taille d'affichage de la miniature d'un motif (elle garde ses proportions dans la boîte).
    fn thumb_size(&self, name: &str) -> Vec2 {
        let key = self
            .edit
            .stamp
            .patterns
            .iter()
            .find(|p| p.name == name)
            .map(thumb_key);
        match key
            .and_then(|k| self.library.thumbs.get(&k))
            .and_then(|t| t.as_ref())
        {
            Some(t) => t.size_vec2(),
            None => THUMB,
        }
    }

    /// Glisser-déposer : fantôme sous le curseur, pose du motif au relâchement s'il est au-dessus de la carte.
    fn library_drag(&mut self, ctx: &egui::Context) {
        let Some(name) = self.library.drag.clone() else {
            return;
        };
        let Some(pos) = ctx.pointer_latest_pos() else {
            self.library.drag = None;
            return;
        };
        if ctx.input(|i| i.pointer.any_released()) {
            self.library.drag = None;
            if let Some(hex) = self.drop_hex(ctx, pos) {
                if self.edit.enabled && self.mode_is_map() {
                    if let Some(i) = self.edit.stamp.patterns.iter().position(|p| p.name == name) {
                        self.edit.stamp.selected = Some(i);
                        self.edit.stamp.variant = 0;
                        self.stamp_at(hex);
                    }
                } else {
                    self.status = t("Activez le mode édition (menu Édition) pour poser un motif.")
                        .to_string();
                }
            }
            return;
        }
        let ghost = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("library_ghost"));
        let painter = ctx.layer_painter(ghost);
        let key = self
            .edit
            .stamp
            .patterns
            .iter()
            .find(|p| p.name == name)
            .map(thumb_key);
        if let Some(Some(tex)) = key.and_then(|k| self.library.thumbs.get(&k)) {
            painter.image(
                tex.id(),
                Rect::from_center_size(pos, tex.size_vec2().min(THUMB)),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::from_white_alpha(170),
            );
        }
        if let Some(hex) = self.drop_hex(ctx, pos) {
            let (x, y) = geom::hex_to_screen(hex);
            let c = self
                .camera
                .world_to_screen(self.canvas_rect, Pos2::new(x as f32, y as f32));
            painter.circle_stroke(
                c,
                7.0,
                egui::Stroke::new(2.0, Color32::from_rgb(255, 200, 80)),
            );
        }
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        ctx.request_repaint();
    }

    fn mode_is_map(&self) -> bool {
        self.mode == crate::app::Mode::Map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(name: &str, tags: &[&str]) -> Pattern {
        let v = PatternVariant {
            label: "défaut".into(),
            anchor_hex: 0,
            objects: vec![],
            floor: vec![],
            roof: vec![],
        };
        let mut p = Pattern::new(name, v);
        p.tags = tags.iter().map(|s| s.to_string()).collect();
        p
    }

    #[test]
    fn filter_matches_name_or_tag_and_the_chosen_tag() {
        let p = pattern("Grande Maison", &["maison", "bâtiment"]);
        assert!(matches_filter(&p, "", None));
        assert!(matches_filter(&p, "maison", None) && matches_filter(&p, "BÂTI", None));
        assert!(!matches_filter(&p, "entrepôt", None));
        assert!(matches_filter(&p, "", Some("Maison")));
        assert!(!matches_filter(&p, "", Some("bureau")));
        assert!(!matches_filter(&pattern("x", &[]), "", Some("maison")));
    }

    #[test]
    fn tags_are_added_once_and_listed_sorted() {
        let mut p = pattern("a", &["Maison"]);
        assert!(!add_tag(&mut p, " maison "));
        assert!(!add_tag(&mut p, "  "));
        assert!(add_tag(&mut p, "bureau"));
        assert_eq!(p.tags, ["Maison", "bureau"]);
        assert_eq!(
            all_tags(&[p, pattern("b", &["MAISON", "abri"])]),
            ["abri", "bureau", "maison"]
        );
    }

    #[test]
    fn tags_survive_json_and_are_optional() {
        let p = pattern("a", &["maison"]);
        assert_eq!(
            Pattern::from_json(&p.to_json().unwrap()).unwrap().tags,
            ["maison"]
        );
        assert!(
            !pattern("b", &[]).to_json().unwrap().contains("tags"),
            "un motif sans étiquette reste au format Gecko"
        );
        assert!(
            Pattern::from_json(r#"{"name":"g","variants":[{"label":"d","anchorHex":0}]}"#)
                .unwrap()
                .tags
                .is_empty()
        );
    }
}
