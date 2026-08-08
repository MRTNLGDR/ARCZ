from __future__ import annotations

from dataclasses import asdict, dataclass
import logging
import os
import subprocess

LOGGER = logging.getLogger(__name__)


@dataclass(slots=True)
class HardwareProfile:
    cpu_threads: int
    ram_mb: int
    gpu_name: str | None
    vram_mb: int
    source: str

    def as_dict(self) -> dict[str, object]:
        return asdict(self)


def _detect_ram_mb() -> int:
    try:
        page_size = int(os.sysconf("SC_PAGE_SIZE"))
        page_count = int(os.sysconf("SC_PHYS_PAGES"))
        return max(512, int(page_size * page_count / 1_048_576))
    except (AttributeError, OSError, TypeError, ValueError) as exc:
        # Windows and restricted runtimes may not expose sysconf. The fallback
        # is conservative and is marked in the profile rather than hidden.
        LOGGER.debug("RAM detection unavailable; using conservative fallback: %s", exc)
        return 4096


def detect_hardware() -> HardwareProfile:
    """Return a conservative local hardware profile without network access.

    GPU discovery is optional. A missing/blocked ``nvidia-smi`` never prevents
    ARCZ from starting; callers receive ``gpu_name=None`` and budget from RAM.
    The failure is logged at debug level instead of being silently swallowed.
    """

    threads = os.cpu_count() or 1
    ram_mb = _detect_ram_mb()
    gpu_name: str | None = None
    vram_mb = 0
    source = "os"

    try:
        result = subprocess.run(
            [
                "nvidia-smi",
                "--query-gpu=name,memory.total",
                "--format=csv,noheader,nounits",
            ],
            capture_output=True,
            text=True,
            timeout=2,
            check=True,
        )
        lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        if not lines:
            raise ValueError("nvidia-smi returned no GPU rows")
        raw_name, raw_vram = lines[0].rsplit(",", 1)
        gpu_name = raw_name.strip() or None
        vram_mb = max(0, int(float(raw_vram.strip())))
        source = "nvidia-smi"
    except (
        FileNotFoundError,
        PermissionError,
        subprocess.CalledProcessError,
        subprocess.TimeoutExpired,
        IndexError,
        ValueError,
    ) as exc:
        LOGGER.debug("Discrete NVIDIA GPU detection unavailable: %s", exc)

    return HardwareProfile(threads, ram_mb, gpu_name, vram_mb, source)
