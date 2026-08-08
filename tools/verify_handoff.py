#!/usr/bin/env python3
"""Verificador autoritativo do handoff ARCZ Earth + Aedifex Global V10.

Princípios:
- não transforma ausência de ferramenta em sucesso;
- não executa rede;
- não aceita mocks, TODOs executáveis ou artefatos vazios no núcleo V10;
- gera relatório JSON e Markdown reproduzível para a próxima IA.

Uso:
    python tools/verify_handoff.py
    python tools/verify_handoff.py --allow-missing-rust

Sem ``--allow-missing-rust``, ausência de cargo/rustc resulta em código 2.
Qualquer falha verificada resulta em código 1.
"""
from __future__ import annotations

import argparse
import compileall
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import tomllib
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
REPORT_JSON = ROOT / "validation" / "verification-report.json"
REPORT_MD = ROOT / "docs" / "audit" / "VALIDATION_REPORT.md"

CORE_SCAN_DIRS = [
    ROOT / "arcz_server",
    ROOT / "app" / "core",
    ROOT / "app" / "region",
    ROOT / "app" / "plugins",
    ROOT / "app" / "procedural",
    ROOT / "app" / "cine",
    ROOT / "app" / "walk",
    ROOT / "app" / "render",
    ROOT / "app" / "sheets",
    ROOT / "app" / "floorplanner",
    ROOT / "app" / "chat",
    ROOT / "app" / "media",
    ROOT / "app" / "prompts",
    ROOT / "app" / "earth",
    ROOT / "app" / "governance",
    ROOT / "app" / "shell",
    ROOT / "integrations" / "aedifex" / "overlay",
    ROOT / "crates" / "arcz-determinism",
    ROOT / "crates" / "arcz-budget",
    ROOT / "crates" / "arcz-validation",
    ROOT / "crates" / "arcz-region",
    ROOT / "crates" / "arcz-tiles",
    ROOT / "crates" / "arcz-roof",
    ROOT / "crates" / "arcz-facade",
    ROOT / "crates" / "arcz-vegetation",
    ROOT / "crates" / "arcz-procedural",
    ROOT / "crates" / "arcz-generation-cli",
    ROOT / "crates" / "arcz-cad",
    ROOT / "crates" / "arcz-bim",
    ROOT / "crates" / "arcz-aedifex",
]

TEXT_EXTENSIONS = {".py", ".js", ".mjs", ".ts", ".tsx", ".rs", ".json", ".toml", ".md"}


@dataclass
class Check:
    name: str
    status: str
    duration_ms: int
    details: dict[str, Any] = field(default_factory=dict)


class Runner:
    def __init__(self) -> None:
        self.checks: list[Check] = []

    def add(self, name: str, status: str, started: float, **details: Any) -> None:
        self.checks.append(Check(name, status, round((time.monotonic() - started) * 1000), details))

    def command(self, name: str, command: list[str], *, cwd: Path = ROOT,
                blocked_when_missing: str | None = None, timeout: int = 300) -> Check:
        started = time.monotonic()
        executable = shutil.which(command[0])
        if not executable:
            status = "BLOCKED" if blocked_when_missing else "FAILED"
            self.add(name, status, started, reason=blocked_when_missing or f"executável ausente: {command[0]}", command=command)
            return self.checks[-1]
        completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=timeout, check=False)
        status = "PASSED" if completed.returncode == 0 else "FAILED"
        self.add(name, status, started, command=command, returncode=completed.returncode,
                 stdout=completed.stdout[-20000:], stderr=completed.stderr[-20000:])
        return self.checks[-1]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def iter_files(roots: list[Path], suffixes: set[str] | None = None):
    for root in roots:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*")):
            if path.is_file() and (suffixes is None or path.suffix.lower() in suffixes):
                yield path


def check_python_compile(runner: Runner) -> None:
    started = time.monotonic()
    ok = compileall.compile_dir(str(ROOT), quiet=1, force=True,
                                rx=re.compile(r"/(target|cache|cache_[^/]+|data/packages|\.pytest_cache)/"))
    runner.add("python_compileall", "PASSED" if ok else "FAILED", started)


def check_json(runner: Runner) -> None:
    started = time.monotonic()
    failures: list[dict[str, str]] = []
    count = 0
    roots = [
        ROOT / "schemas", ROOT / "resources", ROOT / "examples",
        ROOT / "integrations", ROOT / "validation", ROOT / "opensources",
    ]
    candidates = list(iter_files(roots, {".json"}))
    candidates.extend(path for path in (ROOT / "IMPLEMENTATION_STATUS.json", ROOT / "TASKS.json") if path.is_file())
    for path in sorted(set(candidates)):
        count += 1
        try:
            json.loads(path.read_text(encoding="utf-8"))
        except Exception as error:  # noqa: BLE001 - relatório deve capturar tudo
            failures.append({"path": str(path.relative_to(ROOT)), "error": str(error)})
    runner.add("json_parse", "PASSED" if not failures else "FAILED", started, files=count, failures=failures)


