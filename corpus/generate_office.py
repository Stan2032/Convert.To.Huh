#!/usr/bin/env python3
"""Generate binary Office specimens (.doc, .xls, .ppt) from flat ODF.

    python3 corpus/generate_office.py      # needs LibreOffice (soffice)

Each specimen is written as a small flat-ODF document by this script, then
saved to the binary Microsoft format by LibreOffice. That makes the CONTENT
ours and reproducible; the BYTES are not, because LibreOffice stamps save times
into the file. Regenerating gives an equivalent specimen, not an identical one,
which is why the outputs are committed as well (corpus/README conventions).

Each one is built to exercise a loss found on real Govdocs1 files
(docs/RESEARCH.md §19). Font substitution is NOT measurable on these: the
binary files are written by LibreOffice, which names its own fonts in them,
so a substitute looks requested. Fonts are measured on the real files only.

  report.doc    requests Times New Roman and Arial, which this environment
                does not have, and has headings, so font substitution and
                PDF tagging are observable
  figures.xls   stores 25 values to full precision but displays 2 decimals,
                and has a long text cell beside a filled one, so precision
                loss and cell clipping are observable
  slides.ppt    three slides of text in Arial
"""
import subprocess
import sys
import tempfile
from pathlib import Path

OUT = Path(__file__).resolve().parent / "office"

NS = ('xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
      'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
      'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
      'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
      'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
      'xmlns:fo="urn:oasis:names:xsl-formatting:1.0" '
      'xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" '
      'xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0" '
      'xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"')

FONTS = ('<office:font-face-decls>'
         '<style:font-face style:name="Times New Roman" svg:font-family="&apos;Times New Roman&apos;"/>'
         '<style:font-face style:name="Arial" svg:font-family="Arial"/>'
         '</office:font-face-decls>')

PARAS = [
    "Quarterly survey of regional water quality",
    "Samples were collected from eleven monitoring stations along the river basin.",
    "Turbidity exceeded the advisory threshold at three upstream stations in March.",
    "Dissolved oxygen recovered after the reservoir release schedule was adjusted.",
    "Recommendations include additional sampling during the spring runoff period.",
]


def fodt() -> str:
    body = "".join(
        f'<text:h text:outline-level="1" text:style-name="H">{p}</text:h>' if i == 0
        else f'<text:p text:style-name="P">{p}</text:p>'
        for i, p in enumerate(PARAS))
    return (f'<?xml version="1.0" encoding="UTF-8"?>'
            f'<office:document {NS} office:version="1.3" '
            f'office:mimetype="application/vnd.oasis.opendocument.text">{FONTS}'
            '<office:automatic-styles>'
            '<style:style style:name="H" style:family="paragraph">'
            '<style:text-properties style:font-name="Arial" fo:font-size="16pt" fo:font-weight="bold"/></style:style>'
            '<style:style style:name="P" style:family="paragraph">'
            '<style:text-properties style:font-name="Times New Roman" fo:font-size="12pt"/></style:style>'
            '</office:automatic-styles>'
            f'<office:body><office:text>{body}</office:text></office:body></office:document>')


RIVERS = ["Ashby", "Brenton", "Calder", "Dunmore", "Elsworth", "Farrow", "Galway", "Harlan",
          "Ingram", "Jessop", "Kendall", "Lowther", "Marlow", "Norbury", "Oakham", "Pembury",
          "Quarley", "Redmire", "Stanton", "Thorley", "Upwell", "Varley", "Wexcombe",
          "Yarrow", "Zennor"]
# Stored to full double precision, displayed to two decimals: what survives a
# print is what was displayed. Deterministic, and distinct after rounding.
VALUES = [round((i + 2) ** 0.5 * 1.37 + i / 1000, 14) for i in range(len(RIVERS))]
LONG = ("Glossary: turbidity is a measure of water clarity expressed in nephelometric "
        "units, and readings above the advisory threshold trigger resampling.")


