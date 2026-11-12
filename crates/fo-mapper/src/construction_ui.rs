use crate::{app::MapperApp, i18n::t};
use eframe::egui;
use fo_core::{
    Map, build,
    room::{DecorOptions, plan_room_with},
};

impl MapperApp {
    pub(crate) fn begin_edit_building(&mut self) {
        if self.analysis.result.is_none() {
            self.status =
                t("Analysez d'abord toutes les cartes avant de modifier un bâtiment.").into();
            return;
        }
        self.refresh_room_models();
        let Some(object) = self.selection.primary().and_then(|r| self.object(r)) else {
            return;
        };
        if object.tile() < 0 {
            return;
        }
        let Some(current) = &self.current else { return };
        let Some(shell) =
            fo_core::construction::shell_at(&current.map, self.elevation, object.tile() as u32)
        else {
            self.status = t("Aucun bâtiment détecté autour de la sélection.").into();
            return;
        };
        let fid = current.map.objects[self.elevation]
            .iter()
            .find(|o| shell.owns_wall(o))
            .map(|o| o.fid());
        if let Some(style) = self
            .edit
            .room
            .styles()
            .iter()
            .find(|s| fid.is_some_and(|f| s.members.contains(&f)))
        {
            self.edit.room.style = style.label.clone();
        }
        self.edit.room.cols = shell.cols as i32;
        self.edit.room.rows = shell.rows as i32;
        self.edit.room.nx = 1;
        self.edit.room.ny = 1;
        self.edit.room.vary = false;
        self.edit.room.second_floor = false;
        self.edit.room.front = true;
        self.edit.room.edit_shell = Some(shell);
        self.edit.room.door_target = None;
        self.edit.room.picking_door = false;
        self.edit.set_tool(crate::edit_ui::Tool::Room);
        self.refresh_room_models();
    }

    pub(crate) fn edited_building_plan(
        &self,
        model: &fo_core::room::StyleModel,
    ) -> fo_core::room::RoomPlan {
        let Some(shell) = &self.edit.room.edit_shell else {
            return Default::default();
        };
        let r = &self.edit.room;
        let (c, row) = fo_core::geom::hex_col_row(shell.anchor);
        if c + r.nx.max(1) as u32 * r.cols.max(1) as u32 >= 200
            || row + r.ny.max(1) as u32 * (r.rows.max(1) as u32 + 1) >= 200
        {
            return fo_core::room::RoomPlan {
                short: true,
                ..Default::default()
            };
        }
        let mut plan = fo_core::room::plan_building(
            model,
            shell.anchor,
            r.nx.max(1) as u32,
            r.ny.max(1) as u32,
            r.rows as u32,
            r.cols as u32,
            &DecorOptions {
                front: true,
                door: r.door && r.door_target.is_none(),
                windows: r.windows.max(0) as u32,
                seed: r.seed,
                ..Default::default()
            },
        );
        if let Some(target) = r.door_target {
            if let Some(opened) = fo_core::room::opening_at(model, &plan.walls, target, false) {
                plan.walls = opened.walls;
                plan.doors.extend(opened.doors);
            } else {
                plan.short = true;
            }
        }
        plan
    }

