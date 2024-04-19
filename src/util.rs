use rand::prelude::{IteratorRandom, SliceRandom};
use rand::Rng;
use redact_composer::musical::elements::{Key, Mode, Scale, TimeSignature};
use redact_composer::musical::PitchClass;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::timing::elements::Tempo;
use redact_composer::util::IntoSegment;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};

pub fn renderers() -> RenderEngine {
    RenderEngine::new()
        + RandomKey::renderer()
        + RandomTimeSignature::renderer()
        + RandomTempo::renderer()
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomKey;

impl RandomKey {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let mut rng = context.rng();
            let (root, scale, mode) = (
                *PitchClass::values().choose(&mut rng).unwrap(),
                *Scale::values().choose(&mut rng).unwrap(),
                *Mode::values().choose(&mut rng).unwrap(),
            );

            Ok(vec![Key::from((root, scale, mode)).over(segment)])
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomTimeSignature;

impl RandomTimeSignature {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let mut rng = context.rng();

            let beats_per_bar = (2..=7).chain([9, 11, 13]).choose(&mut rng).unwrap();
            let beat_length = context.beat_length();

            Ok(vec![TimeSignature {
                beats_per_bar,
                beat_length,
            }
            .over(segment)])
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomTempo;

impl RandomTempo {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, ctx| {
            let mut rng = ctx.rng();

            Ok(vec![Tempo::from_bpm(rng.gen_range(90..=160)).over(segment)])
        })
    }
}

/// Creates a sawtooth function which will return values between 0.0 to 1.0
pub fn generate_sawtooth_fn(period: f32, offset: f32) -> impl Fn(f32) -> f32 {
    move |t: f32| (t + offset) / period - (0.5 + (t + offset) / period).floor() + 0.5
}

/// Merge two sawtooth functions, with relative amplitudes according to a given ratio (s1/s2).
pub fn merge_sawtooth_fns(
    s1: impl Fn(f32) -> f32,
    s2: impl Fn(f32) -> f32,
    ratio: f32,
) -> impl Fn(f32) -> f32 {
    let (first_scale, second_scale) = if ratio.abs() <= 1.0 {
        (ratio, 1.0 - ratio)
    } else {
        (ratio / (ratio.abs() + 1.0), 1.0 / (ratio.abs() + 1.0))
    };

    move |t: f32| s1(t) * first_scale + s2(t) * second_scale
}