def check_json_schemas(runner: Runner) -> None:
    started = time.monotonic()
    try:
        from jsonschema import Draft202012Validator
    except ImportError as error:
        runner.add("json_schema_self_check", "BLOCKED", started, reason=str(error))
        return
    failures: list[dict[str, str]] = []
    count = 0
    for path in sorted((ROOT / "schemas").glob("*.json")):
        count += 1
        try:
            schema = json.loads(path.read_text(encoding="utf-8"))
            Draft202012Validator.check_schema(schema)
        except Exception as error:  # noqa: BLE001
            failures.append({"path": str(path.relative_to(ROOT)), "error": str(error)})
    runner.add("json_schema_self_check", "PASSED" if not failures else "FAILED", started,
               schemas=count, failures=failures)


def validate_known_resources(runner: Runner) -> None:
    started = time.monotonic()
    try:
        from jsonschema import Draft202012Validator
    except ImportError as error:
        runner.add("resource_schema_validation", "BLOCKED", started, reason=str(error))
        return
    mapping = [
        (ROOT / "resources" / "plugins", "*.json", ROOT / "schemas" / "plugin-manifest-v2.schema.json"),
        (ROOT / "resources" / "profiles", "*.json", ROOT / "schemas" / "regional-profile.schema.json"),
        (ROOT / "resources" / "models", "*.json", ROOT / "schemas" / "ai-model-manifest.schema.json"),
        (ROOT / "resources" / "panoramas", "*.json", ROOT / "schemas" / "panorama-sequence.schema.json"),
    ]
    failures: list[dict[str, str]] = []
    validated = 0
    for directory, pattern, schema_path in mapping:
        if not schema_path.is_file():
            failures.append({"path": str(schema_path.relative_to(ROOT)), "error": "schema ausente"})
            continue
        validator = Draft202012Validator(json.loads(schema_path.read_text(encoding="utf-8")))
        for path in sorted(directory.glob(pattern)) if directory.exists() else []:
            # README manifests documentam formato; não são instâncias instaladas.
            if path.name.startswith("README"):
                continue
            try:
                value = json.loads(path.read_text(encoding="utf-8"))
                errors = sorted(validator.iter_errors(value), key=lambda item: list(item.path))
                if errors:
                    failures.append({"path": str(path.relative_to(ROOT)), "error": "; ".join(e.message for e in errors[:10])})
                else:
                    validated += 1
            except Exception as error:  # noqa: BLE001
                failures.append({"path": str(path.relative_to(ROOT)), "error": str(error)})
    runner.add("resource_schema_validation", "PASSED" if not failures else "FAILED", started,
               validated=validated, failures=failures)


def check_cargo_workspace(runner: Runner) -> None:
    started = time.monotonic()
    failures: list[str] = []
    try:
        root = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        members = root["workspace"]["members"]
        if len(members) != len(set(members)):
            failures.append("workspace contém membros duplicados")
        for member in members:
            directory = (ROOT / member).resolve()
            try:
                directory.relative_to(ROOT)
            except ValueError:
                failures.append(f"membro escapa da raiz: {member}")
                continue
            cargo = directory / "Cargo.toml"
            if not cargo.is_file():
                failures.append(f"Cargo.toml ausente: {member}")
                continue
            tomllib.loads(cargo.read_text(encoding="utf-8"))
    except Exception as error:  # noqa: BLE001
        failures.append(str(error))
        members = []
    runner.add("cargo_workspace_structure", "PASSED" if not failures else "FAILED", started,
               members=len(members), failures=failures)


def strip_rust_non_code(text: str) -> str:
    """Remove comentários/strings preservando quebras para checagem de delimitadores.

    Não substitui o parser do Rust. Serve apenas para detectar arquivo truncado antes
    que a próxima IA tente compilar. O relatório jamais chama isto de compilação.
    """
    out: list[str] = []
    i = 0
    state = "code"
    block_depth = 0
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if state == "code":
            if c == "/" and n == "/":
                state = "line"; out.extend("  "); i += 2; continue
            if c == "/" and n == "*":
                state = "block"; block_depth = 1; out.extend("  "); i += 2; continue
            if c == '"':
                state = "string"; out.append(" "); i += 1; continue
            if c == "'":
                # Lifetimes como 'a não são char literals. Só trate como char quando
                # houver fechamento curto e uma forma plausível.
                close = text.find("'", i + 1, min(len(text), i + 8))
                if close != -1 and "\n" not in text[i:close]:
                    state = "char"; out.append(" "); i += 1; continue
            out.append(c); i += 1; continue
        if state == "line":
            if c == "\n": state = "code"; out.append("\n")
            else: out.append(" ")
            i += 1; continue
        if state == "block":
            if c == "/" and n == "*": block_depth += 1; out.extend("  "); i += 2; continue
            if c == "*" and n == "/":
                block_depth -= 1; out.extend("  "); i += 2
                if block_depth == 0: state = "code"
                continue
            out.append("\n" if c == "\n" else " "); i += 1; continue
        if state in {"string", "char"}:
            delimiter = '"' if state == "string" else "'"
            if c == "\\": out.extend("  "); i += 2; continue
            if c == delimiter: state = "code"; out.append(" "); i += 1; continue
            out.append("\n" if c == "\n" else " "); i += 1
    return "".join(out)


