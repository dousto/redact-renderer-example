use crate::chord_progression::ChordMarkers;
use crate::melody;
use crate::melody::Melody;
use crate::structure::PhraseDivider;
use rand::distributions::{Distribution, WeightedIndex};
use rand::prelude::SliceRandom;
use rand::Rng;
use redact_composer::midi::elements::DrumKit;
use redact_composer::midi::gm::{
    elements::{DrumHit, Instrument},
    DrumHitType,
};
use redact_composer::musical::elements::{Chord, Key, TimeSignature};
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::musical::Notes;
use redact_composer::render::context::TimingRelation::{
    BeginningWithin, During, Overlapping, Within,
};
use redact_composer::render::{AdhocRenderer, RenderEngine, RendererGroup};
use redact_composer::util::IntoCompositionSegment;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::iter::once;

pub fn renderers() -> RenderEngine {
    RenderEngine::new()
        + melody::renderers()
        + BassPart::renderer()
        + MelodyPart::renderer()
        + MelodyLine::renderer()
        + DrumPart::renderer()
}

#[non_exhaustive]
#[derive(Element, Serialize, Deserialize, Debug)]
pub struct BassPart {
    pub instrument: Instrument,
}

impl BassPart {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(instrument: Instrument) -> impl Element {
        Melody::new(BassPart { instrument })
    }

