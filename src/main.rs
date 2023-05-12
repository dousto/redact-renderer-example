use std::{fs, vec};

use rand::Rng;
use redact_composer::converters::MidiConverter;

use redact_composer::composer::{
    Composer, CompositionContext, CompositionSegment, RenderResult, Renderer, SegmentType,
};

use redact_composer::musical::{Chord, Key, Notes, Scale};

fn main() {
    let renderer = TestRenderer {};
    let c = Composer::new(renderer);

    let beat = 480;

    let render_tree = c.compose(CompositionSegment {
        begin: 0,
        end: beat * 4 * 8 * 2,
        segment_type: SegmentType::Abstract(RenderType::Composition),
    });

    fs::create_dir_all("./test-midi")
        .and_then(|()| MidiConverter::convert(&render_tree).save("./test-midi/seeifitworks.mid"))
        .unwrap();
}

pub struct TestRenderer;

#[derive(Debug)]
enum RenderType {
    Composition,
    RandomKey,
    Key(Key),
    ChordProgression,
    ProgressionChords(Vec<Chord>),
    ProgressionRhythm(Vec<u32>),
    ChordMarkers,
    Chord(Chord),
    Part,
    PlayChord,
}

impl Renderer<RenderType> for TestRenderer {
    fn render(
        &self,
        abstract_type: &RenderType,
        begin: u32,
        end: u32,
        context: &CompositionContext<RenderType>,
    ) -> RenderResult<RenderType> {
        let mut rng = rand::thread_rng();
        let beat = 480;

        match abstract_type {
            RenderType::Composition => RenderResult::Success(Some(vec![
                CompositionSegment {
                    segment_type: SegmentType::Part(RenderType::Part),
                    begin,
                    end: begin + (end - begin) / 2,
                },
                CompositionSegment {
                    segment_type: SegmentType::Part(RenderType::Part),
                    begin: begin + (end - begin) / 2,
                    end,
                },
                CompositionSegment {
                    segment_type: SegmentType::Abstract(RenderType::RandomKey),
                    begin,
                    end,
                },
            ])),
            RenderType::ChordProgression => {
                // Test basic 4 chord
                let mut chord_options = Chord::values();
                let chord1 = chord_options[rng.gen_range(0..chord_options.len())];
                chord_options.retain(|c| c != &chord1);
                let chord2 = chord_options[rng.gen_range(0..chord_options.len())];
                chord_options.retain(|c| c != &chord2);
                let chord3 = chord_options[rng.gen_range(0..chord_options.len())];
                chord_options.retain(|c| c != &chord3);
                let chord4 = chord_options[rng.gen_range(0..chord_options.len())];

                let rhythm = Vec::from([beat * 4; 4]);

                RenderResult::Success(Some(vec![
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::ProgressionChords(vec![
                            chord1, chord2, chord3, chord4,
                        ])),
                        begin: begin,
                        end: end,
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::ProgressionRhythm(rhythm)),
                        begin: begin,
                        end: end,
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::ChordMarkers),
                        begin: begin,
                        end: end,
                    },
                ]))
            }
            RenderType::ChordMarkers => {
                let opt_chords = context.get(|n| {
                    if let RenderType::ProgressionChords(chords) = n {
                        Some(chords)
                    } else {
                        None
                    }
                });
                let opt_rhythm = context.get(|n| {
                    if let RenderType::ProgressionRhythm(rhythm) = n {
                        Some(rhythm)
                    } else {
                        None
                    }
                });

                if opt_chords.is_none() || opt_rhythm.is_none() {
                    return RenderResult::MissingContext;
                }

                let chords = opt_chords.unwrap();
                let rhythm = opt_rhythm.unwrap();

                RenderResult::Success(Some(
                    chords
                        .into_iter()
                        .cycle()
                        .zip(
                            rhythm
                                .into_iter()
                                .cycle()
                                .scan((begin, begin), |(chord_begin, chord_end), rhythm_length| {
                                    (*chord_begin, *chord_end) =
                                        (*chord_end, *chord_end + rhythm_length);
                                    Some((*chord_begin, *chord_end))
                                })
                                .take_while(|(_, chord_end)| *chord_end <= end),
                        )
                        .map(|(chord, (b, e))| CompositionSegment {
                            segment_type: SegmentType::Abstract(RenderType::Chord(*chord)),
                            begin: b,
                            end: e,
                        })
                        .collect(),
                ))
            }
            RenderType::RandomKey => {
                let key = Key {
                    tonic: rng.gen_range(0..12),
                    scale: Scale::values()[rng.gen_range(0..Scale::values().len())],
                };

                RenderResult::Success(Some(vec![CompositionSegment {
                    segment_type: SegmentType::Abstract(RenderType::Key(key)),
                    begin,
                    end,
                }]))
            }
            RenderType::Part => RenderResult::Success(Some(
                (0..((end - begin) / (beat * 4)))
                    .into_iter()
                    .map(|i| CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::PlayChord),
                        begin: begin + i * 4 * beat,
                        end: begin + (i + 1) * 4 * beat,
                    })
                    .chain([CompositionSegment {
                        segment_type: SegmentType::Instrument {
                            program: rng.gen_range(0..128),
                        },
                        begin,
                        end,
                    }])
                    .chain([CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::ChordProgression),
                        begin,
                        end,
                    }])
                    .collect(),
            )),
            RenderType::PlayChord => {
                let opt_key = context.get(|n| {
                    if let RenderType::Key(key) = n {
                        Some(key)
                    } else {
                        None
                    }
                });
                let opt_chord = context.get(|n| {
                    if let RenderType::Chord(chord) = n {
                        Some(chord)
                    } else {
                        None
                    }
                });

                if opt_key.is_none() || opt_chord.is_none() {
                    return RenderResult::MissingContext;
                }

                let key = opt_key.unwrap();
                let chord = opt_chord.unwrap();

                let note_options =
                    Notes::from(key.chord(chord)).in_range((key.tonic + 40)..=(key.tonic + 60));

                RenderResult::Success(Some(
                    note_options
                        .into_iter()
                        .rev()
                        .take(4)
                        .rev()
                        .map(|n| CompositionSegment {
                            segment_type: SegmentType::PlayNote {
                                note: n,
                                velocity: 100,
                            },
                            begin: begin,
                            end: end,
                        })
                        .collect(),
                ))
            }
            _ => RenderResult::Success(None),
        }
    }
}
