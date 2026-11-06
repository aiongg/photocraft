//! Built-in profiles PhotoCraft offers that the colour-management library does not carry:
//! ACEScg, and the Dot Gain 20 % compatible gray that is the default working gray.
//! Everything else resolves to the library's `Builtin`.

use std::sync::OnceLock;

use craft::cms::curve::Curve;
use craft::cms::{Builtin, ColorSpace, Profile, math};

/// A built-in profile addressed by a stable id (`aces-cg`, `dot-gain-20-compat`).
#[derive(Clone, Copy)]
pub struct Extra {
    pub id: &'static str,
    pub description: &'static str,
    profile: fn() -> &'static Profile,
}

impl Extra {
    pub fn profile(self) -> &'static Profile {
        (self.profile)()
    }
}

/// ACEScg: AP1 primaries, the ACES white point (~D60), linear transfer.
pub const ACES_CG: Extra = Extra { id: "aces-cg", description: "ACEScg (AP1, linear, PhotoCraft)", profile: aces_cg };
/// Gray matching Photoshop's default working gray (Dot Gain 20 %) with our own curve: a pure
/// gamma of 1.732, fitted on Photoshop 27.11's RGB → Grayscale (sRGB 64 / 128 / 192 and pure red
/// become 46 / 105 / 177 / 107, all within 1 level). Not Adobe's profile.
pub const DOT_GAIN_20: Extra = Extra { id: "dot-gain-20-compat", description: "Dot Gain 20%-compatible (gamma 1.732, PhotoCraft)", profile: dot_gain_20 };

const AP1: [[f64; 2]; 3] = [[0.713, 0.293], [0.165, 0.830], [0.128, 0.044]];

fn aces_cg() -> &'static Profile {
    static CELL: OnceLock<Profile> = OnceLock::new();
    CELL.get_or_init(|| {
        let white = math::xy_to_xyz([0.32168, 0.33767]);
        let mut p = Builtin::LinearSrgb.profile().clone();
        if let Some(m) = math::rgb_to_xyz_matrix(AP1[0], AP1[1], AP1[2], white) {
            // Bradford-adapted to the PCS white, quantized to s15Fixed16, each row nudged so that
            // RGB (1, 1, 1) maps exactly to the stored D50 white.
            let adapt = math::bradford(white, math::D50);
            let d50 = math::d50_quantized();
            let mut q = math::mul(&adapt, &m).map(|row| row.map(math::s15f16_round));
            for (i, row) in q.iter_mut().enumerate() {
                let err = d50[i] - row.iter().sum::<f64>();
                let k = (0..3).max_by(|a, b| row[*a].abs().total_cmp(&row[*b].abs())).unwrap_or(0);
                row[k] = math::s15f16_round(row[k] + err);
            }
            p.matrix = Some(q);
            p.chad = Some(adapt.map(|row| row.map(math::s15f16_round)));
        }
        p.description = ACES_CG.description.into();
        p.with_encoded_bytes()
    })
}

fn dot_gain_20() -> &'static Profile {
    static CELL: OnceLock<Profile> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut p = Builtin::GrayGamma22.profile().clone();
        p.description = DOT_GAIN_20.description.into();
        p.gray_trc = Some(Curve::Gamma(1.732));
        p.with_encoded_bytes()
    })
}

/// The profile a built-in id names: PhotoCraft's extras first, then the library's built-ins.
pub fn from_id(spec: &str) -> Option<&'static Profile> {
    let n: String = spec.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    match n.as_str() {
        "dotgain20" | "dotgain20compat" | "dotgain" => Some(DOT_GAIN_20.profile()),
        "acescg" => Some(ACES_CG.profile()),
        _ if spec.eq_ignore_ascii_case(DOT_GAIN_20.description) => Some(DOT_GAIN_20.profile()),
        _ if spec.eq_ignore_ascii_case(ACES_CG.description) => Some(ACES_CG.profile()),
        _ => Builtin::from_id(spec).map(Builtin::profile),
    }
}

/// Every built-in as `(id, description, profile)`: the library's, then the extras.
pub fn all() -> Vec<(&'static str, &'static str, &'static Profile)> {
    let mut v: Vec<_> = Builtin::ALL.iter().map(|b| (b.id(), b.description(), b.profile())).collect();
    v.extend([ACES_CG, DOT_GAIN_20].map(|e| (e.id, e.description, e.profile())));
    v
}

/// The default working profile of a colour space: the library's, except gray, which is Dot Gain 20 %.
pub fn default_for(cs: ColorSpace) -> Option<&'static Profile> {
    match cs {
        ColorSpace::Gray => Some(DOT_GAIN_20.profile()),
        _ => craft::cms::builtin::default_for(cs),
    }
}
