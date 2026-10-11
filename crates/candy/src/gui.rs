use crate::canvas::{Brush, Canvas, Tool};
use anyhow::Result;
use candy_core::{History, Operation};
use eframe::egui::{self, Color32, RichText, Vec2};
use image::RgbaImage;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};

type JobResult<T> = std::result::Result<T, String>;
enum Event {
    Loaded(PathBuf, JobResult<RgbaImage>),
    Rendered {
        id: u64,
        revision: u64,
        history: History,
        image: JobResult<RgbaImage>,
    },
    Detected {
        id: u64,
        revision: u64,
        regions: JobResult<Vec<candy_macos::TextRegion>>,
    },
    Saved {
        id: u64,
        state: u64,
        path: PathBuf,
        result: JobResult<()>,
    },
}

struct Document {
    id: u64,
    name: String,
    source: Option<PathBuf>,
    output: Option<PathBuf>,
    history: History,
    image: Arc<RgbaImage>,
    texture: egui::TextureHandle,
    canvas: Canvas,
    saved_state: Option<u64>,
    revision: u64,
    rendering: bool,
    saving: bool,
    detecting: bool,
}
impl Document {
    fn dirty(&self) -> bool {
        self.saved_state != Some(self.history.state_id())
    }
    fn busy(&self) -> bool {
        self.rendering || self.saving
    }
}

#[derive(Clone, Copy)]
enum Close {
    App,
    Tab(u64),
}

#[derive(Clone, Copy)]
struct ClosePrompt {
    target: Close,
    document: u64,
}

pub struct Editor {
    documents: Vec<Document>,
    active: usize,
    next_id: u64,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    loading: usize,
    tool: Tool,
    block: u32,
    brush_width: f32,
    brush_color: [u8; 4],
    background: [u8; 3],
    resize_width: u32,
    resize_height: u32,
    aspect_lock: bool,
    notice: String,
    error: Option<String>,
    closing: Option<ClosePrompt>,
    close_after_save: Option<(u64, Close)>,
    allow_close: bool,
}

pub fn run(path: Option<PathBuf>) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([760.0, 520.0])
            .with_app_id("org.candyapp.Candy"),
        ..Default::default()
    };
    eframe::run_native(
        "Candy",
        options,
        Box::new(move |cc| {
            configure(&cc.egui_ctx);
            let mut editor = Editor::new();
            if let Some(path) = path {
                editor.open_path(path, &cc.egui_ctx);
            }
            Ok(Box::new(editor))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Cannot open Candy: {e}"))
}

fn configure(ctx: &egui::Context) {
    // Use macOS's installed Korean font as a fallback, without redistributing it.
    if let Ok(font) = std::fs::read("/System/Library/Fonts/AppleSDGothicNeo.ttc") {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "macos-korean".into(),
            egui::FontData::from_owned(font).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .push("macos-korean".into());
        ctx.set_fonts(fonts);
    }
    ctx.set_theme(egui::ThemePreference::System);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 6.0);
            style.spacing.interact_size.y = 28.0;
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
            let accent = if theme == egui::Theme::Dark {
                Color32::from_rgb(255, 155, 131)
            } else {
                Color32::from_rgb(183, 70, 50)
            };
            style.visuals.selection.bg_fill = accent.gamma_multiply(0.22);
            style.visuals.selection.stroke.color = accent;
        });
    }
}

