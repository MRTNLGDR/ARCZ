#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.aedifex_registry import AedifexRegistry
from arcz_server.schema_validation import SchemaRegistry


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify the pinned local Aedifex installation/fork/build")
    parser.add_argument(
        "--output",
        default="validation/aedifex-integration-status.json",
        help="JSON report path relative to the project root",
    )
    args = parser.parse_args()

    lock = json.loads((ROOT / "integrations/aedifex/UPSTREAM_LOCK.json").read_text(encoding="utf-8"))
    SchemaRegistry(ROOT / "schemas").validate("aedifex-installation.schema.json", lock)
    status = AedifexRegistry(ROOT).status(verify_tree=True)

    output = (ROOT / args.output).resolve()
    if ROOT not in output.parents:
        raise SystemExit("output must remain inside the project root")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(status, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({
        "ready": status["ready"],
        "report": str(output.relative_to(ROOT)),
        "blockers": [item.get("code") for item in status.get("blockers", [])],
    }, ensure_ascii=False))
    return 0 if status["ready"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