def check_rust_delimiters(runner: Runner) -> None:
    started = time.monotonic()
    failures: list[dict[str, Any]] = []
    count = 0
    pairs = {"(": ")", "[": "]", "{": "}"}
    closing = {value: key for key, value in pairs.items()}
    rust_roots = [path for path in CORE_SCAN_DIRS if path.name.startswith("arcz-") and path.parent.name == "crates"]
    for path in iter_files(rust_roots, {".rs"}):
        count += 1
        stack: list[tuple[str, int]] = []
        code = strip_rust_non_code(path.read_text(encoding="utf-8"))
        for index, char in enumerate(code):
            if char in pairs:
                stack.append((char, index))
            elif char in closing:
                if not stack or stack[-1][0] != closing[char]:
                    failures.append({"path": str(path.relative_to(ROOT)), "error": f"fechamento inesperado {char}", "offset": index})
                    break
                stack.pop()
        else:
            if stack:
                failures.append({"path": str(path.relative_to(ROOT)), "error": f"delimitador sem fechamento {stack[-1][0]}", "offset": stack[-1][1]})
    runner.add("rust_delimiter_sanity_not_compile", "PASSED" if not failures else "FAILED", started,
               files=count, failures=failures, warning="Não substitui cargo check")


def check_no_mock_policy(runner: Runner) -> None:
    started = time.monotonic()
    failures: list[dict[str, Any]] = []
    patterns = {
        "todo_macro": re.compile(r"\b(?:todo|unimplemented)!\s*\("),
        "mock_symbol": re.compile(r"\b(?:Mock|Fake|Stub)[A-Z_a-z0-9]*\b"),
        # HTML/DOM `placeholder` is legitimate input guidance. This gate only
        # flags runtime data/results explicitly marked as placeholder.
        "placeholder_runtime": re.compile(
            r"(?:\bPLACEHOLDER\b|placeholder_(?:result|response|data|implementation)|"
            r"(?i:(?:return|resolve)\s+[^;]*\bplaceholder\b))"
        ),
        "simulation_runtime": re.compile(r"\bsimula(?:ção|cao|te|tion)\b", re.IGNORECASE),
    }
    allow = {
        "arcz_server/generation_workers.py": {"Não existe fallback fictício"},
    }
    for path in iter_files(CORE_SCAN_DIRS, {".py", ".js", ".mjs", ".rs"}):
        relative = str(path.relative_to(ROOT)).replace("\\", "/")
        text = path.read_text(encoding="utf-8")
        for line_no, line in enumerate(text.splitlines(), 1):
            if any(fragment in line for fragment in allow.get(relative, set())):
                continue
            for code, pattern in patterns.items():
                if pattern.search(line):
                    failures.append({"path": relative, "line": line_no, "code": code, "text": line.strip()[:300]})
    runner.add("no_mock_no_stub_policy", "PASSED" if not failures else "FAILED", started,
               failures=failures)


def check_no_remote_urls(runner: Runner) -> None:
    started = time.monotonic()
    findings: list[dict[str, Any]] = []
    url = re.compile(r"https?://", re.IGNORECASE)
    for path in iter_files(CORE_SCAN_DIRS, {".py", ".js", ".mjs", ".rs"}):
        relative = str(path.relative_to(ROOT)).replace("\\", "/")
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if url.search(line):
                # XML namespace e loopback são identificadores/localidade, não dependência remota.
                if "http://www.w3.org/2000/svg" in line or "http://127.0.0.1" in line or "http://localhost" in line:
                    continue
                if relative == "arcz_server/aedifex_runtime.py" and "return f\"http://{host}" in line:
                    continue
                findings.append({"path": relative, "line": line_no, "text": line.strip()[:300]})
    runner.add("core_contains_no_hardcoded_remote_url", "PASSED" if not findings else "FAILED", started,
               findings=findings)



