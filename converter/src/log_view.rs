// the log of the converter - only the visible lines are laid out, so it stays fast with thousands of lines

use eframe::egui::{self, Color32, FontId, Rect, RichText, Vec2};

use revolt_converter::convert::LogLevel;

// heights per kind of line: gap above + the text (+ the separator of a heading)
const HEADING_GAP: f32 = 24.0;
const HEADING_TEXT: f32 = 26.0;
const HEADING_LINE: f32 = 8.0;
const STEP_GAP: f32 = 16.0;
const STEP_TEXT: f32 = 20.0;
const STEP_AFTER: f32 = 3.0;
const LINE: f32 = 17.0;

#[derive(Default)]
pub struct LogView
{
    entries: Vec<(LogLevel, String)>,
    // y of every entry from the top - the visible ones are found by a binary search
    tops: Vec<f32>,
    height: f32,
    longest: usize,
}

impl LogView
{
    pub fn push(&mut self, level: LogLevel, text: String)
    {
        self.tops.push(self.height);
        self.height += match level
        {
            LogLevel::Heading => HEADING_GAP + HEADING_TEXT + HEADING_LINE,
            LogLevel::Step => STEP_GAP + STEP_TEXT + STEP_AFTER,
            _ => LINE,
        };
        self.longest = self.longest.max(text.chars().count());
        self.entries.push((level, text));
    }

    pub fn clear(&mut self)
    {
        *self = Self::default();
    }

    pub fn ui(&self, ui: &mut egui::Ui)
    {
        let mono = FontId::monospace(13.0);
        let char_width = ui.fonts_mut(|fonts| fonts.glyph_width(&mono, 'M'));
        let width = (self.longest as f32 * char_width + 40.0).max(ui.available_width());

        egui::ScrollArea::both().stick_to_bottom(true).auto_shrink(false).show_viewport(ui, |ui, viewport|
        {
            ui.set_min_size(Vec2::new(width, self.height));
            let origin = ui.min_rect().min;

            let first = self.tops.partition_point(|&top| top < viewport.min.y - HEADING_GAP - HEADING_TEXT - HEADING_LINE);
            for (i, (level, text)) in self.entries.iter().enumerate().skip(first)
            {
                let top = self.tops[i];
                if top > viewport.max.y { break; }

                let (gap, row, text) = match level
                {
                    LogLevel::Heading => (HEADING_GAP, HEADING_TEXT, RichText::new(text).size(20.0).strong().color(Color32::from_rgb(124, 140, 255))),
                    LogLevel::Step => (STEP_GAP, STEP_TEXT, RichText::new(text).size(15.0).strong().color(ui.visuals().strong_text_color())),
                    LogLevel::Info => (0.0, LINE, RichText::new(text).font(mono.clone()).color(ui.visuals().text_color())),
                    LogLevel::Warning => (0.0, LINE, RichText::new(text).font(mono.clone()).color(Color32::from_rgb(230, 180, 60))),
                    LogLevel::Error => (0.0, LINE, RichText::new(text).font(mono.clone()).color(Color32::from_rgb(230, 80, 80))),
                };

                let rect = Rect::from_min_size(origin + Vec2::new(0.0, top + gap), Vec2::new(width, row));
                // left aligned (ui.put would center it), the id from the entry so a text selection stays on its line while scrolling
                ui.scope_builder(egui::UiBuilder::new().id_salt(i).max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| ui.add(egui::Label::new(text).extend()));

                if *level == LogLevel::Heading
                {
                    let y = rect.bottom() + HEADING_LINE / 2.0;
                    ui.painter().hline(rect.left()..=rect.left() + ui.clip_rect().width(), y, ui.visuals().widgets.noninteractive.bg_stroke);
                }
            }
        });
    }
}
