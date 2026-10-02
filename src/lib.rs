// Several parsed fields (e.g. minion ids, boss flags) aren't consumed yet — see the "Known
// limitations" note for what's intentionally out of scope in this first version.
#![allow(dead_code)]

pub mod data;
pub mod details;
pub mod engine;
pub mod export;
pub mod model;
pub mod pipeline;

#[cfg(target_arch = "wasm32")]
mod wasm;
