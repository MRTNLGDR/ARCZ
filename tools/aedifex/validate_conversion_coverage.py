#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from arcz_server.aedifex_inventory import validate_coverage
from arcz_server.atomic_io import atomic_write_json
from arcz_server.schema_validation import SchemaRegistry


def main() -> int:
    parser = argparse.ArgumentParser(description="Reprova qualquer superfície Aedifex não classificada.")
    parser.add_argument(
        "--inventory",
        type=Path,
        default=ROOT / "integrations/aedifex/generated/UPSTREAM_INVENTORY.json",
    )
    parser.add_argument(
        "--policy",
        type=Path,
        default=ROOT / "integrations/aedifex/CONVERSION_COVERAGE.json",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "integrations/aedifex/generated/CONVERSION_COVERAGE_REPORT.json",
    )
    args = parser.parse_args()
    inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
    policy = json.loads(args.policy.read_text(encoding="utf-8"))
    schemas = SchemaRegistry(ROOT / "schemas")
    schemas.validate("aedifex-conversion-policy.schema.json", policy)
    report = validate_coverage(inventory, policy)
    schemas.validate("aedifex-conversion-coverage.schema.json", report)
    atomic_write_json(args.output, report)
    print(json.dumps(report["counts"], ensure_ascii=False, sort_keys=True))
    if report["blockers"]:
        for blocker in report["blockers"][:100]:
            print(
                f"BLOCKED {blocker['category']} {blocker['id']} {blocker['status']}",
                file=sys.stderr,
            )
    return 0 if report["ready"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
