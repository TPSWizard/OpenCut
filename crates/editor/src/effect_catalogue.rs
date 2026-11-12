//! The effects both editors offer, with their parameters: the desktop
//! inspector draws its rows from this, the Editor API lists it, and the
//! web app's `apps/web/src/effects/definitions` carries the same keys,
//! defaults and ranges. The shaders live in `crates/effects`.

use crate::document::Effect;
use crate::params::Params;

/// A number parameter, or a choice when `options` is not empty (stored as
/// the option's value string).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// `(value, label)` pairs for a choice; the default is `options[default as usize]`.
    pub options: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectSpec {
    pub kind: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub params: &'static [EffectParamSpec],
}

const fn number(key: &'static str, label: &'static str, default: f64, min: f64, max: f64, step: f64) -> EffectParamSpec {
    EffectParamSpec { key, label, default, min, max, step, options: &[] }
}

pub const EFFECT_CATALOGUE: &[EffectSpec] = &[
    EffectSpec {
        kind: "blur",
        label: "Blur",
        description: "Softens the whole element.",
        params: &[number("intensity", "Intensity", 15.0, 0.0, 100.0, 1.0)],
    },
    EffectSpec {
        kind: "color",
        label: "Colour correction",
        description: "Brightness, contrast, saturation, hue, exposure, gamma, warmth and tint.",
        params: &[
            number("brightness", "Brightness", 0.0, -1.0, 1.0, 0.01),
            number("contrast", "Contrast", 0.0, -1.0, 1.0, 0.01),
            number("saturation", "Saturation", 0.0, -1.0, 1.0, 0.01),
            number("hue", "Hue (°)", 0.0, -180.0, 180.0, 1.0),
            number("exposure", "Exposure (stops)", 0.0, -3.0, 3.0, 0.05),
            number("gamma", "Gamma", 1.0, 0.2, 3.0, 0.01),
            number("temperature", "Warmth", 0.0, -1.0, 1.0, 0.01),
            number("tint", "Tint", 0.0, -1.0, 1.0, 0.01),
        ],
    },
    EffectSpec {
        kind: "vignette",
        label: "Vignette",
        description: "Darkens the corners.",
        params: &[
            number("amount", "Amount", 0.5, 0.0, 1.0, 0.01),
            number("radius", "Radius", 0.6, 0.0, 1.5, 0.01),
            number("softness", "Softness", 0.5, 0.0, 1.0, 0.01),
        ],
    },
    EffectSpec {
        kind: "grain",
        label: "Film grain",
        description: "Grain that changes every frame.",
        params: &[
            number("amount", "Amount", 0.3, 0.0, 1.0, 0.01),
            number("size", "Grain size", 2.0, 1.0, 8.0, 0.5),
            number("seed", "Seed", 0.0, 0.0, 100.0, 1.0),
        ],
    },
    EffectSpec {
        kind: "rgb-split",
        label: "RGB split",
        description: "Pulls the red and blue channels apart.",
        params: &[
            number("amount", "Offset (px)", 6.0, 0.0, 50.0, 0.5),
            number("angle", "Angle (°)", 0.0, 0.0, 360.0, 1.0),
        ],
    },
    EffectSpec {
        kind: "pixelate",
        label: "Pixelate",
        description: "Mosaic of flat cells.",
        params: &[number("size", "Cell size (px)", 16.0, 2.0, 200.0, 1.0)],
    },
    EffectSpec {
        kind: "sharpen",
        label: "Sharpen",
        description: "Boosts edges.",
        params: &[number("amount", "Amount", 0.5, 0.0, 1.0, 0.01)],
    },
    EffectSpec {
        kind: "mirror",
        label: "Mirror",
        description: "Reflects one half onto the other.",
        params: &[EffectParamSpec {
            key: "mode",
            label: "Side",
            default: 0.0,
            min: 0.0,
            max: 3.0,
            step: 1.0,
            options: &[
                ("left", "Left onto right"),
                ("right", "Right onto left"),
                ("top", "Top onto bottom"),
                ("bottom", "Bottom onto top"),
            ],
        }],
    },
    EffectSpec {
        kind: "glitch",
        label: "Glitch",
        description: "Bands slide and tear a dozen times a second.",
        params: &[
            number("amount", "Amount", 0.5, 0.0, 1.0, 0.01),
            number("bands", "Bands", 16.0, 4.0, 64.0, 1.0),
            number("seed", "Seed", 0.0, 0.0, 100.0, 1.0),
        ],
    },
    EffectSpec {
        kind: "kaleidoscope",
        label: "Kaleidoscope",
        description: "Mirrored wedges around the centre.",
        params: &[
            number("segments", "Segments", 6.0, 2.0, 16.0, 1.0),
            number("angle", "Rotation (°)", 0.0, 0.0, 360.0, 1.0),
        ],
    },
    EffectSpec {
        kind: "lift-gamma-gain",
        label: "Lift, gamma, gain",
        description: "Three-way colour: shadows, midtones and highlights, each with red, green and blue offsets.",
        params: &[
            number("lift", "Lift", 0.0, -1.0, 1.0, 0.01),
            number("liftRed", "Lift red", 0.0, -1.0, 1.0, 0.01),
            number("liftGreen", "Lift green", 0.0, -1.0, 1.0, 0.01),
            number("liftBlue", "Lift blue", 0.0, -1.0, 1.0, 0.01),
            number("gamma", "Gamma", 1.0, 0.2, 3.0, 0.01),
            number("gammaRed", "Gamma red", 0.0, -1.0, 1.0, 0.01),
            number("gammaGreen", "Gamma green", 0.0, -1.0, 1.0, 0.01),
            number("gammaBlue", "Gamma blue", 0.0, -1.0, 1.0, 0.01),
            number("gain", "Gain", 1.0, 0.0, 3.0, 0.01),
            number("gainRed", "Gain red", 0.0, -1.0, 1.0, 0.01),
            number("gainGreen", "Gain green", 0.0, -1.0, 1.0, 0.01),
            number("gainBlue", "Gain blue", 0.0, -1.0, 1.0, 0.01),
        ],
    },
    EffectSpec {
        kind: "curves",
        label: "Curves",
        description: "A master tone curve through five points and a curve per channel through three; the output at each input level.",
        params: &[
            number("master0", "Master at 0 %", 0.0, 0.0, 1.0, 0.01),
            number("master25", "Master at 25 %", 0.25, 0.0, 1.0, 0.01),
            number("master50", "Master at 50 %", 0.5, 0.0, 1.0, 0.01),
            number("master75", "Master at 75 %", 0.75, 0.0, 1.0, 0.01),
            number("master100", "Master at 100 %", 1.0, 0.0, 1.0, 0.01),
            number("red25", "Red at 25 %", 0.25, 0.0, 1.0, 0.01),
            number("red50", "Red at 50 %", 0.5, 0.0, 1.0, 0.01),
            number("red75", "Red at 75 %", 0.75, 0.0, 1.0, 0.01),
            number("green25", "Green at 25 %", 0.25, 0.0, 1.0, 0.01),
            number("green50", "Green at 50 %", 0.5, 0.0, 1.0, 0.01),
            number("green75", "Green at 75 %", 0.75, 0.0, 1.0, 0.01),
            number("blue25", "Blue at 25 %", 0.25, 0.0, 1.0, 0.01),
            number("blue50", "Blue at 50 %", 0.5, 0.0, 1.0, 0.01),
            number("blue75", "Blue at 75 %", 0.75, 0.0, 1.0, 0.01),
        ],
    },
    EffectSpec {
        kind: "levels",
        label: "Levels",
        description: "Input black and white points, a midtone gamma, and output black and white points.",
        params: &[
            number("inputBlack", "Input black", 0.0, 0.0, 1.0, 0.01),
            number("inputWhite", "Input white", 1.0, 0.0, 1.0, 0.01),
            number("gamma", "Gamma", 1.0, 0.1, 10.0, 0.01),
            number("outputBlack", "Output black", 0.0, 0.0, 1.0, 0.01),
            number("outputWhite", "Output white", 1.0, 0.0, 1.0, 0.01),
        ],
    },
    EffectSpec {
        kind: "shadows-highlights",
        label: "Shadows and highlights",
        description: "Brightens or darkens the dark and the bright parts separately.",
        params: &[
            number("shadows", "Shadows", 0.0, -1.0, 1.0, 0.01),
            number("highlights", "Highlights", 0.0, -1.0, 1.0, 0.01),
            number("range", "Range", 0.5, 0.0, 1.0, 0.01),
        ],
    },
];

