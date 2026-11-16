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

fn tool_icon_name(t: Tool) -> &'static str {
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
