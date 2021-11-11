use std::{collections::HashMap, fs};

use napi::{CallContext, Error, JsNumber, JsObject, JsString, Result};

use crate::{generate_schema, parse_ts, GraphQLKind};

#[cfg(all(
    any(windows, unix),
    target_arch = "x86_64",
    not(target_env = "musl"),
    not(debug_assertions)
))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[module_exports]
fn init(mut exports: JsObject) -> Result<()> {
    exports.create_named_method("generateSchema", generate)?;
    Ok(())
}

#[js_function(4)]
fn generate(ctx: CallContext) -> Result<JsString> {
    let code = ctx.get::<JsString>(0)?.into_utf8()?;
    let manifest = ctx.get::<JsString>(1)?.into_utf8()?;
    let opts = ctx.get::<JsString>(2)?.into_utf8()?;

    let manifest_raw: HashMap<String, u8> = serde_json::from_str(manifest.as_str()?)?;

    let mut manifest: HashMap<String, GraphQLKind> = HashMap::with_capacity(manifest_raw.len());
    manifest_raw.into_iter().for_each(|(s, val)| {
        manifest.insert(s, GraphQLKind::from_u8(val).unwrap());
    });

    let prog = match parse_ts(code.as_str()?, opts.as_str()?) {
        Ok(p) => p,
        Err(e) => return Err(Error::new(napi::Status::Unknown, format!("{:?}", e))),
    };

    let output = match generate_schema(prog.module().unwrap(), manifest) {
        Ok(output) => output,
        Err(e) => return Err(Error::new(napi::Status::Unknown, format!("{:?}", e))),
    };

    ctx.env.create_string(&output)
}
