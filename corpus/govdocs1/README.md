# Govdocs1 real-world tier

110 real files from [Govdocs1](https://digitalcorpora.org/corpora/file-corpora/files/)
(`thread0.zip`): 40 PDF, 40 JPEG, 27 GIF, 3 PNG. Synthetic specimens in the
rest of `corpus/` show that a loss exists; these show how often it applies to
files people actually have.

| File | Contents |
|---|---|
| `MANIFEST.tsv` | Member name, size and SHA-256 for each file. Committed. |
| `files/` | The files themselves. **Not committed**: Govdocs1 is described as "freely redistributed, to the best of our knowledge", which is not CC0. |
| `results.json` | Output of `tools/run_govdocs.py`. Committed. |

```bash
python3 tools/fetch_govdocs.py    # ~40 MB via HTTP range requests, verified by SHA-256
cargo build --release --manifest-path core/Cargo.toml
python3 tools/run_govdocs.py      # ~6 minutes; rewrites results.json
```

Both scripts need network access to `digitalcorpora.s3.amazonaws.com`, so
they are not part of the CI gate. Findings: `docs/RESEARCH.md §18`.
