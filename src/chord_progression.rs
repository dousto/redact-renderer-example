use rand::seq::SliceRandom;
use redact_composer::musical::elements::{Chord, TimeSignature};
use redact_composer::musical::rhythm::Rhythm;
use redact_composer::render::context::TimingRelation::During;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::util::IntoCompositionSegment;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
                .with_timing(During, segment.timing)
                .require()?
                .element;

            // Define a map of possible chords transitions
            let chord_map: HashMap<Chord, Vec<Chord>> = HashMap::from([
                (Chord::I, Chord::values()),
                (Chord::II, vec![Chord::III, Chord::V, Chord::VI]),
                (
                    Chord::III,
                    vec![Chord::II, Chord::IV, Chord::VI, Chord::VII],
                ),
                (Chord::IV, vec![Chord::I, Chord::V, Chord::VI, Chord::VII]),
                (Chord::V, vec![Chord::I, Chord::II, Chord::IV, Chord::VI]),
                (Chord::VI, vec![Chord::II, Chord::III, Chord::IV, Chord::V]),
                (Chord::VII, vec![Chord::I, Chord::III, Chord::VI]),
            ]);

            // Starting from Chord::I or Chord::V, add additional chords based on the possible transitions
            // Make sure the last chord can transition back to the starting chord, enabling nice repetition
            let start_chord = *[Chord::I, Chord::IV, Chord::V, Chord::VI]
                .choose(&mut rng)
                .unwrap();
            let mut last_chord = start_chord;
            let mut chords = vec![start_chord];
            while chords.len() <= 2 || !chord_map[chords.last().unwrap()].contains(&start_chord) {
                let possible_next_chords: Vec<Chord> = chord_map[chords.last().unwrap()]
                    .clone()
                    .into_iter()
                    .filter(|c| c != &last_chord)
                    .filter(|c| {
                        // Wrap up the progression if it gets long
                        chords.len() <= 4 || chord_map[c].contains(&start_chord)
                    })
                    .collect();
                let next_chord = *possible_next_chords.choose(&mut rng).unwrap();
                chords.append(&mut vec![next_chord]);
                last_chord = next_chord
            }

            let rhythm = Rhythm::balanced_timing(ts.bars(4), chords.len() as i32, ts, &mut rng);

            Ok(vec![
                ChordProgression { chords, rhythm }.into_segment(segment.timing)
            ])
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct ChordMarkers;

impl ChordMarkers {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, context| {
            let chord_progression = context
                .find::<ChordProgression>()
                .with_timing(During, segment.timing)
                .require()?
                .element;

            let (chords, rhythm) = (&chord_progression.chords, &chord_progression.rhythm);

            Ok(chords
                .iter()
                .cycle()
                .zip(rhythm.iter_over(segment.timing).filter(|div| !div.is_rest))
                .map(|(chord, div)| chord.into_segment(div.start..div.end))
                .collect())
        })
    }
}