pub fn effect_spec(kind: &str) -> Option<&'static EffectSpec> {
    EFFECT_CATALOGUE.iter().find(|spec| spec.kind == kind)
}

impl EffectParamSpec {
    pub fn is_choice(&self) -> bool {
        !self.options.is_empty()
    }

    /// The index of a choice's stored value, or the default's.
    pub fn choice_index(&self, value: &str) -> usize {
        self.options
            .iter()
            .position(|(option, _)| *option == value)
            .unwrap_or(self.default as usize)
    }
}

impl Effect {
    /// A new effect of `kind` at the catalogue's defaults, or none for an
    /// unknown kind.
    pub fn from_catalogue(kind: &str) -> Option<Self> {
        let spec = effect_spec(kind)?;
        let mut params = Params::new();
        for param in spec.params {
            if param.is_choice() {
                params.set(param.key, param.options[param.default as usize].0);
            } else {
                params.set(param.key, param.default);
            }
        }
        Some(Self {
            id: crate::new_id(),
            kind: kind.to_owned(),
            params,
            enabled: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_builds_every_effect_at_its_defaults() {
        for spec in EFFECT_CATALOGUE {
            let effect = Effect::from_catalogue(spec.kind).unwrap();
            assert_eq!(effect.kind, spec.kind);
            for param in spec.params {
                if param.is_choice() {
                    assert_eq!(effect.params.text(param.key, ""), param.options[0].0);
                } else {
                    assert_eq!(effect.params.number(param.key, f64::NAN), param.default);
                    assert!(param.min <= param.default && param.default <= param.max, "{}.{}", spec.kind, param.key);
                }
            }
        }
        assert!(Effect::from_catalogue("sepia").is_none());
        let blur = Effect::from_catalogue("blur").unwrap();
        assert_eq!(blur.params, Effect::blur().params, "the catalogue's blur matches the old constructor");
    }
}
