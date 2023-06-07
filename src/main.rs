use std::collections::HashMap;
use std::convert::identity;
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
use redact_composer::musical::midi::{Instrument, Instruments};
use redact_composer::musical::{Chord, Key, Notes, Scale};

fn main() {
    let beat = 480;

    let render_tree = Composer::compose(CompositionSegment::new(
        Composition { beat },
        0,
        beat * 4 * 8 * 2,
    ));

    for node in &render_tree {
        println!("{:?}", node);
    }

    println!(
        "Use `Composer::compose_with_seed` using seed {:?} to reproduce this output.",
        render_tree.root().unwrap().value.seed
    );

    fs::create_dir_all("./test-midi")
        .and_then(|()| MidiConverter::convert(&render_tree).save("./test-midi/seeifitworks.mid"))
        .unwrap();
}

#[derive(Debug)]
struct Composition {
    beat: i32,
}
impl SegmentType for Composition {
    fn render(&self, begin: i32, end: i32, _context: CompositionContext) -> RenderResult {
        RenderResult::Success(Some(vec![
            CompositionSegment::new(RandomKey, begin, end),
            CompositionSegment::new(Part::new(ChordPart), begin, end),
            CompositionSegment::new(Harmony, begin, end),
            CompositionSegment::new(RandomChordProgression, begin, begin + (end - begin) / 2),
            CompositionSegment::new(RandomChordProgression, begin + (end - begin) / 2, end),
        ]))
    }
}

#[derive(Debug)]
struct RandomKey;
impl SegmentType for RandomKey {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        let mut rng = context.rng();

        return RenderResult::Success(Some(vec![CompositionSegment::new(
            Key {
                tonic: rng.gen_range(0..12),
                scale: Scale::values()[rng.gen_range(0..Scale::values().len())],
            },
            begin,
            end,
        )]));
    }
}

#[derive(Debug)]
struct RandomChordProgression;
impl SegmentType for RandomChordProgression {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            let beat = composition.beat;
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
            let mut last_chord = start_chord.clone();
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
                last_chord = next_chord.clone()
            }

            let target_length: i32 = beat * 4 * 4;
            let mut rhythm = vec![beat as i32 * 4; chords.len()];

            // Stretch or shrink the time each chord is played, until the chord sequence fits the target length
            let mut rhythm_length: i32 = rhythm.iter().sum();
            while rhythm_length != target_length {
                if rhythm_length > target_length {
                    // When shrinking, choose one of the longest durations and halve it
                    let max_rhythm = rhythm.iter().max().unwrap();
                    let mut max_indices: Vec<usize> = rhythm
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r == &max_rhythm)
                        .map(|(i, _)| i)
                        .collect();
                    max_indices.shuffle(&mut rng);
                    let selected_index = max_indices[rng.gen_range(0..max_indices.len())];
                    rhythm[selected_index] /= 2;
                } else if rhythm_length < target_length {
                    // When stretching, choose one of the shortest durations and double it
                    let min_rhythm = rhythm.iter().min().unwrap();
                    let mut min_indices: Vec<usize> = rhythm
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r == &min_rhythm)
                        .map(|(i, _)| i)
                        .collect();
                    min_indices.shuffle(&mut rng);
                    let selected_index = min_indices[rng.gen_range(0..min_indices.len())];
                    rhythm[selected_index] *= 2;
                }

                rhythm_length = rhythm.iter().sum()
            }

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

#[derive(Debug)]
struct ChordProgression {
    chords: Vec<Chord>,
    rhythm: Vec<i32>,
}
impl SegmentType for ChordProgression {
    fn render(&self, begin: i32, end: i32, _context: CompositionContext) -> RenderResult {
        let (chords, rhythm) = (&self.chords, &self.rhythm);

        // Add chord markers throughout begin..end for ease of context lookup
        RenderResult::Success(Some(
            chords
                .into_iter()
                .cycle()
                .zip(
                    rhythm
                        .into_iter()
                        .cycle()
                        .scan((begin, begin), |(chord_begin, chord_end), rhythm_length| {
                            (*chord_begin, *chord_end) = (*chord_end, *chord_end + rhythm_length);
                            Some((*chord_begin, *chord_end))
                        })
                        .take_while(|(_, chord_end)| *chord_end <= end),
                )
                .map(|(chord, (b, e))| CompositionSegment::new(*chord, b, e))
                .collect(),
        ))
    }
}

