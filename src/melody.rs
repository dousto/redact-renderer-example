use crate::melody::MelodyDirectiveOutput::{NoteChoice, NoteMask};
use crate::structure::PhraseDivider;
use rand::distributions::WeightedIndex;
use rand::Rng;
use redact_composer::elements::PlayNote;
use redact_composer::musical::elements::{Key, TimeSignature};
use redact_composer::musical::rhythm::{Rhythm, Subdivision};
use redact_composer::musical::Notes;
use redact_composer::render::context::TimingRelation::{During, Overlapping};
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::timing::Timing;
use redact_composer::util::{HashMap, IntoCompositionSegment};
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};
use std::ops::{AddAssign, MulAssign, Range};

pub fn renderers() -> RenderEngine {
    RenderEngine::new() + Melody::renderer() + MelodyLine::renderer()
}

#[derive(Element, Serialize, Deserialize, Debug)]
#[element(wrapped_element = "Some(&*self.wrapped_element)")]
#[element(wrapped_element_doc = "The element responsible for producing `MelodyDirective`s.")]
pub struct Melody {
    #[serde(flatten)]
    wrapped_element: Box<dyn Element>,
}

impl Melody {
    pub fn new(wrapped_type: impl Element) -> Self {
        Self {
            wrapped_element: Box::new(wrapped_type),
        }
    }

    pub fn run_to(note: u8) -> MelodyDirective {
        MelodyDirective {
            directive_type: MelodyDirectiveType::RunTo(note),
        }
    }

    pub fn key_note(note: u8) -> MelodyDirective {
        MelodyDirective {
            directive_type: MelodyDirectiveType::KeyNote(note),
        }
    }
}

