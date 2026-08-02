//! `geth-convert`: turn a saved Geth `debug_traceTransaction` /
//! `callTracer` JSON-RPC response into a Root Cause `RawTraceDocument`
//! JSON file, ready for `cargo run -p cli -- analyze <file> --patterns patterns/`.
//!
//! ```text
//! cargo run -p converters --bin geth-convert -- \
//!     --input demo/samples/geth_calltracer_goerli.json \
//!     --tx-hash 0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad \
//!     --block 0x876123 \
//!     --chain-id 0x5 \
//!     --nonce 0x1 \
//!     --output demo/converted/geth_goerli.json
//! ```
#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use converters::geth::{self, GethTraceContext};

struct Args {
    input: PathBuf,
    output: PathBuf,
    tx_hash: String,
    block: String,
    chain_id: String,
    nonce: String,
}

fn parse_args() -> Result<Args, String> {
    let mut input = None;
    let mut output = None;
    let mut tx_hash = None;
    let mut block = None;
    let mut chain_id = None;
    let mut nonce = "0x0".to_string();

    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        let mut next = || iter.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--input" => input = Some(PathBuf::from(next()?)),
            "--output" => output = Some(PathBuf::from(next()?)),
            "--tx-hash" => tx_hash = Some(next()?),
            "--block" => block = Some(next()?),
            "--chain-id" => chain_id = Some(next()?),
            "--nonce" => nonce = next()?,
            other => return Err(format!("unrecognized flag `{other}`")),
        }
    }

    Ok(Args {
        input: input.ok_or("--input <geth_calltracer.json> is required")?,
        output: output.ok_or("--output <out.json> is required")?,
        tx_hash: tx_hash.ok_or("--tx-hash <0x...> is required")?,
        block: block.ok_or("--block <0x...> is required")?,
        chain_id: chain_id.ok_or("--chain-id <0x...> is required")?,
        nonce,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let input_json = fs::read_to_string(&args.input)
        .map_err(|e| format!("reading {}: {e}", args.input.display()))?;

    let ctx = GethTraceContext {
        transaction_hash: args.tx_hash,
        nonce: args.nonce,
        block_number: args.block,
        chain_id: args.chain_id,
    };

    let doc = geth::convert(&input_json, &ctx).map_err(|e| e.to_string())?;
    let out_json =
        serde_json::to_string_pretty(&doc).map_err(|e| format!("serializing output: {e}"))?;
    fs::write(&args.output, out_json)
        .map_err(|e| format!("writing {}: {e}", args.output.display()))?;

    eprintln!("Wrote {}", args.output.display());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
