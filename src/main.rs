use std::collections::HashMap;
use std::f32::consts::PI;
use std::fs;

use rand::distributions::{Distribution, WeightedIndex};
use rand::seq::SliceRandom;
use rand::Rng;
use redact_composer::composer::context::{CompositionContext, SearchScope, TimeRelation};
use redact_composer::composer::{
    Composer, CompositionSegment, Part, PlayNote, RenderResult, SegmentType,
};
use redact_composer::converters::MidiConverter;
use redact_composer::musical::midi::{DrumHit, Instrument, Instruments};
use redact_composer::musical::{
    rhythm::{Rhythm, STANDARD_BEAT_LENGTH},
    Chord, Key, Notes, Scale,
};
use serde::{Deserialize, Serialize};

fn main() {
    let beat = STANDARD_BEAT_LENGTH;

    let render_tree = Composer::compose(CompositionSegment::new(
        Composition { beat },
        0,
        beat * 4 * 8 * 8,
    ));

    println!(
        "Use `Composer::compose_with_seed` using seed {:?} to reproduce this output.",
        render_tree.root().unwrap().value.seed
    );

    fs::create_dir_all("./test-midi")
        .and_then(|_| MidiConverter::convert(&render_tree).save("./test-midi/output.mid"))
        .and_then(|_| {
            fs::write(
                "./test-midi/output.json",
                serde_json::to_string_pretty(&render_tree).unwrap(),
            )
        })
        .unwrap();
}

#[derive(Debug, Serialize, Deserialize)]
struct Composition {
    beat: i32,
}

