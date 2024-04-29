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
use redact_composer::synthesis::{SF2Synthesizable, SF2Synthesizer};
use redact_composer::util::IntoSegment;
use redact_composer::{Composer, Element, Renderer};

fn main() {
    env_logger::init();

    let composer = Composer::from(Renderers::standard());

    let composition_length = composer.options.ticks_per_beat * 6 * 8 * 8;
    let composition = composer.compose(Composition.over(0..composition_length));

    let output_dir = "./composition-outputs";
    let midi = MidiConverter::convert(&composition);
    fs::create_dir_all(output_dir)
        .and_then(|_| midi.save(format!("{}/output.mid", output_dir)))
        .expect("Error saving midi");

    let json = serde_json::to_string_pretty(&composition).expect("Error serializing");
    fs::write(format!("{}/output.json", output_dir), json).expect("Error saving json");

    let sound_font_path = "./sounds/sound_font.sf2";
    let synth = SF2Synthesizer::new(sound_font_path).unwrap_or_else(|_| {
        panic!(
            "SoundFont ({:?}) is not committed with this repo and should be supplied separately",
            sound_font_path
        )
    });
    midi.synthesize_with(&synth)
        .to_file(format!("{}/output.wav", output_dir))
        .expect("Error during synthesis");
}

#[derive(Element, Serialize, Deserialize, Copy, Clone, Debug)]
pub struct Composition;

struct Renderers;

impl Renderers {
    fn standard() -> RenderEngine {
        redact_composer::renderers()
            + Self::composition_renderer()
            + structure::renderers()
            + chord_progression::renderers()
            + orchestration::renderers()
            + parts::renderers()
            + util::renderers()
    }

    fn composition_renderer() -> impl Renderer<Element = Composition> {
        AdhocRenderer::<Composition>::new(|composition, _| {
            Ok(vec![
                RandomKey.over(composition),
                RandomTimeSignature.over(composition),
                RandomTempo.over(composition),
                RandomInstrumentation.over(composition),
                Sections.over(composition),
            ])
        })
    }
}
