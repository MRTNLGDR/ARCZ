#!/usr/bin/env python3
from __future__ import annotations

"""Canonical ARCZ entrypoint for the controlled Aedifex fork.

It reuses the audited vendor pipeline while replacing only catalog localization
with the provenance-safe implementation that excludes remote-only content not
present in the pinned upstream source tree.
"""

import tools.vendor_aedifex as vendor
from arcz_server.aedifex_catalog_localizer import localize_catalog_assets


def _localize(fork):
    return localize_catalog_assets(fork, vendor._local_catalog_value)


def main() -> int:
    vendor.localize_catalog_assets = _localize
    return vendor.main()


if __name__ == "__main__":
    raise SystemExit(main())