fn texture(ctx: &egui::Context, id: u64, image: &RgbaImage) -> egui::TextureHandle {
    // Keep GPU previews bounded; editing and exports always retain full resolution.
    let limit = ctx.input(|i| i.max_texture_side).min(4096) as u32;
    let preview;
    let pixels = if image.width() > limit || image.height() > limit {
        preview = image::DynamicImage::ImageRgba8(image.clone())
            .resize(limit, limit, image::imageops::FilterType::Triangle)
            .into_rgba8();
        &preview
    } else {
        image
    };
    ctx.load_texture(
        format!("document-{id}"),
        egui::ColorImage::from_rgba_unmultiplied(
            [pixels.width() as usize, pixels.height() as usize],
            pixels.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    )
}

impl Editor {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            documents: Vec::new(),
            active: 0,
            next_id: 1,
            tx,
            rx,
            loading: 0,
            tool: Tool::Select,
            block: 12,
            brush_width: 24.0,
            brush_color: [255, 215, 50, 90],
            background: [255; 3],
            resize_width: 1,
            resize_height: 1,
            aspect_lock: true,
            notice: "Ready".into(),
            error: None,
            closing: None,
            close_after_save: None,
            allow_close: false,
        }
    }
    fn spawn(&self, ctx: &egui::Context, job: impl FnOnce() -> Event + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let event = job();
            let _ = tx.send(event);
            ctx.request_repaint();
        });
    }
    fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.loading += 1;
        self.spawn(ctx, move || {
            let result = candy_macos::load(&path).map_err(|e| format!("{e:#}"));
            Event::Loaded(path, result)
        });
    }
    fn open_dialog(&mut self, ctx: &egui::Context) {
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Open image")
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "webp", "heic", "heif", "avif"],
            )
            .pick_files()
        {
            for path in paths {
                self.open_path(path, ctx);
            }
        }
    }
    fn add_document(
        &mut self,
        image: RgbaImage,
        source: Option<PathBuf>,
        name: String,
        ctx: &egui::Context,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let saved_state = source.as_ref().map(|_| 0);
        let tex = texture(ctx, id, &image);
        let history = History::new(image);
        let image = history.original.clone();
        self.documents.push(Document {
            id,
            name,
            source,
            output: None,
            history,
            image,
            texture: tex,
            canvas: Canvas::default(),
            saved_state,
            revision: 0,
            rendering: false,
            saving: false,
            detecting: false,
        });
        if self.closing.is_none() && self.close_after_save.is_none() {
            self.active = self.documents.len() - 1;
            self.sync_dimensions();
        }
    }
    fn sync_dimensions(&mut self) {
        if let Some(doc) = self.documents.get(self.active) {
            self.resize_width = doc.image.width();
            self.resize_height = doc.image.height();
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Loaded(path, result) => {
                    self.loading = self.loading.saturating_sub(1);
                    match result {
                        Ok(image) => {
                            let name = path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            self.add_document(image, Some(path), name, ctx);
                            self.notice = "Image opened".into();
                        }
                        Err(error) => self.error = Some(format!("{}: {error}", path.display())),
                    }
                }
                Event::Rendered {
                    id,
                    revision,
                    history,
                    image,
                } => {
                    if let Some(doc) = self
                        .documents
                        .iter_mut()
                        .find(|d| d.id == id && d.revision == revision)
                    {
                        doc.rendering = false;
                        match image {
                            Ok(image) => {
                                let changed_size = doc.image.dimensions() != image.dimensions();
                                doc.texture = texture(ctx, id, &image);
                                doc.image = Arc::new(image);
                                doc.history = history;
                                doc.canvas.clear_regions();
                                if changed_size {
                                    doc.canvas.fit();
                                }
                                self.notice = "Edit applied".into();
                            }
                            Err(error) => self.error = Some(error),
                        }
                        self.sync_dimensions();
                    }
                }
                Event::Detected {
                    id,
                    revision,
                    regions,
                } => {
                    if let Some(doc) = self.documents.iter_mut().find(|d| d.id == id) {
                        doc.detecting = false;
                        if doc.revision != revision {
                            self.notice =
                                "Image changed. Detect text again to refresh the regions.".into();
                            continue;
                        }
                        match regions {
                            Ok(regions) => {
                                doc.canvas.boxes = regions.into_iter().map(|r| r.rect).collect();
                                doc.canvas.active_box = None;
                                self.notice = format!(
                                    "{} text regions found. Adjust or add boxes before applying.",
                                    doc.canvas.boxes.len()
                                );
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                }
                Event::Saved {
                    id,
                    state,
                    path,
                    result,
                } => {
                    if let Some(doc) = self.documents.iter_mut().find(|d| d.id == id) {
                        doc.saving = false;
                        match result {
                            Ok(()) => {
                                doc.saved_state = Some(state);
                                doc.name = path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned();
                                doc.output = Some(path.clone());
                                self.notice = format!("Saved {}", path.display());
                                if let Some((saving_id, close)) = self.close_after_save
                                    && saving_id == id
                                {
                                    self.close_after_save = None;
                                    self.request_close(close, ctx);
                                }
                            }
                            Err(error) => {
                                self.close_after_save = None;
                                self.error = Some(error);
                            }
                        }
                    }
                }
            }
        }
    }
    fn render_history(&mut self, history: History, ctx: &egui::Context) {
        let Some(doc) = self.documents.get_mut(self.active) else {
            return;
        };
        if doc.busy() {
            return;
        }
        doc.revision += 1;
        doc.rendering = true;
        doc.canvas.clear_regions();
        let id = doc.id;
        let revision = doc.revision;
        self.spawn(ctx, move || {
            let image = history.render().map_err(|e| format!("{e:#}"));
            Event::Rendered {
                id,
                revision,
                history,
                image,
            }
        });
    }
    fn apply(&mut self, operation: Operation, ctx: &egui::Context) {
        if let Some(doc) = self.documents.get(self.active).filter(|d| !d.busy()) {
            let mut history = doc.history.clone();
            history.push(operation);
            self.render_history(history, ctx);
        }
    }
    fn undo(&mut self, redo: bool, ctx: &egui::Context) {
        if let Some(doc) = self.documents.get(self.active).filter(|d| !d.busy()) {
            if (!redo && !doc.history.can_undo()) || (redo && !doc.history.can_redo()) {
                return;
            }
            let mut history = doc.history.clone();
            if redo {
                history.redo();
            } else {
                history.undo();
            }
            self.render_history(history, ctx);
        }
    }
    fn extract(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.documents.get(self.active) else {
            return;
        };
        let Some(rect) = doc.canvas.selection else {
            return;
        };
        if doc.busy() {
            return;
        }
        match candy_core::apply(&doc.image, &Operation::Crop(rect)) {
            Ok(image) => {
                let stem = std::path::Path::new(&doc.name)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let name = format!("{stem}-crop.png");
                self.add_document(image, None, name, ctx);
                self.notice = "Selection opened as a new image".into();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn detect(&mut self, ctx: &egui::Context) {
        let Some(doc) = self
            .documents
            .get_mut(self.active)
            .filter(|d| !d.busy() && !d.detecting)
        else {
            return;
        };
        doc.detecting = true;
        let image = doc.image.clone();
        let id = doc.id;
        let revision = doc.revision;
        self.spawn(ctx, move || Event::Detected {
            id,
            revision,
            regions: candy_macos::detect_text(&image).map_err(|e| format!("{e:#}")),
        });
    }
    fn save(&mut self, save_as: bool, ctx: &egui::Context) -> bool {
        let Some(doc) = self.documents.get(self.active).filter(|d| !d.busy()) else {
            return false;
        };
        let needs_dialog = save_as || doc.output.is_none();
        let path = if needs_dialog {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save image copy")
                .add_filter("PNG", &["png"])
                .add_filter("JPEG", &["jpg", "jpeg"])
                .add_filter("WebP", &["webp"]);
            let suggested = if let Some(output) = &doc.output {
                output
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else if doc.source.is_some() {
                format!(
                    "{}-edited.png",
                    std::path::Path::new(&doc.name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            } else {
                doc.name.clone()
            };
            dialog = dialog.set_file_name(suggested);
            if let Some(parent) = doc
                .output
                .as_ref()
                .or(doc.source.as_ref())
                .and_then(|p| p.parent())
            {
                dialog = dialog.set_directory(parent);
            }
            let Some(mut path) = dialog.save_file() else {
                return false;
            };
            if path.extension().is_none() {
                path.set_extension("png");
            }
            path
        } else {
            doc.output.clone().unwrap()
        };
        if let Some(source) = &doc.source
            && let Err(error) = candy_core::protect_original(source, &path)
        {
            self.error = Some(error.to_string());
            return false;
        }
        let doc = &mut self.documents[self.active];
        doc.saving = true;
        let image = doc.image.clone();
        let id = doc.id;
        let state = doc.history.state_id();
        self.spawn(ctx, move || Event::Saved {
            id,
            state,
            result: candy_core::save_atomic(&image, &path, true)
                .map_err(|e| format!("Cannot save {}: {e:#}", path.display())),
            path,
        });
        true
    }
    fn request_close(&mut self, close: Close, ctx: &egui::Context) {
        let relevant = |d: &&Document| match close {
            Close::App => true,
            Close::Tab(id) => d.id == id,
        };
        if self.documents.iter().filter(relevant).any(|d| d.busy()) {
            self.notice = "Wait for the current edit or save to finish before closing.".into();
            return;
        }
        if let Some(index) = self.documents.iter().position(|d| {
            (matches!(close, Close::App) || matches!(close, Close::Tab(id) if d.id == id))
                && d.dirty()
        }) {
            self.active = index;
            self.sync_dimensions();
            self.closing = Some(ClosePrompt {
                target: close,
                document: self.documents[index].id,
            });
        } else {
            self.finish_close(close, ctx);
        }
    }
    fn finish_close(&mut self, close: Close, ctx: &egui::Context) {
        match close {
            Close::App => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Close::Tab(id) => {
                self.documents.retain(|d| d.id != id);
                self.active = self.active.min(self.documents.len().saturating_sub(1));
                self.sync_dimensions();
            }
        }
        self.closing = None;
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        let (open, save, save_as, undo, redo, close_tab, quit, escape, delete) =
            ctx.input_mut(|i| {
                // egui's logical matching ignores extra Shift; consume modified commands first.
                let save_as = i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::S,
                );
                let redo = i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                );
                (
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                    save_as,
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                    redo,
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::W),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                )
            });
        if self.closing.is_some() {
            if escape {
                self.closing = None;
            }
            return;
        }
        if open {
            self.open_dialog(ctx);
        }
        if save || save_as {
            self.save(save_as, ctx);
        }
        if undo || redo {
            self.undo(redo, ctx);
        }
        if quit {
            self.request_close(Close::App, ctx);
        }
        if close_tab && let Some(doc) = self.documents.get(self.active) {
            self.request_close(Close::Tab(doc.id), ctx);
        }
        if let Some(doc) = self.documents.get_mut(self.active) {
            if escape {
                doc.canvas.cancel();
            }
            if delete && !ctx.egui_wants_keyboard_input() && self.tool == Tool::Text {
                doc.canvas.remove_active_box();
            }
        }
    }
    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Candy").strong().size(17.0));
                ui.separator();
                if ui
                    .button("Open…")
                    .on_hover_text("Open image · Cmd+O")
                    .clicked()
                {
                    self.open_dialog(ctx);
                }
                let ready = self.documents.get(self.active).is_some_and(|d| !d.busy());
                if ui
                    .add_enabled(ready, egui::Button::new("Save…"))
                    .on_hover_text("Save a copy · Cmd+S")
                    .clicked()
                {
                    self.save(false, ctx);
                }
                if ui
                    .add_enabled(ready, egui::Button::new("Save as…"))
                    .on_hover_text("Shift+Cmd+S")
                    .clicked()
                {
                    self.save(true, ctx);
                }
                ui.separator();
                let can_undo = ready && self.documents[self.active].history.can_undo();
                let can_redo = ready && self.documents[self.active].history.can_redo();
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo"))
                    .clicked()
                {
                    self.undo(false, ctx);
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo"))
                    .clicked()
                {
                    self.undo(true, ctx);
                }
                ui.separator();
                if ui.button("Fit").clicked()
                    && let Some(doc) = self.documents.get_mut(self.active)
                {
                    doc.canvas.fit();
                }
                if ui.button("100%").clicked()
                    && let Some(doc) = self.documents.get_mut(self.active)
                {
                    doc.canvas.zoom = Some(1.0);
                    doc.canvas.pan = Vec2::ZERO;
                }
            });
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                for (tool, name) in [
                    (Tool::Select, "Select"),
                    (Tool::Mosaic, "Mosaic"),
                    (Tool::Highlight, "Highlight"),
                    (Tool::Text, "Text mosaic"),
                    (Tool::Resize, "Resize"),
                    (Tool::Flatten, "Flatten alpha"),
                ] {
                    if ui.selectable_label(self.tool == tool, name).clicked() {
                        self.tool = tool;
                        self.sync_dimensions();
                        if let Some(doc) = self.documents.get_mut(self.active) {
                            doc.canvas.cancel_drag();
                        }
                    }
                }
            });
        });
    }
    fn options(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.documents.is_empty() {
            return;
        }
        egui::Panel::top("tool-options").show(ui, |ui| {
            let ready = !self.documents[self.active].busy();
            ui.add_enabled_ui(ready, |ui| {
                ui.horizontal_wrapped(|ui| match self.tool {
                    Tool::Select => {
                        ui.label("Drag to select. Move the box or drag a corner to adjust.");
                        let selected = self.documents[self.active].canvas.selection.is_some();
                        if ui
                            .add_enabled(selected, egui::Button::new("Extract to new image"))
                            .clicked()
                        {
                            self.extract(ctx);
                        }
                    }
                    Tool::Mosaic => {
                        ui.label("Select the area to pixelate.");
                        ui.add(egui::Slider::new(&mut self.block, 2..=96).text("Block px"));
                        if ui
                            .add_enabled(
                                self.documents[self.active].canvas.selection.is_some(),
                                egui::Button::new("Apply mosaic"),
                            )
                            .clicked()
                            && let Some(rect) = self.documents[self.active].canvas.selection
                        {
                            self.apply(
                                Operation::Mosaic {
                                    regions: vec![rect],
                                    block: self.block,
                                },
                                ctx,
                            );
                        }
                    }
                    Tool::Highlight => {
                        ui.label("Draw over the image.");
                        let mut rgb = [
                            self.brush_color[0],
                            self.brush_color[1],
                            self.brush_color[2],
                        ];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            self.brush_color[..3].copy_from_slice(&rgb);
                        }
                        ui.add(
                            egui::Slider::new(&mut self.brush_width, 2.0..=100.0).text("Width px"),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.brush_color[3], 16..=220).text("Opacity"),
                        );
                    }
                    Tool::Text => {
                        let doc = &self.documents[self.active];
                        if ui
                            .add_enabled(
                                !doc.detecting,
                                egui::Button::new(if doc.detecting {
                                    "Detecting…"
                                } else {
                                    "Detect text"
                                }),
                            )
                            .clicked()
                        {
                            self.detect(ctx);
                        }
                        let count = self.documents[self.active].canvas.boxes.len();
                        ui.label(format!("{count} regions"));
                        ui.add(egui::Slider::new(&mut self.block, 2..=96).text("Block px"));
                        if ui
                            .add_enabled(count > 0, egui::Button::new("Apply to all"))
                            .clicked()
                        {
                            let regions = self.documents[self.active].canvas.boxes.clone();
                            self.apply(
                                Operation::Mosaic {
                                    regions,
                                    block: self.block,
                                },
                                ctx,
                            );
                        }
                        if ui
                            .add_enabled(
                                self.documents[self.active].canvas.active_box.is_some(),
                                egui::Button::new("Remove box"),
                            )
                            .clicked()
                        {
                            self.documents[self.active].canvas.remove_active_box();
                        }
                        ui.weak("Drag to add or adjust a box.");
                    }
                    Tool::Resize => {
                        ui.label("Width");
                        let width_changed = ui
                            .add(
                                egui::DragValue::new(&mut self.resize_width)
                                    .range(1..=65535)
                                    .suffix(" px"),
                            )
                            .changed();
                        ui.label("Height");
                        let height_changed = ui
                            .add(
                                egui::DragValue::new(&mut self.resize_height)
                                    .range(1..=65535)
                                    .suffix(" px"),
                            )
                            .changed();
                        ui.checkbox(&mut self.aspect_lock, "Keep ratio");
                        let doc = &self.documents[self.active];
                        if self.aspect_lock && (width_changed || height_changed) {
                            let dims = candy_core::resize_dimensions(
                                doc.image.width(),
                                doc.image.height(),
                                width_changed.then_some(self.resize_width),
                                (!width_changed).then_some(self.resize_height),
                            );
                            if let Ok((w, h)) = dims {
                                self.resize_width = w;
                                self.resize_height = h;
                            }
                        }
                        if ui.button("Apply resize").clicked() {
                            self.apply(
                                Operation::Resize {
                                    width: self.resize_width,
                                    height: self.resize_height,
                                },
                                ctx,
                            );
                        }
                    }
                    Tool::Flatten => {
                        ui.label("Replace transparency with");
                        ui.color_edit_button_srgb(&mut self.background);
                        if ui.button("Flatten alpha").clicked() {
                            self.apply(Operation::Flatten(self.background), ctx);
                        }
                    }
                });
            });
        });
    }
    fn tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.documents.is_empty() {
            return;
        }
        let mut close = None;
        let mut activate = None;
        egui::Panel::top("documents").show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, doc) in self.documents.iter().enumerate() {
                        let name = format!("{}{}", doc.name, if doc.dirty() { " *" } else { "" });
                        if ui.selectable_label(self.active == i, name).clicked() {
                            activate = Some(i);
                        }
                        if ui
                            .small_button("Close")
                            .on_hover_text("Close this image · Cmd+W")
                            .clicked()
                        {
                            close = Some(doc.id);
                        }
                        ui.separator();
                    }
                });
            });
        });
        if let Some(i) = activate {
            self.active = i;
            self.sync_dimensions();
        }
        if let Some(id) = close {
            self.request_close(Close::Tab(id), ctx);
        }
    }
    fn status(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            if let Some(error) = self.error.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if self.loading > 0 {
                    ui.spinner();
                    ui.label(format!("Opening {} image(s)…", self.loading));
                }
                if let Some(doc) = self.documents.get(self.active) {
                    ui.label(format!("{} × {} px", doc.image.width(), doc.image.height()));
                    ui.separator();
                    ui.label(format!("{:.0}%", doc.canvas.actual_zoom * 100.0));
                    let shown_region = if self.tool == Tool::Text {
                        doc.canvas
                            .active_box
                            .and_then(|i| doc.canvas.boxes.get(i).copied())
                    } else {
                        doc.canvas.selection
                    };
                    if let Some(rect) = shown_region {
                        ui.separator();
                        ui.label(format!(
                            "Selection {}, {} · {} × {}",
                            rect.x, rect.y, rect.width, rect.height
                        ));
                    }
                    if doc.rendering || doc.saving || doc.detecting {
                        ui.spinner();
                        ui.label(if doc.saving {
                            "Saving…"
                        } else if doc.rendering {
                            "Applying edit…"
                        } else {
                            "Detecting text locally…"
                        });
                    }
                }
                ui.separator();
                ui.small(&self.notice);
            });
        });
    }
    fn close_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.closing else {
            return;
        };
        let Some(index) = self.documents.iter().position(|d| d.id == prompt.document) else {
            self.closing = None;
            return;
        };
        let close = prompt.target;
        let mut action = 0;
        egui::Modal::new(egui::Id::new("unsaved-changes")).show(ctx, |ui| {
            ui.set_max_width(400.0);
            ui.heading("Save your changes?");
            ui.label(format!("{} has unsaved edits.", self.documents[index].name));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Save copy…").clicked() {
                    action = 1;
                }
                if ui.button("Discard").clicked() {
                    action = 2;
                }
                if ui.button("Cancel").clicked() {
                    action = 3;
                }
            });
        });
        match action {
            1 => {
                self.active = index;
                self.sync_dimensions();
                self.closing = None;
                if self.save(false, ctx) {
                    self.close_after_save = Some((prompt.document, close));
                }
            }
            2 => {
                if matches!(close, Close::App) {
                    self.finish_close(Close::Tab(prompt.document), ctx);
                    self.request_close(Close::App, ctx);
                } else {
                    self.finish_close(close, ctx);
                }
            }
            3 => self.closing = None,
            _ => {}
        }
    }
}

