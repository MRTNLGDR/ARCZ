#!/usr/bin/env python3
"""Servidor local unificado do ARCZ.

Serve a RAIZ do projeto ARCZ na porta 8123 (ou primeira disponivel) e
gerencia as APIs de persistencia, LOD de GLB, thumbnails, geocodificacao
e integracao com Poly Haven.
"""

import base64
import http.server
import json
import os
import re
import hashlib
import subprocess
import urllib.parse
import sys
import threading
import urllib.request
import webbrowser
import time
from datetime import datetime
from pathlib import Path

import arcz_export
from arcz_glb import PIL_DISPONIVEL, processar as processar_glb
from arcz_server import V2Router
from arcz_server.atomic_io import atomic_write_json
from arcz_server.errors import ApiError
from arcz_server.hashing import canonical_json_hash
from arcz_server.project_migrations import migrate_project, migrate_project_file

RAIZ = Path(__file__).resolve().parent  # .../ARCZ
FOTOS = RAIZ / "fotos"
CENAS = RAIZ / "cenas"
CACHE_DEM = RAIZ / "cache_dem"
CACHE_GLB = RAIZ / "cache_glb"
CACHE_GEO = RAIZ / "cache_geo"
# Entorno OSM local (prédios/vias/vegetação gerados pelo arcz-osm-cli, ver
# gerar_ou_obter_entorno): CACHE_ENTORNO guarda o .glb final + metadados,
# CACHE_OVERPASS guarda a resposta crua do Overpass (cache interno do próprio
# arcz-osm, aqui só precisa de um diretório persistente de verdade).
CACHE_ENTORNO = RAIZ / "cache_entorno"
CACHE_OVERPASS = RAIZ / "cache_overpass"
POSICAO = RAIZ / "posicao.json"
TAKES = RAIZ / "takes.json"
PROJETO = RAIZ / "projeto.json"
LUGARES = RAIZ / "lugares.json"
LIB_THUMBS = RAIZ / "lib_thumbs"
TEXTURAS = RAIZ / "texturas"
PASTA_TAKES = RAIZ / "takes"
BIBLIOTECA = RAIZ / "biblioteca"
# Banco de modelos externo (32 GB de CC0/CC-BY que nao cabem no repositorio).
# Fica fora da RAIZ de proposito: e lido pelo manifesto, nunca por caminho
# montado no navegador. `ARCZ_BANCO` troca a pasta sem editar codigo.
BANCO = Path(os.environ.get("ARCZ_BANCO", r"D:\AVANGARD-ASSETS"))
EXPORTACOES = RAIZ / "exportacoes"
PORTA_PADRAO = 8123

for d in (FOTOS, CENAS, CACHE_DEM, CACHE_GLB, CACHE_GEO, CACHE_ENTORNO, CACHE_OVERPASS,
          LIB_THUMBS, TEXTURAS, PASTA_TAKES, BIBLIOTECA, EXPORTACOES):
    d.mkdir(parents=True, exist_ok=True)

# API V2 local-first. A instancia instala o guard de egress antes de qualquer
# conector legado; offline_strict e o modo padrao.
V2_API = V2Router(RAIZ)

# Presets fixos de área do entorno OSM (metros de lado). Fixos, não um slider
# livre: a saída é um único .glb sem tiling/LOD, então uma área grande em
# centro denso pesaria de verdade. Validado aqui, não só no dropdown do
# cliente — nunca confiar só na UI.
PRESETS_LADO_ENTORNO = (100, 150, 200, 250)

_LOCK_ENTORNO = threading.Lock()


class EntornoIndisponivel(RuntimeError):
    """Geração do entorno OSM falhou (CLI ausente, Overpass fora, timeout)."""


def localizar_cli_entorno() -> "Path | None":
    """`target/release` primeiro, `target/debug` como fallback de desenvolvimento."""
    for perfil in ("release", "debug"):
        base = RAIZ / "target" / perfil
        for nome in ("arcz-osm-cli.exe", "arcz-osm-cli"):
            exe = base / nome
            if exe.is_file():
                return exe
    return None


def _chave_entorno(lat: float, lon: float, lado: int, adensar: bool) -> str:
    txt = f"{lat:.5f}|{lon:.5f}|{lado}|{1 if adensar else 0}|v1"
    return hashlib.sha256(txt.encode("utf-8")).hexdigest()[:16]


def gerar_ou_obter_entorno(lat: float, lon: float, lado: int, adensar: bool):
    """(caminho_do_glb, metadados). Gera com o arcz-osm-cli na primeira vez
    para esta (lat, lon, lado, adensar); depois só lê do cache em disco.

    Diferente da rota `/entorno.glb` do servidor Rust (que regenera da
    memória a cada chamada e perde tudo ao reiniciar): aqui o resultado fica
    em CACHE_ENTORNO para sempre, mesmo padrão de cache_dem/cache_glb.
    """
    chave = _chave_entorno(lat, lon, lado, adensar)
    caminho_glb = CACHE_ENTORNO / f"{chave}.glb"
    caminho_json = CACHE_ENTORNO / f"{chave}.json"

    if caminho_glb.is_file() and caminho_json.is_file():
        return caminho_glb, json.loads(caminho_json.read_text(encoding="utf-8"))

    # Coarse lock: servidor.py já roda em ThreadingHTTPServer (thread por
    # requisição), então sem isto dois cliques rápidos no mesmo toggle
    # disparariam dois subprocessos redundantes para a mesma área.
    with _LOCK_ENTORNO:
        # Outra thread pode ter terminado enquanto esperávamos o lock.
        if caminho_glb.is_file() and caminho_json.is_file():
            return caminho_glb, json.loads(caminho_json.read_text(encoding="utf-8"))

        cli = localizar_cli_entorno()
        if cli is None:
            raise EntornoIndisponivel(
                "arcz-osm-cli não compilado — rode: cargo build --release -p arcz-osm-cli"
            )

        tmp_glb = caminho_glb.with_suffix(".parcial")
        args = [
            str(cli),
            "--lat", f"{lat:.6f}",
            "--lon", f"{lon:.6f}",
            "--lado", str(lado),
            "--saida", str(tmp_glb),
            "--cache-dem", str(CACHE_DEM),
            "--cache-overpass", str(CACHE_OVERPASS),
        ]
        if adensar:
            args.append("--adensar")

        try:
            # Lista de argumentos (shell=False, o padrão): sem isso nenhum
            # caminho com espaço precisaria de escaping manual no Windows.
            # encoding="utf-8" explícito: o © de "atribuição" quebra na
            # codificação padrão do console do Windows.
            resultado = subprocess.run(
                args, capture_output=True, text=True, encoding="utf-8", timeout=30
            )
        except subprocess.TimeoutExpired:
            raise EntornoIndisponivel("geração do entorno excedeu 30s (Overpass/DEM lentos?)")

        if resultado.returncode != 0:
            ultima = (resultado.stderr or "falha desconhecida").strip().splitlines()
            raise EntornoIndisponivel(f"arcz-osm-cli falhou: {ultima[-1] if ultima else '?'}")

        linhas = (resultado.stdout or "").strip().splitlines()
        if not linhas or not tmp_glb.is_file():
            raise EntornoIndisponivel("arcz-osm-cli não gravou o .glb ou os metadados")
        try:
            metadados = json.loads(linhas[-1])
        except json.JSONDecodeError as e:
            raise EntornoIndisponivel(f"metadados inválidos do arcz-osm-cli: {e}")

        # .glb primeiro, .json depois: o teste de cache-hit acima exige os
        # dois arquivos, então uma queda no meio nunca deixa uma entrada
        # "metadados sem glb" passando por cache válido.
        tmp_glb.replace(caminho_glb)
        tmp_json = caminho_json.with_suffix(".parcial")
        tmp_json.write_text(json.dumps(metadados, ensure_ascii=False), encoding="utf-8")
        tmp_json.replace(caminho_json)

        return caminho_glb, metadados


