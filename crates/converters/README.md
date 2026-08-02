# converters

Converts real Ethereum client trace output into `ingestion::raw::RawTraceDocument`,
Root Cause's internal raw schema, so people don't have to hand-write that
JSON themselves.

A converter is a **pure function**: it takes bytes/JSON already retrieved
by some other means (a saved RPC response, a CLI-piped file) and emits a
`RawTraceDocument`. No converter here performs network I/O — that mirrors
`ingestion::source::TraceSource`'s own separation of "where bytes come
from" from "how they're interpreted".

## Module map

| Module | Contents |
|---|---|
| `geth` | Geth `debug_traceTransaction` `callTracer` → `RawTraceDocument`. Verified against a real saved Goerli response, and (Milestone 11) a real saved Erigon `callTracer` response too — Erigon implements the identical schema, so this same module handles both; see the module's own docs for exactly what is and isn't faithfully represented. |
| `bin/geth_convert` | `geth-convert`, the CLI entry point wrapping `geth::convert`. Works unmodified against either a Geth or an Erigon `callTracer` response. |

## Usage

```sh
cargo run -p converters --bin geth-convert -- \
    --input demo/samples/geth_calltracer_goerli.json \
    --tx-hash 0xc0ffcf21dc1881c3f20160b530d76f1d37af7b40552f7c59d3f339bad3023dad \
    --block 0x876123 \
    --chain-id 0x5 \
    --nonce 0x1 \
    --output demo/converted/geth_goerli.json

cargo run -p cli -- analyze demo/converted/geth_goerli.json --patterns patterns/
```

The same binary also converts a real Erigon `callTracer` response
unchanged — see `demo/samples/erigon_calltracer_polygon.json` and the
top-level README's "Erigon" section.

## Status

`geth` is supported and verified against a real sample from **both**
Geth and Erigon (see the top-level README's "Trace format converters"
section for the full compatibility matrix, including
Foundry/Hardhat/Tenderly/Nethermind, none of which are implemented
yet). See crate-level rustdoc (`cargo doc -p converters --open`) for
the full API reference.
