//! #10880: the Layers panel's Effect filter, against the cases measured in Photoshop.

use serde_json::json;

use super::effect::EffectKind;
use super::{Filter, FilterBy};
use crate::state::UiState;

/// One pixel layer per effect kind (the three overlays each on their own), "shadow off" whose
/// Drop Shadow is turned off, and "plain" without effects, over the Background.
fn session() -> photocraft_engine::Session {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    for (name, kind, params) in [
        ("plain", "", json!({})),
        ("bevel", "bevelEmboss", json!({})),
        ("stroke", "stroke", json!({})),
        ("inner shadow", "innerShadow", json!({})),
        ("inner glow", "innerGlow", json!({})),
        ("satin", "satin", json!({})),
        ("color", "colorOverlay", json!({"color": "#ff8000"})),
        ("gradient", "gradientOverlay", json!({})),
        ("pattern", "patternOverlay", json!({})),
        ("outer glow", "outerGlow", json!({})),
        ("shadow", "dropShadow", json!({})),
        ("shadow off", "dropShadow", json!({"enabled": false})),
    ] {
        s.execute("layer.new.layer", json!({"name": name})).unwrap();
        if !kind.is_empty() {
            s.execute("layer.layerStyle.replace", json!({"effects": [{"kind": kind, "params": params}]})).unwrap();
        }
    }
    s
}

use super::tests::listed;

fn by_effect(effect: EffectKind) -> Filter {
    Filter::of(&UiState { layer_filter_by: FilterBy::Effect, layer_filter_effect: effect, ..UiState::default() })
}

#[test]
fn each_effect_lists_the_layers_with_it_enabled() {
    let s = session();
    let doc = &s.active().unwrap().doc;
    for (effect, want) in [
        (EffectKind::BevelEmboss, &["bevel"][..]),
        (EffectKind::Stroke, &["stroke"]),
        (EffectKind::InnerShadow, &["inner shadow"]),
        (EffectKind::InnerGlow, &["inner glow"]),
        (EffectKind::Satin, &["satin"]),
        // Overlay covers Color, Gradient and Pattern Overlay.
        (EffectKind::Overlay, &["pattern", "gradient", "color"]),
        (EffectKind::OuterGlow, &["outer glow"]),
        // A turned-off Drop Shadow doesn't count.
        (EffectKind::DropShadow, &["shadow"]),
    ] {
        assert_eq!(listed(doc, &by_effect(effect)), want, "{effect:?}");
    }
}

#[test]
fn effect_filters_at_once_with_bevel_and_emboss() {
    let f = Filter::of(&UiState { layer_filter_by: FilterBy::Effect, ..UiState::default() });
    assert_eq!(f.effect, EffectKind::BevelEmboss);
}

#[test]
fn ui_set_picks_the_effect_by_id() {
    let mut ui = UiState::default();
    for effect in EffectKind::ALL {
        let f = super::patched(&ui, &json!({"by": "effect", "effect": effect.id()})).unwrap();
        super::store(&mut ui, f);
        assert_eq!((ui.layer_filter_by, ui.layer_filter_effect), (FilterBy::Effect, effect));
    }
    let err = super::patched(&ui, &json!({"effect": "colorOverlay"})).unwrap_err();
    assert!(err.contains("colorOverlay") && err.contains("bevelEmboss") && err.contains("dropShadow"), "{err}");
    assert!(super::patched(&ui, &json!({"effect": 3})).is_err());
    assert!(super::patched(&ui, &json!({"effect": null})).is_err());
}

#[test]
fn saved_settings_without_the_effect_still_load() {
    let mut saved = serde_json::to_value(UiState { layer_filter_effect: EffectKind::Satin, ..UiState::default() }).unwrap();
    assert_eq!(saved["layer_filter_effect"], "satin");
    saved.as_object_mut().unwrap().remove("layer_filter_effect");
    let ui: UiState = serde_json::from_value(saved).unwrap();
    assert_eq!(ui.layer_filter_effect, EffectKind::BevelEmboss);
}
