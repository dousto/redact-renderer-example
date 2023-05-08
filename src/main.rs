use std::fs;

use redact_composer::converters::MidiConverter;
use rand::Rng;

use redact_composer::composer::{Composer, Renderer, CompositionSegment, SegmentType, CompositionContext, RenderResult};

use redact_composer::musical::{Chord, Key, Notes, Scale};

fn main() {
    let renderer = TestRenderer {};
    let c = Composer::new(renderer);

    let beat = 480;

    let render_tree = c.compose(CompositionSegment {
        begin: 0, end: beat * 4 * 8,
        segment_type: SegmentType::Abstract(RenderType::Composition)}
    );

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
    Chord(Chord),
    Part,
    PlayChord
}

impl Renderer<RenderType> for TestRenderer {

    fn render(&self,
        abstract_type: &RenderType,
        begin: u32, end: u32,
        context: &CompositionContext<RenderType>
    ) -> RenderResult<RenderType> {
        let mut rng = rand::thread_rng();
        let beat = 480;

        match abstract_type {
            RenderType::Composition => {
                RenderResult::Success(Some(vec![
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::ChordProgression),
                        begin: begin, end: (end - begin) / 2
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::ChordProgression),
                        begin: (end - begin) / 2, end: end
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Part(RenderType::Part),
                        begin, end
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::RandomKey),
                        begin, end
                    }
                ]))
            },
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

                RenderResult::Success(Some(vec![
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::Chord(chord1)),
                        begin: begin, end: begin + (end - begin) / 4
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::Chord(chord2)),
                        begin: begin + (end - begin) / 4, end: begin + (end - begin) / 4 * 2
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::Chord(chord3)),
                        begin: begin + (end - begin) / 4 * 2, end: begin + (end - begin) / 4 * 3
                    },
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::Chord(chord4)),
                        begin: begin + (end - begin) / 4 * 3, end: begin + (end - begin) / 4 * 4
                    },
                ]))
            },
            RenderType::RandomKey => {
                let key = Key {
                    tonic: rng.gen_range(0..12),
                    scale: Scale::values()[rng.gen_range(0..Scale::values().len())],
                };

                RenderResult::Success(Some(vec![
                    CompositionSegment {
                        segment_type: SegmentType::Abstract(RenderType::Key(key)),
                        begin, end
                    }
                ]))
            },
            RenderType::Part => {
                RenderResult::Success(Some(
                    (0..((end - begin) / (beat * 4))).into_iter()
                        .map(|i| CompositionSegment {
                            segment_type: SegmentType::Abstract(RenderType::PlayChord),
                            begin: begin + i * 4 * beat,end: begin + (i + 1) * 4 * beat
                        })
                        .chain(
                            [CompositionSegment {
                                segment_type: SegmentType::Instrument { program: 0 },
                                begin, end,
                            }]
                        )
                        .collect()
                ))
            },
            RenderType::PlayChord => {
                let opt_key = context.get(|n| {
                    match n { RenderType::Key(key) => Some(*key), _ => None }
                });
                let opt_chord = context.get(|n| {
                    match n { RenderType::Chord(chord) => Some(*chord), _ => None }
                });

                if opt_key.is_none() || opt_chord.is_none() { return RenderResult::MissingContext }

                let key = opt_key.unwrap();
                let chord = opt_chord.unwrap();

                let note_options = Notes::from(key.chord(chord)).in_range((key.tonic + 40)..=(key.tonic + 60));

                RenderResult::Success(Some(
                    note_options.into_iter().rev().take(4).rev()
                    .map(|n| CompositionSegment {
                        segment_type: SegmentType::PlayNote {
                            note: n, velocity: 100
                        },
                        begin: begin, end: end
                    })
                    .collect()
                ))
            },
            _ => RenderResult::Success(None)
        }
    }
}
