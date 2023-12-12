use std::collections::HashMap;
use std::f32::consts::PI;
use std::fs;
use std::ops::Range;

use rand::distributions::{Distribution, WeightedIndex};
use rand::prelude::IteratorRandom;
use rand::seq::SliceRandom;
use rand::Rng;
use redact_composer::composer;
use redact_composer::composer::context::CompositionContext;
use redact_composer::composer::context::TimingRelation::*;
use redact_composer::composer::render::{AdhocRenderer, RenderEngine, Renderer, RendererGroup};
use redact_composer::composer::{
    Composer, Composition, CompositionElement, CompositionSegment, Part, PlayNote,
};
use redact_composer::converters::MidiConverter;
use redact_composer::musical::midi::{DrumHit, Instrument, Instruments};
use redact_composer::musical::rhythm::Subdivision;
use redact_composer::musical::timing::{Metronome, Tempo, TimeSignature};
use redact_composer::musical::Scale;
use redact_composer::musical::{
    rhythm::{Rhythm, STANDARD_BEAT_LENGTH},
    Chord, Key, Mode, Notes,
};
use serde::{Deserialize, Serialize};

fn main() {
    let composer = Composer {
        engine: Renderers::standard(),
    };

    let render_tree = composer.compose(CompositionSegment::new(
        Composition,
        0..(STANDARD_BEAT_LENGTH * 6 * 8 * 8),
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

struct Renderers;

impl Renderers {
    fn standard() -> RenderEngine {
        composer::renderers()
            + Renderers::composition_renderer()
            + Self::section_generator::<Sections>()
            + RandomKey::renderer()
            + RandomTimeSignature::renderer()
            + Self::section_renderer()
            + ChordMarkers::renderer()
            + RandomChordProgression::renderer()
            + Harmony::renderer()
            + ChordPart::renderer()
            + PlayChord::renderer()
            + MelodyPart::renderer()
            + MelodyFragment::renderer()
            + MelodyNote::renderer()
            + DrumPart::phrase_renderer()
    }
    fn composition_renderer() -> impl Renderer<Item = Composition> {
        RendererGroup::new()
            + Self::key_generator()
            + Self::time_signature_generator()
            + Self::instrumentation_generator()
            + AdhocRenderer::from(
                |_segment: &Composition, time_range: &Range<i32>, _context: &CompositionContext| {
                    Ok(vec![CompositionSegment::new(Sections, time_range)])
                },
            )
            + AdhocRenderer::from(
                |_segment: &_, time_range: &Range<i32>, _context: &CompositionContext| {
                    Ok(vec![CompositionSegment::new(
                        Tempo::from_bpm(_context.rng().gen_range(100..=140)),
                        time_range,
                    )])
                },
            )
        // Uncomment to include metronome ticks
        // + Metronome::new()
    }

    fn section_renderer() -> impl Renderer<Item = Section> {
        RendererGroup::new()
            + AdhocRenderer::from(
                |_segment: &Section, time_range: &Range<i32>, context: &CompositionContext| {
                    let mut rng = context.rng();
                    let ts = context
                        .find::<TimeSignature>()
                        .with_timing(During, time_range)
                        .require()?
                        .value;

                    let rhythm = Rhythm::balanced_timing(
                        ts.bar(),
                        if ts.beats_per_bar % 3 == 0 {
                            ts.beats_per_bar / 3
                        } else {
                            ts.beats_per_bar / 2
                        },
                        ts,
                        &mut rng,
                    );

                    Ok(rhythm
                        .iter_over(time_range)
                        .map(|div| CompositionSegment::new(PhraseDivider, div.timing))
                        .collect::<Vec<_>>())
                },
            )
            + AdhocRenderer::from(
                |_segment: &Section, time_range: &Range<i32>, _context: &CompositionContext| {
                    Ok(vec![
                        CompositionSegment::new(ChordMarkers, time_range),
                        CompositionSegment::new(RandomChordProgression, time_range),
                        CompositionSegment::new(Harmony, time_range),
                    ])
                },
            )
    }

    fn key_generator<S: CompositionElement>() -> impl Renderer<Item = S> {
        AdhocRenderer::from(
            |_segment: &S, time_range: &Range<i32>, _context: &CompositionContext| {
                Ok(vec![CompositionSegment::new(RandomKey, time_range)])
            },
        )
    }

    fn time_signature_generator<S: CompositionElement>() -> impl Renderer<Item = S> {
        AdhocRenderer::from(
            |_segment: &S, time_range: &Range<i32>, _context: &CompositionContext| {
                Ok(vec![CompositionSegment::new(
                    RandomTimeSignature,
                    time_range,
                )])
            },
        )
    }

    fn section_generator<S: CompositionElement>() -> impl Renderer<Item = S> {
        AdhocRenderer::from(
            |_segment: &S, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                let ts = context
                    .find::<TimeSignature>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;

                let section_length = ts.bar() * 8;
                let num_sections = (time_range.end - time_range.start) / section_length;

                let mut sections = (0..(num_sections / 2))
                    .cycle()
                    .take(num_sections as usize)
                    .collect::<Vec<_>>();
                sections.shuffle(&mut rng);

                Ok(sections
                    .into_iter()
                    .enumerate()
                    .map(|(section, section_name)| {
                        CompositionSegment::named(
                            section_name, //(section / 1) % unique_sections,
                            Section,
                            (section as i32) * section_length
                                ..(section as i32 + 1) * section_length,
                        )
                    })
                    .collect())
            },
        )
    }

    fn instrumentation_generator<S: CompositionElement>() -> impl Renderer<Item = S> {
        AdhocRenderer::from(
            |_segment: &S, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();

                let all_instruments: Vec<Instrument> = Instruments::melodic().into();
                let instrumentation = Instrumentation {
                    instruments: (0..6)
                        .map(|_| *all_instruments.choose(&mut rng).unwrap())
                        .collect(),
                };

                Ok(vec![CompositionSegment::new(instrumentation, time_range)])
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Sections;

#[typetag::serde]
impl CompositionElement for Sections {}

#[derive(Debug, Serialize, Deserialize)]
struct PhraseDivider;

#[typetag::serde]
impl CompositionElement for PhraseDivider {}

#[derive(Debug, Serialize, Deserialize)]
struct ChordMarkers;

#[typetag::serde]
impl CompositionElement for ChordMarkers {}

impl ChordMarkers {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let chord_progression = context
                    .find::<ChordProgression>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;

                let (chords, rhythm) = (&chord_progression.chords, &chord_progression.rhythm);

                Ok(chords
                    .iter()
                    .cycle()
                    .zip(rhythm.iter_over(time_range).filter(|div| !div.is_rest))
                    .map(|(chord, div)| {
                        CompositionSegment::new(*chord, div.timing.start..div.timing.end)
                    })
                    .collect())
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Instrumentation {
    instruments: Vec<Instrument>,
}

#[typetag::serde]
impl CompositionElement for Instrumentation {}

#[derive(Debug, Serialize, Deserialize)]
struct Section;

#[typetag::serde]
impl CompositionElement for Section {}

#[derive(Debug, Serialize, Deserialize)]
struct RandomKey;

#[typetag::serde]
impl CompositionElement for RandomKey {}

impl RandomKey {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                Ok(vec![CompositionSegment::new(
                    Key {
                        tonic: rng.gen_range(0..12),
                        scale: *Scale::values().choose(&mut rng).unwrap(),
                        mode: *Mode::values().choose(&mut rng).unwrap(),
                    },
                    time_range,
                )])
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RandomTimeSignature;

#[typetag::serde]
impl CompositionElement for RandomTimeSignature {}

impl RandomTimeSignature {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();

                Ok(vec![CompositionSegment::new(
                    TimeSignature {
                        beats_per_bar: (2..=7).choose(&mut rng).unwrap(),
                        beat_length: STANDARD_BEAT_LENGTH,
                    },
                    time_range,
                )])
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RandomChordProgression;

#[typetag::serde]
impl CompositionElement for RandomChordProgression {}

impl RandomChordProgression {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                let ts = context
                    .find::<TimeSignature>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;

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
                while chords.len() <= 2 || !chord_map[chords.last().unwrap()].contains(&start_chord)
                {
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

                let rhythm =
                    Rhythm::balanced_timing(ts.bar() * 4, chords.len() as i32, ts, &mut rng);

                Ok(vec![CompositionSegment::new(
                    ChordProgression { chords, rhythm },
                    time_range,
                )])
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChordProgression {
    chords: Vec<Chord>,
    rhythm: Rhythm,
}

#[typetag::serde]
impl CompositionElement for ChordProgression {}

#[derive(Debug, Serialize, Deserialize)]
struct ChordPart {
    instrument: Instrument,
}

#[typetag::serde]
impl CompositionElement for ChordPart {}

impl ChordPart {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::<Self>::from(
            |segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let chord_markers = context
                    .find::<Chord>()
                    .within_ancestor::<Section>()
                    .with_timing(Within, time_range)
                    .require_all()?;

                // Play the chord for each chord marker
                Ok([CompositionSegment::new(segment.instrument, time_range)]
                    .into_iter()
                    .chain(
                        chord_markers
                            .iter()
                            .map(|c| CompositionSegment::new(PlayChord, &c.time_range)),
                    )
                    .collect())
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PlayChord;

impl PlayChord {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                let key = context
                    .find::<Key>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;
                let chord = context
                    .find::<Chord>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;

                // Simple implementation which chooses 4 of the chord notes within a given range and play them simultaneously
                let note_options =
                    Notes::from(key.chord(chord)).in_range((key.tonic + 30)..=(key.tonic + 50));

                Ok(note_options
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
                            time_range,
                        )
                    })
                    .collect())
            },
        )
    }
}

#[typetag::serde]
impl CompositionElement for PlayChord {}

#[derive(Debug, Serialize, Deserialize)]
struct Harmony;

#[typetag::serde]
impl CompositionElement for Harmony {}

impl Harmony {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                let composition = context
                    .find::<Composition>()
                    .with_timing(During, time_range)
                    .require()?;
                let instrumentation = context
                    .find::<Instrumentation>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;

                let mut parts = vec![
                    CompositionSegment::new(Part::percussion(DrumPart), time_range),
                    CompositionSegment::new(
                        Part::instrument(ChordPart {
                            instrument: instrumentation.instruments[0],
                        }),
                        time_range,
                    ),
                ]
                .into_iter()
                .chain(instrumentation.instruments.iter().skip(1).map(|inst| {
                    CompositionSegment::new(
                        Part::instrument(MelodyPart { instrument: *inst }),
                        time_range,
                    )
                }))
                .collect::<Vec<_>>();
                parts.shuffle(&mut rng);

                let progress = (time_range.end - composition.time_range.start) as f32
                    / (composition.time_range.end - composition.time_range.start) as f32
                    * (3.0 / 1.0)
                    * parts.len() as f32;

                Ok(parts
                    .into_iter()
                    .take((progress as usize).max(1))
                    .collect::<Vec<_>>())
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MelodyPart {
    instrument: Instrument,
}

#[typetag::serde]
impl CompositionElement for MelodyPart {}

impl MelodyPart {
    pub fn renderer() -> impl Renderer<Item = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::from(
                |segment: &Self, time_range: &Range<i32>, _context: &CompositionContext| {
                    Ok(vec![CompositionSegment::new(
                        segment.instrument,
                        time_range,
                    )])
                },
            )
            + AdhocRenderer::from(
                |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                    let mut rng = context.rng();
                    let ts = context
                        .find::<TimeSignature>()
                        .with_timing(During, time_range)
                        .require()?
                        .value;

                    let rhythm_precision = ts.half_beat();
                    let max_rhythm_division = ts.beat() * 2;
                    let rhythm = Rhythm::random(
                        ts.bar() * 2,
                        ts,
                        |n| {
                            (((n - rhythm_precision) as f32).clamp(0.0, max_rhythm_division as f32)
                                / max_rhythm_division as f32)
                                .powf(0.5)
                        },
                        |_| 0.2,
                        &mut rng,
                    );

                    Ok(rhythm
                        .iter_over(time_range)
                        .filter(|div| !div.is_rest)
                        .map(|div| {
                            CompositionSegment::new(MelodyNote, div.timing.start..div.timing.end)
                        })
                        .collect())
                },
            )
    }

    pub fn renderer2() -> impl Renderer<Item = Self> {
        RendererGroup::new()
            + AdhocRenderer::<Self>::from(
                |segment: &Self, time_range: &Range<i32>, _context: &CompositionContext| {
                    Ok(vec![CompositionSegment::new(
                        segment.instrument,
                        time_range,
                    )])
                },
            )
            + AdhocRenderer::from(
                |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                    let mut rng = context.rng();
                    let dividers = context
                        .find::<PhraseDivider>()
                        .with_timing(BeginningWithin, time_range)
                        .require_all()?;

                    let filtered_dividers = dividers
                        .iter()
                        .filter(|_| rng.gen_bool(0.5))
                        .collect::<Vec<_>>();

                    Ok(filtered_dividers
                        .iter()
                        .zip((0..2).cycle())
                        .map(|(div, name)| {
                            CompositionSegment::named(name, MelodyFragment, &div.time_range)
                        })
                        .collect::<Vec<_>>())
                },
            )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MelodyFragment;

#[typetag::serde]
impl CompositionElement for MelodyFragment {}

impl MelodyFragment {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                let ts = context
                    .find::<TimeSignature>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;

                let rhythm_precision = ts.half_beat();
                let max_rhythm_division = ts.beat() * 2;
                let rhythm = Rhythm::random(
                    time_range.end - time_range.start,
                    ts,
                    |n| {
                        (((n - rhythm_precision) as f32).clamp(0.0, max_rhythm_division as f32)
                            / max_rhythm_division as f32)
                            .powf(0.5)
                    },
                    |_| 0.2,
                    &mut rng,
                );

                Ok(rhythm
                    .iter_over(time_range)
                    .filter(|div| !div.is_rest)
                    .map(|div| CompositionSegment::new(MelodyNote, div.timing))
                    .collect())
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MelodyNote;

#[typetag::serde]
impl CompositionElement for MelodyNote {}

impl MelodyNote {
    pub fn renderer() -> impl Renderer<Item = Self> {
        AdhocRenderer::from(
            |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                let mut rng = context.rng();
                let ts = context
                    .find::<TimeSignature>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;
                let key = context
                    .find::<Key>()
                    .with_timing(During, time_range)
                    .require()?
                    .value;
                let chord = context
                    .find::<Chord>()
                    .with_timing(During, &(time_range.start..time_range.start))
                    .require()?
                    .value;
                let containing_melody = context
                    .find::<MelodyPart>()
                    .within_ancestor::<Harmony>()
                    .with_timing(During, &(time_range.start..time_range.start))
                    .require()?;

                let opt_prev_chord = context
                    .find::<Chord>()
                    .within_ancestor::<Section>()
                    .matching(|prev_chord| chord != prev_chord)
                    .with_timing(
                        EndingWithin,
                        &((time_range.start - ts.beat())..(time_range.start)),
                    )
                    .get()
                    .map(|s| s.value);

                let opt_next_chord_segment = context
                    .find::<Chord>()
                    .within_ancestor::<Section>()
                    .with_timing(
                        BeginningWithin,
                        &(time_range.start..(time_range.start + ts.bar())),
                    )
                    .get();

                let opt_prev_note = context
                    .find::<PlayNote>()
                    .within_ancestor::<MelodyPart>()
                    .with_timing(
                        EndingWithin,
                        &((time_range.start - ts.beat())..time_range.start),
                    )
                    .get_all()
                    .and_then(|notes| notes.last().map(|n| n.value));

                // Define a range for melody notes to fall within
                let note_range = (key.tonic + 12 * 4)..=(key.tonic + 12 * 7);

                let note_options = Notes::from(key.scale()).in_range(note_range.clone());

                // Note possibilities will be "bumped" up or down in probability based on various factors
                // This bump factor affects how "polarizing" the various factors are
                let bump_factor: f32 = 5.0;

                let weights: Vec<f32> = note_options
                    .iter()
                    .map(|n| {
                        let mut bumps = 0;

                        // Check if there is another note playing at nearly the same time with the same pitch class as this note option
                        let opt_other_note = context
                            .find::<PlayNote>()
                            .within_ancestor::<Harmony>()
                            .matching(|play_note| {
                                Notes::base_note(n) == Notes::base_note(&play_note.note)
                            })
                            .with_timing(
                                BeginningWithin,
                                &((time_range.start - ts.beat())..=(time_range.start + ts.beat())),
                            )
                            .get();

                        if opt_other_note.is_some() {
                            bumps -= 2
                        }

                        // Note options within the current chord are bumped up, unless another part is already playing the note
                        // They are bumped multiple times based on how long the note is to be played
                        let its_a_chord_note = key.chord(chord).contains(&Notes::base_note(n));
                        if !its_a_chord_note {
                            bumps -= 3
                        }
                        if its_a_chord_note {
                            let note_impact =
                                ((time_range.end - time_range.start) / (ts.half_beat()) - 1).pow(2);
                            if opt_other_note.is_none() {
                                bumps += note_impact;
                            } else {
                                bumps -= note_impact.pow(2);
                            }
                        }

                        {
                            // Determine a target note using a cosine wave whose period relates (by some factor) to the melody length, and magnitude relates to the target note range
                            // Then bump down probabilities for note options further from this target
                            let s = rng.gen_range(2..=4);
                            let phase = (PI
                                + (2_i32.pow(s) as f32)
                                    * PI
                                    * ((time_range.start - containing_melody.time_range.start)
                                        as f32
                                        / (containing_melody.time_range.end
                                            - containing_melody.time_range.start)
                                            as f32))
                                .cos();
                            let target: f32 = (phase + 1.0) / 2.0;

                            let target_note = *note_range.start()
                                + (((note_range.end() - note_range.start()) as f32) * target) as u8;
                            let target_distance = target_note.abs_diff(*n);
                            bumps -= (target_distance as i32 - 4).pow(2).clamp(0, 6)
                        }

                        if let Some(prev_chord) = opt_prev_chord {
                            if key.chord(prev_chord).contains(&Notes::base_note(n)) {
                                bumps -= 3
                            }
                        }

                        if let Some(next_chord_segment) = &opt_next_chord_segment {
                            let max_bump = (ts.bar()) / (ts.half_beat());
                            let eights_notes_away = (max_bump
                                - (next_chord_segment.time_range.start - time_range.end).max(0)
                                    / (ts.half_beat()))
                            .max(0);

                            let max_dist = 8;
                            let dist = key
                                .chord(next_chord_segment.value)
                                .iter()
                                .map(|chord_note| Notes::base_note(n).abs_diff(*chord_note))
                                .max()
                                .map(|d| (max_dist - (d as i32)).max(0))
                                .unwrap_or(0);

                            bumps -= ((dist - eights_notes_away)
                                * 4
                                * if its_a_chord_note { 0 } else { 2 })
                            .clamp(0, 6)
                        }

                        // Bump up small note jumps, and bump down large note leaps
                        if let Some(prev_note) = &opt_prev_note {
                            let prev_note = prev_note.note;
                            let jump_length = prev_note.abs_diff(*n);
                            // Give more down bumps for the same note being repeated
                            if jump_length == 0 {
                                bumps -= 8
                            } else {
                                bumps -= jump_length as i32 - 4;
                            }
                        }

                        10.0 * bump_factor.powf(bumps as f32)
                    })
                    .collect();

                let dist = WeightedIndex::new(weights).unwrap();

                Ok(vec![CompositionSegment::new(
                    PlayNote {
                        note: note_options[dist.sample(&mut rng)],
                        velocity: rng.gen_range(60..=110),
                    },
                    time_range,
                )])
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DrumPart;

#[typetag::serde]
impl CompositionElement for DrumPart {}

impl DrumPart {
    pub fn renderer() -> impl Renderer<Item = Self> {
        RendererGroup::new()
            + AdhocRenderer::from(
                |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                    Ok(vec![CompositionSegment::new(
                        Instrument::from(context.rng().gen_range(0..=30)),
                        time_range,
                    )])
                },
            )
            + AdhocRenderer::from(
                |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                    let mut rng = context.rng();
                    let ts = context
                        .find::<TimeSignature>()
                        .with_timing(During, time_range)
                        .require()?
                        .value;

                    let rhythm_precision = ts.beat() / 2;
                    let max_rhythm_division = ts.beat();
                    let rhythm = Rhythm::random(
                        ts.bar(),
                        ts,
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
                            vec![
                                DrumHit::AcousticBassDrum,
                                DrumHit::AcousticSnare,
                                DrumHit::ClosedHiHat,
                            ]
                            .choose(&mut rng)
                            .copied()
                            .unwrap()
                        })
                        .collect();

                    Ok(rhythm
                        .iter_over(time_range)
                        .filter(|div| !div.is_rest)
                        .zip(drum_hits.into_iter().cycle())
                        .map(|(div, drum_hit)| {
                            CompositionSegment::new(
                                PlayNote {
                                    note: drum_hit.into(),
                                    velocity: rng.gen_range(90..=110),
                                },
                                div.timing.start..div.timing.end,
                            )
                        })
                        .collect())
                },
            )
    }

    pub fn phrase_renderer() -> impl Renderer<Item = Self> {
        RendererGroup::new()
            + AdhocRenderer::from(
                |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                    Ok(vec![CompositionSegment::new(
                        Instrument::from(context.rng().gen_range(0..=30)),
                        time_range,
                    )])
                },
            )
            + AdhocRenderer::from(
                |_segment: &Self, time_range: &Range<i32>, context: &CompositionContext| {
                    let mut rng = context.rng();
                    let ts = context
                        .find::<TimeSignature>()
                        .with_timing(During, time_range)
                        .require()?
                        .value;
                    let dividers = context
                        .find::<PhraseDivider>()
                        .with_timing(BeginningWithin, time_range)
                        .require_all()?;

                    let mut phrase_lengths = dividers
                        .iter()
                        .map(|div| div.time_range.end - div.time_range.start)
                        .collect::<Vec<_>>();
                    phrase_lengths.sort();
                    phrase_lengths.dedup();

                    let drum_beats = phrase_lengths
                        .iter()
                        .enumerate()
                        .map(|(idx, l)| {
                            let rhythm_precision = ts.sixteenth();
                            let mut beat_rhythm = Rhythm::random(
                                l - rhythm_precision,
                                ts,
                                |n| {
                                    (((n - rhythm_precision) as f32).clamp(0.0, ts.beat() as f32)
                                        / ts.beat() as f32)
                                        .powf(0.1)
                                },
                                |_| 0.5,
                                &mut rng,
                            );
                            beat_rhythm = Rhythm(vec![Subdivision {
                                timing: 0..rhythm_precision,
                                is_rest: false,
                            }]) + beat_rhythm;

                            let mut hits = beat_rhythm
                                .0
                                .iter()
                                .filter(|div| !div.is_rest)
                                .map(|_| {
                                    vec![
                                        DrumHit::AcousticBassDrum,
                                        DrumHit::AcousticSnare,
                                        DrumHit::ClosedHiHat,
                                    ]
                                    .choose(&mut rng)
                                    .copied()
                                    .unwrap()
                                })
                                .collect::<Vec<_>>();

                            let forced_hit = if idx % 2 == 0 {
                                DrumHit::AcousticBassDrum
                            } else {
                                DrumHit::AcousticSnare
                            };
                            if hits.get(0).is_some() {
                                hits[0] = forced_hit;
                                beat_rhythm.0[0].is_rest = false;
                            };

                            (*l, (beat_rhythm, hits))
                        })
                        .collect::<HashMap<_, _>>();

                    Ok(dividers
                        .iter()
                        .flat_map(|div| {
                            if let Some((rhythm, hits)) =
                                drum_beats.get(&(div.time_range.end - div.time_range.start))
                            {
                                rhythm
                                    .iter_over(&div.time_range)
                                    .filter(|div| !div.is_rest)
                                    .zip(hits.iter().cycle())
                                    .map(|(div, drum_hit)| {
                                        CompositionSegment::new(
                                            PlayNote {
                                                note: (*drum_hit).into(),
                                                velocity: rng.gen_range(90..=110),
                                            },
                                            div.timing.start..div.timing.end,
                                        )
                                    })
                                    .collect()
                            } else {
                                vec![]
                            }
                        })
                        .collect::<Vec<_>>())
                },
            )
    }
}
