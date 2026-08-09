#!/usr/bin/env python3
from __future__ import annotations

"""Convert an extracted ARCZ source archive into a real Git checkout safely.

The source snapshot is committed to a timestamped backup branch before the
working tree is switched to origin/main. No local source is silently discarded.
Ignored asset/vendor folders remain in place because they are not part of the
tracked checkout.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / ".arcz"
ORIGIN = "https://github.com/MRTNLGDR/ARCZ.git"
STATE.mkdir(parents=True, exist_ok=True)


def run(args: list[str | os.PathLike[str]], *, allow_failure: bool = False, capture: bool = False):
    completed = subprocess.run(
        [str(value) for value in args],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=capture,
        check=False,
        shell=False,
    )
    if completed.returncode and not allow_failure:
        detail = (completed.stderr or completed.stdout or "").strip()
        raise RuntimeError(f"command failed ({completed.returncode}): {' '.join(map(str, args))}\n{detail}")
    return completed


def find_git() -> Path | None:
    found = shutil.which("git.exe") or shutil.which("git")
    if found:
        return Path(found).resolve()
    program_files = Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
    for candidate in (program_files / "Git/cmd/git.exe", program_files / "Git/bin/git.exe"):
        if candidate.is_file():
            return candidate.resolve()
    return None


def install_git() -> Path | None:
    winget = shutil.which("winget.exe")
    if not winget:
        candidate = Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft/WindowsApps/winget.exe"
        winget = str(candidate) if candidate.is_file() else None
    if not winget:
        return None
    run(
        [
            winget,
            "install",
            "--id",
            "Git.Git",
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ],
        allow_failure=True,
    )
    return find_git()


def main() -> int:
    if (ROOT / ".git").is_dir():
        return 0
    if os.name != "nt":
        raise RuntimeError("ZIP adoption is a Windows launcher operation")

    git = find_git() or install_git()
    if not git:
        raise RuntimeError("Git could not be installed, so the source ZIP cannot be adopted safely")

    stamp = time.strftime("%Y%m%d-%H%M%S")
    backup_branch = f"arcz-local-backup-{stamp}"

    run([git, "init", "-b", "arcz-local-bootstrap"])
    run([git, "config", "user.name", "ARCZ Launcher"])
    run([git, "config", "user.email", "arcz-launcher@localhost"])
    run([git, "add", "-A"])
    staged = run([git, "diff", "--cached", "--quiet"], allow_failure=True).returncode
    if staged == 0:
        # An archive with zero tracked files is not a valid ARCZ source tree.
        raise RuntimeError("source archive contains no trackable ARCZ files")
    run([git, "commit", "-m", "ARCZ local source snapshot before Git adoption"])
    snapshot = run([git, "rev-parse", "HEAD"], capture=True).stdout.strip()
    run([git, "branch", backup_branch, snapshot])
    run([git, "remote", "add", "origin", ORIGIN])
    run([git, "fetch", "--depth", "1", "origin", "main"])

    # Keep ignored local resources safe. A healthy repository must not track a
    # path that its own .gitignore hides, but refuse the conversion rather than
    # overwriting one if such a conflict appears.
    remote_paths = set(
        run([git, "ls-tree", "-r", "--name-only", "origin/main"], capture=True).stdout.splitlines()
    )
    ignored_paths = set(
        run(
            [git, "ls-files", "--others", "--ignored", "--exclude-standard"],
            capture=True,
        ).stdout.splitlines()
    )
    conflicts = sorted(remote_paths & ignored_paths)
    if conflicts:
        raise RuntimeError(
            "Git adoption refused because ignored local files collide with tracked origin paths: "
            + ", ".join(conflicts[:20])
        )

    changed = run([git, "diff", "--name-only", snapshot, "origin/main"], capture=True).stdout.splitlines()
    run([git, "checkout", "-B", "main", "origin/main"])
    run([git, "branch", "--set-upstream-to=origin/main", "main"])

    report = {
        "schema_version": 1,
        "origin": ORIGIN,
        "adopted_at": stamp,
        "source_snapshot": snapshot,
        "backup_branch": backup_branch,
        "remote_branch": "origin/main",
        "changed_tracked_files": len(changed),
        "local_source_preserved": True,
    }
    (STATE / "git-adoption.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"ARCZ Git adoption failed: {error}", file=sys.stderr)
        raise SystemExit(1)