#[typetag::serde]
impl SegmentType for Composition {
    fn render(&self, begin: i32, end: i32, _context: CompositionContext) -> RenderResult {
        let section_length = self.beat * 4 * 8;
        let sections = (end - begin) / section_length;
        let unique_sections = 2;

        RenderResult::Success(Some(
            vec![CompositionSegment::new(RandomKey, begin, end)]
                .into_iter()
                .chain((0..sections).map(|section| {
                    CompositionSegment::named(
                        section % unique_sections,
                        Section,
                        section * section_length,
                        (section + 1) * section_length,
                    )
                }))
                .collect(),
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Section;

#[typetag::serde]
impl SegmentType for Section {
    fn render(&self, begin: i32, end: i32, _: CompositionContext) -> RenderResult {
        RenderResult::Success(Some(vec![
            CompositionSegment::new(Harmony, begin, end),
            CompositionSegment::new(Part::instrument(ChordPart), begin, end),
            CompositionSegment::new(RandomChordProgression, begin, end),
        ]))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RandomKey;

#[typetag::serde]
impl SegmentType for RandomKey {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        let mut rng = context.rng();

        RenderResult::Success(Some(vec![CompositionSegment::new(
            Key {
                tonic: rng.gen_range(0..12),
                scale: Scale::values()[rng.gen_range(0..Scale::values().len())],
            },
            begin,
            end,
        )]))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RandomChordProgression;

#[typetag::serde]
impl SegmentType for RandomChordProgression {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            // Define a map of possible chords transitions
            let chord_map: HashMap<Chord, Vec<Chord>> = HashMap::from([
                (Chord::I, Chord::values()),
                (Chord::II, vec![Chord::III, Chord::V, Chord::VI]),
                (Chord::III, vec![Chord::II, Chord::IV, Chord::VI]),
                (Chord::IV, vec![Chord::I, Chord::V, Chord::VII]),
                (Chord::V, vec![Chord::I, Chord::II, Chord::IV, Chord::VI]),
                (Chord::VI, vec![Chord::II, Chord::IV, Chord::V]),
                (Chord::VII, vec![Chord::I]),
            ]);

            // Starting from Chord::I or Chord::V, add additional chords based on the possible transitions
            // Make sure the last chord can transition back to the starting chord, enabling nice repetition
            let start_chord = [Chord::I, Chord::V][rng.gen_range(0..=1)];
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
                let next_chord = possible_next_chords[rng.gen_range(0..possible_next_chords.len())];
                chords.append(&mut vec![next_chord]);
                last_chord = next_chord
            }

            let beat = composition.beat;
            let rhythm = Rhythm::balanced_timing(beat * 4 * 4, chords.len() as i32, beat, &mut rng);

            RenderResult::Success(Some(vec![CompositionSegment::new(
                ChordProgression { chords, rhythm },
                begin,
                end,
            )]))
        } else {
            RenderResult::MissingContext
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChordProgression {
    chords: Vec<Chord>,
    rhythm: Rhythm,
}

#[typetag::serde]
impl SegmentType for ChordProgression {
    fn render(&self, begin: i32, end: i32, _context: CompositionContext) -> RenderResult {
        let (chords, rhythm) = (&self.chords, &self.rhythm);

        // Add chord markers throughout begin..end for ease of context lookup
        RenderResult::Success(Some(
            chords
                .iter()
                .cycle()
                .zip(rhythm.iter_over(begin..end).filter(|div| !div.is_rest))
                .map(|(chord, div)| {
                    CompositionSegment::new(*chord, div.timing.start, div.timing.end)
                })
                .collect(),
        ))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RandomInstrument;

#[typetag::serde]
impl SegmentType for RandomInstrument {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        let instruments: Vec<Instrument> = Instruments::melodic().into();

        RenderResult::Success(Some(vec![CompositionSegment::new(
            *instruments.choose(&mut context.rng()).unwrap(),
            begin,
            end,
        )]))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChordPart;

#[typetag::serde]
impl SegmentType for ChordPart {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let Some(chord_markers) = context.get_all_segments::<Chord>(
            TimeRelation::within(begin..end),
            SearchScope::within_any::<ChordProgression>(),
        ) {
            // Play the chord for each chord marker
            RenderResult::Success(Some(
                [CompositionSegment::new(RandomInstrument, begin, end)]
                    .into_iter()
                    .chain(
                        chord_markers
                            .iter()
                            .map(|c| CompositionSegment::new(PlayChord, c.begin, c.end)),
                    )
                    .collect(),
            ))
        } else {
            RenderResult::MissingContext
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayChord;

#[typetag::serde]
impl SegmentType for PlayChord {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(key), Some(chord)) = (
            context.rng(),
            context.get::<Key>(TimeRelation::during(begin..end), SearchScope::anywhere()),
            context.get::<Chord>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            // Simple implementation which chooses 4 of the chord notes within a given range and play them simultaneously
            let note_options =
                Notes::from(key.chord(chord)).in_range((key.tonic + 30)..=(key.tonic + 50));

            RenderResult::Success(Some(
                note_options
                    .into_iter()
                    .rev()
                    .take(4)
                    .rev()
                    .map(|n| {
                        CompositionSegment::new(
                            PlayNote {
                                note: n,
                                velocity: rng.gen_range(80..=110),
                            },
                            begin,
                            end,
                        )
                    })
                    .collect(),
            ))
        } else {
            RenderResult::MissingContext
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Harmony;

#[typetag::serde]
impl SegmentType for Harmony {
    fn render(&self, begin: i32, end: i32, _context: CompositionContext) -> RenderResult {
        RenderResult::Success(Some(vec![
            CompositionSegment::new(Part::instrument(MelodyPart), begin, end),
            CompositionSegment::new(Part::instrument(MelodyPart), begin, end),
            CompositionSegment::new(Part::instrument(MelodyPart), begin, end),
            CompositionSegment::new(Part::instrument(MelodyPart), begin, end),
            CompositionSegment::new(Part::percussion(DrumPart), begin, end),
        ]))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MelodyPart;

#[typetag::serde]
impl SegmentType for MelodyPart {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            let instruments: Vec<Instrument> = Instruments::melodic().into();
            let instrument = *instruments.choose(&mut context.rng()).unwrap();

            let beat = composition.beat;

            let rhythm_precision = beat / 2;
            let max_rhythm_division = beat * 2;
            let rhythm = Rhythm::random(
                beat * 8,
                |n| {
                    (((n - rhythm_precision) as f32).clamp(0.0, max_rhythm_division as f32)
                        / max_rhythm_division as f32)
                        .powf(0.5)
                },
                |_| 0.2,
                &mut rng,
            );

            RenderResult::Success(Some(
                rhythm
                    .iter_over(begin..end)
                    .filter(|div| !div.is_rest)
                    .map(|div| {
                        CompositionSegment::new(MelodyNote, div.timing.start, div.timing.end)
                    })
                    .chain([CompositionSegment::new(instrument, begin, end)])
                    .collect(),
            ))
        } else {
            RenderResult::MissingContext
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MelodyNote;

#[typetag::serde]
impl SegmentType for MelodyNote {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition), Some(key), Some(chord), Some(melody_segment)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
            context.get::<Key>(TimeRelation::during(begin..end), SearchScope::anywhere()),
            context.get::<Chord>(TimeRelation::during(end..end), SearchScope::anywhere()),
            context.get_segment::<MelodyPart>(
                TimeRelation::during(begin..end),
                SearchScope::within_ancestor::<Harmony>(),
            ),
        ) {
            let opt_prev_note = context
                .get_all_segments::<PlayNote>(
                    TimeRelation::ending_within((begin - composition.beat)..begin),
                    SearchScope::within_ancestor::<MelodyPart>(),
                )
                .and_then(|notes| notes.last().unwrap().segment_type_as::<PlayNote>());

            // Define a range for melody notes to fall within
            let range_begin = key.tonic + 12 * 4 + 6;
            let range_end = key.tonic + 12 * 7;

            let note_options = Notes::from(key.scale()).in_range(range_begin..=range_end);

            // Note possibilities will be "bumped" up or down in probability based on various factors
            // This bump factor affects how "polarizing" the various factors are
            let bump_factor: f32 = 1.5;

            let weights: Vec<f32> = note_options
                .iter()
                .map(|n| {
                    let n = *n as i32;
                    let mut bumps = 0;

                    // Check if there is another note playing at nearly the same time with the same pitch class as this note option
                    let opt_other_note = context.get_where::<PlayNote>(
                        |play_note| {
                            Notes::base_note(&(n as u8)) == Notes::base_note(&play_note.note)
                        },
                        TimeRelation::beginning_within(
                            (begin - composition.beat / 2)..=(begin + composition.beat / 2),
                        ),
                        SearchScope::within_ancestor::<Harmony>(),
                    );

                    // Note options within the current chord are bumped up, unless another part is already playing the note
                    // They are bumped multiple times based on how long the note is to be played
                    let its_a_chord_note = key.chord(chord).contains(&Notes::base_note(&(n as u8)));
                    if its_a_chord_note {
                        let note_impact = ((end - begin) / (composition.beat / 2) - 1).pow(2);
                        if opt_other_note.is_none() {
                            bumps += note_impact;
                        } else {
                            bumps -= note_impact;
                        }
                    }

                    {
                        // Determine a target note using a cosine wave whose period relates (by some factor) to the melody length, and magnitude relates to the target note range
                        // Then bump down probabilities for note options further from this target
                        let s = rng.gen_range(2..=4);
                        let phase = (PI
                            + (2_i32.pow(s) as f32)
                                * PI
                                * ((begin - melody_segment.begin) as f32
                                    / (melody_segment.end - melody_segment.begin) as f32))
                            .cos();
                        let target: f32 = (phase + 1.0) / 2.0;

                        let target_note = (range_begin as i32)
                            + (((range_end - range_begin) as f32) * target) as i32;
                        let target_distance = (target_note - n).abs();
                        bumps -= (target_distance - 4).pow(2)
                    }

                    {
                        let next_chord_segment = context.get_segment::<Chord>(
                            TimeRelation::beginning_within(begin..(begin + 4 * composition.beat)),
                            SearchScope::within_any::<ChordProgression>(),
                        );
                        let next_chord =
                            next_chord_segment.and_then(|s| s.segment_type_as::<Chord>());

                        if let (Some(next_chord_segment), Some(next_chord)) =
                            (next_chord_segment, next_chord)
                        {
                            let max_bump = (4 * composition.beat) / (composition.beat / 2);
                            let eights_notes_away = (max_bump
                                - (next_chord_segment.begin - end).max(0) / (composition.beat / 2))
                                .max(0);

                            let max_dist = 8;
                            let dist = key
                                .chord(next_chord)
                                .iter()
                                .map(|chord_note| {
                                    Notes::base_note(&(n as u8)).abs_diff(*chord_note)
                                })
                                .max()
                                .map(|d| (max_dist - (d as i32)).max(0))
                                .unwrap_or(0);

                            bumps += (dist + eights_notes_away) * 2
                        }
                    }

                    // Bump up small note jumps, and bump down large note leaps
                    if let Some(prev_note) = &opt_prev_note {
                        let prev_note = prev_note.note as i32;
                        let jump_length = (prev_note - n).abs();
                        // Give more down bumps for the same note being repeated
                        if jump_length == 0 {
                            bumps -= 8
                        } else {
                            bumps -= jump_length - 4;
                        }
                    }

                    bump_factor.powf(bumps as f32)
                })
                .collect();

            let dist = WeightedIndex::new(weights).unwrap();

            RenderResult::Success(Some(vec![CompositionSegment::new(
                PlayNote {
                    note: note_options[dist.sample(&mut rng)],
                    velocity: rng.gen_range(60..=110),
                },
                begin,
                end,
            )]))
        } else {
            RenderResult::MissingContext
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DrumPart;

#[typetag::serde]
impl SegmentType for DrumPart {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            let drum_kit = Instrument::from(rng.gen_range(0..=30));
            let beat = composition.beat;

            let rhythm_precision = beat / 4;
            let max_rhythm_division = beat * 2;
            let rhythm = Rhythm::random(
                beat * 4,
                |n| {
                    (((n - rhythm_precision) as f32).clamp(0.0, max_rhythm_division as f32)
                        / max_rhythm_division as f32)
                        .powf(0.1)
                },
                |_| 0.5,
                &mut rng,
            );

            let drum_hits: Vec<DrumHit> = rhythm
                .0
                .iter()
                .filter(|div| !div.is_rest)
                .map(|_| {
                    *vec![
                        DrumHit::AcousticBassDrum,
                        DrumHit::AcousticSnare,
                        DrumHit::ClosedHiHat,
                    ]
                    .choose(&mut rng)
                    .unwrap()
                })
                .collect();

            RenderResult::Success(Some(
                rhythm
                    .iter_over(begin..end)
                    .filter(|div| !div.is_rest)
                    .zip(drum_hits.into_iter().cycle())
                    .map(|(div, drum_hit)| {
                        CompositionSegment::new(
                            PlayNote {
                                note: drum_hit.into(),
                                velocity: rng.gen_range(90..=110),
                            },
                            div.timing.start,
                            div.timing.end,
                        )
                    })
                    .chain([CompositionSegment::new(drum_kit, begin, end)])
                    .collect(),
            ))
        } else {
            RenderResult::MissingContext
        }
    }
}
