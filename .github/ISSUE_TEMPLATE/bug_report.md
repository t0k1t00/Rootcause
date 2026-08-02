---
name: Bug report
about: Something in the engine, CLI, or a converter isn't working as documented
title: ""
labels: bug
---

**What happened?**
A clear description of the incorrect behavior.

**Expected behavior**
What you expected to happen instead (quote the relevant doc section if
the behavior contradicts documented behavior).

**Steps to reproduce**
The exact command(s) you ran. Attach or paste a minimal trace/pattern
file if the bug depends on specific input — please redact any real,
sensitive on-chain data first if your input came from a live trace.

```sh
rootcause analyze ...
```

**Environment**
- `rootcause version` output:
- OS:

**Is this a false positive or false negative in a specific detection
pattern?** If so, please say which pattern (`family`/pattern name from
the finding) and attach the trace — this is a correctness bug in that
pattern, not a security report (see `SECURITY.md` for what does count as
one).