def fods() -> str:
    rows = "".join(
        f'<table:table-row><table:table-cell office:value-type="string"><text:p>{name} weir</text:p></table:table-cell>'
        f'<table:table-cell table:style-name="N2" office:value-type="float" office:value="{v!r}">'
        f'<text:p>{v:.2f}</text:p></table:table-cell></table:table-row>'
        for name, v in zip(RIVERS, VALUES))
    rows += (f'<table:table-row><table:table-cell office:value-type="string"><text:p>{LONG}</text:p>'
             '</table:table-cell><table:table-cell office:value-type="string"><text:p>adjacent'
             '</text:p></table:table-cell></table:table-row>')
    return (f'<?xml version="1.0" encoding="UTF-8"?>'
            f'<office:document {NS} office:version="1.3" '
            f'office:mimetype="application/vnd.oasis.opendocument.spreadsheet">{FONTS}'
            '<office:styles><number:number-style style:name="F2">'
            '<number:number number:decimal-places="2" number:min-integer-digits="1"/>'
            '</number:number-style></office:styles>'
            '<office:automatic-styles><style:style style:name="N2" style:family="table-cell" '
            'style:data-style-name="F2"/></office:automatic-styles>'
            f'<office:body><office:spreadsheet><table:table table:name="Readings">'
            f'<table:table-column table:number-columns-repeated="2"/>{rows}'
            '</table:table></office:spreadsheet></office:body></office:document>')


SLIDES = [
    ("Monitoring network", "Eleven stations sampled monthly along the basin"),
    ("Spring findings", "Turbidity advisories at three upstream stations"),
    ("Next steps", "Extend sampling through the runoff period"),
]


def fodp() -> str:
    pages = "".join(
        f'<draw:page draw:name="p{i}" draw:master-page-name="M">'
        f'<draw:frame svg:x="2cm" svg:y="2cm" svg:width="24cm" svg:height="3cm">'
        f'<draw:text-box><text:p text:style-name="T">{title}</text:p></draw:text-box></draw:frame>'
        f'<draw:frame svg:x="2cm" svg:y="7cm" svg:width="24cm" svg:height="8cm">'
        f'<draw:text-box><text:p text:style-name="B">{body}</text:p></draw:text-box></draw:frame>'
        '</draw:page>'
        for i, (title, body) in enumerate(SLIDES))
    return (f'<?xml version="1.0" encoding="UTF-8"?>'
            f'<office:document {NS} office:version="1.3" '
            f'office:mimetype="application/vnd.oasis.opendocument.presentation">{FONTS}'
            '<office:master-styles><style:master-page style:name="M"/></office:master-styles>'
            '<office:automatic-styles>'
            '<style:style style:name="T" style:family="paragraph">'
            '<style:text-properties style:font-name="Arial" fo:font-size="32pt"/></style:style>'
            '<style:style style:name="B" style:family="paragraph">'
            '<style:text-properties style:font-name="Arial" fo:font-size="20pt"/></style:style>'
            '</office:automatic-styles>'
            f'<office:body><office:presentation>{pages}</office:presentation></office:body></office:document>')


def truth() -> dict[str, str]:
    """The exact content of each specimen, as stored rather than as displayed.

    Written beside each specimen as <stem>.truth.txt. For a synthetic file the
    content is known, so tools/measure_office.py compares against it instead
    of an extractor: catppt reads nothing from a LibreOffice-written .ppt.
    """
    return {
        "report": "\n".join(PARAS),
        "figures": "\n".join(f"{n} weir {v!r}" for n, v in zip(RIVERS, VALUES)) + f"\n{LONG}\nadjacent",
        "slides": "\n".join(f"{t}\n{b}" for t, b in SLIDES),
    }


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    jobs = [("report", "fodt", fodt(), "doc:MS Word 97"),
            ("figures", "fods", fods(), "xls:MS Excel 97"),
            ("slides", "fodp", fodp(), "ppt:MS PowerPoint 97")]
    print("generating office specimens:")
    with tempfile.TemporaryDirectory() as td, tempfile.TemporaryDirectory() as prof:
        for stem, ext, xml, target in jobs:
            src = Path(td) / f"{stem}.{ext}"
            src.write_text(xml, "utf-8")
            subprocess.run(["soffice", f"-env:UserInstallation=file://{prof}", "--headless",
                            "--convert-to", target, "--outdir", str(OUT), str(src)],
                           check=True, capture_output=True, timeout=180)
            out = OUT / f"{stem}.{target.split(':')[0]}"
            if not out.is_file():
                print(f"  FAILED {out.name}", file=sys.stderr)
                return 1
            (OUT / f"{stem}.truth.txt").write_text(truth()[stem] + "\n", "utf-8")
            print(f"  {out.name:<14} {out.stat().st_size:>7} bytes  + {stem}.truth.txt")
    print("done")
    return 0


if __name__ == "__main__":
    sys.exit(main())
