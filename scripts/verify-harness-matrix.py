#!/usr/bin/env python3
"""Run real keyboard acceptance across room presets and terminal backgrounds.

Each child verifier owns its xterm/Xvfb processes and cleanup. Generated contact
sheets require manual inspection; this script only reports behavioral checks.
"""

import argparse
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--program", action="append", default=[])
    parser.add_argument(
        "--harness", action="append", choices=["pi", "omp", "claude", "hermes"]
    )
    parser.add_argument("--background", action="append", choices=["dark", "light"])
    parser.add_argument(
        "--terminal-theme", action="append",
        choices=["auto", "mono", "soft-green", "amber", "night"]
    )
    options = parser.parse_args()
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    harnesses = options.harness or ["pi", "omp", "claude", "hermes"]
    matrix = []
    themes = options.terminal_theme or ["auto", "mono", "soft-green", "amber", "night"]
    for background in options.background or ["dark", "light"]:
        for theme in themes:
            case = output / f"{background}-{theme}"
            case.mkdir(exist_ok=True)
            command = [
                sys.executable, str(ROOT / "scripts/verify-harness-keyboard.py"),
                "--binary", str(options.binary.resolve()), "--ui", "both",
                "--background", background, "--terminal-theme", theme,
                "--output", str(case),
            ]
            for program in options.program:
                command.extend(["--program", program])
            for harness in harnesses:
                command.extend(["--harness", harness])
            print(f"Running {background}/{theme}: {', '.join(harnesses)}", flush=True)
            with (case / "run.log").open("w") as log:
                status = subprocess.run(
                    command, stdout=log, stderr=subprocess.STDOUT
                ).returncode
            results_path = case / "results.json"
            results = json.loads(results_path.read_text()) if results_path.exists() else []
            passed = (
                status == 0 and len(results) == len(harnesses)
                and all(r.get("passed") for r in results)
            )
            record = {
                "background": background, "terminalTheme": theme, "command": command,
                "exitCode": status, "behaviorPassed": passed,
                "requiresManualInspection": True, "results": results,
            }
            matrix.append(record)
            (output / "matrix.json").write_text(json.dumps(matrix, indent=2) + "\n")
            print(json.dumps(record), flush=True)
            if not passed:
                return 1
            compose = [
                sys.executable, str(ROOT / "scripts/compose-harness-evidence.py"), str(case)
            ]
            for harness in harnesses:
                compose.extend(["--harness", harness])
            subprocess.run(compose, check=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
