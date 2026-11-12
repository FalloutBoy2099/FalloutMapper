use crate::{app::MapperApp, i18n::t};
use eframe::egui;
use fo_core::{
    MapObject, geom,
    map::{ExitDestination, exit},
};
use std::sync::mpsc::{self, Receiver};

#[derive(Clone)]
struct Link {
    source: String,
    number: i32,
    elevation: usize,
    hex: u32,
    direction: u8,
    dest: ExitDestination,
    target: Option<String>,
    problem: Option<String>,
}
#[derive(Clone)]
struct ReturnDraft {
    link: Link,
    exit_tile: i32,
    arrival: i32,
}
#[derive(Default)]
pub(crate) struct LinksState {
    pub open: bool,
    graph: Vec<Link>,
    receiver: Option<Receiver<Vec<Link>>>,
    draft: Option<ReturnDraft>,
    pending: Option<ReturnDraft>,
    root: String,
    scanned: bool,
}

fn map_links(file: &str, map: &fo_core::Map, maps: &[fo_core::MapEntry]) -> Vec<Link> {
    let number = maps
        .iter()
        .find(|m| m.file == file)
        .and_then(|m| m.number)
        .map_or(-1, |n| n as i32);
    map.objects
        .iter()
        .enumerate()
        .flat_map(|(e, objects)| {
            objects.iter().filter_map(move |o| {
                let dest = o.exit_destination()?;
                if dest.map < 0 || o.tile() < 0 {
                    return None;
                }
                let target = maps
                    .iter()
                    .find(|m| m.number == Some(dest.map as u32))
                    .map(|m| m.file.clone());
                let problem = if target.is_none() {
                    Some("Carte de destination introuvable".into())
                } else if !(0..40000).contains(&dest.tile) || !(0..3).contains(&dest.elevation) {
                    Some("Point d'arrivée invalide".into())
                } else {
                    None
                };
                Some(Link {
                    source: file.into(),
                    number,
                    elevation: e,
                    hex: o.tile() as u32,
                    direction: o.exit_direction().unwrap_or(0),
                    dest,
                    target,
                    problem,
                })
            })
        })
        .collect()
}

impl MapperApp {
    fn refresh_links(&mut self, ctx: &egui::Context) {
        let Some(game) = &self.game else { return };
        let data = game.data.clone();
        let maps = game.maps.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.links.receiver = Some(rx);
        self.links.scanned = true;
        std::thread::spawn(move || {
            let mut links = Vec::new();
            let mut loaded = std::collections::HashMap::new();
            for entry in &maps {
                if let Ok(bytes) = data.read_map_bytes(&entry.file) {
                    if let Ok(map) = fo_core::read_map(&bytes, &*data) {
                        links.extend(map_links(&entry.file, &map, &maps));
                        loaded.insert(entry.file.clone(), map);
                    }
                }
            }
            for link in &mut links {
                if link.problem.is_some() {
                    continue;
                }
                if let Some(map) = link.target.as_ref().and_then(|file| loaded.get(file)) {
                    let e = link.dest.elevation as usize;
                    if !map.header.has_elevation(e) {
                        link.problem = Some("Élévation de destination absente".into());
                    } else if fo_core::reach::walk_map(map, e, &data).blocked
                        [link.dest.tile as usize]
                    {
                        link.problem = Some("Hexagone d'arrivée bloqué".into());
                    } else if map.objects[e]
                        .iter()
                        .any(|o| o.tile() == link.dest.tile && o.exit_destination().is_some())
                    {
                        link.problem =
                            Some("L'arrivée est sur une autre sortie : risque de boucle".into());
                    }
                } else {
                    link.problem = Some("Carte de destination illisible".into());
                }
            }
            let _ = tx.send(links);
            ctx.request_repaint();
        });
    }

