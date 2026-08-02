# Security Policy

Root Cause is a static analysis tool: it reads a trace file you provide and
a pattern library you provide, and produces a report. It does not execute
untrusted code, does not make network calls, and does not hold any
credentials or private keys. That said, it does parse untrusted, externally
sourced input (trace JSON, potentially fetched from a third-party RPC
provider or block explorer, and `.rcdsl` pattern files from a library you
may not have authored yourself), so parser and deserialization bugs are a
real, in-scope class of vulnerability.

## Reporting a vulnerability

If you believe you've found a security vulnerability in Root Cause itself —
for example, a crash, panic, resource-exhaustion, or memory-safety issue
triggerable by a malicious trace file or pattern file — please report it
privately rather than opening a public GitHub issue:

1. Use [GitHub's private vulnerability reporting](https://github.com/rootcause-project/rootcause/security/advisories/new)
   for this repository, if enabled, **or**
2. Email the maintainers (see the repository's GitHub profile for current
   contact details) with a description of the issue, steps to reproduce,
   and, if possible, a minimal trace or pattern file that triggers it.

Please include:

- The version or commit hash you tested against (`rootcause version`
  prints exact versions for every engine crate).
- Whether the issue is a panic/crash, a hang, or something else.
- A minimal reproduction, if you have one — this project's own error
  handling already treats every ingestion, parsing, and I/O failure as a
  recoverable `Result` rather than a panic (see
  `docs/adr/0003-error-handling-strategy.md`), so an input that *does*
  panic represents a genuine bug in that guarantee, not expected behavior.

We'll acknowledge reports and aim to keep you updated as a fix is
developed. Please give us a reasonable window to address the issue before
any public disclosure.

## What's out of scope

Root Cause's findings are, by design, advisory: a `GROUNDED` finding is a
reviewable candidate for human judgment, not a proof of exploitability, and
an `Abstain`/`Ungrounded`/absent finding is not a guarantee of safety (see
"Precision is prioritized over recall" in `README.md`). Disagreeing with a
specific finding, or finding a false positive/negative in a detection
pattern, is a correctness issue best filed as a normal public GitHub issue
or PR — not a security report against this project.