impl eframe::App for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close(Close::App, &ctx);
        }
        self.keyboard(&ctx);
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>()
        });
        for path in dropped {
            self.open_path(path, &ctx);
        }
        let title = self
            .documents
            .get(self.active)
            .map(|d| format!("{}{} — Candy", d.name, if d.dirty() { " *" } else { "" }))
            .unwrap_or_else(|| "Candy".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        self.toolbar(ui, &ctx);
        self.tabs(ui, &ctx);
        self.options(ui, &ctx);
        self.status(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if let Some(doc) = self.documents.get_mut(self.active) {
                    let enabled = !doc.busy() && self.closing.is_none();
                    if let Some(operation) = doc.canvas.show(
                        ui,
                        &doc.texture,
                        [doc.image.width(), doc.image.height()],
                        self.tool,
                        Brush {
                            width: self.brush_width,
                            color: self.brush_color,
                        },
                        enabled,
                    ) {
                        self.apply(operation, &ctx);
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() / 2.0 - 100.0).max(24.0));
                        ui.label(
                            RichText::new("A little room for your images.")
                                .size(24.0)
                                .strong(),
                        );
                        ui.add_space(8.0);
                        ui.label("Drop an image here, or open one to get started.");
                        ui.add_space(16.0);
                        if ui.button("Open image…").clicked() {
                            self.open_dialog(&ctx);
                        }
                        ui.add_space(12.0);
                        ui.weak("PNG · JPEG · WebP · HEIC · AVIF");
                    });
                }
            });
        self.close_dialog(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Editor, egui::Context) {
        let ctx = egui::Context::default();
        let mut editor = Editor::new();
        editor.add_document(
            RgbaImage::from_pixel(12, 8, image::Rgba([255; 4])),
            None,
            "first.png".into(),
            &ctx,
        );
        (editor, ctx)
    }
    #[test]
    fn late_load_cannot_retarget_an_unsaved_close_prompt() {
        let (mut editor, ctx) = setup();
        let original = editor.documents[0].id;
        editor.request_close(Close::Tab(original), &ctx);
        editor
            .tx
            .send(Event::Loaded(
                PathBuf::from("second.png"),
                Ok(RgbaImage::new(4, 4)),
            ))
            .unwrap();
        editor.poll(&ctx);
        let prompt = editor.closing.unwrap();
        assert_eq!(prompt.document, original);
        assert!(matches!(prompt.target, Close::Tab(id) if id == original));
        assert_eq!(editor.documents[editor.active].id, original);
        assert_eq!(editor.documents.len(), 2);
    }
    #[test]
    fn stale_ocr_results_are_discarded_after_an_edit() {
        let (mut editor, ctx) = setup();
        let doc = &mut editor.documents[0];
        doc.detecting = true;
        doc.revision = 3;
        editor
            .tx
            .send(Event::Detected {
                id: doc.id,
                revision: 2,
                regions: Ok(vec![candy_macos::TextRegion {
                    rect: candy_core::PixelRect::new(0, 0, 2, 2),
                    text: "old".into(),
                }]),
            })
            .unwrap();
        editor.poll(&ctx);
        assert!(!editor.documents[0].detecting);
        assert!(editor.documents[0].canvas.boxes.is_empty());
    }
    #[test]
    fn failed_render_keeps_the_last_successful_document_state() {
        let (mut editor, ctx) = setup();
        let doc = &mut editor.documents[0];
        doc.rendering = true;
        doc.revision = 1;
        let mut candidate = doc.history.clone();
        candidate.push(Operation::Resize {
            width: 0,
            height: 0,
        });
        editor
            .tx
            .send(Event::Rendered {
                id: doc.id,
                revision: 1,
                history: candidate,
                image: Err("Invalid dimensions".into()),
            })
            .unwrap();
        editor.poll(&ctx);
        assert_eq!(editor.documents[0].history.state_id(), 0);
        assert_eq!(editor.documents[0].image.dimensions(), (12, 8));
        assert!(!editor.documents[0].rendering);
        assert!(editor.error.is_some());
    }
}
