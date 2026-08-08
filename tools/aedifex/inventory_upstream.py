#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from arcz_server.aedifex_inventory import inventory_upstream
from arcz_server.atomic_io import atomic_write_json
from arcz_server.schema_validation import SchemaRegistry


def main() -> int:
    parser = argparse.ArgumentParser(description="Inventaria integralmente um checkout Aedifex fixado.")
    parser.add_argument("--source", type=Path, default=ROOT / "opensources/upstream/aedifex")
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "integrations/aedifex/generated/UPSTREAM_INVENTORY.json",
    )
    parser.add_argument("--expected-commit")
    args = parser.parse_args()
    lock = json.loads((ROOT / "integrations/aedifex/UPSTREAM_LOCK.json").read_text(encoding="utf-8"))
    value = inventory_upstream(args.source, expected_commit=args.expected_commit or lock["commit"])
    SchemaRegistry(ROOT / "schemas").validate("aedifex-upstream-inventory.schema.json", value)
    atomic_write_json(args.output, value)
    print(args.output)
    print(json.dumps(value["counts"], ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
