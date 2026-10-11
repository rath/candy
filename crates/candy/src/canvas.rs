use candy_core::{Operation, PixelRect};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Select,
    Mosaic,
    Highlight,
    Text,
    Resize,
    Flatten,
}

#[derive(Clone, Copy)]
pub struct Brush {
    pub width: f32,
    pub color: [u8; 4],
}

#[derive(Clone, Copy)]
pub struct Transform {
    pub origin: Pos2,
    pub scale: f32,
}
impl Transform {
    pub fn to_image(self, p: Pos2) -> [f32; 2] {
        [
            (p.x - self.origin.x) / self.scale,
            (p.y - self.origin.y) / self.scale,
        ]
    }
    pub fn to_screen(self, p: [f32; 2]) -> Pos2 {
        self.origin + Vec2::new(p[0], p[1]) * self.scale
    }
    fn rect(self, r: PixelRect) -> Rect {
        Rect::from_min_size(
            self.to_screen([r.x as f32, r.y as f32]),
            Vec2::new(r.width as f32, r.height as f32) * self.scale,
        )
    }
}

enum Drag {
    New([f32; 2]),
    Move { start: [f32; 2], rect: PixelRect },
    Resize([f32; 2]),
    Pan,
    Brush(Vec<[f32; 2]>),
}