PASTAS_DE_MODELO = [RAIZ / "modelos"]
EXTENSOES = {".glb", ".gltf"}

MIME = {
    ".wasm": "application/wasm",
    ".glb": "model/gltf-binary",
    ".gltf": "model/gltf+json",
    ".js": "text/javascript",
    ".mjs": "text/javascript",
    ".json": "application/json",
    ".css": "text/css",
    ".html": "text/html; charset=utf-8",
    ".ktx2": "image/ktx2",
    ".svg": "image/svg+xml",
}


def obter_tile_dem(z: int, x: int, y: int) -> bytes | None:
    """Serve somente tile Terrarium já materializado no armazenamento local.

    Esta rota faz parte do runtime core e, portanto, nunca baixa dado de um
    provider. Importadores opcionais devem validar licença/checksum/proveniência
    e gravar o pacote antes do uso. Ausência é retornada como 404 pelo handler;
    o cliente também propaga ``DEM_LOCAL_MISSING`` em vez de inventar cota 0.
    """
    if not (0 <= z <= 15 and 0 <= x < 2**z and 0 <= y < 2**z):
        return None
    destino = CACHE_DEM / str(z) / str(x) / f"{y}.png"
    return destino.read_bytes() if destino.is_file() else None


def caminho_seguro(rel: str) -> Path | None:
    """Resolve `rel` dentro da RAIZ. Devolve None se escapar do projeto."""
    if not rel:
        return None
    alvo = (RAIZ / rel.lstrip("/\\")).resolve()
    try:
        alvo.relative_to(RAIZ)
    except ValueError:
        return None
    return alvo if alvo.is_file() else None


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(RAIZ), **k)

    def guess_type(self, path):
        ext = os.path.splitext(str(path))[1].lower()
        return MIME.get(ext) or super().guess_type(path)

    @staticmethod
    def _origens_floorplanner_permitidas() -> set[str]:
        # O Aedifex roda como sidecar local porque sua UI React/Next não pode
        # ser injetada no front ES Modules puro sem introduzir um segundo build
        # no núcleo do ARCZ. CORS é estrito e limitado a loopback; nunca `*`.
        defaults = {"http://127.0.0.1:8124", "http://localhost:8124",
                    "http://[::1]:8124"}
        configured = {item.strip().rstrip("/") for item in
                      os.environ.get("ARCZ_FLOORPLANNER_ORIGINS", "").split(",") if item.strip()}
        return defaults | configured

    def _cors_origin(self) -> str | None:
        origin = (self.headers.get("Origin") or "").strip().rstrip("/")
        return origin if origin in self._origens_floorplanner_permitidas() else None

    def end_headers(self):
        # App local: HTML/CSS/JS nunca podem vir do cache do navegador, senao a
        # correcao esta no disco e a tela continua com a versao velha.
        if os.path.splitext(self.path.split("?")[0])[1].lower() in (".html", ".css", ".js", ".mjs", ".json"):
            self.send_header("Cache-Control", "no-store, must-revalidate")
        origin = self._cors_origin()
        if origin and (self.path.startswith("/api/v2/") or self.path.startswith("/api/governance/")):
            self.send_header("Access-Control-Allow-Origin", origin)
            self.send_header("Vary", "Origin")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            self.send_header("Access-Control-Allow-Headers",
                             "Content-Type, Cache-Control, Last-Event-ID, X-ARCZ-Filename, "
                             "X-ARCZ-Roles, X-ARCZ-License, X-ARCZ-Provenance, X-ARCZ-Metadata, "
                             "X-ARCZ-Revision, X-ARCZ-Format, X-ARCZ-Scene-Hash, X-ARCZ-Semantic-Manifest")
            self.send_header("Access-Control-Max-Age", "600")
        super().end_headers()

    def do_OPTIONS(self):
        rota = self.path.split("?", 1)[0]
        if rota.startswith("/api/v2/") or rota.startswith("/api/governance/"):
            if not self._cors_origin():
                return self.responder({"ok": False, "error": {
                    "code": "CORS_ORIGIN_DENIED", "message": "Origem não autorizada",
                    "retryable": False, "details": {}}}, 403)
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self.send_error(404, "rota desconhecida")

    def do_GET(self):
        rota = self.path.split("?")[0]
        consulta = self.path.split("?", 1)[1] if "?" in self.path else ""
        if rota == "/api/v2" or rota.startswith("/api/v2/") or rota == "/api/governance/snapshot":
            if V2_API.handle_get(self, rota, consulta):
                return
        if rota in ("/", "/index.html"):
            index_path = RAIZ / "index.html"
            if index_path.is_file():
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                conteudo = self.index_versionado(index_path)
                self.send_header("Content-Length", str(len(conteudo)))
                self.end_headers()
                self.wfile.write(conteudo)
                return

        if rota == "/api/modelos":
            return self.listar_modelos()
        if rota == "/api/cenas":
            return self.listar_cenas()
        if rota == "/api/posicao":
            return self.responder(
                json.loads(POSICAO.read_text(encoding="utf-8")) if POSICAO.is_file() else {}
            )
        if rota == "/api/takes":
            return self.responder(
                json.loads(TAKES.read_text(encoding="utf-8")) if TAKES.is_file() else {}
            )
        if rota == "/api/projeto":
            return self.obter_projeto()
        if rota == "/api/lugares":
            return self.responder(
                json.loads(LUGARES.read_text(encoding="utf-8")) if LUGARES.is_file() else []
            )
        if rota == "/api/geocode":
            return self.executar_geocode(consulta)
        if rota == "/api/polyhaven/assets":
            return self.obter_polyhaven_assets(consulta)
        if rota == "/api/biblioteca":
            return self.listar_biblioteca()
        if rota == "/api/biblioteca/banco":
            return self.listar_banco()
        if rota == "/banco-glb":
            return self.servir_banco_glb(consulta)
        if rota == "/api/armazenamento":
            return self.medir_armazenamento()
        if rota == "/api/entorno-osm":
            return self.servir_entorno_osm(consulta)
        if rota == "/api/entorno-osm.glb":
            return self.entregar_entorno_glb(consulta)
        if rota == "/glb-corrigido":
            return self.servir_glb_corrigido(consulta)
        if rota == "/glb-lod":
            return self.servir_glb_lod(consulta)
        if rota.startswith("/dem/"):
            return self.servir_dem(rota)
        return super().do_GET()

    @staticmethod
    def index_versionado(index_path: Path) -> bytes:
        """Carimba ?v=<mtime> em app/*.css e app/*.js do index.

        Sem isso o navegador reaproveita a folha de estilo antiga do cache de
        disco e a correcao no arquivo nao aparece na tela.
        """
        html = index_path.read_text(encoding="utf-8")
        pasta_app = RAIZ / "app"
        marca = 0
        if pasta_app.is_dir():
            marca = max((int(p.stat().st_mtime) for p in pasta_app.glob("*.*")), default=0)

        def carimbar(m):
            return f'{m.group(1)}?v={marca}"'

        html = re.sub(r'("\./app/[A-Za-z0-9_.-]+)"', carimbar, html)
        return html.encode("utf-8")

    def do_POST(self):
        rota = self.path.split("?")[0]
        tamanho = int(self.headers.get("Content-Length") or 0)
        upload_grande = (
            rota in {"/api/foto", "/api/v2/reference-media/upload"}
            or (rota.startswith("/api/v2/floorplanner/projects/") and rota.endswith("/exports/upload"))
        )
        limite = 512 * 1024 * 1024 if upload_grande else 32 * 1024 * 1024
        if tamanho < 0 or tamanho > limite:
            return self.responder({"ok": False, "error": {"code": "REQUEST_TOO_LARGE",
                "message": f"corpo excede {limite} bytes", "retryable": False,
                "details": {"content_length": tamanho, "limit": limite}}}, 413)
        corpo = self.rfile.read(tamanho) if tamanho else b""
        if rota.startswith("/api/v2/"):
            if V2_API.handle_post(self, rota, corpo):
                return
        if rota == "/api/foto":
            return self.salvar_foto(corpo)
        if rota == "/api/cena":
            return self.salvar_cena(corpo)
        if rota == "/api/posicao":
            return self.salvar_posicao(corpo)
        if rota == "/api/takes":
            return self.salvar_takes(corpo)
        if rota == "/api/projeto":
            return self.salvar_projeto(corpo)
        if rota == "/api/thumb":
            return self.salvar_thumb(corpo)
        if rota == "/api/lugares":
            return self.salvar_lugares(corpo)
        if rota == "/api/polyhaven/baixar":
            return self.baixar_polyhaven_modelo(corpo)
        if rota == "/api/polyhaven/baixar_textura":
            return self.baixar_polyhaven_textura(corpo)
        if rota == "/api/cache/limpar":
            return self.limpar_cache(corpo)
        if rota == "/api/exportar":
            return self.exportar_recorte(corpo)
        self.send_error(404, "rota desconhecida")

    # -- Exportacao do recorte de cena --------------------------------------
    def exportar_recorte(self, corpo: bytes):
        """Junta os modelos do perimetro (e o relevo) num GLB / glTF / OBJ."""
        try:
            dados = json.loads(corpo.decode("utf-8"))
        except Exception as e:
            return self.responder({"ok": False, "erro": f"JSON invalido: {e}"}, 400)

        formato = str(dados.get("formato", "glb")).lower()
        if formato not in ("glb", "gltf", "obj"):
            return self.responder({"ok": False, "erro": "formato: glb, gltf ou obj"}, 400)
        nome = self.slug(dados.get("nome") or "") or datetime.now().strftime("recorte-%Y%m%d-%H%M%S")

        avisos: list[str] = []
        documentos = []
        for item in dados.get("itens") or []:
            rel = str(item.get("arquivo") or "")
            caminho = caminho_seguro(rel)
            if caminho is None or caminho.suffix.lower() not in EXTENSOES:
                avisos.append(f"{item.get('nome') or rel or 'item'}: arquivo indisponivel para exportar")
                continue
            try:
                doc, binario = arcz_export.carregar_documento(caminho, RAIZ)
            except Exception as e:
                avisos.append(f"{caminho.name}: nao consegui ler ({e})")
                continue
            documentos.append({
                "doc": doc,
                "bin": binario,
                "matriz": item.get("matriz"),
                "nome": self.slug(item.get("nome") or caminho.stem) or caminho.stem,
            })

        relevo = dados.get("relevo") or {}
        if relevo.get("incluir"):
            try:
                item = arcz_export.malha_terreno(
                    relevo.get("poligono") or [],
                    relevo.get("centro") or {},
                    obter_tile_dem,
                    resolucao=int(relevo.get("resolucao", 80)),
                    zoom=int(relevo.get("zoom", 14)),
                )
            except Exception as e:
                item = None
                avisos.append(f"relevo: {e}")
            if item:
                documentos.append(item)
            else:
                avisos.append("relevo: sem tile DEM para o perimetro (ou Pillow ausente)")

        if not documentos:
            return self.responder({"ok": False, "erro": "nada exportavel no perimetro", "avisos": avisos}, 400)

        try:
            doc, binario, avisos_mescla = arcz_export.mesclar(documentos, nome)
        except Exception as e:
            return self.responder({"ok": False, "erro": f"falha ao juntar os modelos: {e}"}, 500)
        avisos.extend(avisos_mescla)

        EXPORTACOES.mkdir(parents=True, exist_ok=True)
        arquivos: list[str] = []
        try:
            if formato == "glb":
                destino = EXPORTACOES / f"{nome}.glb"
                destino.write_bytes(arcz_export.para_glb(doc, binario))
                arquivos.append(destino.name)
            elif formato == "gltf":
                json_bytes, bin_bytes = arcz_export.para_gltf(doc, binario, f"{nome}.bin")
                destino = EXPORTACOES / f"{nome}.gltf"
                destino.write_bytes(json_bytes)
                arquivos.append(destino.name)
                if bin_bytes:
                    (EXPORTACOES / f"{nome}.bin").write_bytes(bin_bytes)
                    arquivos.append(f"{nome}.bin")
            else:
                obj, mtl, texturas = arcz_export.para_obj(doc, binario, nome)
                destino = EXPORTACOES / f"{nome}.obj"
                destino.write_text(obj, encoding="utf-8")
                (EXPORTACOES / f"{nome}.mtl").write_text(mtl, encoding="utf-8")
                arquivos.extend([destino.name, f"{nome}.mtl"])
                for arquivo, conteudo in texturas.items():
                    (EXPORTACOES / arquivo).write_bytes(conteudo)
                    arquivos.append(arquivo)
        except Exception as e:
            return self.responder({"ok": False, "erro": f"falha ao gravar {formato}: {e}"}, 500)

        total = sum((EXPORTACOES / a).stat().st_size for a in arquivos)
        return self.responder({
            "ok": True,
            "nome": nome,
            "formato": formato,
            "url": f"/exportacoes/{destino.name}",
            "arquivos": arquivos,
            "mb": round(total / 1048576, 2),
            "modelos": len(documentos),
            "avisos": avisos,
        })

    # -- Armazenamento local -----------------------------------------------
    @staticmethod
    def medir_pasta(pasta: Path) -> tuple[int, int]:
        """(bytes, arquivos) de uma pasta, recursivo."""
        total = arquivos = 0
        if pasta.is_dir():
            for p in pasta.rglob("*"):
                if p.is_file():
                    try:
                        total += p.stat().st_size
                    except OSError:
                        continue
                    arquivos += 1
        return total, arquivos

    def medir_armazenamento(self):
        alvos = {
            "Relevo (DEM)": CACHE_DEM,
            "Modelos convertidos": CACHE_GLB,
            "Geocodificação": CACHE_GEO,
            "Entorno OSM (prédios locais)": CACHE_ENTORNO,
            "Cache Overpass": CACHE_OVERPASS,
            "Biblioteca 3D": BIBLIOTECA,
            "Modelos do projeto": RAIZ / "modelos",
            "Texturas PBR": TEXTURAS,
            "Takes renderizados": PASTA_TAKES,
            "Recortes exportados": EXPORTACOES,
            "Miniaturas": LIB_THUMBS,
        }
        itens = []
        for nome, pasta in alvos.items():
            tamanho, arquivos = self.medir_pasta(pasta)
            itens.append({
                "nome": nome,
                "pasta": pasta.name,
                "bytes": tamanho,
                "mb": round(tamanho / 1048576, 1),
                "arquivos": arquivos,
                "limpavel": pasta in (CACHE_DEM, CACHE_GLB, CACHE_GEO, CACHE_ENTORNO, CACHE_OVERPASS),
            })
        return self.responder({"itens": itens, "total_mb": round(sum(i["bytes"] for i in itens) / 1048576, 1)})

    def limpar_cache(self, corpo: bytes):
        """Apaga apenas caches regeneraveis, nunca dado do usuario."""
        try:
            alvo = json.loads(corpo.decode("utf-8")).get("pasta", "")
        except Exception:
            alvo = ""
        permitidas = {
            "cache_dem": CACHE_DEM, "cache_glb": CACHE_GLB, "cache_geo": CACHE_GEO,
            "cache_entorno": CACHE_ENTORNO, "cache_overpass": CACHE_OVERPASS,
        }
        if alvo not in permitidas:
            return self.responder({"ok": False, "erro": f"pasta precisa ser uma de {sorted(permitidas)}"}, 400)

        pasta = permitidas[alvo]
        liberado = apagados = 0
        for p in sorted(pasta.rglob("*"), reverse=True):
            try:
                if p.is_file():
                    liberado += p.stat().st_size
                    p.unlink()
                    apagados += 1
                elif p.is_dir():
                    p.rmdir()
            except OSError:
                continue
        return self.responder({"ok": True, "pasta": alvo, "arquivos": apagados,
                               "mb": round(liberado / 1048576, 1)})

    # -- Poly Haven Ingestion ----------------------------------------------
    def obter_polyhaven_assets(self, consulta: str):
        qs = urllib.parse.parse_qs(consulta)
        tipo = qs.get("type", ["models"])[0]
        CACHE_GEO.mkdir(parents=True, exist_ok=True)
        cache_file = CACHE_GEO / f"polyhaven_{tipo}.json"

        # Cache local por 24 horas
        if cache_file.is_file() and (time.time() - cache_file.stat().st_mtime) < 86400:
            return self.responder(json.loads(cache_file.read_text(encoding="utf-8")))

        url = f"https://api.polyhaven.com/assets?t={tipo}"
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "ARCZ-Visualizador/1.0"})
            V2_API.policy.assert_url(url)
            with urllib.request.urlopen(req, timeout=15) as r:
                res = json.loads(r.read().decode("utf-8"))
            cache_file.write_text(json.dumps(res, ensure_ascii=False), encoding="utf-8")
            return self.responder(res)
        except Exception as e:
            if cache_file.is_file():
                return self.responder(json.loads(cache_file.read_text(encoding="utf-8")))
            return self.responder({"erro": f"falha ao consultar Poly Haven: {e}"}, 502)

    def baixar_polyhaven_modelo(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
            asset_id = dados.get("id", "")
            res = dados.get("resolution", "1k")
            if not asset_id:
                return self.responder({"ok": False, "erro": "ID obrigatorio"}, 400)
            
            nome_slug = self.slug(asset_id)
            pasta_destino = BIBLIOTECA / nome_slug
            pasta_destino.mkdir(parents=True, exist_ok=True)
            arquivo_glb = pasta_destino / f"{nome_slug}.glb"

            if not arquivo_glb.is_file():
                info_url = f"https://api.polyhaven.com/files/{asset_id}"
                V2_API.policy.assert_url(info_url)
                req = urllib.request.Request(info_url, headers={"User-Agent": "ARCZ-Visualizador/1.0"})
                with urllib.request.urlopen(req, timeout=15) as r:
                    info_files = json.loads(r.read().decode("utf-8"))
                
                gltf_info = info_files.get("gltf", {}).get(res, {}).get("gltf", {}) or \
                            info_files.get("gltf", {}).get("1k", {}).get("gltf", {})
                
                download_url = gltf_info.get("url")
                if not download_url:
                    return self.responder({"ok": False, "erro": "modelo .glb/.gltf nao encontrado no Poly Haven"}, 404)
                
                V2_API.policy.assert_url(download_url)
                req_dl = urllib.request.Request(download_url, headers={"User-Agent": "ARCZ-Visualizador/1.0"})
                with urllib.request.urlopen(req_dl, timeout=30) as r_dl:
                    glb_bytes = r_dl.read()
                
                arquivo_glb.write_bytes(glb_bytes)
                (pasta_destino / "LICENCA.txt").write_text(
                    f"Asset: {asset_id}\nFonte: Poly Haven (https://polyhaven.com/a/{asset_id})\nLicenca: CC0 1.0 Universal\n",
                    encoding="utf-8"
                )

            return self.responder({
                "ok": True,
                "nome": nome_slug,
                "url": "/" + arquivo_glb.relative_to(RAIZ).as_posix()
            })
        except Exception as e:
            return self.responder({"ok": False, "erro": f"falha ao baixar modelo: {e}"}, 500)

    def baixar_polyhaven_textura(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
            asset_id = dados.get("id", "")
            res = dados.get("resolution", "1k")
            if not asset_id:
                return self.responder({"ok": False, "erro": "ID obrigatorio"}, 400)
            
            nome_slug = self.slug(asset_id)
            pasta_destino = TEXTURAS / nome_slug
            pasta_destino.mkdir(parents=True, exist_ok=True)

            info_url = f"https://api.polyhaven.com/files/{asset_id}"
            V2_API.policy.assert_url(info_url)
            req = urllib.request.Request(info_url, headers={"User-Agent": "ARCZ-Visualizador/1.0"})
            with urllib.request.urlopen(req, timeout=15) as r:
                info_files = json.loads(r.read().decode("utf-8"))
            
            mapas_baixados = {}
            for map_name in ("diff", "nor_gl", "rough", "metal", "arm"):
                map_info = info_files.get(map_name, {}).get(res, {}).get("jpg", {}) or \
                           info_files.get(map_name, {}).get(res, {}).get("png", {})
                if map_info and "url" in map_info:
                    ext = ".jpg" if "jpg" in map_info.get("url", "") else ".png"
                    dest_file = pasta_destino / f"{map_name}{ext}"
                    if not dest_file.is_file():
                        V2_API.policy.assert_url(map_info["url"])
                        req_map = urllib.request.Request(map_info["url"], headers={"User-Agent": "ARCZ-Visualizador/1.0"})
                        with urllib.request.urlopen(req_map, timeout=20) as r_map:
                            dest_file.write_bytes(r_map.read())
                    mapas_baixados[map_name] = "/" + dest_file.relative_to(RAIZ).as_posix()

            return self.responder({
                "ok": True,
                "nome": nome_slug,
                "mapas": mapas_baixados
            })
        except Exception as e:
            return self.responder({"ok": False, "erro": f"falha ao baixar textura: {e}"}, 500)

    @staticmethod
    def _defaults_projeto_v2(dados=None):
        """Compatibilidade pública sobre a migração V2 única e validada.

        Não mantenha defaults paralelos neste arquivo. Toda mudança de schema
        deve entrar em ``arcz_server/project_migrations.py`` com teste.
        """
        import secrets
        migrated, _ = migrate_project(
            dict(dados or {}),
            project_seed=secrets.randbits(63),
            schemas=V2_API.schemas,
        )
        return migrated

    def obter_projeto(self):
        if PROJETO.is_file():
            try:
                dados, report = migrate_project_file(PROJETO, schemas=V2_API.schemas)
                if report.warnings:
                    dados.setdefault("warnings", []).extend(report.warnings)
                return self.responder(dados)
            except Exception as project_error:
                print(f"AVISO: projeto.json inválido; tentando backup: {project_error}", file=sys.stderr)
                backup = PROJETO.with_name(PROJETO.name + ".bak")
                if backup.is_file():
                    try:
                        dados = json.loads(backup.read_text(encoding="utf-8"))
                        dados, _ = migrate_project(dados, schemas=V2_API.schemas)
                        dados.setdefault("warnings", []).append("projeto.json corrompido/inválido; backup carregado")
                        return self.responder(dados)
                    except Exception as backup_error:
                        print(f"AVISO: backup do projeto também é inválido: {backup_error}", file=sys.stderr)
        pos = json.loads(POSICAO.read_text(encoding="utf-8")) if POSICAO.is_file() else {}
        takes = json.loads(TAKES.read_text(encoding="utf-8")) if TAKES.is_file() else []
        dados, _ = migrate_project({
            "posicao": pos,
            "takes": takes,
            "pecas": [],
            "criado_em": datetime.now().isoformat(timespec="seconds"),
        }, schemas=V2_API.schemas)
        return self.responder(dados)

    def salvar_projeto(self, corpo: bytes):
        try:
            recebido = json.loads(corpo.decode("utf-8"))
        except Exception as e:
            return self.responder({"ok": False, "erro": f"JSON invalido: {e}"}, 400)
        if not isinstance(recebido, dict) or not recebido.get("posicao"):
            return self.responder({"ok": False, "erro": "projeto sem posicao"}, 400)
        atual = {}
        if PROJETO.is_file():
            try:
                atual = json.loads(PROJETO.read_text(encoding="utf-8"))
            except Exception:
                atual = {}
        try:
            dados, migration = migrate_project(recebido, schemas=V2_API.schemas)
        except ApiError as error:
            return self.responder({"ok": False, "error": error.to_dict()["error"]}, error.status)
        dados["save_revision"] = max(int(atual.get("save_revision", 0)), int(dados.get("save_revision", 0))) + 1
        dados["atualizado_em"] = datetime.now().isoformat(timespec="seconds")
        sem_hash = dict(dados)
        sem_hash.pop("content_hash", None)
        dados["content_hash"] = canonical_json_hash(sem_hash)
        try:
            V2_API.schemas.validate("project-v2.schema.json", dados)
            arquivo_hash = atomic_write_json(PROJETO, dados, backup=True, indent=2)
        except Exception as e:
            return self.responder({"ok": False, "erro": f"falha atomica ao salvar: {e}"}, 500)
        return self.responder({
            "ok": True,
            "arquivo": str(PROJETO),
            "save_revision": dados["save_revision"],
            "content_hash": dados["content_hash"],
            "file_hash": arquivo_hash,
            "migration": migration.to_dict(),
        })

    def salvar_thumb(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
            nome = self.slug(dados.get("nome", ""))
            crua = dados["png"].split(",", 1)[-1]
            png = base64.b64decode(crua)
        except Exception as e:
            return self.responder({"ok": False, "erro": f"thumb invalido: {e}"}, 400)
        LIB_THUMBS.mkdir(parents=True, exist_ok=True)
        destino = LIB_THUMBS / f"{nome}.png"
        destino.write_bytes(png)
        return self.responder({"ok": True, "url": f"/lib_thumbs/{destino.name}"})

    def executar_geocode(self, consulta: str):
        """Compatibilidade da rota antiga sobre o indice local V2.

        Nao consulta Nominatim automaticamente. Um conector remoto pode importar
        dados para o indice, mas a busca operacional permanece offline.
        """
        qs = urllib.parse.parse_qs(consulta)
        q = qs.get("q", [""])[0].strip()
        if not q:
            return self.responder([], 200)
        try:
            resultados = V2_API.geocoder.search(q, limit=5)
        except ApiError as e:
            return self.responder(e.payload(), e.status)
        legado = []
        for item in resultados:
            west, south, east, north = item["bbox_wgs84"]
            legado.append({
                "place_id": item["id"],
                "display_name": item["display_name"],
                "lat": str(item["lat"]), "lon": str(item["lon"]),
                "boundingbox": [str(south), str(north), str(west), str(east)],
                "type": item["scale"], "source_package": item["source_package"]
            })
        return self.responder(legado)

    def salvar_lugares(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
        except Exception as e:
            return self.responder({"ok": False, "erro": f"JSON invalido: {e}"}, 400)
        LUGARES.write_text(json.dumps(dados, indent=2, ensure_ascii=False), encoding="utf-8")
        return self.responder({"ok": True, "arquivo": str(LUGARES)})

    def servir_glb_lod(self, consulta: str):
        qs = urllib.parse.parse_qs(consulta)
        rel = urllib.parse.unquote(qs.get("arquivo", [""])[0])
        try:
            max_tex = int(qs.get("tex", ["1024"])[0])
        except ValueError:
            return self.send_error(400, "parametro tex invalido")
        max_tex = max(64, min(max_tex, 8192))
        return self.entregar_glb(rel, max_tex)

    def servir_glb_corrigido(self, consulta: str):
        rel = urllib.parse.unquote(urllib.parse.parse_qs(consulta).get("arquivo", [""])[0])
        return self.entregar_glb(rel, None)

    def entregar_glb(self, rel: str, max_tex):
        """Converte material (e aplica LOD real de textura) com cache em disco."""
        origem = caminho_seguro(rel)
        if origem is None:
            return self.send_error(404, "arquivo fora do projeto")
        return self.entregar_glb_de(origem, max_tex)

    def entregar_glb_de(self, origem: Path, max_tex):
        """Mesma entrega, a partir de um caminho ja validado por quem chamou."""
        st = origem.stat()
        sufixo = f"lod{max_tex}" if max_tex else "orig"
        marca = hashlib.sha256(
            f"{origem}|{st.st_mtime_ns}|{st.st_size}|{sufixo}|v2".encode("utf-8")
        ).hexdigest()[:16]
        destino = CACHE_GLB / f"{marca}_{sufixo}.glb"
        if not destino.is_file():
            try:
                saida = processar_glb(origem.read_bytes(), max_tex)
            except Exception as e:
                return self.send_error(500, f"nao converti o glTF: {e}")
            CACHE_GLB.mkdir(parents=True, exist_ok=True)
            tmp = destino.with_suffix(".parcial")
            tmp.write_bytes(saida)
            tmp.replace(destino)
        dados = destino.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "model/gltf-binary")
        self.send_header("Content-Length", str(len(dados)))
        self.send_header("Cache-Control", "max-age=3600")
        self.end_headers()
        self.wfile.write(dados)

    def servir_dem(self, rota: str):
        try:
            z, x, y = (int(p) for p in rota[len("/dem/"):].removesuffix(".png").split("/"))
        except ValueError:
            return self.send_error(400, "tile invalido")
        if not (0 <= z <= 15 and 0 <= x < 2 ** z and 0 <= y < 2 ** z):
            return self.send_error(404, "fora da cobertura")
        dados = obter_tile_dem(z, x, y)
        if dados is None:
            return self.send_error(502, "DEM indisponivel")
        self.send_response(200)
        self.send_header("Content-Type", "image/png")
        self.send_header("Content-Length", str(len(dados)))
        self.send_header("Cache-Control", "max-age=86400")
        self.end_headers()
        self.wfile.write(dados)

    # -- Entorno OSM local (prédios/vias/vegetação, sem Cesium Ion) --------
    @staticmethod
    def _ler_parametros_entorno(consulta: str):
        """(lat, lon, lado, adensar) da query, já validados — ou levanta ValueError."""
        qs = urllib.parse.parse_qs(consulta)
        lat = float(qs.get("lat", [""])[0])
        lon = float(qs.get("lon", [""])[0])
        lado = int(float(qs.get("lado", ["150"])[0]))
        adensar = qs.get("adensar", ["0"])[0].lower() not in ("0", "", "false")
        if not (-90 <= lat <= 90 and -180 <= lon <= 180):
            raise ValueError("lat/lon fora de alcance")
        if lado not in PRESETS_LADO_ENTORNO:
            raise ValueError(f"lado precisa ser um de {PRESETS_LADO_ENTORNO}")
        return lat, lon, lado, adensar

    def servir_entorno_osm(self, consulta: str):
        """Metadados (contagens) do entorno gerado — gera na primeira vez, cacheado depois."""
        try:
            lat, lon, lado, adensar = self._ler_parametros_entorno(consulta)
        except (ValueError, IndexError) as e:
            return self.responder({"erro": f"parametros invalidos: {e}"}, 400)
        try:
            _, metadados = gerar_ou_obter_entorno(lat, lon, lado, adensar)
        except EntornoIndisponivel as e:
            return self.responder({"erro": str(e)}, 502)
        return self.responder(metadados)

    def entregar_entorno_glb(self, consulta: str):
        """O .glb do entorno. Recalcula a mesma chave da query — nunca aceita
        um caminho vindo do cliente."""
        try:
            lat, lon, lado, adensar = self._ler_parametros_entorno(consulta)
        except (ValueError, IndexError) as e:
            return self.send_error(400, f"parametros invalidos: {e}")
        try:
            caminho_glb, _ = gerar_ou_obter_entorno(lat, lon, lado, adensar)
        except EntornoIndisponivel as e:
            return self.send_error(502, str(e))

        dados = caminho_glb.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "model/gltf-binary")
        self.send_header("Content-Length", str(len(dados)))
        self.send_header("Cache-Control", "max-age=3600")
        self.end_headers()
        self.wfile.write(dados)

    def salvar_posicao(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
        except Exception as e:
            return self.responder({"ok": False, "erro": f"JSON invalido: {e}"}, 400)
        dados["salvo_em"] = datetime.now().isoformat(timespec="seconds")
        POSICAO.write_text(json.dumps(dados, indent=2, ensure_ascii=False), encoding="utf-8")
        return self.responder({"ok": True, "arquivo": str(POSICAO)})

    def salvar_foto(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
            crua = dados["png"].split(",", 1)[-1]
            png = base64.b64decode(crua)
        except Exception as e:
            return self.responder({"ok": False, "erro": f"PNG invalido: {e}"}, 400)
        if dados.get("pasta") == "takes":
            base = PASTA_TAKES
            base.mkdir(parents=True, exist_ok=True)
            nome = self.slug(dados.get("nome", "")) or datetime.now().strftime("take-%H%M%S")
        else:
            base = FOTOS
            base.mkdir(parents=True, exist_ok=True)
            nome = datetime.now().strftime("foto-%Y%m%d-%H%M%S")
            rotulo = self.slug(dados.get("nome", ""))
            if rotulo:
                nome += "-" + rotulo
        destino = base / (nome + ".png")
        destino.write_bytes(png)
        return self.responder(
            {"ok": True, "arquivo": str(destino), "kb": round(len(png) / 1024),
             "url": f"/{base.name}/{destino.name}"}
        )

    def salvar_takes(self, corpo: bytes):
        try:
            dados = json.loads(corpo.decode("utf-8"))
        except Exception as e:
            return self.responder({"ok": False, "erro": f"JSON invalido: {e}"}, 400)
        TAKES.write_text(json.dumps(dados, indent=2, ensure_ascii=False), encoding="utf-8")
        return self.responder({"ok": True, "arquivo": str(TAKES), "planos": len(dados)})

    def salvar_cena(self, corpo: bytes):
        try:
            cena = json.loads(corpo.decode("utf-8"))
        except Exception as e:
            return self.responder({"ok": False, "erro": f"JSON invalido: {e}"}, 400)
        nome = self.slug(cena.get("nome") or "") or datetime.now().strftime("cena-%Y%m%d-%H%M%S")
        CENAS.mkdir(parents=True, exist_ok=True)
        destino = CENAS / (nome + ".json")
        cena["nome"] = nome
        cena["salvo_em"] = datetime.now().isoformat(timespec="seconds")
        destino.write_text(json.dumps(cena, indent=2, ensure_ascii=False), encoding="utf-8")
        return self.responder({"ok": True, "arquivo": str(destino), "nome": nome})

    @staticmethod
    def modelo_utilizavel(caminho: Path) -> bool:
        """Diz se o arquivo abre de verdade no viewer.

        Na biblioteca CC0 existem .glb que na verdade sao glTF JSON (o
        /glb-lod rejeita) e .gltf cujo .bin/textura nao veio junto. Um card
        desses nunca carrega: melhor nao oferecer no catalogo.
        """
        try:
            with caminho.open("rb") as arquivo:
                cabecalho = arquivo.read(4)
            if cabecalho == b"glTF":
                return True
            if caminho.suffix.lower() == ".glb":
                return False          # .glb sem o magic: o pipeline de LOD recusa
            dados = json.loads(caminho.read_text(encoding="utf-8"))
        except Exception:
            return False

        for lista in ("buffers", "images"):
            for item in dados.get(lista, []):
                uri = item.get("uri")
                if not uri or uri.startswith("data:"):
                    continue
                if not (caminho.parent / urllib.parse.unquote(uri)).is_file():
                    return False
        return True

    # -- Banco de modelos externo -------------------------------------------
    @staticmethod
    def eh_glb(caminho: Path) -> bool:
        """Binario glTF de verdade? (o banco mistura .blend sem extensao)."""
        try:
            with caminho.open("rb") as arquivo:
                return arquivo.read(4) == b"glTF"
        except OSError:
            return False

    @classmethod
    def manifesto_banco(cls):
        """Le (e memoriza) o manifesto do banco. Devolve {id: (asset, Path)}.

        O indice guarda o caminho ja resolvido: o cliente so manda o `id`, e a
        rota nunca monta caminho com texto vindo do navegador.
        """
        arq = BANCO / "metadata" / "manifest.json"
        if not arq.is_file():
            return {}
        marca = arq.stat().st_mtime_ns
        if getattr(cls, "_banco_marca", None) == marca:
            return cls._banco_indice

        try:
            dados = json.loads(arq.read_text(encoding="utf-8"))
        except Exception as e:
            print(f"ARCZ: manifesto do banco ilegivel: {e}")
            return {}

        indice = {}
        for a in dados.get("assets", []):
            ident = a.get("id")
            if not ident:
                continue
            # abo-*/ph-* moram achatados em furniture/<categoria>/<id> (sem
            # extensao); os do glTF-Sample-Assets mantem a arvore de origem.
            candidatos = [
                BANCO / "furniture" / str(a.get("categoria") or a.get("category", "")) / ident,
                BANCO / "sources" / str(a.get("source", "")) / str(a.get("path", "")),
            ]
            alvo = next((c for c in candidatos if c.is_file()), None)
            # No banco os 534 itens do Poly Haven sao .blend comprimido (zstd
            # ou gzip), nao glTF: entrariam na grade como card que nunca abre.
            # Quem quer Poly Haven usa a fonte online, que serve GLB de fato.
            if alvo and cls.eh_glb(alvo):
                indice[ident] = (a, alvo)

        cls._banco_marca = marca
        cls._banco_indice = indice
        return indice

    def listar_banco(self):
        indice = self.manifesto_banco()
        if not indice:
            return self.responder(
                {"erro": f"banco de modelos nao encontrado em {BANCO}"}, 200)
        itens = [{
            "nome": a.get("name") or ident,
            "url": f"/banco-glb?id={urllib.parse.quote(ident)}",
            "mb": round(a.get("size_bytes", alvo.stat().st_size) / 1048576, 1),
            "categoria_banco": a.get("category", "uncategorized"),
            "licenca": a.get("license", "?"),
            "fonte": a.get("source", "?"),
            "thumb": None
        } for ident, (a, alvo) in indice.items()]
        itens.sort(key=lambda i: i["nome"].lower())
        return self.responder(itens)

    def servir_banco_glb(self, consulta: str):
        qs = urllib.parse.parse_qs(consulta)
        ident = qs.get("id", [""])[0]
        registro = self.manifesto_banco().get(ident)
        if registro is None:
            return self.send_error(404, "modelo fora do banco")
        try:
            max_tex = int(qs.get("tex", ["1024"])[0])
        except ValueError:
            return self.send_error(400, "parametro tex invalido")
        # Modelos do banco chegam a 90 MB: passar pelo mesmo LOD que a
        # biblioteca local evita derrubar o navegador com textura 8k.
        return self.entregar_glb_de(registro[1], max(64, min(max_tex, 8192)))

    def listar_biblioteca(self):
        itens = []
        if BIBLIOTECA.is_dir():
            for pasta in sorted(BIBLIOTECA.iterdir()):
                if not pasta.is_dir():
                    continue
                # .glb primeiro; muitos itens CC0 vêm como .gltf + .bin externo.
                modelos = sorted(pasta.glob("*.glb")) + sorted(pasta.glob("*.gltf"))
                for g in modelos:
                    if not self.modelo_utilizavel(g):
                        continue
                    nome_slug = self.slug(pasta.name)
                    thumb_path = LIB_THUMBS / f"{nome_slug}.png"
                    thumb_url = f"/lib_thumbs/{nome_slug}.png" if thumb_path.is_file() else None
                    itens.append({
                        "nome": pasta.name,
                        "url": "/" + g.relative_to(RAIZ).as_posix(),
                        "mb": round(g.stat().st_size / 1048576, 1),
                        "thumb": thumb_url
                    })
                    break
        return self.responder(itens)

    def listar_cenas(self):
        itens = []
        if CENAS.is_dir():
            for p in sorted(CENAS.glob("*.json")):
                try:
                    itens.append(json.loads(p.read_text(encoding="utf-8")))
                except Exception:
                    continue
        return self.responder(itens)

    @staticmethod
    def slug(txt: str) -> str:
        return re.sub(r"[^A-Za-z0-9._-]+", "-", str(txt)).strip("-.")[:60]

    def responder(self, obj, codigo=200):
        corpo = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(codigo)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(corpo)))
        # Sem isto o navegador pode servir /api/projeto do cache e o app sobe com
        # um projeto velho, que na primeira mudanca sobrescreve o do disco.
        self.send_header("Cache-Control", "no-store, must-revalidate")
        self.end_headers()
        self.wfile.write(corpo)

    def listar_modelos(self):
        itens = []
        for pasta in PASTAS_DE_MODELO:
            if not pasta.is_dir():
                continue
            for p in sorted(pasta.iterdir()):
                if p.is_file() and p.suffix.lower() in EXTENSOES:
                    itens.append(
                        {
                            "nome": p.name,
                            "pasta": pasta.name,
                            "url": "/" + p.relative_to(RAIZ).as_posix(),
                            "caminho": str(p),
                            "mb": round(p.stat().st_size / 1048576, 1),
                        }
                    )
        corpo = json.dumps(itens, ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(corpo)))
        self.end_headers()
        self.wfile.write(corpo)

    def log_message(self, fmt, *args):
        # The standard library access log is intentionally disabled because the
        # diagnostics/event stores are the auditable source. Return explicitly
        # rather than hiding a missing implementation behind ``pass``.
        return None


def subir(porta: int):
    for tentativa in range(porta, porta + 20):
        try:
            return http.server.ThreadingHTTPServer(("127.0.0.1", tentativa), Handler), tentativa
        except OSError:
            continue
    raise SystemExit(f"nenhuma porta livre entre {porta} e {porta + 19}")


def abrir_navegador_com_gpu(url: str):
    import subprocess
    candidatos = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ]
    flags = [
        "--ignore-gpu-blocklist",
        "--enable-gpu-rasterization",
        "--enable-zero-copy",
        "--use-gl=angle",
        "--use-angle=d3d11"
    ]
    for exe in candidatos:
        if os.path.isfile(exe):
            try:
                subprocess.Popen([exe] + flags + [url])
                return
            except (OSError, subprocess.SubprocessError) as exc:
                print(f"AVISO: navegador GPU não abriu em {exe}: {exc}", file=sys.stderr)
    webbrowser.open(url)


def main():
    porta = int(sys.argv[1]) if len(sys.argv) > 1 else PORTA_PADRAO
    servidor, porta = subir(porta)
    url = f"http://127.0.0.1:{porta}/"
    # O sidecar Aedifex recebe a porta real selecionada pelo gateway. Isso evita
    # assumir 8123 quando a porta padrão já estiver ocupada.
    V2_API.aedifex_runtime.api_url = url.rstrip("/")

    print(f"ARCZ Visualizador Unificado — servindo {RAIZ}")
    if not PIL_DISPONIVEL:
        print("AVISO: Pillow ausente — /glb-lod so converte material, sem reduzir textura.")
    print(f"Abra:  {url}")
    print("Ctrl+C para parar.")
    if os.environ.get("ARCZ_SEM_NAVEGADOR") != "1":
        threading.Timer(0.7, lambda: abrir_navegador_com_gpu(url)).start()
    try:
        servidor.serve_forever()
    except KeyboardInterrupt:
        print("\nparado.")


if __name__ == "__main__":
    main()