    pub(crate) fn links_window(&mut self, ctx: &egui::Context) {
        if let Some(game) = &self.game {
            let root = game.data.root.display().to_string();
            if self.links.root != root {
                let open = self.links.open;
                self.links = LinksState {
                    open,
                    root,
                    ..Default::default()
                };
            }
        }
        if let Some(rx) = &self.links.receiver {
            if let Ok(links) = rx.try_recv() {
                self.links.graph = links;
                self.links.receiver = None;
            }
        }
        if let Some(pending) = self.links.pending.clone() {
            if self.loading.is_none()
                && self
                    .current
                    .as_ref()
                    .is_some_and(|c| Some(&c.file) == pending.link.target.as_ref())
            {
                self.links.pending = None;
                let e = pending.link.dest.elevation as usize;
                if e >= 3
                    || !(0..40000).contains(&pending.exit_tile)
                    || !(0..40000).contains(&pending.arrival)
                {
                    self.status = t("Point de liaison invalide.").into();
                    return;
                }
                let source = self
                    .docs
                    .iter()
                    .filter_map(|d| d.current.as_ref())
                    .find(|c| c.file == pending.link.source)
                    .map(|c| c.map.clone())
                    .or_else(|| {
                        self.game.as_ref().and_then(|g| {
                            g.data
                                .read_map_bytes(&pending.link.source)
                                .ok()
                                .and_then(|b| fo_core::read_map(&b, &*g.data).ok())
                                .map(std::sync::Arc::new)
                        })
                    });
                let safe = source
                    .as_ref()
                    .zip(self.game.as_ref())
                    .is_some_and(|(map, g)| {
                        map.header.has_elevation(pending.link.elevation)
                            && !fo_core::reach::walk_map(map, pending.link.elevation, &g.data)
                                .blocked[pending.arrival as usize]
                            && !map.objects[pending.link.elevation].iter().any(|o| {
                                o.tile() == pending.arrival && o.exit_destination().is_some()
                            })
                    });
                if !safe {
                    self.status=t("Retour non créé : l'arrivée sur la carte source est bloquée ou sur une sortie.").into();
                    return;
                }
                let blocked = self
                    .current
                    .as_ref()
                    .zip(self.game.as_ref())
                    .map(|(c, g)| {
                        fo_core::reach::walk_map(&c.map, e, &g.data).blocked
                            [pending.exit_tile as usize]
                    })
                    .unwrap_or(true);
                if blocked {
                    self.status = t("Retour non créé : cet hexagone est bloqué.").into();
                } else {
                    let dest = ExitDestination {
                        map: pending.link.number,
                        tile: pending.arrival,
                        elevation: pending.link.elevation as i32,
                        orientation: 0,
                    };
                    let made = self.mutate_as("Création d'une liaison retour", |map, _| {
                        if !map.header.has_elevation(e)
                            || map.objects[e].iter().any(|o| {
                                o.tile() == pending.exit_tile && o.exit_destination().is_some()
                            })
                        {
                            return false;
                        }
                        let obj = MapObject::exit_grid(
                            exit::flip_direction(pending.link.direction),
                            dest,
                            pending.exit_tile as u32,
                            map.max_object_id() + 1,
                        );
                        map.add_object(e, obj);
                        true
                    });
                    self.status=t(if made {"Retour créé dans la carte active ; Ctrl+S pour enregistrer, Ctrl+Z pour annuler."} else {"Retour non créé : élévation absente ou sortie déjà présente."}).into();
                }
            }
        }
        if !self.links.open {
            return;
        }
        if !self.links.scanned && self.links.receiver.is_none() {
            self.refresh_links(ctx);
        }
        let mut open = true;
        let mut refresh = false;
        let mut navigate = None;
        let mut prepare = None;
        let maps = self
            .game
            .as_ref()
            .map(|g| g.maps.clone())
            .unwrap_or_default();
        let active = self.current.as_ref().map(|c| c.file.clone());
        let mut outgoing = self
            .current
            .as_ref()
            .map(|c| map_links(&c.file, &c.map, &maps))
            .unwrap_or_default();
        for link in &mut outgoing {
            if let Some(cached) = self
                .links
                .graph
                .iter()
                .find(|p| p.source == link.source && p.hex == link.hex && p.dest == link.dest)
            {
                if link.problem.is_none() {
                    link.problem = cached.problem.clone();
                }
            }
        }
        let incoming: Vec<_> = self
            .links
            .graph
            .iter()
            .filter(|l| l.target.as_ref() == active.as_ref())
            .cloned()
            .collect();
        egui::Window::new(t("Liaisons entre cartes")).open(&mut open).default_size([650.0,500.0]).show(ctx,|ui|{
            if ui.button(t("Actualiser les liaisons")).clicked(){refresh=true;}
            if self.links.receiver.is_some(){ui.spinner();}
            ui.weak(t("Après enregistrement des cartes, actualisez pour vérifier les destinations et les retours."));
            ui.label(format!("{} : {}",t("Carte active"),active.as_deref().unwrap_or("—")));
            egui::ScrollArea::vertical().max_height(290.0).show(ui,|ui|{
                for (heading,links) in [("Sorties",&outgoing),("Entrées depuis d'autres cartes",&incoming)] {
                    ui.heading(t(heading));
                    for link in links {
                        ui.group(|ui|{
                            ui.label(format!("{} [{}:{}] → {} [{}:{}]",link.source,link.elevation,link.hex,link.target.as_deref().unwrap_or("?"),link.dest.elevation,link.dest.tile));
                            if let Some(problem)=&link.problem{ui.colored_label(egui::Color32::LIGHT_RED,problem);}
                            ui.horizontal(|ui|{
                                if ui.button(t("Ouvrir la source")).clicked(){navigate=Some(link.source.clone());}
                                if let Some(target)=&link.target {
                                    if ui.button(t("Ouvrir la destination")).clicked(){navigate=Some(target.clone());}
                                    let reciprocal=self.links.graph.iter().any(|back|back.source==*target&&back.dest.map==link.number);
                                    ui.label(t(if reciprocal {"Retour présent"}else{"Retour absent"}));
                                    if ui.add_enabled(link.problem.is_none()&&link.number>=0,egui::Button::new(t("Créer le retour"))).clicked(){prepare=Some(link.clone());}
                                }
                            });
                        });
                    }
                }
            });
            if let Some(draft)=&mut self.links.draft {
                ui.separator();ui.label(format!("{} → {}",draft.link.target.as_deref().unwrap_or("?"),draft.link.source));
                ui.label(t("Hexagone de la sortie retour sur la carte destination"));
                ui.add(crate::stepper::Stepper::new(&mut draft.exit_tile).range(0..=39999));
                ui.label(t("Hexagone d'arrivée sur la carte source"));
                ui.add(crate::stepper::Stepper::new(&mut draft.arrival).range(0..=39999));
                if ui.button(t("Ouvrir la destination et créer le retour")).clicked(){self.links.pending=Some(draft.clone());navigate=draft.link.target.clone();}
            }
        });
        self.links.open = open;
        if refresh {
            self.refresh_links(ctx);
        }
        if let Some(link) = prepare {
            let exit_tile = geom::hex_neighbors(link.dest.tile.max(0) as u32)
                .into_iter()
                .next()
                .unwrap_or(link.dest.tile.max(0) as u32) as i32;
            let arrival = geom::hex_neighbors(link.hex)
                .into_iter()
                .next()
                .unwrap_or(link.hex) as i32;
            self.links.draft = Some(ReturnDraft {
                link,
                exit_tile,
                arrival,
            });
        }
        if let Some(file) = navigate {
            self.mode = crate::app::Mode::Map;
            self.request_load(&file, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exits_are_resolved_by_engine_number_not_filename_or_header() {
        let maps = vec![
            fo_core::MapEntry {
                file: "source".into(),
                number: Some(7),
                display: None,
                group: None,
                group_name: None,
                place: None,
            },
            fo_core::MapEntry {
                file: "target".into(),
                number: Some(99),
                display: None,
                group: None,
                group_name: None,
                place: None,
            },
        ];
        let mut map = fo_core::Map::new_empty("source", 1);
        map.header.map_number = 123;
        map.add_object(
            0,
            MapObject::exit_grid(
                0,
                ExitDestination {
                    map: 99,
                    tile: 20050,
                    elevation: 0,
                    orientation: 0,
                },
                20060,
                1,
            ),
        );
        let links = map_links("source", &map, &maps);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].number, 7);
        assert_eq!(links[0].target.as_deref(), Some("target"));
        assert!(links[0].problem.is_none());
        assert_eq!(map_links("unregistered", &map, &maps)[0].number, -1);
    }
    #[test]
    fn invalid_destinations_are_reported_and_world_exits_are_ignored() {
        let mut map = fo_core::Map::new_empty("source", 1);
        for (id, dest) in [
            ExitDestination {
                map: 99,
                tile: 40000,
                elevation: 3,
                orientation: 0,
            },
            ExitDestination {
                map: exit::WORLD_MAP_EXIT,
                tile: 0,
                elevation: 0,
                orientation: 0,
            },
        ]
        .into_iter()
        .enumerate()
        {
            map.add_object(0, MapObject::exit_grid(0, dest, 20050, id as i32));
        }
        let links = map_links("source", &map, &[]);
        assert_eq!(links.len(), 1);
        assert!(links[0].problem.is_some());
    }
}
