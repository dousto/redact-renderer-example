mod chord_progression;
mod melody;
mod orchestration;
mod parts;
mod structure;
mod util;

use serde::{Deserialize, Serialize};
use std::{fs, vec};

use crate::orchestration::{Instrumentation, RandomInstrumentation};
use crate::structure::Sections;
use crate::util::{RandomKey, RandomTempo, RandomTimeSignature};
use redact_composer::midi::convert::MidiConverter;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::util::IntoCompositionSegment;
use redact_composer::{Composer, Element, Renderer};

fn main() {
    env_logger::init();

    let composer = Composer::from(Renderers::standard());

    let composition_length = composer.options.ticks_per_beat * 6 * 8 * 8;
    let composition = composer.compose(Composition.into_segment(0..composition_length));

    fs::create_dir_all("./test-midi")
        .and_then(|_| MidiConverter::convert(&composition).save("./test-midi/output.mid"))
        .and_then(|_| {
            fs::write(
                "./test-midi/output.json",
                serde_json::to_string_pretty(&composition).unwrap(),
            )
        })
        .unwrap();

    // Attempt a deserialization to surface element name collisions if they exist
    serde_json::from_str::<redact_composer::Composition>(
        serde_json::to_string_pretty(&composition).unwrap().as_str(),
    )
    .unwrap();
}

#[derive(Element, Serialize, Deserialize, Copy, Clone, Debug)]
pub struct Composition;

struct Renderers;

impl Renderers {
    fn composition_renderer() -> impl Renderer<Element = Composition> {
        AdhocRenderer::<Composition>::new(|segment, _| {
            Ok(vec![
                RandomKey.into_segment(segment.timing),
                RandomTimeSignature.into_segment(segment.timing),
                RandomTempo.into_segment(segment.timing),
                RandomInstrumentation.into_segment(segment.timing),
                Sections.into_segment(segment.timing),
                // Metronome::new().into_segment(timing)
            ])
        })
    }

    fn standard() -> RenderEngine {
        redact_composer::renderers()
            + Self::composition_renderer()
            + structure::renderers()
            + chord_progression::renderers()
            + orchestration::renderers()
            + parts::renderers()
            + util::renderers()
    }
}
