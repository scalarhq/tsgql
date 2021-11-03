#[cfg(not(feature = "node"))]
fn main() {
    use std::fs::{self};
    use tsgql::{generate_schema, parse_ts};
    let filepath = std::env::args().nth(2).unwrap();
    let outpath = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "./generated.schema".into());

    println!("filepath={}, outpath={}", filepath, outpath);

    let code = fs::read_to_string(filepath).expect("failed to read file");
    let prog = parse_ts(
        code.as_str(),
        "{
            \"syntax\": \"typescript\",
            \"tsx\": true,
            \"decorators\": false,
            \"dynamicImport\": false
      }",
    )
    .unwrap();

    // generate_schema(prog)
}

#[cfg(feature = "node")]
fn main() {}
