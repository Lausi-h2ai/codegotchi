#!/usr/bin/env python3
"""Compose retained harness screenshots for manual review (requires ImageMagick)."""

import argparse
import json
import pathlib
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=pathlib.Path)
    parser.add_argument("--harness", action="append", required=True)
    options = parser.parse_args()
    directory = options.directory.resolve()
    groups = {
        "layouts": ["full-edited", "compact-edited", "minimal-edited", "narrow-edited"],
        "states": [
            "edited",
            "response",
            "history",
            "strict-response",
            "care-after",
            "care-keyboard-response",
        ],
    }
    sheets = []
    for harness in options.harness:
        for group, suffixes in groups.items():
            sources = [directory / f"{harness}-{suffix}.png" for suffix in suffixes]
            if any(not source.is_file() for source in sources):
                missing = [source.name for source in sources if not source.is_file()]
                raise FileNotFoundError(
                    f"{harness} {group} source frames missing: {', '.join(missing)}"
                )
            output = directory / f"{harness}-{group}-sheet.png"
            subprocess.run(
                [
                    "montage",
                    "-background",
                    "#202020",
                    "-fill",
                    "white",
                    "-font",
                    "DejaVu-Sans",
                    "-pointsize",
                    "14",
                    "-label",
                    "%f",
                    *map(str, sources),
                    "-geometry",
                    "640x520+6+6",
                    "-tile",
                    "2x",
                    str(output),
                ],
                check=True,
                timeout=30,
            )
            sheets.append(
                {"sheet": output.name, "sources": [source.name for source in sources]}
            )
    (directory / "sheets.json").write_text(
        json.dumps({"requiresManualInspection": True, "sheets": sheets}, indent=2)
        + "\n"
    )


if __name__ == "__main__":
    main()
