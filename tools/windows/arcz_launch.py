#!/usr/bin/env python3
from __future__ import annotations

"""Canonical Windows controller used only through ARCZ.bat.

The normal user path is deliberately one entrypoint:

    ARCZ.bat

It updates the current Git branch without destroying local edits, prepares the
pinned local vendors when needed, builds the Rust workspace, materializes a real
Blender runtime, executes the repository gates after a commit changes and opens
the local-only ARCZ server. Missing capability is an error, never a mock.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import urllib.request
import webbrowser

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / ".arcz"
LOGS = STATE / "logs"
TOOLCHAINS = ROOT / "vendor" / "toolchains"
ASSETS = ROOT / "resources" / "assets"
PREPARED_HEAD = STATE / "prepared-head.txt"
VERIFIED_HEAD = STATE / "verified-head.txt"
SERVER_PID = STATE / "server.pid"
LAUNCH_LOG = LOGS / "launcher-latest.log"

STATE.mkdir(parents=True, exist_ok=True)
LOGS.mkdir(parents=True, exist_ok=True)
TOOLCHAINS.mkdir(parents=True, exist_ok=True)
ASSETS.mkdir(parents=True, exist_ok=True)


class Tee:
    def __init__(self, path: Path) -> None:
        self.stream = path.open("w", encoding="utf-8", errors="replace")

    def write(self, text: str) -> None:
        print(text, flush=True)
        self.stream.write(text + "\n")
        self.stream.flush()

    def close(self) -> None:
        self.stream.close()


LOG = Tee(LAUNCH_LOG)


def step(text: str) -> None:
    LOG.write("")
    LOG.write(f"=== {text} ===")


def q(value: object) -> str:
    text = str(value)
    return f'"{text}"' if any(char.isspace() for char in text) else text


def run(
    args: list[str | os.PathLike[str]],
    *,
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    allow_failure: bool = False,
    capture: bool = False,
) -> subprocess.CompletedProcess[str]:
    command = [str(value) for value in args]
    LOG.write("+ " + " ".join(q(value) for value in command))
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=capture,
        check=False,
        shell=False,
    )
    if capture:
        stdout = completed.stdout or ""
        stderr = completed.stderr or ""
        if stdout.strip():
            LOG.write(stdout.rstrip())
        if stderr.strip():
            LOG.write(stderr.rstrip())
    if completed.returncode and not allow_failure:
        raise RuntimeError(
            f"command failed rc={completed.returncode}: {' '.join(command)}"
        )
    return completed


def which(name: str, candidates: tuple[Path, ...] = ()) -> Path | None:
    found = shutil.which(name)
    if found:
        path = Path(found)
        if path.is_file():
            return path.resolve()
    for candidate in candidates:
        if candidate and candidate.is_file():
            return candidate.resolve()
    return None


def winget() -> Path | None:
    candidates = (
        Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft/WindowsApps/winget.exe",
    )
    return which("winget.exe", candidates)


def winget_install(package_id: str) -> bool:
    executable = winget()
    if not executable:
        return False
    LOG.write(f"[ARCZ] installing {package_id} with winget")
    result = run(
        [
            executable,
            "install",
            "--id",
            package_id,
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ],
        allow_failure=True,
    )
    return result.returncode == 0


def read_marker(path: Path) -> str:
    try:
        return path.read_text(encoding="ascii").strip()
    except OSError:
        return ""


def git_executable() -> Path:
    step("Git + source update")
    program_files = Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
    git = which(
        "git.exe",
        (
            program_files / "Git/cmd/git.exe",
            program_files / "Git/bin/git.exe",
        ),
    )
    if not git:
        winget_install("Git.Git")
        git = which("git.exe", (program_files / "Git/cmd/git.exe",))
    if not git:
        raise RuntimeError("Git is missing and automatic Git.Git installation failed")
    run([git, "--version"])
    return git


def update_repository(git: Path, *, skip: bool) -> None:
    if skip:
        LOG.write("[ARCZ] Git update skipped by --skip-update")
        return
    if not (ROOT / ".git").is_dir():
        raise RuntimeError(
            "repository has no .git; ARCZ.bat should adopt source ZIPs before this controller runs"
        )
    branch_result = run(
        [git, "-C", ROOT, "symbolic-ref", "--quiet", "--short", "HEAD"],
        allow_failure=True,
        capture=True,
    )
    branch = (branch_result.stdout or "").strip()
    if not branch:
        raise RuntimeError("Git checkout is detached; automatic update refuses to guess a branch")

    # Fetch before touching the worktree. If the Internet is unavailable, a
    # previously prepared checkout must still be allowed to run fully offline.
    fetched = run(
        [git, "-C", ROOT, "fetch", "--prune", "origin"],
        allow_failure=True,
    )
    if fetched.returncode:
        LOG.write(
            "[WARN] git fetch failed; continuing with the current local commit. "
            "Setup will still fail closed later if this commit lacks required vendors."
        )
        return

    remote = f"refs/remotes/origin/{branch}"
    exists = run(
        [git, "-C", ROOT, "show-ref", "--verify", "--quiet", remote],
        allow_failure=True,
    ).returncode == 0
    if not exists:
        LOG.write(f"[WARN] origin/{branch} does not exist; local branch kept after fetch")
        return

    dirty = run(
        [git, "-C", ROOT, "status", "--porcelain", "--untracked-files=all"],
        capture=True,
    ).stdout.strip()
    stashed = False
    stash_name = ""
    if dirty:
        stamp = time.strftime("%Y%m%d-%H%M%S")
        stash_name = f"ARCZ launcher autostash {stamp}"
        LOG.write("[ARCZ] local changes found; preserving them temporarily in git stash")
        run([git, "-C", ROOT, "stash", "push", "-u", "-m", stash_name])
        (STATE / "last-autostash.txt").write_text(stash_name + "\n", encoding="utf-8")
        stashed = True

    merge_error: Exception | None = None
    try:
        run([git, "-C", ROOT, "merge", "--ff-only", f"origin/{branch}"])
        LOG.write(f"[OK] synchronized {branch} with origin/{branch}")
    except Exception as error:
        merge_error = error
    finally:
        if stashed:
            restored = run([git, "-C", ROOT, "stash", "pop"], allow_failure=True)
            if restored.returncode:
                raise RuntimeError(
                    f"Git update used stash '{stash_name}', but automatic reapply conflicted. "
                    "Your edits remain recoverable in the worktree/stash; resolve the Git conflict before ARCZ continues."
                )
            LOG.write("[OK] local edits restored after Git update")
    if merge_error is not None:
        raise merge_error


def head_sha(git: Path) -> str:
    value = run([git, "-C", ROOT, "rev-parse", "HEAD"], capture=True).stdout.strip()
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise RuntimeError(f"invalid Git HEAD: {value!r}")
    return value


def venv_python(*, refresh_dependencies: bool) -> Path:
    step("Repository Python environment")
    venv = ROOT / ".venv"
    python = venv / "Scripts/python.exe"
    if not python.is_file():
        run([sys.executable, "-m", "venv", venv])
        refresh_dependencies = True
    if refresh_dependencies:
        run([python, "-m", "pip", "install", "--upgrade", "pip"])
        run(
            [
                python,
                "-m",
                "pip",
                "install",
                "-r",
                ROOT / "requirements.txt",
                "-r",
                ROOT / "requirements-dev.txt",
            ]
        )
    return python


def semver_tuple(value: str) -> tuple[int, int, int]:
    match = re.search(r"(\d+)\.(\d+)(?:\.(\d+))?", value)
    if not match:
        return (0, 0, 0)
    return (int(match.group(1)), int(match.group(2)), int(match.group(3) or 0))


def node_tools() -> tuple[Path, Path]:
    step("Node.js 22+")
    program_files = Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
    node = which("node.exe", (program_files / "nodejs/node.exe",))
    version = ""
    if node:
        version = run([node, "--version"], capture=True).stdout.strip()
    if semver_tuple(version) < (22, 0, 0):
        winget_install("OpenJS.NodeJS.LTS")
        node = which("node.exe", (program_files / "nodejs/node.exe",))
        version = run([node, "--version"], capture=True).stdout.strip() if node else ""
    if not node or semver_tuple(version) < (22, 0, 0):
        raise RuntimeError("Node.js 22+ could not be installed automatically")
    node_dir = node.parent
    os.environ["PATH"] = str(node_dir) + os.pathsep + os.environ.get("PATH", "")
    npm = which("npm.cmd", (node_dir / "npm.cmd",))
    if not npm:
        raise RuntimeError("npm.cmd is missing next to Node.js")
    return node, npm


def bun_tool(npm: Path) -> Path:
    step("Repo-local Bun build tool")
    prefix = TOOLCHAINS / "bun"
    bun = prefix / "node_modules/.bin/bun.cmd"
    if not bun.is_file():
        run([npm, "install", "--prefix", prefix, "--no-audit", "--no-fund", "bun@1.3.14"])
    if not bun.is_file():
        raise RuntimeError("Bun 1.3.14 was not materialized in vendor/toolchains")
    os.environ["PATH"] = str(bun.parent) + os.pathsep + os.environ.get("PATH", "")
    run([bun, "--version"])
    return bun


def cargo_tool() -> Path:
    step("Rust 1.97.1")
    cargo_home = Path.home() / ".cargo/bin"
    rustup = which("rustup.exe", (cargo_home / "rustup.exe",))
    if not rustup:
        winget_install("Rustlang.Rustup")
        rustup = which("rustup.exe", (cargo_home / "rustup.exe",))
    if not rustup:
        raise RuntimeError("rustup could not be installed automatically")
    os.environ["PATH"] = str(cargo_home) + os.pathsep + os.environ.get("PATH", "")
    run(
        [
            rustup,
            "toolchain",
            "install",
            "1.97.1",
            "--profile",
            "minimal",
            "--component",
            "rustfmt",
            "--component",
            "clippy",
            "--target",
            "wasm32-unknown-unknown",
        ]
    )
    cargo = which("cargo.exe", (cargo_home / "cargo.exe",))
    if not cargo:
        raise RuntimeError("cargo.exe missing after Rust installation")
    return cargo


def _run_offline_probe(python: Path, script: Path, *args: str) -> tuple[int, str]:
    env = os.environ.copy()
    env["ARCZ_NETWORK_MODE"] = "offline_strict"
    result = subprocess.run(
        [str(python), str(script), *args],
        cwd=ROOT,
        env=env,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return result.returncode, result.stdout or ""


def interactive_preflight(python: Path) -> bool:
    runtime_rc, runtime_text = _run_offline_probe(
        python, ROOT / "tools/runtime_preflight.py", "--profile", "interactive"
    )
    ifc_rc, ifc_text = _run_offline_probe(python, ROOT / "tools/ifc_preflight.py")
    target = STATE / "interactive-preflight.json"
    try:
        runtime_report = json.loads(runtime_text)
    except json.JSONDecodeError:
        runtime_report = {"ready": False, "raw": runtime_text[-8000:]}
    try:
        ifc_report = json.loads(ifc_text)
    except json.JSONDecodeError:
        ifc_report = {"ready": False, "raw": ifc_text[-8000:]}
    combined = {
        "schema_version": 1,
        "profile": "interactive+ifc",
        "network_mode": "offline_strict",
        "ready": runtime_rc == 0 and ifc_rc == 0,
        "runtime": runtime_report,
        "ifc": ifc_report,
    }
    target.write_text(json.dumps(combined, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return bool(combined["ready"])


def prepare_interactive(python: Path) -> None:
    step("Pinned Cesium + controlled Aedifex + verified IfcOpenShell")
    env = os.environ.copy()
    env["ARCZ_NETWORK_MODE"] = "import_assisted"
    run(
        [python, ROOT / "tools/prepare_local_runtime.py", "--interactive"],
        env=env,
    )
    if not interactive_preflight(python):
        LOG.write((STATE / "interactive-preflight.json").read_text(encoding="utf-8"))
        raise RuntimeError("interactive+IFC profile remained blocked after preparation")


def build_rust(cargo: Path) -> None:
    step("Rust release workers")
    run([cargo, "+1.97.1", "build", "--release", "--workspace", "--locked"])


def blender_vendor_ready(python: Path) -> bool:
    code = (
        "from pathlib import Path; "
        "from tools.runtime_preflight import _blender_check; "
        "r=_blender_check(Path.cwd()); "
        "raise SystemExit(0 if r['status']=='READY' else 1)"
    )
    result = subprocess.run([str(python), "-c", code], cwd=ROOT, check=False)
    return result.returncode == 0


def find_blender() -> Path | None:
    direct = which("blender.exe")
    if direct:
        return direct
    roots = [
        Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "Blender Foundation",
        Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Blender Foundation",
    ]
    candidates: list[Path] = []
    for base in roots:
        if base.is_dir():
            candidates.extend(path for path in base.rglob("blender.exe") if path.is_file())
    return sorted(candidates, reverse=True)[0] if candidates else None


def find_blender_license(directory: Path) -> Path | None:
    for name in ("GPL3-license.txt", "LICENSE", "LICENSE.txt", "COPYING", "copyright.txt"):
        candidate = directory / name
        if candidate.is_file() and candidate.stat().st_size:
            return candidate
    for candidate in directory.rglob("*"):
        if candidate.is_file() and re.match(r"(?i)^(license|copying|copyright)", candidate.name):
            if candidate.stat().st_size:
                return candidate
    return None


def ensure_blender_vendor(python: Path, *, skip: bool) -> None:
    if skip:
        LOG.write("[ARCZ] Blender/Cycles skipped by --skip-photoreal")
        return
    step("Real Blender/Cycles vendor")
    if blender_vendor_ready(python):
        LOG.write("[OK] vendor/blender already passes SHA-256 integrity")
        return
    blender = find_blender()
    if not blender:
        for package in ("BlenderFoundation.Blender", "BlenderFoundation.Blender.LTS"):
            if winget_install(package):
                blender = find_blender()
                if blender:
                    break
    if not blender:
        raise RuntimeError(
            "Blender could not be installed/found; photoreal capability is not reported as ready"
        )
    license_file = find_blender_license(blender.parent)
    if not license_file:
        raise RuntimeError("Blender executable exists but its license file could not be located")
    run(
        [
            python,
            ROOT / "tools/vendor_blender.py",
            "--source",
            blender.parent,
            "--license-file",
            license_file,
            "--force",
        ]
    )
    if not blender_vendor_ready(python):
        raise RuntimeError("Blender vendor failed the post-copy integrity gate")
    LOG.write("[OK] Blender copied to vendor/blender and validated")


def smoke_cycles(python: Path, *, skip: bool) -> None:
    if skip:
        return
    step("Real Cycles production-worker smoke")
    output = STATE / "cycles-smoke"
    run(
        [
            python,
            ROOT / "tools/smoke_blender_cycles.py",
            "--keep-output",
            output,
        ]
    )
    beauty = output / "output/render/cycles-smoke.png"
    manifest = output / "output/manifest.json"
    if not beauty.is_file() or not manifest.is_file():
        raise RuntimeError("Cycles smoke returned success without PNG/manifest")
    LOG.write(f"[OK] real Cycles PNG: {beauty}")


def validation_suite(python: Path, node: Path, cargo: Path) -> None:
    step("Regression gates before opening")
    run([python, "-m", "compileall", "-q", "arcz_server", "tools", "tests_python"])
    run([python, "-m", "pytest", "-q"])
    tests = sorted((ROOT / "tests_js").glob("*.mjs"))
    if tests:
        run([node, "--test", "--experimental-default-type=module", *tests])
    run([cargo, "+1.97.1", "fmt", "--all", "--", "--check"])
    run([cargo, "+1.97.1", "check", "--locked", "--workspace", "--all-targets"])
    run([cargo, "+1.97.1", "test", "--locked", "--workspace"])
    run(
        [
            cargo,
            "+1.97.1",
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]
    )


def stop_previous_server() -> None:
    try:
        pid = int(SERVER_PID.read_text(encoding="ascii").strip())
    except (OSError, ValueError):
        return
    run(["taskkill.exe", "/PID", str(pid), "/T", "/F"], allow_failure=True)
    SERVER_PID.unlink(missing_ok=True)


def wait_health(process: subprocess.Popen[bytes]) -> None:
    health_url = "http://127.0.0.1:8123/api/v2/health"
    for _ in range(120):
        if process.poll() is not None:
            break
        try:
            with urllib.request.urlopen(health_url, timeout=1.0) as response:  # noqa: S310 - loopback
                payload = json.loads(response.read().decode("utf-8"))
            if response.status == 200 and payload.get("ok") is True:
                if payload.get("network_mode") != "offline_strict":
                    raise RuntimeError(f"runtime leaked network mode: {payload.get('network_mode')}")
                return
        except Exception:
            pass
        time.sleep(0.5)
    stderr = LOGS / "server-err.log"
    detail = stderr.read_text(encoding="utf-8", errors="replace")[-8000:] if stderr.is_file() else ""
    raise RuntimeError(f"ARCZ server did not become healthy\n{detail}")


def start_arcz(python: Path, *, no_browser: bool) -> None:
    step("Open ARCZ in offline_strict")
    stop_previous_server()
    env = os.environ.copy()
    env.update(
        {
            "ARCZ_NETWORK_MODE": "offline_strict",
            "ARCZ_BANCO": str(ASSETS),
            "ARCZ_SEM_NAVEGADOR": "1",
        }
    )
    stdout_path = LOGS / "server-out.log"
    stderr_path = LOGS / "server-err.log"
    stdout_handle = stdout_path.open("wb")
    stderr_handle = stderr_path.open("wb")
    creationflags = 0
    if os.name == "nt":
        creationflags = subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.DETACHED_PROCESS
    process = subprocess.Popen(
        [str(python), "arcz_local.py", "8123"],
        cwd=ROOT,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=stdout_handle,
        stderr=stderr_handle,
        creationflags=creationflags,
        close_fds=True,
    )
    stdout_handle.close()
    stderr_handle.close()
    SERVER_PID.write_text(str(process.pid), encoding="ascii")
    wait_health(process)
    LOG.write("[OK] http://127.0.0.1:8123/ is healthy and offline_strict")
    if not no_browser:
        webbrowser.open("http://127.0.0.1:8123/")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--force-setup", action="store_true")
    parser.add_argument("--skip-update", action="store_true")
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--skip-photoreal", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if os.name != "nt":
        raise RuntimeError("ARCZ.bat controller must run on Windows")

    os.chdir(ROOT)
    LOG.write("============================================================")
    LOG.write(" ARCZ : UPDATE -> PREPARE -> TEST -> OPEN")
    LOG.write("============================================================")
    LOG.write(f"repo={ROOT}")

    git = git_executable()
    update_repository(git, skip=args.skip_update)
    head = head_sha(git)
    changed_setup = args.force_setup or read_marker(PREPARED_HEAD) != head
    changed_tests = args.force_setup or read_marker(VERIFIED_HEAD) != head

    python = venv_python(refresh_dependencies=changed_setup or changed_tests)
    node, npm = node_tools()
    bun_tool(npm)
    cargo = cargo_tool()

    os.environ["ARCZ_BANCO"] = str(ASSETS)
    os.environ["ARCZ_NETWORK_MODE"] = "offline_strict"

    if changed_setup or not interactive_preflight(python):
        prepare_interactive(python)
        PREPARED_HEAD.write_text(head + "\n", encoding="ascii")
    else:
        LOG.write("[OK] pinned interactive vendors + IfcOpenShell match the validated commit")

    build_rust(cargo)
    ensure_blender_vendor(python, skip=args.skip_photoreal)
    smoke_cycles(python, skip=args.skip_photoreal)

    if changed_tests:
        validation_suite(python, node, cargo)
        VERIFIED_HEAD.write_text(head + "\n", encoding="ascii")
    else:
        LOG.write("[OK] commit already passed the local regression suite")

    if not interactive_preflight(python):
        LOG.write((STATE / "interactive-preflight.json").read_text(encoding="utf-8"))
        raise RuntimeError("interactive+IFC preflight turned red immediately before launch")

    start_arcz(python, no_browser=args.no_browser)
    LOG.write(f"[DONE] {head} validated and opened without mock/fallback remote")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        LOG.write(f"[REAL FAILURE] {error.__class__.__name__}: {error}")
        raise SystemExit(1)
    finally:
        LOG.close()
