use crate::i18n::t;
use eframe::egui::{self, Color32};
use fo_core::GameData;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
};

#[derive(Default)]
pub(crate) struct SourceEditor {
    path: String,
    text: String,
    target: String,
    compiler: String,
    includes: String,
    dirty: bool,
    log: String,
    receiver: Option<Receiver<Result<(String, String), String>>>,
    jump: Option<usize>,
    pub completed: Option<String>,
    register: bool,
    locals: i32,
    root: Option<PathBuf>,
    /// Analyse statique de la source (procédures, appels, variables, messages) et sélection dans son panneau.
    facts: Option<fo_script::ssl::ScriptFacts>,
    facts_stale: bool,
    facts_sel: Option<String>,
    gvar_query: String,
}

fn find_source(root: &Path, stem: &str) -> Option<PathBuf> {
    let mut pending = vec![
        (root.join("scripts_src"), 0),
        (root.join("scripts"), 0),
        (root.join("source"), 0),
    ];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && depth < 6 {
                pending.push((p, depth + 1));
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ssl"))
                && p.file_stem().is_some_and(|s| s.eq_ignore_ascii_case(stem))
            {
                return Some(p);
            }
        }
    }
    None
}
fn draft_path(root: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hash);
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("FalloutMapper/script-drafts");
    dir.join(format!("{:x}.txt", hash.finish()))
}
fn capture_details() -> bool {
    std::env::var_os("FO_SHOT").is_some() && std::env::var_os("FO_SHOT_SSL_DETAILS").is_some()
}
fn save_source(path: &Path, text: &str) -> Result<(), String> {
    if !path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("ssl"))
    {
        return Err("Le fichier source doit avoir l'extension .ssl.".into());
    }
    let bytes: Vec<_> = text
        .chars()
        .map(|c| {
            u8::try_from(c as u32).map_err(|_| format!("Caractère SSL non pris en charge : {c}"))
        })
        .collect::<Result<_, _>>()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let backup = path.with_extension("ssl.bak");
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

