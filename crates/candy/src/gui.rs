use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;

pub fn run(path: Option<PathBuf>) -> Result<()> {
    let image = path.as_ref().map(|p| candy_macos::load(p)).transpose()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Candy",
        options,
        Box::new(move |cc| {
            let texture = image.map(|image| {
                cc.egui_ctx.load_texture(
                    "image",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                )
            });
            Ok(Box::new(Preview { texture }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Cannot open editor: {e}"))
}
struct Preview {
    texture: Option<egui::TextureHandle>,
}
impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(texture) = &self.texture {
                ui.add(
                    egui::Image::new(texture)
                        .max_size(ui.available_size())
                        .shrink_to_fit(),
                );
            } else {
                ui.heading("Candy");
                ui.label("Open an image with candy edit photo.png");
            }
        });
    }
}