#[derive(Debug)]
struct RandomInstrument;
impl SegmentType for RandomInstrument {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        let mut rng = context.rng();
        let instruments: Vec<Instrument> = Instruments::melodic().into();
        let selected_instrument = *instruments.choose(&mut rng).unwrap();

        RenderResult::Success(Some(vec![CompositionSegment::new(
            selected_instrument,
            begin,
            end,
        )]))
    }
}

#[derive(Debug)]
struct ChordPart;
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

#[derive(Debug)]
struct PlayChord;
impl SegmentType for PlayChord {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(key), Some(chord)) = (
            context.rng(),
            context.get::<Key>(TimeRelation::during(begin..end), SearchScope::anywhere()),
            context.get::<Chord>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            // Simple implementation which chooses 4 of the chord notes within a given range and play them simultaneously
            let note_options =
                Notes::from(key.chord(&chord)).in_range((key.tonic + 30)..=(key.tonic + 50));

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

#[derive(Debug)]
struct Harmony;
impl SegmentType for Harmony {
    fn render(&self, begin: i32, end: i32, _context: CompositionContext) -> RenderResult {
        RenderResult::Success(Some(vec![
            CompositionSegment::new(Part::new(MelodyPart), begin, end),
            CompositionSegment::new(Part::new(MelodyPart), begin, end),
            CompositionSegment::new(Part::new(MelodyPart), begin, end),
            CompositionSegment::new(Part::new(MelodyPart), begin, end),
        ]))
    }
}

#[derive(Debug)]
struct MelodyPart;
impl SegmentType for MelodyPart {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
        ) {
            let beat = composition.beat;
            let eighths_per_beat = 2;
            let eighth_length = beat / eighths_per_beat;

            // Generate two measures of rhythm with eighth beat precision
            let eight_beat_rhythm: Vec<(i32, i32)> = (0..=(8 * eighths_per_beat))
                .into_iter()
                .scan(None, |t, u| {
                    *t = if t.is_none() {
                        Some(Some((u, u)))
                    } else {
                        Some(Some((t.unwrap().unwrap().0, u)))
                    };

                    // Determine whether to cut the rhythm length so far into either a rhythm segment or rest
                    let length = (t.unwrap().unwrap().1 - t.unwrap().unwrap().0) as f64;
                    if u == 16 || rng.gen_bool((length / 4.0).cbrt()) {
                        let note_cut = (t.unwrap().unwrap().0, t.unwrap().unwrap().1);
                        *t = Some(Some((t.unwrap().unwrap().1, t.unwrap().unwrap().1)));

                        if rng.gen_bool((length / 4.0).cbrt()) {
                            Some(Some(note_cut)) // Duration for a note to play
                        } else {
                            Some(None) // Rest
                        }
                    } else {
                        t.unwrap().unwrap().1 = u;
                        Some(None)
                    }
                })
                .flat_map(identity)
                .collect();

            RenderResult::Success(Some(
                (0..)
                    .into_iter()
                    .take_while(|i| i * 8 * beat < end - begin)
                    .flat_map(|eight_beat_idx| {
                        eight_beat_rhythm.iter().map(move |t| {
                            (
                                begin + eight_beat_idx * (8 * beat) + t.0 * eighth_length,
                                begin + eight_beat_idx * (8 * beat) + t.1 * eighth_length,
                            )
                        })
                    })
                    .map(|(rhythm_begin, rhythm_end)| {
                        CompositionSegment::new(MelodyNote, rhythm_begin, rhythm_end)
                    })
                    .chain([CompositionSegment::new(RandomInstrument, begin, end)])
                    .collect(),
            ))
        } else {
            RenderResult::MissingContext
        }
    }
}

