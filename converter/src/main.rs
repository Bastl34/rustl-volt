// the converter window: converts the original Re-Volt files (Steam install) into engine formats (glb, png, ...)

mod log_view;
mod ui;

fn main() -> eframe::Result
{
    let options = eframe::NativeOptions
    {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([900.0, 650.0]).with_title("Re-Volt Converter"),
        ..Default::default()
    };

    eframe::run_native("Re-Volt Converter", options, Box::new(|_cc| Ok(Box::new(ui::ConverterApp::new()))))
}