def check_browser_local_first_boot(runner: Runner) -> None:
    """Prova estática das invariantes de boot sem rede/dado inventado.

    Não substitui o smoke test no navegador real. Evita, porém, que uma próxima
    edição reintroduza CDN, satélite/DEM remoto por padrão ou altura zero
    silenciosa sem o relatório perceber.
    """
    started = time.monotonic()
    failures: list[str] = []
    index = (ROOT / "index.html").read_text(encoding="utf-8")
    state = (ROOT / "app" / "estado.js").read_text(encoding="utf-8")
    main = (ROOT / "app" / "main.js").read_text(encoding="utf-8")
    environment = (ROOT / "app" / "ambiente.js").read_text(encoding="utf-8")
    terrain = (ROOT / "app" / "relevo.js").read_text(encoding="utf-8")
    library = (ROOT / "app" / "lib.js").read_text(encoding="utf-8")
    server = (ROOT / "servidor.py").read_text(encoding="utf-8")

    if re.search(r"https?://", index, re.IGNORECASE):
        failures.append("index.html contém recurso remoto")
    required_state = [
        'network_mode: "offline_strict"',
        'relevo: "ellipsoid"',
        'imagery: "naturalearth_local"',
    ]
    for token in required_state:
        if token not in state:
            failures.append(f"estado local-first ausente: {token}")
    if 'token_mapbox: ""' in state:
        failures.append("estado ainda persiste token_mapbox")
    load_pos = main.find("await estadoApp.carregarDoServidor()")
    dem_pos = main.find('if (st.ambiente?.relevo === "dem")')
    if load_pos < 0 or dem_pos < load_pos:
        failures.append("DEM não está condicionado ao estado carregado")
    for token in ("FONTES_REMOTAS", "fontePermitida", "NETWORK_MODES.IMPORT_ASSISTED", "FONTE_LOCAL_PADRAO"):
        if token not in environment:
            failures.append(f"guard de imagery remota ausente: {token}")
    if "DEM_LOCAL_MISSING" not in terrain or "fallback silencioso" in terrain:
        failures.append("relevo não propaga ausência local explicitamente")
    if "https://cdn.polyhaven" in library:
        failures.append("biblioteca ainda abre CDN no navegador")
    dem_function = server[server.find("def obter_tile_dem"):server.find("def caminho_seguro")]
    if "urlopen" in dem_function or "https://" in dem_function:
        failures.append("rota DEM core ainda baixa provider")
    runner.add("browser_local_first_boot_contract", "PASSED" if not failures else "FAILED", started,
               failures=failures,
               warning="Prova estática; executar smoke browser/firewall no hardware alvo")