#[derive(Default)]
pub struct Canvas {
    pub selection: Option<PixelRect>,
    pub boxes: Vec<PixelRect>,
    pub active_box: Option<usize>,
    pub zoom: Option<f32>,
    pub pan: Vec2,
    pub actual_zoom: f32,
    drag: Option<Drag>,
    prior_selection: Option<PixelRect>,
    prior_boxes: Vec<PixelRect>,
    prior_active_box: Option<usize>,
}
impl Canvas {
    pub fn fit(&mut self) {
        self.zoom = None;
        self.pan = Vec2::ZERO;
    }
    pub fn clear_regions(&mut self) {
        self.selection = None;
        self.boxes.clear();
        self.active_box = None;
        self.drag = None;
    }
    pub fn remove_active_box(&mut self) {
        if let Some(i) = self.active_box.take()
            && i < self.boxes.len()
        {
            self.boxes.remove(i);
        }
    }
    pub fn cancel_drag(&mut self) {
        if self.drag.is_some() {
            self.cancel();
        }
    }
    pub fn cancel(&mut self) {
        if self.drag.take().is_some() {
            self.selection = self.prior_selection;
            self.boxes = std::mem::take(&mut self.prior_boxes);
            self.active_box = self.prior_active_box;
        } else {
            self.selection = None;
            self.active_box = None;
        }
    }
    fn selected(&self, tool: Tool) -> Option<PixelRect> {
        if tool == Tool::Text {
            self.active_box.and_then(|i| self.boxes.get(i).copied())
        } else {
            self.selection
        }
    }
    fn set_selected(&mut self, tool: Tool, rect: Option<PixelRect>) {
        if tool == Tool::Text {
            if let (Some(i), Some(rect)) = (self.active_box, rect)
                && let Some(target) = self.boxes.get_mut(i)
            {
                *target = rect;
            }
        } else {
            self.selection = rect;
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        texture: &TextureHandle,
        dimensions: [u32; 2],
        tool: Tool,
        brush: Brush,
        enabled: bool,
    ) -> Option<Operation> {
        let Brush {
            width: brush_width,
            color: brush_color,
        } = brush;
        let (area, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.0)),
            Sense::click_and_drag(),
        );
        let size = Vec2::new(dimensions[0] as f32, dimensions[1] as f32);
        let fit = ((area.width() - 40.0).max(1.0) / size.x)
            .min((area.height() - 40.0).max(1.0) / size.y)
            .min(1.0);
        let mut scale = self.zoom.unwrap_or(fit);
        let mut transform = Transform {
            origin: area.center() - size * scale / 2.0 + self.pan,
            scale,
        };
        if response.hovered() {
            let factor = ui.input(|i| {
                let pinch = i.zoom_delta();
                if (pinch - 1.0).abs() > 0.001 {
                    pinch
                } else {
                    (i.smooth_scroll_delta.y * 0.003).exp()
                }
            });
            if (factor - 1.0).abs() > 0.001 {
                let pointer = ui.input(|i| i.pointer.hover_pos()).unwrap_or(area.center());
                let image_point = transform.to_image(pointer);
                scale = (scale * factor).clamp(0.01, 16.0);
                self.zoom = Some(scale);
                self.pan =
                    pointer - area.center() + size * scale / 2.0 - Vec2::from(image_point) * scale;
                transform = Transform {
                    origin: area.center() - size * scale / 2.0 + self.pan,
                    scale,
                };
            }
        }
        self.actual_zoom = scale;
        let image_rect = Rect::from_min_size(transform.origin, size * scale);
        let painter = ui.painter().with_clip_rect(area);
        let dark = ui.visuals().dark_mode;
        painter.rect_filled(area, 0.0, Color32::from_gray(if dark { 24 } else { 226 }));
        // Only paint visible checkerboard cells; zooming never creates huge meshes.
        let visible = image_rect.intersect(area);
        let tile = 12.0;
        if visible.is_positive() {
            let columns = (visible.width() / tile).ceil() as i32;
            let rows = (visible.height() / tile).ceil() as i32;
            for y in 0..=rows {
                for x in 0..=columns {
                    let start = visible.min + Vec2::new(x as f32 * tile, y as f32 * tile);
                    let rect = Rect::from_min_size(start, Vec2::splat(tile)).intersect(visible);
                    let base = if dark { 54 } else { 246 };
                    let shade = if (x + y) % 2 == 0 { base } else { base - 12 };
                    painter.rect_filled(rect, 0.0, Color32::from_gray(shade));
                }
            }
        }
        painter.image(
            texture.id(),
            image_rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        let pointer = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|i| i.pointer.latest_pos()));
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        let selecting = matches!(tool, Tool::Select | Tool::Mosaic | Tool::Text);
        if enabled
            && response.drag_started()
            && let Some(p) = pointer
        {
            self.prior_selection = self.selection;
            self.prior_boxes = self.boxes.clone();
            self.prior_active_box = self.active_box;
            let original = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
            let point = transform.to_image(original);
            if space {
                self.drag = Some(Drag::Pan);
            } else if tool == Tool::Highlight && image_rect.contains(original) {
                self.drag = Some(Drag::Brush(vec![point]));
            } else if selecting {
                if tool == Tool::Text {
                    self.active_box = self
                        .boxes
                        .iter()
                        .position(|r| transform.rect(*r).expand(7.0).contains(original));
                }
                let selected = self.selected(tool);
                let mut resize = None;
                if let Some(rect) = selected {
                    let screen = transform.rect(rect);
                    let corners = [
                        screen.left_top(),
                        screen.right_top(),
                        screen.right_bottom(),
                        screen.left_bottom(),
                    ];
                    for (index, corner) in corners.iter().enumerate() {
                        if corner.distance(original) < 9.0 {
                            resize = Some(transform.to_image(corners[(index + 2) % 4]));
                        }
                    }
                }
                if let Some(anchor) = resize {
                    self.drag = Some(Drag::Resize(anchor));
                } else if let Some(rect) =
                    selected.filter(|r| transform.rect(*r).contains(original))
                {
                    self.drag = Some(Drag::Move { start: point, rect });
                } else if image_rect.contains(original) {
                    if tool == Tool::Text {
                        let x = point[0].clamp(0.0, (dimensions[0] - 1) as f32) as u32;
                        let y = point[1].clamp(0.0, (dimensions[1] - 1) as f32) as u32;
                        self.boxes.push(PixelRect::new(x, y, 1, 1));
                        self.active_box = Some(self.boxes.len() - 1);
                    }
                    self.drag = Some(Drag::New(point));
                }
            }
        }
        if enabled
            && response.clicked()
            && tool == Tool::Text
            && let Some(p) = pointer
        {
            self.active_box = self
                .boxes
                .iter()
                .position(|r| transform.rect(*r).contains(p));
        }
        let mut updated = None;
        if enabled
            && let Some(p) = pointer
            && let Some(drag) = self.drag.as_mut()
        {
            let point = transform.to_image(p);
            match drag {
                Drag::Pan => {
                    self.pan += ui.input(|i| i.pointer.delta());
                }
                Drag::New(anchor) | Drag::Resize(anchor) => {
                    updated = Some(PixelRect::from_points(
                        *anchor,
                        point,
                        dimensions[0],
                        dimensions[1],
                    ));
                }
                Drag::Move { start, rect } => {
                    let x = (rect.x as f32 + point[0] - start[0])
                        .round()
                        .clamp(0.0, (dimensions[0] - rect.width) as f32)
                        as u32;
                    let y = (rect.y as f32 + point[1] - start[1])
                        .round()
                        .clamp(0.0, (dimensions[1] - rect.height) as f32)
                        as u32;
                    updated = Some(Some(PixelRect::new(x, y, rect.width, rect.height)));
                }
                Drag::Brush(points) => {
                    let point = [point[0].clamp(0.0, size.x), point[1].clamp(0.0, size.y)];
                    if points
                        .last()
                        .is_none_or(|last| (last[0] - point[0]).hypot(last[1] - point[1]) >= 0.5)
                    {
                        points.push(point);
                    }
                }
            }
        }
        if let Some(rect) = updated {
            self.set_selected(tool, rect);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(if space {
                egui::CursorIcon::Grab
            } else if selecting || tool == Tool::Highlight {
                egui::CursorIcon::Crosshair
            } else {
                egui::CursorIcon::Default
            });
        }
        let accent = if dark {
            Color32::from_rgb(255, 155, 131)
        } else {
            Color32::from_rgb(183, 70, 50)
        };
        if tool == Tool::Text {
            for (i, rect) in self.boxes.iter().enumerate() {
                let screen = transform.rect(*rect);
                painter.rect_filled(screen, 0.0, accent.gamma_multiply(0.12));
                painter.rect_stroke(
                    screen,
                    0.0,
                    Stroke::new(if Some(i) == self.active_box { 2.0 } else { 1.0 }, accent),
                    StrokeKind::Inside,
                );
            }
        }
        if let Some(selected) = self.selected(tool).filter(|_| selecting) {
            let rect = transform.rect(selected);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.5, accent), StrokeKind::Inside);
            for corner in [
                rect.left_top(),
                rect.right_top(),
                rect.right_bottom(),
                rect.left_bottom(),
            ] {
                painter.rect_filled(
                    Rect::from_center_size(corner, Vec2::splat(7.0)),
                    1.0,
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    Rect::from_center_size(corner, Vec2::splat(7.0)),
                    1.0,
                    Stroke::new(1.0, accent),
                    StrokeKind::Inside,
                );
            }
        }
        if let Some(Drag::Brush(points)) = &self.drag {
            let stroke = Stroke::new(
                brush_width * scale,
                Color32::from_rgba_unmultiplied(
                    brush_color[0],
                    brush_color[1],
                    brush_color[2],
                    brush_color[3],
                ),
            );
            if points.len() > 1 {
                painter
                    .with_clip_rect(image_rect.intersect(area))
                    .add(egui::Shape::line(
                        points.iter().map(|p| transform.to_screen(*p)).collect(),
                        stroke,
                    ));
            }
        }
        if enabled
            && response.clicked()
            && tool == Tool::Highlight
            && let Some(point) = pointer.filter(|p| image_rect.contains(*p))
        {
            return Some(Operation::Highlight {
                points: vec![transform.to_image(point)],
                width: brush_width,
                color: brush_color,
            });
        }
        if response.drag_stopped()
            && let Some(Drag::Brush(points)) = self.drag.take()
        {
            return Some(Operation::Highlight {
                points,
                width: brush_width,
                color: brush_color,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transform_roundtrips_independently_of_zoom_and_pan() {
        for scale in [0.2, 1.0, 3.0] {
            let transform = Transform {
                origin: Pos2::new(-42.0, 81.0),
                scale,
            };
            let point = [420.0, 213.0];
            let result = transform.to_image(transform.to_screen(point));
            assert!((result[0] - point[0]).abs() < 0.001);
            assert!((result[1] - point[1]).abs() < 0.001);
        }
    }
    #[test]
    fn escape_restores_regions_from_before_the_drag() {
        let before = PixelRect::new(1, 2, 30, 40);
        let mut canvas = Canvas {
            prior_selection: Some(before),
            selection: Some(PixelRect::new(9, 9, 2, 2)),
            drag: Some(Drag::New([9.0; 2])),
            ..Default::default()
        };
        canvas.cancel();
        assert_eq!(canvas.selection, Some(before));
    }
    #[test]
    fn cancel_new_ocr_box_restores_a_valid_active_index() {
        let old = PixelRect::new(2, 3, 10, 10);
        let mut canvas = Canvas {
            boxes: vec![old, PixelRect::new(20, 20, 4, 4)],
            active_box: Some(1),
            prior_boxes: vec![old],
            prior_active_box: Some(0),
            drag: Some(Drag::New([20.0; 2])),
            ..Default::default()
        };
        canvas.cancel();
        assert_eq!(canvas.boxes, vec![old]);
        assert_eq!(canvas.active_box, Some(0));
        canvas.set_selected(Tool::Text, Some(PixelRect::new(4, 5, 10, 10)));
        assert_eq!(canvas.boxes[0].x, 4);
    }
}
