//! Wrapper for Minecraft's `DeltaTracker` — the frame timer.

use crate::mapping::MappedObject;
use jni::objects::{GlobalRef, JValue};

/// Minecraft's `DeltaTracker`.
#[derive(Debug, Clone, MappedObject)]
#[mapped(class = DeltaTracker)]
pub struct DeltaTracker {
    jni_ref: GlobalRef,
}

impl DeltaTracker {
    /// Wraps an existing `DeltaTracker` JVM object.
    pub fn new(jni_ref: GlobalRef) -> DeltaTracker {
        DeltaTracker { jni_ref }
    }

    /// How far into the current game tick this frame is, `0.0..1.0` — the
    /// factor Minecraft interpolates entities and the camera with.
    pub fn partial_tick(&self) -> anyhow::Result<f32> {
        Ok(self
            .call_method("getGameTimeDeltaPartialTick", &[JValue::from(true)])?
            .f()?)
    }
}
