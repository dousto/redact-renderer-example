use rand::prelude::{IteratorRandom, SliceRandom};
use rand::Rng;
use redact_composer::elements::{Part, PlayNote};
use redact_composer::midi::gm::elements::Instrument;
use redact_composer::musical::elements::{Key, Mode, Scale, TimeSignature};
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::render::context::TimingRelation::During;
use redact_composer::render::{AdhocRenderer, RenderEngine, RendererGroup};
use redact_composer::timing::elements::Tempo;
use redact_composer::util::IntoCompositionSegment;
use redact_composer::{Element, Renderer, Segment};
use serde::{Deserialize, Serialize};

pub fn renderers() -> RenderEngine {
    RenderEngine::new()
        + RandomKey::renderer()
        + RandomTimeSignature::renderer()
        + RandomTempo::renderer()
        + Metronome::renderer()
        + Beat::renderer()
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomKey;

impl RandomKey {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let mut rng = context.rng();
            Ok(vec![Key {
                tonic: rng.gen_range(0..12),
                scale: *Scale::values().choose(&mut rng).unwrap(),
                mode: *Mode::values().choose(&mut rng).unwrap(),
            }
            .into_segment(segment.timing)])
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomTimeSignature;

impl RandomTimeSignature {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let mut rng = context.rng();

            let beats_per_bar = (2..=7).choose(&mut rng).unwrap();
            let beat_length = context.beat_length();

            Ok(vec![TimeSignature {
                beats_per_bar,
                beat_length,
            }
            .into_segment(segment.timing)])
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomTempo;

impl RandomTempo {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, ctx| {
            let mut rng = ctx.rng();

            Ok(vec![
                Tempo::from_bpm(rng.gen_range(100..=160)).into_segment(segment.timing)
            ])
        })
    }
}

/// Plays a woodblock instrument over each beat according to the [`TimeSignature`]. The first
/// beat of each measure is accented with a different tone.
///
/// Mostly useful for debugging, since computers already keep time very well.
#[derive(Element, Debug, Serialize, Deserialize)]
pub struct Metronome;

#[derive(Element, Debug, Serialize, Deserialize)]
pub(super) struct Beat(pub(super) i32);

impl Metronome {
    #[allow(dead_code, clippy::new_ret_no_self)]
    pub fn new() -> impl Element {
        Part::instrument(Metronome)
    }

    pub fn renderer() -> impl Renderer<Element = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::new(|segment, _| {
                Ok(vec![Instrument::Woodblock.into_segment(segment.timing)])
            })
            + AdhocRenderer::<Self>::new(|segment, context| {
                let time_signatures = context
                    .find::<TimeSignature>()
                    .with_timing(During, segment.timing)
                    .require_all()?;

                Ok(time_signatures
                    .iter()
                    .flat_map(|ts_segment| {
                        let ts = ts_segment.element;
                        let tick = 0..ts.beat();

                        Rhythm::from(tick)
                            .iter_over(ts_segment.timing)
                            .enumerate()
                            .map(|(idx, div)| {
                                Segment::new(Beat(idx as i32 % ts.beats_per_bar + 1), div.timing())
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>())
            })
    }
}

impl Beat {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, _| {
            Ok(vec![Segment::new(
                PlayNote {
                    note: if segment.element.0 == 1 { 88 } else { 100 },
                    velocity: 100,
                },
                segment.timing,
            )])
        })
    }
}
