use rand::prelude::SliceRandom;
use redact_composer::error::RendererError::MissingContext;
use redact_composer::midi::elements::DrumKit;
use redact_composer::midi::gm::elements::Instrument;
use redact_composer::midi::gm::Instruments;
use redact_composer::render::{AdhocRenderer, RenderEngine};
use redact_composer::util::IntoCompositionSegment;
use redact_composer::{Element, Renderer};
use serde::{Deserialize, Serialize};

pub fn renderers() -> RenderEngine {
    RenderEngine::new() + RandomInstrumentation::renderer()
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct Instrumentation {
    pub drums: DrumKit,
    pub bass: Instrument,
    pub melody: Instrument,
    pub extras: Vec<Instrument>,
}

impl Instrumentation {
    pub fn melody_instruments() -> Vec<Instrument> {
        Instruments::melodic().into()
    }

    pub fn drum_instruments() -> Vec<DrumKit> {
        (0..=19)
            .chain(24..=30)
            .chain(32..=36)
            .chain(40..=42)
            .map(DrumKit::from)
            .collect::<Vec<_>>()
    }

    pub fn bass_instruments() -> Vec<Instrument> {
        (Instruments::bass() - Instrument::SlapBass1 - Instrument::SlapBass2 + Instruments::organ()
            - Instrument::Harmonica
            - Instrument::TangoAccordion
            + Instruments::synth_pad()
            - Instrument::PadBowed
            - Instrument::PadSweep
            - Instrument::PadMetallic
            - Instrument::PadWarm
            + Instruments::synth_lead()
            - Instrument::LeadFifths)
            .into()
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct RandomInstrumentation;

impl RandomInstrumentation {
    pub fn renderer() -> impl Renderer<Element = Self> {
        AdhocRenderer::<Self>::new(|segment, ctx| {
            let mut rng = ctx.rng();

            let melody = Instrumentation::melody_instruments()
                .choose(&mut rng)
                .copied()
                .ok_or(MissingContext(String::from(
                    "No available melody instruments.",
                )))?;

            let bass = Instrumentation::bass_instruments()
                .choose(&mut rng)
                .copied()
                .ok_or(MissingContext(String::from(
                    "No available bass instruments.",
                )))?;

            let drums = Instrumentation::drum_instruments()
                .choose(&mut rng)
                .copied()
                .ok_or(MissingContext(String::from(
                    "No available drum instruments.",
                )))?;

            let extras = Instrumentation::melody_instruments()
                .into_iter()
                .filter(|i| i != &melody)
                .collect::<Vec<_>>()
                .choose_multiple(&mut rng, 2)
                .copied()
                .collect::<Vec<_>>();

            Ok(vec![Instrumentation {
                drums,
                bass,
                melody,
                extras,
            }
            .into_segment(segment.timing)])
        })
    }
}

#[derive(Element, Serialize, Deserialize, Debug)]
pub struct PartArrangement;
