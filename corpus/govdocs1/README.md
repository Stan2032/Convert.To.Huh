# Govdocs1 real-world tier

206 real files from [Govdocs1](https://digitalcorpora.org/corpora/file-corpora/files/):
40 PDF, 40 JPEG, 27 GIF and 20 each of DOC, XLS and PPT from `thread0.zip`,
and all 39 PNGs from the ten thread archives. Synthetic specimens in the
rest of `corpus/` show that a loss exists; these show how often it applies to
files people actually have.

| File | Contents |
|---|---|
| `MANIFEST.tsv` | Member name, size and SHA-256 for each file. Committed. |
| `files/` | The files themselves. **Not committed**: Govdocs1 is described as "freely redistributed, to the best of our knowledge", which is not CC0. |
| `results.json` | Output of `tools/run_govdocs.py`: each file through `huh convert` / `huh clean`. Committed. |
| `office.json` | Output of `tools/measure_office.py`: the 60 office documents through LibreOffice to PDF. Committed. |
| `pdf_fidelity.json` | Output of `core/examples/pdf_fidelity.rs`: visual fidelity of `huh clean` on the 40 PDFs, stored and as viewed. Committed. |
| `edges.json` | Output of `core/examples/real_edges.rs`: every verified image edge over every real file of its source format. Committed. |

```bash
python3 tools/fetch_govdocs.py    # ~40 MB via HTTP range requests, verified by SHA-256
cargo build --release --manifest-path core/Cargo.toml
python3 tools/run_govdocs.py      # ~6 minutes; rewrites results.json
python3 tools/measure_office.py   # ~2 minutes; needs LibreOffice and catdoc
cargo run --release --manifest-path core/Cargo.toml --example pdf_fidelity -- \
    corpus/govdocs1/files corpus/govdocs1/pdf_fidelity.json   # ~15 minutes
python3 tools/real_world_blocks.py --write   # results -> registry real_world counts
cargo run --release --manifest-path core/Cargo.toml --example real_edges -- \
    corpus/govdocs1/files corpus/govdocs1/edges.json   # ~1 minute
```

The fetch script needs network access to `digitalcorpora.s3.amazonaws.com`, so
they are not part of the CI gate. Findings: `docs/RESEARCH.md §18` to §18.4, §19.