impl Melody {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, _| Ok(vec![MelodyLine.into_segment(segment.timing)]))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MelodyDirectiveType {
    RunTo(u8),
    KeyNote(u8),
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct MelodyDirective {
    directive_type: MelodyDirectiveType,
}

impl MelodyDirective {
    pub(self) fn apply(
        &self,
        _directive_timing: &Timing,
        prev_note: Option<&u8>,
        _time: &Range<i32>,
        key: &Key,
    ) -> MelodyDirectiveOutput {
        match self.directive_type {
            MelodyDirectiveType::RunTo(n) => {
                let prev_note = if let Some(prev_note) = prev_note {
                    *prev_note
                } else {
                    n
                };

                if n == prev_note {
                    return NoteChoice([(n, 1.0)].into_iter().collect::<HashMap<_, _>>());
                }

                let run_up = n > prev_note;
                let run_range = (1..(n.abs_diff(prev_note) + 2))
                    .filter(|diff| diff != &0)
                    .map(|offset| {
                        if run_up {
                            prev_note + offset
                        } else {
                            prev_note - offset
                        }
                    })
                    .collect::<Vec<_>>();
                let run_notes = Notes::from(key.scale()).in_range(run_range.clone());

                let map = run_notes
                    .into_iter()
                    .rev()
                    .enumerate()
                    .map(|(i, note_option)| (note_option, ((i + 1) as f32).powf(3.0)))
                    .collect::<HashMap<_, _>>();

                NoteChoice(map)
            }
            MelodyDirectiveType::KeyNote(n) => {
                NoteChoice([(n, 1.0)].into_iter().collect::<HashMap<_, _>>())
            }
        }
    }
}

enum MelodyDirectiveOutput {
    /// Provides note choices as a HashMap keyed by note number and valued by weight.
    /// When two [`NoteChoice`]s are applied simultaneously, their note
    /// weights are added respective to each note.
    #[allow(dead_code)]
    NoteChoice(HashMap<u8, f32>),
    /// Provides a note mask as a HashMap keyed by note number and a probability mask. Values should
    /// be between 0.0 and 1.0 (Effectively it multiplies to the note choice probabilities.)
    /// When two [`NoteMask`]s are applied simultaneously, they are also
    /// applied multiplicatively.
    #[allow(dead_code)]
    NoteMask(HashMap<u8, f32>),
}

impl MelodyDirectiveOutput {
    pub fn merge_into(self, other: &mut HashMap<u8, f32>) {
        match self {
            NoteChoice(map) => {
                map.into_iter().for_each(|(note, weight)| {
                    if let Some(existing) = other.get_mut(&note) {
                        existing.add_assign(weight);
                    } else {
                        other.insert(note, weight);
                    }
                });
            }
            NoteMask(map) => {
                map.into_iter().for_each(|(note, weight)| {
                    if let Some(existing) = other.get_mut(&note) {
                        existing.mul_assign(weight);
                    }
                });
            }
        }
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
#[element(name = "melody::MelodyLine")]
struct MelodyLine;

impl MelodyLine {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, ctx| {
            let mut rng = ctx.rng();
            let ts = {
                if let Ok(ts) = ctx
                    .find::<TimeSignature>()
                    .with_timing(During, segment.timing)
                    .within_ancestor::<Melody>()
                    .require()
                {
                    ts.element
                } else {
                    ctx.find::<TimeSignature>()
                        .with_timing(During, segment.timing)
                        .require()?
                        .element
                }
            };
            let key = {
                if let Ok(key) = ctx
                    .find::<Key>()
                    .with_timing(During, segment.timing)
                    .within_ancestor::<Melody>()
                    .require()
                {
                    key.element
                } else {
                    ctx.find::<Key>()
                        .with_timing(During, segment.timing)
                        .require()?
                        .element
                }
            };
            let dividers = ctx
                .find::<PhraseDivider>()
                .with_timing(Overlapping, segment.timing)
                .get_all();
            let directives = ctx
                .find::<MelodyDirective>()
                .within_ancestor::<Melody>()
                .with_timing(Overlapping, segment.timing)
                .require_all()?;

            let allowed_divisions = [
                (vec![ts.beats(2)], 16),
                (vec![ts.triplet(), ts.triplet(), ts.triplet()], 8),
                (vec![ts.beat() + ts.half_beat()], 4),
                (vec![ts.beat()], 2),
                (vec![ts.half_beat()], 1),
                // (vec![ts.half_beat() / 2, ts.half_beat() / 2], 1),
            ];

            let mut rhythm_rng = ctx.rng_with_seed(rng.gen::<u64>());
            let rhythm = if let Some(dividers) = dividers {
                dividers.into_iter().fold(Rhythm::new(), |acc, div| {
                    acc + Rhythm::random_with_subdivisions_weights(
                        div.timing.end - div.timing.start,
                        &allowed_divisions,
                        &mut rhythm_rng,
                    )
                })
            } else {
                Rhythm::random_with_subdivisions_weights(
                    segment.timing.len(),
                    &allowed_divisions,
                    &mut rng,
                )
            };

            let lead_amount = rng.gen_range(0..=2) * ts.half_beat();

            let mut notes = rhythm
                .iter_over(segment.timing)
                .scan(None, |prev_note, t| {
                    let note_choices = directives
                        .iter()
                        .filter(|directive| {
                            directive
                                .timing
                                .start_shifted_by(-lead_amount)
                                .intersects(&t.timing())
                        })
                        .map(|d| {
                            d.element
                                .apply(d.timing, prev_note.as_ref(), &t.timing(), key)
                        })
                        .fold(HashMap::default(), |mut acc, t| {
                            t.merge_into(&mut acc);

                            acc
                        });

                    if let Ok(dist) = WeightedIndex::new(note_choices.values()) {
                        let chosen_note =
                            *note_choices.keys().collect::<Vec<_>>()[rng.sample(dist)];
                        prev_note.replace(chosen_note);

                        Some((Some(chosen_note), t))
                    } else {
                        Some((None, t))
                    }
                })
                .collect::<Vec<_>>();

            Self::merge_ranges(&mut notes, &mut rng);

            let play_notes = notes
                .into_iter()
                .flat_map(|(opt_note, div)| opt_note.map(|note| (note, div)))
                .map(|(note, div)| {
                    PlayNote {
                        note,
                        velocity: rng.gen_range(80..=110),
                    }
                    .into_segment(div.timing())
                })
                .collect::<Vec<_>>();

            Ok(play_notes)
        })
    }

    fn merge_ranges(input_vec: &mut Vec<(Option<u8>, Subdivision)>, rng: &mut impl Rng) {
        if input_vec.is_empty() {
            return;
        }

        let mut result = Vec::new();
        let mut current_merged = input_vec[0];

        for item in input_vec.iter().skip(1) {
            let (opt, range) = *item;
            if let Some(curr) = opt {
                if let Some(prev) = current_merged.0 {
                    if prev == curr && current_merged.1.end >= range.start {
                        // Merge the ranges
                        if rng.gen_bool(0.7) {
                            current_merged.1.end = current_merged.1.end.max(range.end);
                            continue;
                        }
                    }
                }
            }
            result.push(current_merged);
            current_merged = (opt, range);
        }

        result.push(current_merged);
        *input_vec = result;
    }
}