def check_aedifex_v10_contracts(runner: Runner) -> None:
    """Static integration gate for V10 source contracts.

    This does not claim browser/build parity. It prevents regressions in the
    single-authority, split-workspace and single-chat contracts while the real
    pinned upstream is unavailable in this environment.
    """
    started = time.monotonic()
    failures: list[str] = []
    required = [
        ROOT / "docs/integration/AEDIFEX_CAPABILITY_LEDGER_V10.md",
        ROOT / "docs/integration/MASS_CONVERSION_EXECUTION_PLAN.md",
        ROOT / "integrations/aedifex/FEATURE_MATRIX_CONVERSION.json",
        ROOT / "integrations/aedifex/CONVERSION_COVERAGE.json",
        ROOT / "integrations/aedifex/CONVERSION_MATRIX.json",
        ROOT / "schemas/aedifex-conversion-matrix.schema.json",
        ROOT / "tools/build_aedifex_conversion_matrix.py",
        ROOT / "docs/integration/CONVERSION_MATRIX_GUIDE.md",
        ROOT / "docs/integration/USER_REQUIREMENT_TRACEABILITY_V10.md",
        ROOT / "integrations/aedifex/AUTHOR_REPOSITORY_AUDIT.json",
        ROOT / "app/floorplanner/site-authoring-layout.js",
        ROOT / "app/floorplanner/floorplanner-host.js",
        ROOT / "app/chat/global-chat-panel.js",
        ROOT / "arcz_server/chat_workspace.py",
        ROOT / "arcz_server/aedifex_inventory.py",
        ROOT / "app/earth/cinematic-globe.js",
    ]
    for path in required:
        if not path.is_file() or path.stat().st_size < 64:
            failures.append(f"arquivo obrigatório ausente/vazio: {path.relative_to(ROOT)}")

    try:
        lock = json.loads((ROOT / "integrations/aedifex/UPSTREAM_LOCK.json").read_text(encoding="utf-8"))
        matrix = json.loads((ROOT / "integrations/aedifex/FEATURE_MATRIX_CONVERSION.json").read_text(encoding="utf-8"))
        conversion = json.loads((ROOT / "integrations/aedifex/CONVERSION_MATRIX.json").read_text(encoding="utf-8"))
        if matrix.get("node_kind_count") != len(lock.get("required_node_kinds", [])):
            failures.append("FEATURE_MATRIX node_kind_count diverge do lock")
        if set(matrix.get("node_kinds", [])) != set(lock.get("required_node_kinds", [])):
            failures.append("FEATURE_MATRIX não cobre exatamente os node kinds do lock")
        if matrix.get("authority", {}).get("single_chat") is None:
            failures.append("autoridade de chat único ausente")
        if [item.get("id") for item in conversion.get("node_kinds", [])] != lock.get("required_node_kinds", []):
            failures.append("CONVERSION_MATRIX não cobre os node kinds em ordem do lock")
        if [item.get("id") for item in conversion.get("tool_families", [])] != lock.get("required_tool_families", []):
            failures.append("CONVERSION_MATRIX não cobre as famílias MCP em ordem do lock")
        if {item.get("id") for item in conversion.get("packages", [])} != set(lock.get("packages", {})):
            failures.append("CONVERSION_MATRIX não cobre exatamente os pacotes pinados")
        body = dict(conversion); expected_hash = body.pop("matrix_hash", "")
        actual_hash = hashlib.sha256(json.dumps(body, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()
        if expected_hash != actual_hash:
            failures.append("CONVERSION_MATRIX hash divergente")
        author_audit = json.loads((ROOT / "integrations/aedifex/AUTHOR_REPOSITORY_AUDIT.json").read_text(encoding="utf-8"))
        if author_audit.get("repository_count") != 15:
            failures.append("auditoria dos repositórios TangSY não cobre os 15 repositórios públicos encontrados")
        if author_audit.get("result", {}).get("selected") != ["TangSY/aedifex"]:
            failures.append("auditoria do autor não preserva Aedifex como única seleção de kernel")
    except Exception as error:  # noqa: BLE001
        failures.append(f"matriz/lock inválidos: {error}")

    def text(rel: str) -> str:
        path = ROOT / rel
        return path.read_text(encoding="utf-8") if path.is_file() else ""

    host = text("app/floorplanner/floorplanner-host.js")
    page = text("integrations/aedifex/overlay/apps/arcz-floorplanner/app/page.tsx")
    chat = text("arcz_server/chat_workspace.py")
    cinematic = text("app/earth/cinematic-globe.js")
    status = text("IMPLEMENTATION_STATUS.json")
    expectations = {
        "Floorplanner mantém viewer real": "viewer" in host and "show_globe" in host and "auto_publish" in host,
        "um único Editor no host overlay": page.count("<Editor") == 1,
        "nenhum segundo AIChatPanel montado": "<AIChatPanel" not in page,
        "chat tem tool run e aprovação": "AWAITING_APPROVAL" in chat and "approve_tool_run" in chat and "reject_tool_run" in chat,
        "flyTo aguarda callbacks reais": "complete:" in cinematic and "cancel:" in cinematic and "flyToCamera" in cinematic,
        "status não conserva chat.dual": '"chat.dual"' not in status and '"chat.global"' in status,
    }
    failures.extend(name for name, ok in expectations.items() if not ok)
    runner.add(
        "aedifex_v10_source_contracts",
        "PASSED" if not failures else "FAILED",
        started,
        failures=failures,
        warning="Gate estático; não substitui build Aedifex, Cesium real ou E2E browser",
    )


def check_documentation_v10(runner: Runner) -> None:
    started = time.monotonic()
    failures: list[str] = []
    required_tokens = {
        "README.md": ["Global V10", "Aedifex Building Authoring Kernel"],
        "LEIA-PRIMEIRO.md": ["Global V10", "Não declarado como concluído"],
        "AGENTS.md": ["Aedifex V10", "chat global"],
        "docs/integration/AEDIFEX_CAPABILITY_LEDGER_V10.md": ["46 node kinds", "ghost preview"],
        "docs/integration/MASS_CONVERSION_EXECUTION_PLAN.md": ["Onda 0", "Onda 9", "CONVERSION_MATRIX.json"],
        "docs/integration/CONVERSION_MATRIX_GUIDE.md": ["Regra fail-closed", "Hash"],
        "docs/integration/USER_REQUIREMENT_TRACEABILITY_V10.md": ["Modelar diretamente", "Sem mocks/simulações"],
    }
    for rel, tokens in required_tokens.items():
        path = ROOT / rel
        if not path.is_file():
            failures.append(f"ausente: {rel}")
            continue
        value = path.read_text(encoding="utf-8")
        for token in tokens:
            if token not in value:
                failures.append(f"{rel} sem token obrigatório: {token}")
    runner.add("documentation_v10_contract", "PASSED" if not failures else "FAILED", started, failures=failures)

def check_release_entrypoints(runner: Runner) -> None:
    """Validate portable launch/install contracts without claiming clean-machine E2E."""
    started = time.monotonic()
    failures: list[str] = []
    required = [
        "QUICKSTART.md", "LICENSE", "THIRD_PARTY_NOTICES.md", ".env.example",
        "run.bat", "stop.bat", "install.ps1", "uninstall.ps1",
        "install.sh", "run.sh", "stop.sh", "uninstall.sh",
        "scripts/windows/common.ps1", "scripts/windows/install.ps1",
        "scripts/windows/run.ps1", "scripts/windows/stop.ps1",
        "scripts/windows/uninstall.ps1", "scripts/linux/common.sh",
        "scripts/linux/install.sh", "scripts/linux/run.sh",
        "scripts/linux/stop.sh", "scripts/linux/uninstall.sh",
        "Dockerfile", "docker-compose.yml", "tools/runtime_preflight.py",
    ]
    for rel in required:
        path = ROOT / rel
        if not path.is_file() or path.stat().st_size == 0:
            failures.append(f"ausente/vazio: {rel}")
    quickstart = (ROOT / "QUICKSTART.md").read_text(encoding="utf-8") if (ROOT / "QUICKSTART.md").is_file() else ""
    env_example = (ROOT / ".env.example").read_text(encoding="utf-8") if (ROOT / ".env.example").is_file() else ""
    dockerfile = (ROOT / "Dockerfile").read_text(encoding="utf-8") if (ROOT / "Dockerfile").is_file() else ""
    for token in ("Windows", "Linux", "runtime_preflight.py", "BLOCKED"):
        if token not in quickstart:
            failures.append(f"QUICKSTART sem token: {token}")
    active_env = [line.strip() for line in env_example.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    secret_assignments = []
    for line in active_env:
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if any(token in key.upper() for token in ("API_KEY", "TOKEN", "SECRET", "PASSWORD")) and value.strip():
            secret_assignments.append(key.strip())
    if "offline_strict" not in env_example or secret_assignments:
        failures.append(f".env.example precisa ser local-first e sem segredos preenchidos: {secret_assignments}")
    if "runtime_preflight.py --profile interactive" not in dockerfile:
        failures.append("Dockerfile não executa preflight interativo antes do servidor")
    runner.add("release_entrypoint_contract", "PASSED" if not failures else "FAILED", started,
               files=len(required), failures=failures,
               warning="Gate estático; instalação limpa permanece um gate de runtime")

    linux_scripts = [
        "scripts/linux/common.sh", "scripts/linux/install.sh", "scripts/linux/run.sh",
        "scripts/linux/stop.sh", "scripts/linux/uninstall.sh",
        "install.sh", "run.sh", "stop.sh", "uninstall.sh",
    ]
    runner.command("linux_launcher_syntax", ["bash", "-n", *linux_scripts], timeout=60,
                   blocked_when_missing="bash ausente; scripts Linux não puderam ser analisados")

    powershell = shutil.which("pwsh") or shutil.which("powershell")
    started = time.monotonic()
    if not powershell:
        runner.add("windows_launcher_runtime", "BLOCKED", started,
                   reason="PowerShell não está disponível neste ambiente; scripts Windows não foram executados",
                   command="powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 ...")
    else:
        scripts = [
            ROOT / "install.ps1", ROOT / "uninstall.ps1",
            *sorted((ROOT / "scripts/windows").glob("*.ps1")),
        ]
        quoted = ",".join("'" + str(path).replace("'", "''") + "'" for path in scripts)
        expression = (
            "$failed=$false; foreach($p in @(" + quoted + ")) { "
            "$tokens=$null; $errors=$null; "
            "[System.Management.Automation.Language.Parser]::ParseFile($p,[ref]$tokens,[ref]$errors)|Out-Null; "
            "if($errors.Count -gt 0){$errors|ForEach-Object{Write-Error $_};$failed=$true} }; "
            "if($failed){exit 1}"
        )
        completed = subprocess.run([powershell, "-NoProfile", "-Command", expression], cwd=ROOT,
                                   text=True, capture_output=True, timeout=120, check=False)
        runner.add("windows_launcher_runtime", "PASSED" if completed.returncode == 0 else "FAILED", started,
                   returncode=completed.returncode, stdout=completed.stdout[-12000:], stderr=completed.stderr[-12000:])

    docker = shutil.which("docker")
    started = time.monotonic()
    if not docker:
        runner.add("docker_compose_runtime", "BLOCKED", started,
                   reason="Docker/Compose ausente; configuração universal não foi validada neste ambiente",
                   command="docker compose config && docker compose up --build")
    else:
        completed = subprocess.run([docker, "compose", "config"], cwd=ROOT, text=True,
                                   capture_output=True, timeout=120, check=False)
        runner.add("docker_compose_runtime", "PASSED" if completed.returncode == 0 else "FAILED", started,
                   returncode=completed.returncode, stdout=completed.stdout[-12000:], stderr=completed.stderr[-12000:])


def check_aedifex_vendor(runner: Runner) -> None:
    started = time.monotonic()
    try:
        from arcz_server.aedifex_registry import AedifexRegistry
        status = AedifexRegistry(ROOT).status()
    except Exception as error:  # noqa: BLE001
        runner.add("aedifex_vendor_and_build", "FAILED", started, error=str(error))
        return
    if status.get("ready"):
        runner.add("aedifex_vendor_and_build", "PASSED", started, status=status)
    else:
        runner.add(
            "aedifex_vendor_and_build",
            "BLOCKED",
            started,
            reason="upstream/fork/build Aedifex pinados ainda não foram materializados e compilados",
            blockers=status.get("blockers", []),
            command="python tools/vendor_aedifex.py --source <checkout-local> && python tools/build_aedifex_sidecar.py",
        )



def check_photoreal_dependencies(runner: Runner) -> None:
    """Separate implemented worker contracts from unavailable local binaries/models."""
    started = time.monotonic()
    manifest = ROOT / "resources" / "workers" / "render.photoreal.worker.json"
    launcher = ROOT / "workers" / "blender" / "launch_blender.py"
    renderer = ROOT / "workers" / "blender" / "render_floor_scene.py"
    missing_contract = [str(p.relative_to(ROOT)) for p in (manifest, launcher, renderer) if not p.is_file()]
    if missing_contract:
        runner.add("photoreal_worker_contract", "FAILED", started, missing=missing_contract)
    else:
        runner.add("photoreal_worker_contract", "PASSED", started, files=[str(p.relative_to(ROOT)) for p in (manifest, launcher, renderer)])

    started = time.monotonic()
    blender = os.environ.get("ARCZ_BLENDER") or shutil.which("blender")
    if blender and Path(blender).is_file():
        runner.add("blender_photoreal_runtime", "PASSED", started, executable=str(Path(blender).resolve()))
    else:
        runner.add("blender_photoreal_runtime", "BLOCKED", started,
                   reason="Blender/Cycles local não instalado; o worker não pode produzir imagens reais",
                   command="instale Blender local e defina ARCZ_BLENDER=<caminho>")

    started = time.monotonic()
    required_tasks = {"chat.global", "prompt.enhance", "prompt.translate", "render-diffusion", "upscale"}
    try:
        from arcz_server.ai_broker import ModelRegistry
        from arcz_server.schema_validation import SchemaRegistry
        registry = ModelRegistry(
            [ROOT / "resources" / "models", ROOT / "data" / "models"],
            SchemaRegistry(ROOT / "schemas"),
        )
        manifests = registry.list(verify=True)
        installed_tasks = {
            str(item.get("task"))
            for item in manifests
            if isinstance(item.get("status"), dict) and item["status"].get("installed") is True
        }
        invalid = [
            {"manifest_path": item.get("manifest_path"), "errors": item.get("status", {}).get("errors", [])}
            for item in manifests
            if not item.get("status", {}).get("installed")
        ]
    except Exception as error:  # noqa: BLE001
        runner.add("local_ai_render_models", "FAILED", started, error=str(error))
        return
    missing = sorted(required_tasks - installed_tasks)
    if invalid:
        runner.add("local_ai_render_models", "FAILED", started, invalid=invalid, missing_tasks=missing)
    elif missing:
        runner.add("local_ai_render_models", "BLOCKED", started,
                   reason="modelos locais necessários não foram materializados/verificados",
                   missing_tasks=missing,
                   manifests=[str(item.get("manifest_path")) for item in manifests])
    else:
        runner.add("local_ai_render_models", "PASSED", started, tasks=sorted(installed_tasks))

def check_cesium_vendor(runner: Runner) -> None:
    started = time.monotonic()
    root = ROOT / "vendor" / "cesium"
    cesium = root / "Cesium"
    required = [
        cesium / "Cesium.js",
        cesium / "Widgets" / "widgets.css",
        cesium / "Assets" / "Textures" / "NaturalEarthII" / "tilemapresource.xml",
        root / "LICENSE.md",
        root / "manifest.json",
    ]
    missing = [str(path.relative_to(ROOT)) for path in required if not path.is_file()]
    if missing:
        runner.add(
            "cesium_local_vendor",
            "BLOCKED",
            started,
            reason="vendor CesiumJS foi excluído do arquivo de origem e precisa ser instalado localmente",
            missing=missing,
            command="python tools/vendor_cesium.py --source <local> --license-file <local> --version 1.143.0",
        )
        return
    failures: list[str] = []
    try:
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
        files = {item["path"]: item for item in manifest.get("files", [])}
        for path in required[:3]:
            rel = path.relative_to(cesium).as_posix()
            item = files.get(rel)
            if not item:
                failures.append(f"manifest sem {rel}")
            elif sha256_file(path) != item.get("sha256"):
                failures.append(f"checksum divergente: {rel}")
        if manifest.get("dependency") != "CesiumJS":
            failures.append("manifest dependency != CesiumJS")
    except Exception as error:  # noqa: BLE001
        failures.append(str(error))
    runner.add("cesium_local_vendor", "PASSED" if not failures else "FAILED", started,
               files=len(files) if 'files' in locals() else 0, failures=failures)




def remove_runtime_artifacts_for_release_check() -> list[str]:
    removed: list[str] = []
    patterns = [
        "**/__pycache__", ".pytest_cache", "**/*.pyc", "**/*.pyo",
        "jobs/*.sqlite3*", "data/**/*.sqlite3*", "data/registry.sqlite3*",
        "scene/staging/*", "data/floorplanner/exports/*", "data/media/content/*",
        "logs/*",
    ]
    for pattern in patterns:
        for path in ROOT.glob(pattern):
            if not path.exists():
                continue
            removed.append(str(path.relative_to(ROOT)))
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink(missing_ok=True)
    for directory in (
        ROOT / "jobs", ROOT / "data" / "indexes", ROOT / "scene" / "staging",
        ROOT / "data" / "floorplanner" / "exports", ROOT / "data" / "media" / "content",
        ROOT / "logs",
    ):
        directory.mkdir(parents=True, exist_ok=True)
        (directory / ".gitkeep").touch()
    return removed

def check_runtime_artifacts(runner: Runner) -> None:
    started = time.monotonic()
    forbidden: list[str] = []
    for pattern in (
        "**/__pycache__", ".pytest_cache", "**/*.pyc", "jobs/*.sqlite3*",
        "data/**/*.sqlite3*", "data/registry.sqlite3*", "scene/staging/*",
        "data/floorplanner/exports/*", "data/media/content/*", "logs/*",
    ):
        for path in ROOT.glob(pattern):
            if path.name == ".gitkeep":
                continue
            forbidden.append(str(path.relative_to(ROOT)))
    runner.add("release_tree_has_no_runtime_artifacts", "PASSED" if not forbidden else "FAILED", started,
               forbidden=sorted(set(forbidden)))


def generate_reports(runner: Runner, allow_missing_rust: bool) -> tuple[dict[str, Any], int]:
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    failed = [c for c in runner.checks if c.status == "FAILED"]
    blocked = [c for c in runner.checks if c.status == "BLOCKED"]
    overall = "FAILED" if failed else ("BLOCKED" if blocked else "PASSED")
    report = {
        "schema_version": 1,
        "generated_at": now,
        "root": str(ROOT),
        "overall": overall,
        "allow_missing_rust": allow_missing_rust,
        "checks": [asdict(item) for item in runner.checks],
        "summary": {
            "passed": sum(c.status == "PASSED" for c in runner.checks),
            "failed": len(failed),
            "blocked": len(blocked),
        },
        "release_rule": "FAILED impede entrega; BLOCKED impede declarar validação completa.",
    }
    REPORT_JSON.parent.mkdir(parents=True, exist_ok=True)
    REPORT_JSON.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    lines = [
        "# ARCZ Earth + Aedifex Global V10 — relatório de validação", "", f"Gerado em: `{now}`", "",
        f"**Resultado geral:** `{overall}`", "",
        "> `BLOCKED` não significa aprovado. Significa que a verificação não pôde ser executada neste ambiente.", "",
        "| Verificação | Estado | Duração |", "|---|---:|---:|",
    ]
    for item in runner.checks:
        lines.append(f"| `{item.name}` | **{item.status}** | {item.duration_ms} ms |")
    lines.extend(["", "## Detalhes", ""])
    for item in runner.checks:
        lines.extend([f"### {item.name} — {item.status}", "", "```json",
                      json.dumps(item.details, ensure_ascii=False, indent=2)[:30000], "```", ""])
    REPORT_MD.parent.mkdir(parents=True, exist_ok=True)
    REPORT_MD.write_text("\n".join(lines), encoding="utf-8")
    if failed:
        return report, 1
    rust_blockers = {"rustfmt_check", "cargo_check_workspace", "cargo_test_workspace"}
    non_rust_blocked = [item for item in blocked if item.name not in rust_blockers]
    if non_rust_blocked:
        return report, 2
    if blocked and not allow_missing_rust:
        return report, 2
    return report, 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--allow-missing-rust", action="store_true",
                        help="gera relatório com Rust BLOCKED, sem transformar bloqueio em sucesso")
    args = parser.parse_args()
    runner = Runner()
    check_python_compile(runner)
    runner.command("python_pytest", [sys.executable, "-m", "pytest", "-q"], timeout=300)
    runner.command("job_cancel_race_stress", [sys.executable, "tools/job_cancel_stress.py", "--iterations", "100"], timeout=180)
    js_tests = [str(path.relative_to(ROOT)) for path in sorted((ROOT / "tests_js").glob("*.mjs"))]
    runner.command("javascript_tests", ["node", "--test", "--experimental-default-type=module", *js_tests], timeout=120)
    runner.command(
        "typescript_overlay_syntax",
        ["node", "tools/check_typescript_syntax.mjs", "integrations/aedifex/overlay"],
        blocked_when_missing="node/TypeScript compiler API ausente neste ambiente",
        timeout=120,
    )
    started = time.monotonic()
    js_files = list((ROOT / "app").rglob("*.js"))
    failures = []
    node = shutil.which("node")
    if not node:
        runner.add("javascript_syntax", "BLOCKED", started, reason="node ausente")
    else:
        for path in js_files:
            result = subprocess.run([node, "--check", str(path)], cwd=ROOT, text=True, capture_output=True, check=False)
            if result.returncode:
                failures.append({"path": str(path.relative_to(ROOT)), "stderr": result.stderr[-4000:]})
        runner.add("javascript_syntax", "PASSED" if not failures else "FAILED", started, files=len(js_files), failures=failures)
    runner.command(
        "aedifex_conversion_matrix_generation",
        [sys.executable, "tools/build_aedifex_conversion_matrix.py"],
        timeout=60,
    )
    check_json(runner)
    check_json_schemas(runner)
    validate_known_resources(runner)
    check_cargo_workspace(runner)
    check_rust_delimiters(runner)
    runner.command("rustfmt_check", ["cargo", "fmt", "--all", "--", "--check"],
                   blocked_when_missing="cargo/rustfmt ausente neste ambiente", timeout=300)
    runner.command("cargo_check_workspace", ["cargo", "check", "--workspace", "--all-targets"],
                   blocked_when_missing="cargo/rustc ausente neste ambiente", timeout=1200)
    runner.command("cargo_test_workspace", ["cargo", "test", "--workspace", "--all-targets"],
                   blocked_when_missing="cargo/rustc ausente neste ambiente", timeout=1800)
    check_no_mock_policy(runner)
    check_no_remote_urls(runner)
    check_browser_local_first_boot(runner)
    check_aedifex_v10_contracts(runner)
    check_documentation_v10(runner)
    check_release_entrypoints(runner)
    check_cesium_vendor(runner)
    check_aedifex_vendor(runner)
    check_photoreal_dependencies(runner)
    removed = remove_runtime_artifacts_for_release_check()
    started_cleanup = time.monotonic()
    runner.add("runtime_artifact_cleanup", "PASSED", started_cleanup, removed=removed)
    check_runtime_artifacts(runner)
    report, exit_code = generate_reports(runner, args.allow_missing_rust)
    print(json.dumps(report["summary"], ensure_ascii=False))
    print(f"Relatório: {REPORT_MD}")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
