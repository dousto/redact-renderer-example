use crate::chord_progression::ChordMarkers;
use crate::melody;
use crate::melody::{Melody, MelodyDirective};
use crate::structure::{PhraseDivider, Section};
use crate::util::{generate_sawtooth_fn, merge_sawtooth_fns};
use rand::distributions::{Distribution, WeightedIndex};
use rand::prelude::{IteratorRandom, SliceRandom};
use rand::Rng;
use redact_composer::midi::elements::DrumKit;
use redact_composer::midi::gm::{
    elements::{DrumHit, Instrument},
    DrumHitType,
};
use redact_composer::musical::elements::{Chord, Key, TimeSignature};
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::musical::{Interval, NoteIterator};
use redact_composer::render::context::TimingRelation::{
    BeginningWithin, During, Overlapping, Within,
};
use redact_composer::render::{AdhocRenderer, RenderEngine, RendererGroup};
use redact_composer::util::IntoSegment;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::iter::once;

pub fn renderers() -> RenderEngine {
    RenderEngine::new()
        + melody::renderers()
        + BassPart::renderer()
        + MelodyPart::renderer()
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
            + AdhocRenderer::<Self>::new(|bass_part, _| {
                Ok(vec![bass_part.element.instrument.over(bass_part)])
            })
            + AdhocRenderer::<Self>::new(|bass_part, ctx| {
                let mut rng = ctx.rng();
                let key = ctx
                    .find::<Key>()
                    .with_timing(During, bass_part)
                    .require()?
                    .element;
                let dividers = ctx
                    .find::<PhraseDivider>()
                    .with_timing(Within, bass_part)
                    .require_all()?;
                let chords = ctx
                    .find::<Chord>()
                    .within::<ChordMarkers>()
                    .with_timing(Overlapping, bass_part)
                    .require_all()?;

                let directives = chords
                    .iter()
                    .flat_map(|ch| {
                        let note_range =
                            key.root().in_octave(2)..(key.root().in_octave(3) + Interval(8));
                        let run_to_note = [Interval::P1, Interval::P4, Interval::P5]
                            .into_iter()
                            .map(|i| ch.element.root() + i)
                            .filter(|pc| key.contains(pc))
                            .flat_map(|pc| pc.iter_notes_in_range(note_range.clone()))
                            .choose(&mut rng)
                            .unwrap();
                        let key_note = *ch
                            .element
                            .root()
                            .notes_in_range(note_range.clone())
                            .choose(&mut rng)
                            .unwrap();
                        let preceding_div = dividers
                            .iter()
                            .find(|div| div.timing.end == ch.timing.start);
                        let current_div = dividers
                            .iter()
                            .find(|div| div.timing.contains(&ch.timing.start));

                        let run_to_directive = preceding_div
                            .map(|preceding_div| Melody::run_to(run_to_note).over(preceding_div));

                        let key_note_directive = current_div
                            .map(|current_div| Melody::key_note(key_note).over(current_div));

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

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct MelodyPart {
    instrument: Instrument,
}

impl MelodyPart {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(instrument: Instrument) -> impl Element {
        Melody::new(MelodyPart { instrument })
    }

    pub fn renderer() -> impl Renderer<Element = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::new(|melody_part, _| {
                Ok(vec![melody_part.element.instrument.over(melody_part)])
            })
            + AdhocRenderer::<Self>::new(|melody_part, ctx| {
                let mut rng = ctx.rng();
                let ts = ctx
                    .find::<TimeSignature>()
                    .with_timing(During, melody_part)
                    .require()?
                    .element;
                let chords = ctx
                    .find::<Chord>()
                    .with_timing(Overlapping, melody_part)
                    .require_all()?;
                let dividers = ctx
                    .find::<PhraseDivider>()
                    .with_timing(Overlapping, melody_part)
                    .require_all()?;
                let section = ctx
                    .find::<Section>()
                    .with_timing(During, melody_part)
                    .require()?;
                let existing_key_notes = ctx
                    .find::<MelodyDirective>()
                    .within::<MelodyPart>()
                    .with_timing(Within, melody_part)
                    .matching(|dir| matches!(dir, MelodyDirective::KeyNote(_)))
                    .get_all()
                    .unwrap_or_default();

                let period = melody_part.timing.len() / rng.gen_range(1..=8);
                let offset = rng.gen_range(0..period);
                let msawtooth = generate_sawtooth_fn(period as f32, offset as f32);
                let period = section.timing.len() / rng.gen_range(1..=8);
                let offset = rng.gen_range(0..period);
                let ssawtooth = generate_sawtooth_fn(period as f32, offset as f32);
                let combsaw = merge_sawtooth_fns(msawtooth, ssawtooth, 1.0);

                let note_starts = dividers
                    .iter()
                    .flat_map(|div| {
                        let chord = chords
                            .iter()
                            .find(|ch| ch.timing.contains(&div.timing.start))
                            .map(|segment| segment.element)?;

                        let note_choices = chord
                            .iter_notes_in_range(
                                chord.root().in_octave(3)..=chord.root().in_octave(5),
                            )
                            .filter(|n| {
                                // Randomly remove note choices with probability corresponding to the number
                                // of other parts playing the same pitch
                                let overlaps = existing_key_notes
                                    .iter()
                                    .filter(|dir| dir.timing.intersects(div))
                                    .filter(|dir| match dir.element {
                                        MelodyDirective::RunTo(_) => false,
                                        MelodyDirective::KeyNote(kn) => {
                                            kn.pitch_class() == n.pitch_class()
                                        }
                                    })
                                    .count();

                                rng.gen_bool(0.5_f64.powf(overlaps as f64))
                            })
                            .collect::<Vec<_>>();
                        let start = combsaw((div.timing.start - melody_part.timing.start) as f32);
                        let start_note = note_choices[(start * note_choices.len() as f32) as usize];
                        Some((start_note, div.timing.start))
                    })
                    .collect::<Vec<_>>();

                let key_notes = note_starts
                    .iter()
                    .map(|(note, time)| {
                        Melody::key_note(*note).over(*time..(time + ts.half_beat()))
                    })
                    .collect::<Vec<_>>();
                let run_to_notes = note_starts
                    .windows(2)
                    .map(|notes| Melody::run_to(notes[1].0).over(notes[0].1..notes[1].1))
                    .collect::<Vec<_>>();

                Ok(key_notes
                    .into_iter()
                    .chain(run_to_notes)
                    .collect::<Vec<_>>())
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
            + AdhocRenderer::<Self>::new(|drum_part, _| {
                Ok(vec![drum_part.element.kit.over(drum_part)])
            })
            + AdhocRenderer::<Self>::new(|drum_part, context| {
                let mut rng = context.rng();
                let ts = context
                    .find::<TimeSignature>()
                    .with_timing(During, drum_part)
                    .require()?
                    .element;
                let dividers = context
                    .find::<PhraseDivider>()
                    .with_timing(BeginningWithin, drum_part)
                    .require_all()?;

                let mut phrase_lengths = dividers
                    .iter()
                    .map(|div| div.timing.len())
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
                        if let Some((rhythm, hits)) = drum_beats.get(&div.timing.len()) {
                            let mut alt_hit_rng = context.rng_with_seed(idx);
                            let mut modified_rhythm = rhythm.clone();
                            let mut modified_hits = hits.clone();
                            let forced_hit =
                                if (div.timing.start - drum_part.timing.start) % ts.bar() == 0 {
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
                                .iter_over(div)
                                .filter(|div| !div.is_rest)
                                .zip(modified_hits.iter().cycle())
                                .map(|(div, drum_hit)| {
                                    DrumHit {
                                        hit: *drum_hit,
                                        velocity: rng.gen_range(90..110),
                                    }
                                    .over(div)
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
