//! Layers panel tree: the rows in display order (top of the stack first, each group directly
//! above its contents) (#126).
//!
//! A group's disclosure triangle is the library `TreeRow`'s: it opens or closes the group, and
//! ⌥-click sets every group in the document to the new state (Photoshop). The state is the
//! group's `expanded` flag, document data saved in `.pcraft` and as the PSD section divider's
//! open/closed folder type, toggled through `layer.setExpanded` (a view change: no history step).

use photocraft_doc::{Document, Layer, LayerContent};

use crate::PhotocraftApp;

/// Rows as the Layers panel lists them: topmost layer first, a group directly above its
/// children, and the contents of closed groups left out (unless `show_all`, used while the
/// panel is filtered by kind so matches inside closed groups still show).
pub fn display_rows(doc: &Document, show_all: bool) -> Vec<(usize, &Layer)> {
    fn rec<'a>(layers: &'a [Layer], depth: usize, show_all: bool, out: &mut Vec<(usize, &'a Layer)>) {
        for l in layers.iter().rev() {
            out.push((depth, l));
            if let LayerContent::Group(g) = &l.content
                && (g.expanded || show_all)
            {
                rec(&g.children, depth + 1, show_all, out);
            }
        }
    }
    let mut out = Vec::new();
    rec(&doc.layers, 0, show_all, &mut out);
    out
}

/// The Layers panel's rows for `app`: [`display_rows`] (every row while filtered) without a Type
/// Mask session's working text, which is no layer to the user: Photoshop shows only the mask
/// overlay while you type (#3036).
pub fn panel_rows<'a>(app: &PhotocraftApp, doc: &'a Document) -> Vec<(usize, &'a Layer)> {
    let mut rows = display_rows(doc, crate::layer_filter_ui::Filter::of(&app.ui).active());
    if let Some(working) = app.ui.text_edit.as_ref().filter(|ed| ed.mask).map(|ed| ed.layer) {
        rows.retain(|(_, l)| l.id.0 != working);
    }
    rows
}

#[cfg(test)]
mod tests;
