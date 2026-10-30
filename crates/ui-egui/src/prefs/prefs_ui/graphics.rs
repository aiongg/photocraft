use super::{Map, PhotocraftApp, Value, prefs};

/// Performance › Graphics: the compositor's GPU memory budget and whether the active document's
/// layers fit it (the GPU Memory preference sets it).
pub(super) fn gpu_memory_lines(app: &PhotocraftApp) -> Vec<String> {
    let mut v = Vec::new();
    if app.perf.gpu_budget_mb > 0 {
        let budget = crate::chrome_ui::fmt_bytes(app.perf.gpu_budget_mb.saturating_mul(1 << 20));
        v.push(crate::i18n::fmt(tl!("GPU memory budget: {budget}"), &[("budget", &budget)]));
    }
    let doc = app.session.active().map(|st| st.doc.id.0);
    if let Some(fit) = app.perf.gpu_layer_fit.filter(|f| Some(f.doc) == doc) {
        let args = [("needed", crate::chrome_ui::fmt_bytes(fit.needed)), ("budget", crate::chrome_ui::fmt_bytes(fit.budget))];
        let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
        v.push(if fit.fits() {
            crate::i18n::fmt(tl!("This document's layers: {needed} of {budget} for layers"), &args)
        } else {
            crate::i18n::fmt(tl!("This document's layers need {needed} but may hold {budget}: whole-document redraws use the CPU"), &args)
        });
    }
    v
}

/// Preferences › Performance › Graphics Adapter: which GPU draws on a machine with several. The
/// value is the adapter's name; empty is automatic. A saved adapter that isn't present now (an
/// unplugged eGPU) stays listed, marked ⚠, so opening the dialog doesn't silently change the choice.
pub(super) fn adapter_options(adapters: &[String], current: &str) -> Vec<(String, String)> {
    let mut names: Vec<String> = adapters.to_vec();
    // A saved name fragment ("nvidia") isn't an adapter's full name but still selects one; only a
    // value no listed adapter matches is marked.
    let kept = !current.is_empty() && !names.iter().any(|n| n == current);
    let missing = kept && !names.iter().any(|n| prefs::gpu_adapter_matches(n, current));
    if kept {
        names.push(current.to_string());
    }
    let mut options = vec![(String::new(), tl!("Automatic").to_string())];
    options.extend(names.into_iter().map(|n| {
        let label = if missing && n == current { format!("⚠ {n}") } else { n.clone() };
        (n, label)
    }));
    options
}

/// The Rendering Mode choices: one user-facing setting; legacy flags remain compatible with older settings.
pub(super) fn rendering_mode_options() -> Vec<(String, String)> {
    vec![
        ("auto".to_string(), tl!("Automatic (recommended)").to_string()),
        ("gpu".to_string(), tl!("GPU").to_string()),
        ("cpu".to_string(), tl!("CPU / Compatibility").to_string()),
    ]
}

pub(super) fn rendering_mode_value(obj: &Map<String, Value>) -> String {
    obj.get("renderingMode").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
        if obj.get("useGpu").and_then(Value::as_bool) == Some(false) || obj.get("gpuBackend").and_then(Value::as_str) == Some("cpu") {
            "cpu".into()
        } else {
            "auto".into()
        }
    })
}

/// Settings read only when the app starts, which say "Applies at next launch." under their row.
pub(super) fn read_at_launch(path: &str) -> bool {
    match path {
        // Only Windows and Linux draw their own title bar.
        "interface.systemTitleBar" => cfg!(all(not(target_arch = "wasm32"), not(target_os = "macos"))),
        // The desktop app decides on the control server before its window opens.
        "integrations.allowAgentControl" | "integrations.controlPort" => cfg!(not(target_arch = "wasm32")),
        _ => false,
    }
}
