//! Champ numérique avec un bouton « − » à gauche et un bouton « + » à droite (même hauteur, largeur confortable). La valeur
//! reste modifiable au clavier et par glisser, comme un `DragValue`.

use eframe::egui::{self, Response, Widget};
use std::ops::RangeInclusive;

pub(crate) struct Stepper<'a> {
    value: &'a mut i32,
    range: RangeInclusive<i32>,
    prefix: String,
    speed: f64,
    step: i32,
}

impl<'a> Stepper<'a> {
    pub(crate) fn new(value: &'a mut i32) -> Self {
        Self {
            value,
            range: i32::MIN..=i32::MAX,
            prefix: String::new(),
            speed: 1.0,
            step: 1,
        }
    }

    pub(crate) fn range(mut self, range: RangeInclusive<i32>) -> Self {
        self.range = range;
        self
    }

    pub(crate) fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Vitesse du glisser (valeurs par pixel) ; le pas des boutons suit : un dixième de la vitesse, au moins 1.
    pub(crate) fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self.step = (speed / 10.0).round().max(1.0) as i32;
        self
    }
}

impl Widget for Stepper<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let height = ui.spacing().interact_size.y.max(26.0);
        let button_width = height * 1.15;
        let (min, max) = (*self.range.start(), *self.range.end());
        let mut changed = false;
        let inner = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if ui
                .add_sized([button_width, height], egui::Button::new("−"))
                .clicked()
            {
                *self.value = self.value.saturating_sub(self.step).clamp(min, max);
                changed = true;
            }
            let drag = egui::DragValue::new(&mut *self.value)
                .range(self.range.clone())
                .prefix(self.prefix.clone())
                .speed(self.speed);
            changed |= ui
                .add_sized([(height * 2.6).max(68.0), height], drag)
                .changed();
            if ui
                .add_sized([button_width, height], egui::Button::new("+"))
                .clicked()
            {
                *self.value = self.value.saturating_add(self.step).clamp(min, max);
                changed = true;
            }
        });
        let mut response = inner.response;
        if changed {
            response.mark_changed();
        }
        response
    }
}
