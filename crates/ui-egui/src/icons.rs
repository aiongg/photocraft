//! PhotoCraft's icon names. The glyphs come from the framework's catalog and from the app's own set in
//! `icon_data.rs`, registered once by [`register`]; drawing is the framework's `Icon` / `IconSpec::svg`.

use craft::behavior::IconId;
use craft::ui::primitives::register_app_icons;

use crate::icon_data::PHOTOCRAFT_ICONS;
use crate::state::Tool;

/// Registers the app's own icon set with the framework. The registry is process-wide and ignores a set
/// registered twice, so a second window or a test may call it again.
pub fn register() {
    register_app_icons(&PHOTOCRAFT_ICONS);
}

/// Panel footer buttons share one glyph per action and one size, so the Layers footer and the
/// Swatches, Gradients and Patterns footers read the same, as in Photoshop (#10656).
pub const FOOTER_SIZE: f32 = 26.0;
/// The footer's "new" glyph.
pub const FOOTER_NEW: &str = "square_plus";
/// The footer's "new group" glyph.
pub const FOOTER_NEW_GROUP: &str = "folder";
/// The footer's "delete" glyph.
pub const FOOTER_DELETE: &str = "trash";

/// The icon of tool `t`.
pub fn tool_icon(t: Tool) -> IconId {
    IconId::new(tool_icon_name(t))
}

<<<<<<< HEAD
fn tool_icon_name(t: Tool) -> &'static str {
=======
/// Rasterize the same styled SVG used by the toolbar for a composed native cursor.
pub(crate) fn rasterize(name: &str, side: u32) -> Result<egui::ColorImage, String> {
    if !white_icons().contains(name) {
        return Err(format!("Cursor icon {name:?} is missing"));
    }
    let egui::ImageSource::Bytes { bytes, .. } = white_icons().source(name) else {
        return Err(format!("Cursor icon {name:?} has no embedded SVG bytes"));
    };
    egui_extras::image::load_svg_bytes_with_size(&bytes, egui::load::SizeHint::Width(side), &Default::default())
        .map_err(|error| format!("Could not rasterize cursor icon {name:?}: {error}"))
}

/// Paint an icon centred in `rect`.
pub fn paint(ui: &egui::Ui, rect: Rect, name: &str, size: f32, tint: Color32) {
    let r = Rect::from_center_size(rect.center(), Vec2::splat(size));
    image(name, size, tint).paint_at(ui, r);
}

/// An icon as the pointer, above every window: `name` with its hotspot `hot` (a fraction of the
/// icon box) on `p`, white with a dark outline so it reads on any image. The caller hides the OS
/// cursor (`CursorIcon::None`).
pub fn cursor(ctx: &egui::Context, name: &str, p: egui::Pos2, hot: Vec2, size: f32) {
    let rect = Rect::from_min_size(p - hot * size, Vec2::splat(size));
    let area = egui::Area::new(egui::Id::unique("pc-icon-cursor")).order(egui::Order::Tooltip).fixed_pos(rect.min).constrain(false).interactable(false);
    area.show(ctx, |ui| {
        let outline = image(name, size, Color32::from_black_alpha(200));
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
            outline.paint_at(ui, rect.translate(egui::vec2(dx, dy)));
        }
        image(name, size, Color32::WHITE).paint_at(ui, rect);
    });
}

/// A solid black arrow as the pointer, above every window, its tip on `p` (the reference
/// application's arrow, hotspot at its tip): filled black with a thin light outline so it reads on
/// any image, and the same on every system. The caller hides the OS cursor (`CursorIcon::None`).
pub fn cursor_arrow(ctx: &egui::Context, p: egui::Pos2) {
    paint_arrow(ctx, p, Color32::BLACK, Color32::from_white_alpha(235));
}

/// The same arrow white with a black outline, its tip on `p`: Path Selection's pointer (#13295).
/// The caller hides the OS cursor (`CursorIcon::None`).
pub fn cursor_arrow_white(ctx: &egui::Context, p: egui::Pos2) {
    paint_arrow(ctx, p, Color32::WHITE, Color32::from_black_alpha(235));
}

