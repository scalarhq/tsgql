#[cfg(feature = "node")]
extern crate napi_build;

fn main() {
    #[cfg(feature = "node")]
    napi_build::setup();
}
