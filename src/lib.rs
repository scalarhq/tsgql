mod codegen;

pub use codegen::*;

#[cfg(feature = "node")]
#[macro_use]
extern crate napi_derive;
#[cfg(feature = "node")]
mod node;

#[cfg(feature = "node")]
pub use node::*;
