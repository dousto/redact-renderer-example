use rand::distributions::{Distribution, WeightedIndex};
use rand::Rng;
use redact_composer::error::RendererError;
use redact_composer::musical::elements::{Chord, TimeSignature};
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::musical::{ChordShape, PitchClass};
use redact_composer::musical::{Interval, Key, PitchClassCollection};
use redact_composer::render::context::TimingRelation::During;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::util::IntoSegment;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering::Less;

pub fn renderers() -> RenderEngine {
    RenderEngine::new() + RandomChordProgression::renderer() + ChordMarkers::renderer()
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct ChordProgression {
    pub chords: Vec<Chord>,
    pub rhythm: Rhythm,
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomChordProgression;

impl RandomChordProgression {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let mut rng = context.rng();
            let ts = context
                .find::<TimeSignature>()
                .with_timing(During, segment)
                .require()?
                .element;
            let key = context
                .find::<Key>()
                .with_timing(During, segment)
                .require()?
                .element;

            // High level logic:
            // 1. Lay out chord choices and weigh them based on a transition function
            // 2. Trim off the lower percentile (< echelon) and choose one from the remaining
            // 3. Repeat 1-2 until max_chords is reached.
            // 4. Choose a chord between min_chords and max_chords that transitions well back to the
            //    first chord. Discard the chords after this one.
            // 5. Assign a random rhythm to the remaining chords and return the progression.
            let echelon = rng.gen_range(0.2..1.0);
            let min_chords = 2;
            let max_chords = 6;

            let all_chord_choices = key.chords_with_shape(ChordShape::triad());
            let mut next_chord_choices: Vec<(&Chord, f32)> = all_chord_choices
                .iter()
                .map(|ch| (ch, Self::transition_weight(key, Some(ch), ch)))
                .collect();
            next_chord_choices.sort_unstable_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Less));
            let mut chosen_chords = vec![];
            let mut cyclability = vec![];

            while chosen_chords.len() <= max_chords {
                let last_chord = {
                    let (_, min_weight) = next_chord_choices
                        [(next_chord_choices.len() as f32 * echelon).floor() as usize];
                    let top_choices = next_chord_choices
                        .iter()
                        .skip_while(|(_, w)| w < &min_weight)
                        .copied()
                        .collect::<Vec<_>>();
                    let dist =
                        WeightedIndex::new(top_choices.iter().map(|(_, w)| w)).map_err(|_| {
                            RendererError::MissingContext(String::from("No next chords."))
                        })?;

                    top_choices[dist.sample(&mut rng)].0
                };

                if chosen_chords.len() >= min_chords {
                    cyclability.push(Self::transition_weight(
                        key,
                        Some(last_chord),
                        &chosen_chords[0],
                    ));
                }

                chosen_chords.push(*last_chord);

                // Prepare next choices
                next_chord_choices = all_chord_choices
                    .iter()
                    .map(|ch| (ch, Self::transition_weight(key, Some(last_chord), ch)))
                    .collect::<Vec<_>>();
                next_chord_choices
                    .sort_unstable_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Less));
            }

            // Choose a decent point to cycle the progression
            let cycle_weighted_indices = cyclability.into_iter().enumerate().collect::<Vec<_>>();
            let dist = WeightedIndex::new(
                cycle_weighted_indices
                    .iter()
                    // Polarize the weights to increase the likelihood of a good one
                    .map(|(_, w)| w.powf(2.0)),
            )
            .map_err(|_| RendererError::MissingContext(String::from("No cycle choices.")))?;
            let cycle_idx = cycle_weighted_indices[dist.sample(&mut rng)].0;

            chosen_chords = chosen_chords
                .into_iter()
                .take(min_chords + cycle_idx)
                .collect();

            let rhythm =
                Rhythm::balanced_timing(ts.bars(4), chosen_chords.len() as i32, ts, &mut rng);

            Ok(vec![ChordProgression {
                chords: chosen_chords,
                rhythm,
            }
            .over(segment)])
        })
    }
    fn transition_weight(key: &Key, current: Option<&Chord>, candidate: &Chord) -> f32 {
        let (base_weight, current) = if let Some(current) = current {
            if current.root() == candidate.root() {
                (0.1, current)
            } else {
                (1.0, current)
            }
        } else {
            (1.0, candidate)
        };

        let current_pitches = current.pitch_classes();

        let steps = current_pitches
            .iter()
            .map(|cp| {
                candidate
                    .pitch_classes()
                    .iter()
                    .map(|tp| cp.interval_to(tp).min(cp.interval_from(tp)))
                    .min()
                    .unwrap_or(Interval(0))
            })
            .sum::<Interval>()
            .0;

        fn harmony_from_focal_pitch(chord: &Chord, focal_pitch: &PitchClass) -> f32 {
            chord
                .pitch_classes()
                .iter()
                .map(|p| {
                    RandomChordProgression::harmonic_presence(chord.root().interval_to(p))
                        * RandomChordProgression::harmonic_presence(focal_pitch.interval_to(p)).min(
                            RandomChordProgression::harmonic_presence(focal_pitch.interval_from(p)),
                        )
                })
                .sum::<f32>()
                / chord.pitch_classes().len() as f32
        }

        let key_harm = harmony_from_focal_pitch(candidate, &key.root());
        let from_chord_harm = harmony_from_focal_pitch(candidate, &current.root());
        let to_chord_harm = harmony_from_focal_pitch(candidate, &candidate.root());

        base_weight
            * 0.6_f32.powf(steps as f32)
            * to_chord_harm
            * (0.5 * key_harm + 0.5 * from_chord_harm)
    }

    // Approximate sum of a particular interval's occurrences in the harmonic series,
    // normalized to `0.0..=1.0`, then square root it for a more linear curve.
    fn harmonic_presence(i: Interval) -> f32 {
        ((match i.to_simple() {
            Interval::P1 => 2.0,
            Interval::m2 => 0.059375,
            Interval::M2 => 0.18,
            Interval::m3 => 0.06125,
            Interval::M3 => 0.37625,
            Interval::P4 => 0.044375,
            Interval::TT => 0.140625,
            Interval::P5 => 1.0,
            Interval::m6 => 0.15625,
            Interval::M6 => 0.05875,
            Interval::m7 => 0.345625,
            Interval::M7 => 0.199375,
            _ => unreachable!(),
        }) / 2.0f32)
            .powf(0.5)
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct ChordMarkers;

impl ChordMarkers {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let chord_progression = context
                .find::<ChordProgression>()
                .with_timing(During, segment)
                .require()?
                .element;

            let (chords, rhythm) = (&chord_progression.chords, &chord_progression.rhythm);

            Ok(chords
                .iter()
                .cycle()
                .zip(rhythm.iter_over(segment).filter(|div| !div.is_rest))
                .map(|(chord, div)| chord.over(div))
                .collect())
        })
    }
}