impl SourceEditor {
    /// Analyse la source actuelle : les inclusions sont cherchées à côté du fichier puis dans les dossiers d'inclusion indiqués.
    fn analyze(&mut self) {
        let mut dirs: Vec<PathBuf> = Path::new(&self.path)
            .parent()
            .map(Path::to_path_buf)
            .into_iter()
            .collect();
        dirs.extend(
            self.includes
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
        self.facts = Some(fo_script::ssl::analyze(
            &self.text,
            &fo_script::ssl::DirProvider { dirs },
        ));
        self.facts_stale = false;
    }

    /// Panneau d'analyse : procédures (clic : aller à la ligne), qui appelle quoi, variables et messages utilisés.
    fn analysis_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(t("Analyse du script")).default_open(capture_details()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("Analyser")).on_hover_text(t("Relit la source et ses inclusions : procédures, appels, variables, messages")).clicked() {
                    self.analyze();
                }
                if self.facts.is_some() && self.facts_stale {
                    ui.weak(t("(source modifiée depuis l'analyse)"));
                }
            });
            let Some(facts) = &self.facts else { return };
            if !facts.missing_includes.is_empty() {
                ui.colored_label(Color32::YELLOW, format!("{} {}", t("Inclusions introuvables :"), facts.missing_includes.join(", ")));
            }
            let unused = facts.unused().join(", ");
            if !unused.is_empty() {
                ui.label(format!("{} {unused}", t("Procédures jamais appelées :")));
            }
            if !facts.undefined.is_empty() {
                ui.colored_label(Color32::LIGHT_RED, format!("{} {}", t("Déclarées sans corps :"), facts.undefined.join(", ")));
            }
            let mut jump = None;
            let mut select = None;
            egui::ScrollArea::vertical().id_salt("ssl_procs").max_height(160.0).show(ui, |ui| {
                for p in facts.procedures.iter().filter(|p| p.file == 0) {
                    let text = format!("{} ({}) — {} {}", p.name, p.arg_count, t("ligne"), p.line);
                    if ui.selectable_label(self.facts_sel.as_deref() == Some(p.name.as_str()), text).clicked() {
                        select = Some(p.name.clone());
                        jump = Some(p.line as usize);
                    }
                }
            });
            if let Some(n) = select {
                self.facts_sel = Some(n);
            }
            if let Some(n) = jump {
                self.jump = Some(n);
            }
            if let Some(p) = self.facts_sel.as_deref().and_then(|n| facts.find(n)) {
                ui.separator();
                let join = |v: &std::collections::BTreeSet<i64>| v.iter().map(i64::to_string).collect::<Vec<_>>().join(", ");
                let callers = facts.callers_of(&p.name);
                let rows: Vec<(String, String)> = vec![
                    (t("Appelée par").into(), callers.join(", ")),
                    (t("Appelle").into(), p.calls.iter().cloned().collect::<Vec<_>>().join(", ")),
                    (t("Variables globales lues").into(), join(&p.gvar_read)),
                    (t("Variables globales écrites").into(), join(&p.gvar_write)),
                    (t("Variables de carte lues").into(), join(&p.mvar_read)),
                    (t("Variables de carte écrites").into(), join(&p.mvar_write)),
                    (t("Variables locales lues").into(), join(&p.lvar_read)),
                    (t("Variables locales écrites").into(), join(&p.lvar_write)),
                    (t("Messages").into(), join(&p.messages)),
                    (t("Prototypes créés").into(), join(&p.created)),
                ];
                egui::Grid::new("ssl_facts").num_columns(2).max_col_width((ui.available_width() * 0.60).max(120.0)).show(ui, |ui| {
                    for (k, v) in rows.into_iter().filter(|(_, v)| !v.is_empty()) {
                        ui.label(k);
                        ui.label(v);
                        ui.end_row();
                    }
                    if p.dynamic > 0 {
                        ui.label(t("Références non littérales"));
                        ui.label(p.dynamic.to_string());
                        ui.end_row();
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(t("Variable globale n°"));
                ui.add(egui::TextEdit::singleline(&mut self.gvar_query).desired_width(70.0));
            });
            if let Ok(n) = self.gvar_query.trim().parse::<i64>() {
                let users: Vec<String> = facts.users_of_gvar(n).into_iter().map(|(p, w)| format!("{p} ({})", if w { t("écrit") } else { t("lit") })).collect();
                ui.label(if users.is_empty() { t("Aucune procédure ne l'utilise.").to_string() } else { users.join(", ") });
            }
        });
    }

    pub fn init(&mut self, game: &GameData, script: Option<&str>) {
        if self.root.as_ref() != Some(&game.root) {
            *self = Self::default();
            self.root = Some(game.root.clone());
            self.compiler = fo_script::compiler::find_compiler(&game.root)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            self.includes = format!(
                "{};{}",
                game.root.join("scripts_src/headers").display(),
                game.root.join("headers").display()
            );
        }
        if self.path.is_empty() {
            self.for_script(game, script);
        }
    }
    fn for_script(&mut self, game: &GameData, script: Option<&str>) {
        let stem = script.unwrap_or("newscript.int").trim_end_matches(".int");
        self.target = stem.into();
        if let Some(path) = find_source(&game.root, stem) {
            self.path = path.display().to_string();
            self.load();
        } else {
            self.path = game
                .root
                .join("scripts_src")
                .join(format!("{stem}.ssl"))
                .display()
                .to_string();
            self.reset_analysis();
            self.text.clear();
            self.log =
                t("Source SSL introuvable : indiquez son chemin ou créez une nouvelle source.")
                    .into();
        }
    }
    fn reset_analysis(&mut self) {
        self.facts = None;
        self.facts_sel = None;
        self.facts_stale = false;
        self.jump = None;
    }
    fn load(&mut self) {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                self.reset_analysis();
                self.text = bytes.iter().map(|&b| b as char).collect();
                self.dirty = false;
                self.log.clear();
                self.target = Path::new(&self.path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            }
            Err(e) => self.log = e.to_string(),
        }
    }
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        game: Arc<GameData>,
        script: Option<&str>,
    ) -> egui::Rect {
        self.init(&game, script);
        if capture_details() && self.facts.is_none() && !self.text.is_empty() {
            self.analyze();
        }
        if let Some(receiver) = &self.receiver {
            if let Ok(result) = receiver.try_recv() {
                self.log = match result {
                    Ok((file, msg)) => {
                        self.completed = Some(file);
                        msg
                    }
                    Err(e) => e,
                };
                self.receiver = None;
            }
        }
        // Les réglages peuvent défiler sans faire disparaître le code, même
        // quand les panneaux du compilateur et de l'analyse sont dépliés.
        let content_width = ui.available_width();
        let controls_height = (ui.available_height() * 0.48).max(100.0);
        egui::ScrollArea::vertical().id_salt("ssl_controls").max_width(content_width).max_height(controls_height).auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(t("Source SSL"));
            ui.add(egui::TextEdit::singleline(&mut self.path).desired_width((ui.available_width()-90.0).max(80.0)));
            if ui
                .add_enabled(!self.dirty, egui::Button::new(t("Ouvrir")))
                .clicked()
            {
                self.load();
            }
        });
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(!self.dirty,egui::Button::new(t("Source du script sélectionné"))).clicked(){self.for_script(&game,script);}
            if ui.add_enabled(!self.dirty,egui::Button::new(t("Nouvelle source"))).clicked(){
                let names=game.list("scripts/scripts.lst");
                let name=(1..10000).map(|i|format!("fm{i:04}")).find(|s|!names.iter().any(|n|n.eq_ignore_ascii_case(&format!("{s}.int")))&&!game.root.join("scripts_src").join(format!("{s}.ssl")).exists()).unwrap_or_else(||"fmscript".into());
                self.reset_analysis();self.target=name.clone();self.path=game.root.join("scripts_src").join(format!("{name}.ssl")).display().to_string();self.register=true;
                self.text="procedure start;\nprocedure map_enter_p_proc;\n\nprocedure start begin\nend\n\nprocedure map_enter_p_proc begin\nend\n".into();self.dirty=true;
            }
            if ui.button(t("Enregistrer SSL")).clicked(){match save_source(Path::new(&self.path),&self.text){Ok(())=>{self.dirty=false;self.log=t("Source enregistrée.").into();},Err(e)=>self.log=e}}
            if ui.button(t("Restaurer le brouillon")).clicked(){match std::fs::read_to_string(draft_path(&game.root)){Ok(d)=>{if let Some((path,text))=d.split_once('\n'){self.reset_analysis();self.path=path.into();self.text=text.into();self.target=Path::new(path).file_stem().unwrap_or_default().to_string_lossy().into_owned();self.dirty=true;}},Err(e)=>self.log=e.to_string()}}
            if self.dirty{ui.label("●");}
        });
        egui::CollapsingHeader::new(t("Compilateur et inclusions")).default_open(capture_details()).show(ui, |ui| {
            ui.label(t("Exécutable compile.exe / sslc"));
            ui.add(egui::TextEdit::singleline(&mut self.compiler).desired_width(f32::INFINITY));
            ui.label(t("Dossiers d'inclusion séparés par des points-virgules"));
            ui.add(egui::TextEdit::singleline(&mut self.includes).desired_width(f32::INFINITY));
            ui.checkbox(
                &mut self.register,
                t("Ajouter le nouveau script à scripts.lst"),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Variables locales du nouveau script"));
                ui.add(crate::stepper::Stepper::new(&mut self.locals).range(0..=10000));
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(t("Nom du script compilé"));
            ui.add(egui::TextEdit::singleline(&mut self.target).desired_width(180.0));
            if ui
                .add_enabled(
                    self.receiver.is_none() && !self.text.trim().is_empty(),
                    egui::Button::new(t("Compiler et installer INT")),
                )
                .clicked()
            {
                let target = self
                    .target
                    .trim()
                    .trim_end_matches(".int")
                    .to_ascii_lowercase();
                if target.is_empty()
                    || (self.register && target.len()>13)
                    || !target
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    self.log = t("Nom de script invalide.").into();
                } else {
                    let compiler = PathBuf::from(&self.compiler);
                    let source = PathBuf::from(&self.path);
                    let text = self.text.clone();
                    let includes: Vec<_> = self
                        .includes
                        .split(';')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                        .collect();
                    let (tx, rx) = mpsc::channel();
                    let ctx = ui.ctx().clone();
                    let game = game.clone();
                    self.receiver = Some(rx);
                    let register = self.register;
                    let locals = self.locals.max(0) as u32;
                    self.log = t("Compilation en cours…").into();
                    std::thread::spawn(move || {
                        let result =
                            fo_script::compiler::compile(&compiler, &source, &text, &includes)
                                .and_then(|report| {
                                    let Some(bytes) = report.bytes else {
                                        return Err(report.output);
                                    };
                                    let path = game
                                        .save_data_file(&format!("scripts/{target}.int"), &bytes)
                                        .map_err(|e| e.to_string())?;
                                    if register {
                                        fo_script::compiler::register_script(
                                            &game, &target, locals,
                                        )?;
                                    }
                                    Ok((
                                        format!("{target}.int"),
                                        format!("{}\n{}", path.display(), report.output),
                                    ))
                                });
                        let _ = tx.send(result);
                        ctx.request_repaint();
                    });
                }
            }
            if self.receiver.is_some() {
                ui.spinner();
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(150));
            }
        });
        self.analysis_ui(ui);
        let lines: Vec<_> = self.log.lines().map(str::to_owned).collect();
        egui::ScrollArea::vertical()
            .id_salt("ssl_diagnostics")
            .max_height(120.0)
            .show(ui, |ui| {
                for line in lines {
                    if let Some(n) = fo_script::compiler::diagnostic_line(&line) {
                        if ui.link(line).clicked() {
                            self.jump = Some(n);
                        }
                    } else {
                        ui.label(line);
                    }
                }
            });
        if self.compiler.is_empty() {
            ui.colored_label(Color32::YELLOW, t("Indiquez le chemin du compilateur SSL pour produire un INT."));
        }
        });
        ui.separator();
        let font = egui::FontId::monospace(14.0);
        let row = ui.fonts_mut(|f| f.row_height(&font));
        let mut area = egui::ScrollArea::both()
            .id_salt("ssl_editor")
            .max_width(content_width)
            .auto_shrink([false, false]);
        if let Some(line) = self.jump.take() {
            area = area.vertical_scroll_offset(line.saturating_sub(1) as f32 * row);
        }
        let editor = area.show(ui, |ui| {
            let dark = ui.visuals().dark_mode;
            let mut layout = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                ui.fonts_mut(|f| {
                    f.layout_job(crate::highlight::highlight(
                        buffer.as_str(),
                        font.clone(),
                        dark,
                    ))
                })
            };
            if ui
                .add(
                    egui::TextEdit::multiline(&mut self.text)
                        .font(font.clone())
                        .code_editor()
                        .layouter(&mut layout)
                        .desired_width(f32::INFINITY)
                        .desired_rows(40),
                )
                .changed()
            {
                self.dirty = true;
                self.facts_stale = true;
                let path = draft_path(&game.root);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(error) = std::fs::write(path, format!("{}\n{}", self.path, self.text)) {
                    self.log = format!("Brouillon non enregistré : {error}");
                }
            }
        });
        editor.inner_rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_root() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("fomap-ssl-layout-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(root.join("data")).unwrap();
        root
    }

    #[test]
    fn code_stays_visible_with_all_settings_open_in_a_small_panel() {
        let root = scratch_root();
        let game = Arc::new(GameData::open(&root).unwrap());
        for size in [egui::vec2(360.0, 420.0), egui::vec2(640.0, 560.0)] {
            let ctx = egui::Context::default();
            ctx.memory_mut(|m| m.set_everything_is_visible(true));
            let source = (0..35)
                .map(|i| format!("procedure example_{i} begin\nend\n"))
                .collect::<String>();
            let mut editor = SourceEditor {
                root: Some(root.clone()),
                path: root.join("sample.ssl").display().to_string(),
                text: source,
                includes: "a/very/long/path/to/headers;another/long/path".into(),
                log: "sample.ssl:12: unknown identifier".into(),
                ..SourceEditor::default()
            };
            editor.analyze();
            for frame in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    time: Some(frame as f64),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |root_ui| {
                    egui::CentralPanel::default().show(root_ui, |ui| {
                        let bounds = ui.max_rect();
                        let code = editor.ui(ui, game.clone(), None);
                        assert!(
                            code.height() >= size.y * 0.40,
                            "code area too short: {code:?}"
                        );
                        assert!(
                            bounds.expand(1.0).contains_rect(code),
                            "editor outside panel: {code:?}, {bounds:?}"
                        );
                    });
                });
                output.textures_delta.clear();
            }
        }
        std::fs::remove_dir(root.join("data")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn opening_another_source_clears_previous_analysis_and_jump() {
        let root = scratch_root();
        let path = root.join("sample.ssl");
        std::fs::write(&path, "procedure old_source begin\nend\n").unwrap();
        let mut editor = SourceEditor {
            path: path.display().to_string(),
            ..SourceEditor::default()
        };
        editor.load();
        editor.analyze();
        editor.facts_sel = Some("old_source".into());
        editor.jump = Some(24);
        std::fs::write(&path, "procedure new_source begin\nend\n").unwrap();
        editor.load();
        assert!(editor.text.contains("new_source"));
        assert!(editor.facts.is_none() && editor.facts_sel.is_none() && editor.jump.is_none());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root.join("data")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