    pub(crate) fn rebuild_selected_building(&mut self) {
        self.refresh_room_models();
        if let Some(previous) = self.edit.room.edit_shell.clone() {
            if previous.elevation != self.elevation {
                self.status = t("Revenez à l'élévation du bâtiment avant de le modifier.").into();
                return;
            }
            let detected = self.current.as_ref().and_then(|c| {
                fo_core::construction::shell_at(&c.map, previous.elevation, previous.anchor)
            });
            if detected.is_none() {
                self.status = t("Le bâtiment à modifier n'existe plus à cet emplacement.").into();
                return;
            }
            self.edit.room.edit_shell = detected;
        }
        let (Some(shell), Some(model)) = (self.edit.room.edit_shell.clone(), self.room_model())
        else {
            return;
        };
        let plan = self.edited_building_plan(&model);
        let Some(current) = &self.current else { return };
        let mut old_doors: Vec<_> = current.map.objects[shell.elevation]
            .iter()
            .filter(|o| {
                o.object_type() == Some(fo_core::ObjectType::Scenery)
                    && o.subtype == fo_core::formats::scenery_kind::DOOR
                    && o.tile() >= 0
                    && shell.contains(o.tile() as u32)
            })
            .cloned()
            .collect();
        old_doors.sort_by_key(|o| std::cmp::Reverse(o.script_id() >= 0));
        if old_doors.iter().filter(|o| o.script_id() >= 0).count() > plan.doors.len() {
            self.status=t("Des portes scriptées seraient supprimées : conservez autant de portes ou modifiez-les séparément.").into();
            return;
        }
        let mut changed = match fo_core::construction::reshape(&current.map, &shell, &plan) {
            Ok(map) => map,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let e = shell.elevation;
        let reused = old_doors.len().min(plan.doors.len());
        for (mut door, placement) in old_doors.into_iter().zip(&plan.doors) {
            use fo_core::map::word;
            let Some(pid) = model.pid_of(placement.fid) else {
                self.status = t("Prototype de porte introuvable.").into();
                return;
            };
            door.words[word::TILE] = placement.hex as i32;
            door.words[word::FID] = placement.fid as i32;
            door.words[word::PID] = pid as i32;
            door.words[word::ORIENTATION] = placement.orient as i32;
            door.words[word::OFFSET_X] = placement.offset.0;
            door.words[word::OFFSET_Y] = placement.offset.1;
            changed.add_object(e, door);
        }
        let mut additions = plan.clone();
        additions.doors.drain(..reused);
        let mut counts = Default::default();
        build::place_plan(
            &mut changed,
            &self.game.as_ref().unwrap().data,
            &model,
            e,
            &additions,
            &mut counts,
        );
        if counts.missing > 0 {
            self.status = t("Bâtiment inchangé : des prototypes sont introuvables.").into();
            return;
        }
        if self.edit.room.floor {
            let ids = self.room_floor_ids();
            build::fill_tiles(
                &mut changed,
                e,
                fo_core::map::TileLayer::Floor,
                &build::interior_squares(&plan.hexes),
                &ids,
                self.analysis
                    .stats(fo_core::map::TileLayer::Floor)
                    .map(|s| s.as_ref()),
                self.edit.room.seed,
            );
        }
        if self.edit.room.roof {
            let ids = self.room_roof_ids();
            build::fill_tiles(
                &mut changed,
                e,
                fo_core::map::TileLayer::Roof,
                &build::floor_squares(&plan.hexes),
                &ids,
                self.analysis
                    .stats(fo_core::map::TileLayer::Roof)
                    .map(|s| s.as_ref()),
                self.edit.room.seed,
            );
        }
        self.mutate_as("Modification d'un bâtiment", |map, _| {
            *map = changed;
            true
        });
        self.selection.clear();
        if let Some(cur) = &self.current {
            self.edit.room.edit_shell = fo_core::construction::shell_at(&cur.map, e, shell.anchor);
        }
        self.status = t("Bâtiment modifié ; mobilier conservé. Ctrl+Z pour annuler.").into();
    }

    pub(crate) fn construction_style_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(t("Style de bâtiment"));
        let styles = self.edit.room.styles().to_vec();
        let old = self.edit.room.style.clone();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("construction_style")
                .selected_text(&old)
                .show_ui(ui, |ui| {
                    for style in &styles {
                        ui.selectable_value(
                            &mut self.edit.room.style,
                            style.label.clone(),
                            &style.label,
                        );
                    }
                });
            let current = styles
                .iter()
                .position(|style| style.label == self.edit.room.style);
            // Précédent / suivant en texte, de la taille des boutons − / + des champs numériques ; la liste se parcourt en boucle.
            let height = ui.spacing().interact_size.y.max(26.0);
            let button_w = height * 1.15;
            let at = current.unwrap_or(0);
            if !styles.is_empty() {
                if ui.add_sized([button_w, height], egui::Button::new("<")).on_hover_text(t("Style précédent")).clicked() {
                    self.edit.room.style = styles[(at + styles.len() - 1) % styles.len()].label.clone();
                }
                if ui.add_sized([button_w, height], egui::Button::new(">")).on_hover_text(t("Style suivant")).clicked() {
                    self.edit.room.style = styles[(at + 1) % styles.len()].label.clone();
                }
            }
        });
        egui::CollapsingHeader::new(t("Palettes visuelles")).default_open(self.edit.room.palette_open).show(ui,|ui|{
            egui::ScrollArea::vertical().id_salt("building_palettes").max_height(330.0).show_rows(ui,108.0,styles.len(),|ui,range|{
                for index in range {
                    let style=&styles[index];
                    if !self.edit.room.palette_thumbs.contains_key(&style.label) {
                        let Some(res)=self.analysis.result.as_ref() else {continue};
                        let Some(game)=self.game.as_ref() else {continue};
                        let mut model=res.walls.model(&|f|style.members.contains(&f)).strong_only();
                        model.set_widths(&|f|super::room_ui::visible_width(&game.art,f));
                        let plan=plan_room_with(&model,20050,8,8,&DecorOptions{front:true,door:true,windows:1,..Default::default()});
                        let mut map=Map::new_empty("palette",1);
                        build::place_plan(&mut map,&game.data,&model,0,&plan,&mut Default::default());
                        let names=game.data.list("art/tiles/tiles.lst");
                        if let Some((_,ids))=model.floor_choice(&names) {build::fill_tiles(&mut map,0,fo_core::map::TileLayer::Floor,&build::interior_squares(&plan.hexes),&ids,self.analysis.stats(fo_core::map::TileLayer::Floor).map(|s|s.as_ref()),1);}
                        let scene=fo_scene::build_scene(&map,0,&game.art);
                        let objects:Vec<_>=scene.items.iter().filter(|p|p.object.is_some()).collect();
                        if objects.is_empty() {continue;}
                        let x=objects.iter().map(|p|p.x).min().unwrap()-8;
                        let y=objects.iter().map(|p|p.y).min().unwrap()-8;
                        let w=objects.iter().map(|p|p.x+p.sprite.width as i32).max().unwrap()-x+8;
                        let h=objects.iter().map(|p|p.y+p.sprite.height as i32).max().unwrap()-y+8;
                        let image=fo_scene::render_shaded(&scene,fo_scene::LayerSet::all(),(x,y,w,h),2,None,[26,26,26,255],&mut |_|Some(1.0));
                        let tex=ui.ctx().load_texture(format!("palette_{}",style.label),egui::ColorImage::from_rgba_unmultiplied([image.width as usize,image.height as usize],&image.data),egui::TextureOptions::LINEAR);
                        self.edit.room.palette_thumbs.insert(style.label.clone(),tex);
                    }
                    if let Some(tex)=self.edit.room.palette_thumbs.get(&style.label) {
                        let size=tex.size_vec2()*(140.0/tex.size_vec2().x.max(tex.size_vec2().y));
                        let button=egui::Button::image_and_text(egui::Image::new((tex.id(),size)),&style.label).selected(self.edit.room.style==style.label);
                        if ui.add_sized([ui.available_width(),102.0],button).on_hover_text(t("Murs, sol, portes et fenêtres appris ensemble dans les cartes du jeu.")).clicked() {self.edit.room.style=style.label.clone();}
                    }
                }
            });
        });
        if old != self.edit.room.style {
            self.edit.room.auto_floor = true;
            self.edit.room.auto_roof = true;
            self.refresh_room_models();
        }
        if let Some(model) = self.room_model() {
            let names = self.floor_names();
            let floor = model
                .floor_choice(&names)
                .map(|p| p.0)
                .unwrap_or_else(|| "—".into());
            let roof = model
                .roof_choice(&names)
                .map(|p| p.0)
                .unwrap_or_else(|| "—".into());
            ui.weak(format!("{} : {floor} · {} : {roof}", t("Sol"), t("Toit")));
            let mut swatches = Vec::new();
            if let Some(game) = &self.game {
                for (label, choice) in [
                    ("Sol", model.floor_choice(&names)),
                    ("Toit", model.roof_choice(&names)),
                ] {
                    if let Some((_, ids)) = choice
                        && let Some(sprite) = ids
                            .first()
                            .and_then(|&id| game.art.tile_sprite(id as usize))
                    {
                        swatches.push((label, sprite));
                    }
                }
                if let Some((fid, orientation)) = model.door_sample()
                    && let Some((_, sprite)) =
                        game.art
                            .object_sprite(fo_core::Fid(fid), orientation as u32, 0)
                {
                    swatches.push(("Porte", sprite));
                }
                if let Some(fid) = model.window_sample()
                    && let Some((_, sprite)) = game.art.object_sprite(fo_core::Fid(fid), 0, 0)
                {
                    swatches.push(("Fenêtre", sprite));
                }
            }
            ui.horizontal_wrapped(|ui| {
                for (label, sprite) in swatches {
                    ui.vertical(|ui| {
                        ui.small(t(label));
                        let tex = self.texture_for(ui.ctx(), &sprite);
                        let size = egui::vec2(sprite.width as f32, sprite.height as f32);
                        let size = size * (48.0 / size.max_elem());
                        ui.image((tex, size));
                    });
                }
            });
        }
    }
}
