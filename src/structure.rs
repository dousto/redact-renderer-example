use crate::chord_progression::{ChordMarkers, RandomChordProgression};
use crate::parts::{BassPart, DrumPart, MelodyPart};
use crate::Instrumentation;
use rand::prelude::IteratorRandom;
use rand::Rng;
use redact_composer::elements::Part;
use redact_composer::musical::elements::TimeSignature;
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::render::context::TimingRelation::During;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::timing::TimingSequenceUtil;
use redact_composer::util::{IntoCompositionSegment, RangeOps};
use redact_composer::SegmentRef;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};
use std::iter::once;
use std::ops::Range;

pub fn renderers() -> RenderEngine {
    RenderEngine::new() + Sections::renderer() + Section::renderer()
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct Sections;

impl Sections {
    fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let mut rng = context.rng();
            let ts = context
                .find::<TimeSignature>()
                .with_timing(During, segment.timing)
                .require()?
                .element;

            let min_section_length = ts.bars(8);
            let trimmed_len =
                segment.timing.len() - segment.timing.len() % (min_section_length * 2);
            if trimmed_len <= min_section_length {
                Ok(vec![Section.into_segment(segment.timing)])
            } else {
                let num_splits = (2..=6)
                    .filter(|divisor| trimmed_len % (divisor * min_section_length) == 0)
                    .choose(&mut rng)
                    .unwrap_or(0);

                if num_splits == 0 {
                    Ok(Vec::new())
                } else {
                    Ok(Rhythm::from([trimmed_len / num_splits])
                        .iter_over(segment.timing) // Check this
                        .map(|div| {
                            Sections.into_named_segment(
                                rng.gen_range(0..num_splits).to_string(),
                                div.timing(),
                            )
                        })
                        .collect::<Vec<_>>())
                }
            }
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct Section;

impl Section {
    fn renderer() -> impl Renderer<Element = Section> {
        AdhocRenderer::<Self>::new(|segment, ctx| {
            let mut rng = ctx.rng();
            let instrumentation = ctx
                .find::<Instrumentation>()
                .with_timing(During, segment.timing)
                .require()?
                .element;
            let ts = ctx
                .find::<TimeSignature>()
                .with_timing(During, segment.timing)
                .require()?
                .element;

            let dividers = {
                let rhythm = Rhythm::random_with_subdivisions_weights(
                    ts.bar(),
                    &(1..=ts.beats_per_bar)
                        .map(|n| (vec![ts.half_beat() * n], n))
                        .collect::<Vec<_>>(),
                    &mut ctx.rng_with_seed("dividers"),
                );

                rhythm
                    .iter_over(segment.timing)
                    .map(|div| PhraseDivider.into_segment(div.timing()))
                    .collect::<Vec<_>>()
            };
            let typed_dividers = dividers
                .iter()
                .flat_map(|div| div.try_into().ok())
                .collect::<Vec<SegmentRef<PhraseDivider>>>();

            let bass_parts = segment
                .timing
                .divide_into(segment.timing.len() / 4)
                .into_iter()
                .map(|r| {
                    Part::instrument(BassPart::new(instrumentation.bass))
                        .into_named_segment("Bass".to_string(), r)
                });

            let drum_parts = segment
                .timing
                .divide_into(segment.timing.len() / 4)
                .into_iter()
                .map(|r| {
                    Part::percussion(DrumPart::new(instrumentation.drums))
                        .into_named_segment("Drums".to_string(), r)
                });

            let period = segment.timing.len() as f32 / 4.0;
            let offset = rng.gen_range(0.0..period);

            let sawtooth =
                |t: f32| (t + offset) / period - (0.5 + (t + offset) / period).floor() + 0.5;

            let melody_parts3 = once(&instrumentation.melody)
                .chain(instrumentation.extras.iter())
                .enumerate()
                .flat_map(|(idx, inst)| {
                    let activation: Range<f32> = if idx == 0 {
                        0.0..0.7
                    } else if idx == 1 {
                        0.6..0.8
                    } else {
                        0.8..1.0
                    };

                    let play_times = typed_dividers
                        .iter()
                        .filter(|div| {
                            let s_start = sawtooth(div.timing.start as f32);
                            let s_end = sawtooth(div.timing.end as f32);
                            activation.intersects(&(s_start..s_end))
                                || s_start > s_end
                                    && (activation.intersects(&(s_start..1.0))
                                        || activation.intersects(&(0.0..s_end)))
                        })
                        .map(|div| *div.timing)
                        .collect::<Vec<_>>()
                        .join();

                    play_times
                        .into_iter()
                        .map(|t| {
                            Part::instrument(MelodyPart::new(*inst)).into_named_segment(
                                ((idx as f32 * sawtooth(t.start as f32)) as i32).to_string(),
                                t,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            Ok(vec![
                ChordMarkers.into_segment(segment.timing),
                RandomChordProgression.into_segment(segment.timing),
            ]
            .into_iter()
            .chain(dividers)
            .chain(bass_parts)
            .chain(drum_parts)
            .chain(melody_parts3)
            .collect::<Vec<_>>())
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug, Copy, Clone)]
pub struct PhraseDivider;