    fn renderer() -> impl Renderer<Element = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::new(|segment, _| {
                Ok(vec![segment
                    .element
                    .instrument
                    .into_segment(segment.timing)])
            })
            + AdhocRenderer::<Self>::new(|segment, ctx| {
                let mut rng = ctx.rng();
                let key = ctx
                    .find::<Key>()
                    .with_timing(During, segment.timing)
                    .require()?
                    .element;
                let dividers = ctx
                    .find::<PhraseDivider>()
                    .with_timing(Within, segment.timing)
                    .require_all()?;
                let chords = ctx
                    .find::<Chord>()
                    .within::<ChordMarkers>()
                    .with_timing(Overlapping, &(segment.timing.start..=segment.timing.end))
                    .require_all()?;

                let directives = chords
                    .iter()
                    .flat_map(|ch| {
                        let chord_root = *Notes::from(once(key.note(ch.element.root())))
                            .in_range((key.tonic + 2 * 12 + 6)..=(key.tonic + 4 * 12))
                            .choose(&mut rng)
                            .unwrap();
                        let chord_fifth = *Notes::from(once(key.note(ch.element.fifth())))
                            .in_range((key.tonic + 2 * 12 + 6)..=(key.tonic + 4 * 12))
                            .choose(&mut rng)
                            .unwrap();
                        let preceding_div = dividers
                            .iter()
                            .find(|div| div.timing.end == ch.timing.start);
                        let current_div = dividers
                            .iter()
                            .find(|div| div.timing.contains(&ch.timing.start));

                        let run_to_directive = preceding_div.map(|preceding_div| {
                            Melody::run_to(if rng.gen_bool(0.5) {
                                chord_root
                            } else {
                                chord_fifth
                            })
                            .into_segment(preceding_div.timing)
                        });

                        let key_note_directive = current_div.map(|current_div| {
                            Melody::key_note(chord_root).into_segment(current_div.timing)
                        });

                        once(run_to_directive)
                            .chain(once(key_note_directive))
                            .flatten()
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();

                Ok(directives)
            })
    }
}

#[non_exhaustive]
#[derive(Element, Serialize, Deserialize, Debug)]
pub struct MelodyLine;

impl MelodyLine {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> impl Element {
        Melody::new(MelodyLine)
    }

    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, ctx| {
            let mut rng = ctx.rng();
            let key = ctx
                .find::<Key>()
                .with_timing(During, segment.timing)
                .require()?
                .element;
            let ts = ctx
                .find::<TimeSignature>()
                .with_timing(During, segment.timing)
                .require()?
                .element;
            let chords = ctx
                .find::<Chord>()
                .with_timing(Overlapping, &(segment.timing.start..=segment.timing.end))
                .require_all()?;
            let dividers = ctx
                .find::<PhraseDivider>()
                .with_timing(Overlapping, segment.timing)
                .require_all()?;

            let run_to_notes = dividers
                .iter()
                .flat_map(|div| {
                    chords
                        .iter()
                        .find(|ch| ch.timing.contains(&div.timing.end))
                        .map(|ch| {
                            let run_to_note = *Notes::from(key.chord(ch.element))
                                .in_range((key.tonic + (12 * 4))..=(key.tonic + (12 * 6) + 6))
                                .choose(&mut rng)
                                .unwrap();

                            [
                                Melody::run_to(run_to_note).into_segment(
                                    (div.timing.start + ts.half_beat())..div.timing.end,
                                ),
                                Melody::key_note(run_to_note).into_segment(
                                    div.timing.end..(div.timing.end + ts.half_beat()),
                                ),
                            ]
                        })
                })
                .flatten()
                .collect::<Vec<_>>();

            Ok(run_to_notes.into_iter().collect::<Vec<_>>())
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct MelodyPart {
    instrument: Instrument,
}

impl MelodyPart {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(instrument: Instrument) -> impl Element {
        MelodyPart { instrument }
    }

    pub fn renderer() -> impl Renderer<Element = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::new(|segment, _| {
                Ok(vec![segment
                    .element
                    .instrument
                    .into_segment(segment.timing)])
            })
            + AdhocRenderer::<Self>::new(|segment, _| {
                Ok(vec![MelodyLine::new().into_segment(segment.timing)])
            })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct DrumPart {
    kit: DrumKit,
}

impl DrumPart {
    pub fn new(kit: DrumKit) -> Self {
        Self { kit }
    }

    pub fn renderer() -> impl Renderer<Element = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::new(|segment, _| {
                Ok(vec![segment.element.kit.into_segment(segment.timing)])
            })
            + AdhocRenderer::<Self>::new(|segment, context| {
                let mut rng = context.rng();
                let ts = context
                    .find::<TimeSignature>()
                    .with_timing(During, segment.timing)
                    .require()?
                    .element;
                let dividers = context
                    .find::<PhraseDivider>()
                    .with_timing(BeginningWithin, segment.timing)
                    .require_all()?;

                let mut phrase_lengths = dividers
                    .iter()
                    .map(|div| div.timing.end - div.timing.start)
                    .collect::<Vec<_>>();
                phrase_lengths.sort();
                phrase_lengths.dedup();

                let hit_probabilities = [
                    (DrumHitType::AcousticBassDrum, 1),
                    (DrumHitType::AcousticSnare, 1),
                    (DrumHitType::ClosedHiHat, 8),
                    (DrumHitType::PedalHiHat, 4),
                ];
                let dist = WeightedIndex::new(hit_probabilities.iter().map(|i| i.1)).unwrap();

                let rest_probability = rng.gen_range(0.3..=0.9);

                let drum_beats = phrase_lengths
                    .iter()
                    .map(|l| {
                        let rhythm_precision = ts.quarter_beat();
                        let mut beat_rhythm = Rhythm::random(
                            l - rhythm_precision,
                            ts,
                            |n| {
                                (((n - rhythm_precision) as f32).clamp(0.0, ts.beat() as f32)
                                    / ts.beat() as f32)
                                    .powf(0.1)
                            },
                            |_| rest_probability,
                            &mut rng,
                        );
                        beat_rhythm = Rhythm::from([rhythm_precision]) + beat_rhythm;

                        let hits = beat_rhythm
                            .iter()
                            .map(|_| hit_probabilities[dist.sample(&mut rng)].0)
                            .collect::<Vec<_>>();

                        (*l, (beat_rhythm, hits))
                    })
                    .collect::<HashMap<_, _>>();

                Ok(dividers
                    .iter()
                    .enumerate()
                    .flat_map(|(idx, div)| {
                        if let Some((rhythm, hits)) =
                            drum_beats.get(&(div.timing.end - div.timing.start))
                        {
                            let mut alt_hit_rng = context.rng_with_seed(idx);
                            let mut modified_rhythm = rhythm.clone();
                            let mut modified_hits = hits.clone();
                            let forced_hit =
                                if (div.timing.start - segment.timing.start) % ts.bar() == 0 {
                                    DrumHitType::AcousticBassDrum
                                } else {
                                    [DrumHitType::AcousticSnare, DrumHitType::AcousticBassDrum]
                                        [alt_hit_rng.gen_range(0..=1)]
                                };
                            if hits.first().is_some() {
                                modified_hits[0] = forced_hit;
                                modified_rhythm.0[0].is_rest = false;
                            };

                            modified_rhythm
                                .iter_over(div.timing)
                                .filter(|div| !div.is_rest)
                                .zip(modified_hits.iter().cycle())
                                .map(|(div, drum_hit)| {
                                    DrumHit {
                                        hit: *drum_hit,
                                        velocity: rng.gen_range(90..110),
                                    }
                                    .into_segment(div.timing())
                                })
                                .collect()
                        } else {
                            vec![]
                        }
                    })
                    .collect::<Vec<_>>())
            })
    }
}