#[derive(Debug)]
struct MelodyNote;
impl SegmentType for MelodyNote {
    fn render(&self, begin: i32, end: i32, context: CompositionContext) -> RenderResult {
        if let (mut rng, Some(composition), Some(key), Some(chord), Some(melody_segment)) = (
            context.rng(),
            context.get::<Composition>(TimeRelation::during(begin..end), SearchScope::anywhere()),
            context.get::<Key>(TimeRelation::during(begin..end), SearchScope::anywhere()),
            context.get::<Chord>(TimeRelation::during(end..end), SearchScope::anywhere()),
            context.get_segment::<MelodyPart>(
                TimeRelation::during(begin..end),
                SearchScope::anywhere(),
            ),
        ) {
            let opt_prev_note = context.get::<PlayNote>(
                TimeRelation::during((begin - composition.beat * 2)..end),
                SearchScope::within_ancestor::<MelodyPart>(),
            );

            // Define a range for melody notes to fall within
            let range_begin = key.tonic + 12 * 4 + 6;
            let range_end = key.tonic + 12 * 7;

            let note_options = Notes::from(key.scale()).in_range(range_begin..=range_end);

            // Note possibilities will be "bumped" up or down in probability based on various factors
            // This bump factor affects how "polarizing" the various factors are
            let bump_factor: f32 = 10.0;

            let weights: Vec<f32> = note_options
                .iter()
                .map(|n| {
                    let n = *n as i32;
                    let mut bumps = 0;

                    // Check if there is another note playing the same pitch class as this note option
                    let opt_other_note = context
                        .get_all_segments_where::<PlayNote>(
                            |play_note| {
                                Notes::base_note(&(n as u8))
                                    == Notes::base_note(&(play_note.note as u8))
                            },
                            TimeRelation::overlapping(begin..begin),
                            SearchScope::within_ancestor::<Harmony>(),
                        )
                        .and_then(|notes| {
                            let shared_start_notes: Vec<&CompositionSegment> = notes
                                .into_iter()
                                .filter(|note| note.begin == begin)
                                .collect();
                            shared_start_notes
                                .first()
                                .map(|s| s.segment_type_as::<PlayNote>())
                        });

                    // Note options within the current chord are bumped up, unless another part is already playing the note
                    // They are bumped multiple times based on how long the note is to be played
                    let its_a_chord_note = key.chord(chord).contains(&Notes::base_note(&(n as u8)));
                    if its_a_chord_note {
                        let note_impact =
                            (((end - begin) / (composition.beat / 2)) as i32 - 1).pow(2);
                        if opt_other_note.is_none() {
                            bumps += note_impact;
                        } else {
                            bumps -= note_impact;
                        }
                    } else {
                        bumps -= (((end - begin) / (composition.beat / 2)) as i32 - 1).pow(2)
                    }

                    {
                        // Determine a target note using a cosine wave whose period relates (by some factor) to the melody length, and magnitude relates to the target note range
                        // Then bump down probabilities for note options further from this target
                        let s = rng.gen_range(1..=8);
                        let phase = (PI
                            + (s as f32)
                                * PI
                                * ((begin - melody_segment.begin) as f32
                                    / (melody_segment.end - melody_segment.begin) as f32))
                            .cos();
                        let target: f32 = (phase + 1.0) / 2.0;

                        let target_note = (range_begin as i32)
                            + (((range_end - range_begin) as f32) * target) as i32;
                        let target_distance = (target_note - n as i32).abs();
                        bumps -= target_distance
                    }

                    // Bump up small note jumps, and bump down large note leaps
                    if let Some(prev_note) = &opt_prev_note {
                        let prev_note = prev_note.note as i32;
                        let jump_length = (prev_note - n).abs();
                        bumps -= jump_length - 4;

                        // Give more down bumps for the same note being repeated
                        if jump_length == 0 {
                            bumps -= 30
                        }
                    }

                    // Cancel out some note options to prevent them from being played
                    // Specifically notes that are "out of chord" during a chord transition
                    if end - begin > composition.beat
                        && !key.chord(chord).contains(&Notes::base_note(&(n as u8)))
                    {
                        0.0
                    } else {
                        bump_factor.powf(bumps as f32)
                    }
                })
                .collect();

            let dist = WeightedIndex::new(weights).unwrap();

            RenderResult::Success(Some(vec![CompositionSegment::new(
                PlayNote {
                    note: note_options[dist.sample(&mut rng)],
                    velocity: rng.gen_range(80..=110),
                },
                begin,
                end,
            )]))
        } else {
            RenderResult::MissingContext
        }
    }
}