fn paint_arrow(ctx: &egui::Context, p: egui::Pos2, fill: Color32, edge: Color32) {
    // The outline of the arrow in pixels from its tip, and its split into triangles.
    const OUTLINE: [(f32, f32); 7] = [(0.0, 0.0), (0.0, 16.0), (3.6, 12.6), (6.8, 19.0), (9.6, 17.8), (6.6, 11.6), (11.5, 11.5)];
    const TRIANGLES: [[u32; 3]; 5] = [[0, 1, 2], [0, 2, 5], [0, 5, 6], [2, 3, 4], [2, 4, 5]];
    let points: Vec<egui::Pos2> = OUTLINE.iter().map(|&(x, y)| p + egui::vec2(x, y)).collect();
    // A new id every frame would restart the Area's fade-in each time and leave the arrow pale.
    let area =
        egui::Area::new(egui::Id::unique("pc-arrow-cursor")).order(egui::Order::Tooltip).fixed_pos(p).constrain(false).interactable(false).fade_in(false);
    area.show(ctx, |ui| {
        let vertices = points.iter().map(|&pos| egui::epaint::Vertex { pos, uv: egui::epaint::WHITE_UV, color: fill }).collect();
        let body = egui::Mesh { indices: TRIANGLES.iter().flatten().copied().collect(), vertices, ..Default::default() };
        ui.painter().add(body);
        ui.painter().add(egui::Shape::closed_line(points, egui::Stroke::new(1.0, edge)));
    });
}

/// A small icon beside the OS arrow pointer at `p` (Photoshop's cursor badges: scissors for
/// cutting the selected pixels). Below-right of the arrow, white with a dark outline. The caller
/// keeps the arrow (`CursorIcon::Default`).
pub fn cursor_badge(ctx: &egui::Context, name: &str, p: egui::Pos2) {
    badge(ctx, name, p + egui::vec2(20.0, 19.0), false);
}

/// A small icon below right of a crosshair centred on `p` (the Perspective Crop's idle pointer).
pub fn crosshair_badge(ctx: &egui::Context, name: &str, p: egui::Pos2) {
    badge(ctx, name, p + egui::vec2(13.0, 13.0), false);
}

/// Photoshop's duplicate badge: a second, dark arrowhead behind a white one.
pub fn cursor_copy_badge(ctx: &egui::Context, p: egui::Pos2) {
    badge(ctx, "mouse-pointer-2", p + egui::vec2(20.0, 19.0), true);
}

fn badge(ctx: &egui::Context, name: &str, centre: egui::Pos2, doubled: bool) {
    const SIZE: f32 = 12.0;
    let rect = Rect::from_center_size(centre, Vec2::splat(SIZE));
    let area =
        egui::Area::new(egui::Id::unique(("pc-cursor-badge", name))).order(egui::Order::Tooltip).fixed_pos(rect.min).constrain(false).interactable(false);
    area.show(ctx, |ui| {
        let outline = image(name, SIZE, Color32::from_black_alpha(220));
        let halo = [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)];
        if doubled {
            let behind = rect.translate(egui::vec2(4.0, 4.0));
            for (dx, dy) in halo {
                image(name, SIZE, Color32::WHITE).paint_at(ui, behind.translate(egui::vec2(dx, dy)));
            }
            image(name, SIZE, Color32::BLACK).paint_at(ui, behind);
        }
        for (dx, dy) in halo {
            outline.paint_at(ui, rect.translate(egui::vec2(dx, dy)));
        }
        image(name, SIZE, Color32::WHITE).paint_at(ui, rect);
    });
}

