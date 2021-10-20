use anyhow::{Context, Error, Result};
use once_cell::sync::Lazy;
use std::{fs, sync::Arc};
use swc::{
    config::{JsMinifyOptions, JscTarget, Options, ParseOptions, SourceMapsConfig},
    try_with_handler, Compiler,
};
use swc_common::{FileName, FilePathMapping, SourceMap};
use swc_ecmascript::ast::Program;
use typefirstql::generate_schema;

fn main() {
    let filepath = std::env::args().nth(2).unwrap();
    let outpath = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "./generated.schema".into());

    println!("filepath={}, outpath={}", filepath, outpath);

    let code = fs::read_to_string(filepath).expect("failed to read file");
    let prog = parse_sync(
        code.as_str(),
        "{
            \"syntax\": \"typescript\",
            \"tsx\": true,
            \"decorators\": false,
            \"dynamicImport\": false
      }",
    )
    .unwrap();

    generate_schema(prog)
}
