use crate::chord_progression::{ChordMarkers, RandomChordProgression};
use crate::parts::{BassPart, DrumPart, MelodyPart};
use crate::util::generate_sawtooth_fn;
use crate::Instrumentation;
use rand::prelude::IteratorRandom;
use rand::Rng;
use redact_composer::elements::Part;
use redact_composer::musical::elements::TimeSignature;
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::render::context::TimingRelation::During;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::timing::TimingSequenceUtil;
use redact_composer::util::{IntoSegment, RangeOps};
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
        AdhocRenderer::<Self>::new(|sections, context| {
            let mut rng = context.rng();
            let ts = context
                .find::<TimeSignature>()
                .with_timing(During, &sections)
                .require()?
                .element;

            let min_section_length = ts.bars(8);
            let trimmed_len =
                sections.timing.len() - sections.timing.len() % (min_section_length * 2);
            if trimmed_len <= min_section_length {
                Ok(vec![Section.over(sections)])
            } else {
                let num_splits = (2..=6)
                    .filter(|divisor| trimmed_len % (divisor * min_section_length) == 0)
                    .choose(&mut rng)
                    .unwrap_or(0);

                if num_splits == 0 {
                    Ok(Vec::new())
                } else {
                    Ok(Rhythm::from([trimmed_len / num_splits])
                        .iter_over(sections)
                        .map(|div| {
                            Sections
                                .over(div)
                                .named(rng.gen_range(0..num_splits).to_string())
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
        AdhocRenderer::<Self>::new(|section, ctx| {
            let mut rng = ctx.rng();
            let instrumentation = ctx
                .find::<Instrumentation>()
                .with_timing(During, &section)
                .require()?
                .element;
            let ts = ctx
                .find::<TimeSignature>()
                .with_timing(During, &section)
                .require()?
                .element;

            let dividers = {
                let rhythm = Rhythm::random_with_subdivisions_weights(
                    2 * ts.bar(),
                    &(1..=ts.beats_per_bar)
                        .map(|n| (vec![ts.half_beat() * n], 1))
                        .collect::<Vec<_>>(),
                    &mut ctx.rng_with_seed("dividers"),
                );

                rhythm
                    .iter_over(section)
                    .map(|div| PhraseDivider.over(div))
                    .collect::<Vec<_>>()
            };
            let typed_dividers = dividers
                .iter()
                .flat_map(|div| div.try_into().ok())
                .collect::<Vec<SegmentRef<PhraseDivider>>>();

            let bass_parts = section
                .timing
                .divide_into(section.timing.len() / 4)
                .into_iter()
                .map(|divided_timing| {
                    Part::instrument(BassPart::new(instrumentation.bass))
                        .over(divided_timing)
                        .named("Bass".to_string())
                });

            let drum_parts = section
                .timing
                .divide_into(section.timing.len() / 4)
                .into_iter()
                .map(|divided_timing| {
                    Part::percussion(DrumPart::new(instrumentation.drums))
                        .over(divided_timing)
                        .named("Drums".to_string())
                });

            let period = section.timing.len() as f32 / 4.0;
            let offset = rng.gen_range(0.0..period);
            let sawtooth = generate_sawtooth_fn(period, offset);

            let melody_parts3 = once(&instrumentation.melody)
                .chain(instrumentation.extras.iter())
                .enumerate()
                .flat_map(|(idx, inst)| {
                    let activation: Range<f32> = if idx == 0 {
                        0.0..0.7
                    } else if idx == 1 {
                        0.6..0.8
                    } else {
                        0.7..1.0
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
                        .map(|play_timing| {
                            Part::instrument(MelodyPart::new(*inst))
                                .over(play_timing)
                                .named(
                                    ((idx as f32 * sawtooth(play_timing.start as f32)) as i32)
                                        .to_string(),
                                )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            Ok(vec![
                ChordMarkers.over(section),
                RandomChordProgression.over(section),
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