pub fn tool_icon(t: Tool) -> &'static str {
>>>>>>> upstream/main
    match t {
        Tool::Move => "move",
        Tool::Artboard => "artboard",
        Tool::RectMarquee => "square-dashed",
        Tool::EllipseMarquee => "ellipse_dashed",
        Tool::SingleRowMarquee => "marquee_row",
        Tool::SingleColumnMarquee => "marquee_column",
        Tool::Brush => "brush",
        Tool::Pencil => "pencil",
        Tool::MixerBrush => "palette",
        Tool::ColorReplacement => "brush_replace",
        Tool::Eraser => "eraser",
        Tool::BackgroundEraser => "eraser_background",
        Tool::MagicEraser => "eraser_magic",
        Tool::Eyedropper => "pipette",
        Tool::ColorSampler => "sampler",
        Tool::Ruler => "ruler",
        Tool::Note => "message-square",
        Tool::Count => "target",
        Tool::Lasso => "lasso",
        Tool::PolygonLasso => "pentagon",
        Tool::MagneticLasso => "lasso_magnetic",
        Tool::MagicWand => "wand_sparkles",
        Tool::Crop => "crop",
        Tool::PerspectiveCrop => "perspective_crop",
        Tool::Slice => "slice_knife",
        Tool::SliceSelect => "slice_select",
        Tool::Frame => "frame_placeholder",
        Tool::Gradient => "gradient",
        Tool::PaintBucket => "paint_bucket",
        Tool::Type | Tool::VerticalType => "type",
        Tool::HorizontalTypeMask => "type_mask",
        Tool::VerticalTypeMask => "type_mask_vertical",
        Tool::Hand => "hand",
        Tool::RotateView => "compass",
        Tool::Zoom => "zoom_in",
        Tool::SpotHealing | Tool::Healing => "bandage",
        Tool::Remove => "bandage_sparkle",
        Tool::Patch => "lasso_select",
        Tool::ContentAwareMove => "arrow-left-right",
        Tool::RedEye => "eye",
        Tool::CloneStamp => "stamp",
        Tool::PatternStamp => "grid",
        Tool::HistoryBrush => "history-brush",
        Tool::ArtHistoryBrush => "sparkles",
        Tool::Blur => "droplet",
        Tool::Sharpen => "triangle",
        Tool::Smudge => "pointer",
        Tool::Dodge => "lollipop",
        Tool::Burn => "flame",
        Tool::Sponge => "cloud",
        Tool::QuickSelection => "quick_selection",
        Tool::ObjectSelection => "square-dashed-mouse-pointer",
        Tool::Pen => "pen_tool",
        Tool::FreeformPen => "pen_freeform",
        Tool::CurvaturePen => "pen_curvature",
        Tool::AddAnchorPoint => "pen_add_anchor",
        Tool::DeleteAnchorPoint => "pen_delete_anchor",
        Tool::ConvertPoint => "convert_point",
        Tool::PathSelection => "mouse_pointer_2",
        Tool::DirectSelection => "direct_select",
        Tool::Rectangle => "rectangle-horizontal",
        Tool::EllipseShape => "circle",
        Tool::Triangle => "triangle",
        Tool::Polygon => "pentagon",
        Tool::Star => "star",
        Tool::Line => "slash",
        Tool::CustomShape => "cloud",
    }
}

#[cfg(test)]
mod tests {
    use craft::ui::primitives::has_icon;

    use super::*;
    use crate::icon_data::ICONS;

    /// A missing glyph paints nothing and only logs, so a call site left on a renamed (hyphenated)
    /// name is caught here: every name PhotoCraft asks for resolves once the app set is registered.
    #[test]
    fn every_name_photocraft_asks_for_resolves() {
        register();
        register();
        for t in Tool::ALL {
            assert!(has_icon(&tool_icon(t)), "{t:?}: {}", tool_icon(t).name());
        }
        for name in [FOOTER_NEW, FOOTER_NEW_GROUP, FOOTER_DELETE, "symmetry_vertical", "symmetry_horizontal", "symmetry_diagonal", "symmetry_dual"] {
            assert!(has_icon(&IconId::new(name)), "{name}");
        }
        for &(name, _) in ICONS {
            assert!(has_icon(&IconId::new(name)), "{name}");
        }
    }
}
