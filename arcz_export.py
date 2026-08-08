#!/usr/bin/env python3
"""Exportacao de recortes de cena do ARCZ.

Funcoes puras, sem HTTP, para poderem ser testadas isoladamente
(`python -m unittest tests_python.test_arcz_export`).

Operacoes:

1. `carregar_documento` — le .glb ou .gltf (buffers e imagens externos ou em
   `data:`) e devolve o documento com UM unico buffer binario embutido.
2. `mesclar` — junta varios documentos num glTF so, cada um pendurado num no
   com a matriz que o coloca no lugar certo do recorte.
3. `para_glb` / `para_gltf` / `para_obj` — serializa o resultado.
4. `malha_terreno` — recorta o relevo real (tiles Terrarium) dentro do
   perimetro desenhado e devolve um item pronto para `mesclar`.

Convencao de eixos: o recorte sai em Y-up (padrao glTF). Quem chama manda a
matriz de cada item ja nessa convencao; a malha de terreno e gerada
diretamente em (leste, cima, -norte).
"""

from __future__ import annotations

import base64
import json
import math
import struct
import urllib.parse
from pathlib import Path

from arcz_glb import desempacotar, empacotar

# WGS84
_A = 6378137.0
_F = 1.0 / 298.257223563
_E2 = _F * (2 - _F)

TIPOS_COMPONENTE = {
    5120: ("b", 1),
    5121: ("B", 1),
    5122: ("h", 2),
    5123: ("H", 2),
    5125: ("I", 4),
    5126: ("f", 4),
}

ITENS_POR_TIPO = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT2": 4, "MAT3": 9, "MAT4": 16}

MIME_POR_EXTENSAO = {
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".webp": "image/webp",
    ".ktx2": "image/ktx2",
}

EXTENSAO_POR_MIME = {v: k for k, v in MIME_POR_EXTENSAO.items()}


# --------------------------------------------------------------------- leitura
def _alinhar(dados: bytearray) -> None:
    resto = len(dados) % 4
    if resto:
        dados.extend(b"\x00" * (4 - resto))


def _resolver_uri(uri: str, base: Path, raiz: Path | None) -> bytes:
    """Devolve os bytes de um `uri` de buffer/imagem do glTF."""
    if uri.startswith("data:"):
        cabeca, _, corpo = uri.partition(",")
        if ";base64" in cabeca:
            return base64.b64decode(corpo)
        return urllib.parse.unquote_to_bytes(corpo)
    alvo = (base / urllib.parse.unquote(uri)).resolve()
    if raiz is not None:
        try:
            alvo.relative_to(Path(raiz).resolve())
        except ValueError:
            raise ValueError(f"arquivo fora do projeto: {uri}")
    return alvo.read_bytes()


def embutir(doc: dict, binario: bytes, base: Path, raiz: Path | None = None) -> tuple[dict, bytes]:
    """Junta todos os buffers e imagens externas num unico chunk binario."""
    saida = bytearray()
    inicios: list[int] = []

    for i, buf in enumerate(doc.get("buffers") or []):
        uri = buf.get("uri")
        dados = _resolver_uri(uri, base, raiz) if uri else bytes(binario)
        if i == 0 and not uri and not dados:
            dados = b""
        _alinhar(saida)
        inicios.append(len(saida))
        saida.extend(dados)

    for view in doc.get("bufferViews") or []:
        indice = view.get("buffer", 0)
        view["byteOffset"] = inicios[indice] + view.get("byteOffset", 0) if inicios else 0
        view["buffer"] = 0

    views = doc.setdefault("bufferViews", [])
    for img in doc.get("images") or []:
        uri = img.pop("uri", None)
        if uri is None:
            continue
        dados = _resolver_uri(uri, base, raiz)
        _alinhar(saida)
        views.append({"buffer": 0, "byteOffset": len(saida), "byteLength": len(dados)})
        saida.extend(dados)
        img["bufferView"] = len(views) - 1
        if "mimeType" not in img:
            ext = Path(urllib.parse.unquote(uri).split("?")[0]).suffix.lower()
            img["mimeType"] = MIME_POR_EXTENSAO.get(ext, "image/png")

    doc["buffers"] = [{"byteLength": len(saida)}] if saida else []
    return doc, bytes(saida)


def carregar_documento(caminho: Path, raiz: Path | None = None) -> tuple[dict, bytes]:
    """Le um .glb ou .gltf do disco com tudo embutido num buffer so."""
    caminho = Path(caminho)
    dados = caminho.read_bytes()
    if dados[:4] == b"glTF":
        doc, binario = desempacotar(dados)
    else:
        doc, binario = json.loads(dados.decode("utf-8")), b""
    return embutir(doc, binario, caminho.parent, raiz)


# --------------------------------------------------------------------- mescla
def _remapear_texturas(valor, base_textura: int):
    """Anda pelo material e desloca todo `{"index": n}` de textura."""
    if isinstance(valor, dict):
        for chave, item in valor.items():
            if chave == "index" and isinstance(item, int):
                valor[chave] = item + base_textura
            else:
                _remapear_texturas(item, base_textura)
    elif isinstance(valor, list):
        for item in valor:
            _remapear_texturas(item, base_textura)


def mesclar(itens: list[dict], nome_cena: str = "recorte") -> tuple[dict, bytes, list[str]]:
    """Junta documentos glTF num so.

    `itens`: [{"doc", "bin", "matriz" (16 floats, coluna a coluna), "nome"}].
    Devolve (documento, binario, avisos). Animacoes e skins sao descartadas —
    o recorte e uma fotografia da cena, nao um rig.
    """
    saida = {
        "asset": {"version": "2.0", "generator": "ARCZ Earth · recorte de cena"},
        "scenes": [{"name": nome_cena, "nodes": []}],
        "scene": 0,
        "nodes": [],
        "meshes": [],
        "materials": [],
        "accessors": [],
        "bufferViews": [],
        "textures": [],
        "images": [],
        "samplers": [],
    }
    binario = bytearray()
    avisos: list[str] = []
    usadas: list[str] = []
    requeridas: list[str] = []

    for item in itens:
        doc = json.loads(json.dumps(item["doc"]))  # copia: nao mexe no original
        dados = item.get("bin") or b""
        nome = item.get("nome") or "item"

        base_view = len(saida["bufferViews"])
        base_acessor = len(saida["accessors"])
        base_material = len(saida["materials"])
        base_malha = len(saida["meshes"])
        base_no = len(saida["nodes"])
        base_textura = len(saida["textures"])
        base_imagem = len(saida["images"])
        base_sampler = len(saida["samplers"])

        _alinhar(binario)
        deslocamento = len(binario)
        binario.extend(dados)

        for view in doc.get("bufferViews") or []:
            novo = dict(view)
            novo["buffer"] = 0
            novo["byteOffset"] = deslocamento + view.get("byteOffset", 0)
            saida["bufferViews"].append(novo)

        for acessor in doc.get("accessors") or []:
            novo = dict(acessor)
            if "bufferView" in novo:
                novo["bufferView"] += base_view
            esparso = novo.get("sparse")
            if esparso:
                esparso = json.loads(json.dumps(esparso))
                esparso["indices"]["bufferView"] += base_view
                esparso["values"]["bufferView"] += base_view
                novo["sparse"] = esparso
            saida["accessors"].append(novo)

        for img in doc.get("images") or []:
            novo = dict(img)
            if "bufferView" in novo:
                novo["bufferView"] += base_view
            saida["images"].append(novo)

        for sampler in doc.get("samplers") or []:
            saida["samplers"].append(dict(sampler))

        for tex in doc.get("textures") or []:
            novo = json.loads(json.dumps(tex))
            if "source" in novo:
                novo["source"] += base_imagem
            if "sampler" in novo:
                novo["sampler"] += base_sampler
            for ext in (novo.get("extensions") or {}).values():
                if isinstance(ext, dict) and isinstance(ext.get("source"), int):
                    ext["source"] += base_imagem
            saida["textures"].append(novo)

        for material in doc.get("materials") or []:
            novo = json.loads(json.dumps(material))
            _remapear_texturas(novo, base_textura)
            saida["materials"].append(novo)

        for malha in doc.get("meshes") or []:
            nova = json.loads(json.dumps(malha))
            for prim in nova.get("primitives") or []:
                prim["attributes"] = {k: v + base_acessor for k, v in (prim.get("attributes") or {}).items()}
                if "indices" in prim:
                    prim["indices"] += base_acessor
                if "material" in prim:
                    prim["material"] += base_material
                alvos = prim.get("targets")
                if alvos:
                    prim["targets"] = [{k: v + base_acessor for k, v in alvo.items()} for alvo in alvos]
                draco = (prim.get("extensions") or {}).get("KHR_draco_mesh_compression")
                if draco:
                    draco["bufferView"] += base_view
            saida["meshes"].append(nova)

        for no in doc.get("nodes") or []:
            novo = json.loads(json.dumps(no))
            novo.pop("camera", None)
            if novo.pop("skin", None) is not None and "skins" not in avisos:
                avisos.append("skins")
            if "mesh" in novo:
                novo["mesh"] += base_malha
            if novo.get("children"):
                novo["children"] = [c + base_no for c in novo["children"]]
            saida["nodes"].append(novo)

        if doc.get("animations"):
            avisos.append(f"{nome}: animacoes descartadas")

        cena = (doc.get("scenes") or [{}])[doc.get("scene", 0) or 0]
        raizes = [n + base_no for n in (cena.get("nodes") or [])]
        if not raizes:
            raizes = [base_no + i for i in range(len(doc.get("nodes") or []))]

        embrulho = {"name": nome, "children": raizes}
        matriz = item.get("matriz")
        if matriz and len(matriz) == 16:
            embrulho["matrix"] = [float(v) for v in matriz]
        saida["nodes"].append(embrulho)
        saida["scenes"][0]["nodes"].append(len(saida["nodes"]) - 1)

        for ext in doc.get("extensionsUsed") or []:
            if ext not in usadas:
                usadas.append(ext)
        for ext in doc.get("extensionsRequired") or []:
            if ext not in requeridas:
                requeridas.append(ext)

    if binario:
        saida["buffers"] = [{"byteLength": len(binario)}]
    if usadas:
        saida["extensionsUsed"] = usadas
    if requeridas:
        saida["extensionsRequired"] = requeridas
    for chave in ("materials", "accessors", "bufferViews", "textures", "images", "samplers", "meshes"):
        if not saida[chave]:
            del saida[chave]
    if "skins" in avisos:
        avisos = [a for a in avisos if a != "skins"] + ["malhas com skin foram exportadas na pose de repouso"]
    return saida, bytes(binario), avisos


def para_glb(doc: dict, binario: bytes) -> bytes:
    return empacotar(doc, binario)


def para_gltf(doc: dict, binario: bytes, nome_bin: str) -> tuple[bytes, bytes]:
    """(.gltf, .bin) — o JSON aponta para o binario ao lado."""
    doc = json.loads(json.dumps(doc))
    if binario:
        doc["buffers"] = [{"byteLength": len(binario), "uri": nome_bin}]
    return json.dumps(doc, indent=1, ensure_ascii=False).encode("utf-8"), binario


# ------------------------------------------------------------------ acessores
def ler_acessor(doc: dict, binario: bytes, indice: int) -> list[tuple]:
    """Le um accessor do glTF como lista de tuplas (ja desintercalada)."""
    acessor = (doc.get("accessors") or [])[indice]
    quantidade = acessor.get("count", 0)
    itens = ITENS_POR_TIPO.get(acessor.get("type", "SCALAR"), 1)
    formato, tamanho = TIPOS_COMPONENTE[acessor["componentType"]]
    if "bufferView" not in acessor:
        return [tuple([0] * itens)] * quantidade

    view = (doc.get("bufferViews") or [])[acessor["bufferView"]]
    inicio = view.get("byteOffset", 0) + acessor.get("byteOffset", 0)
    passo = view.get("byteStride") or itens * tamanho
    desempacotar_item = struct.Struct("<" + formato * itens).unpack_from
    return [desempacotar_item(binario, inicio + i * passo) for i in range(quantidade)]


def _multiplicar(a: list[float], b: list[float]) -> list[float]:
    """Produto de matrizes 4x4 em ordem de coluna (convencao glTF)."""
    fora = [0.0] * 16
    for c in range(4):
        for l in range(4):
            fora[c * 4 + l] = sum(a[k * 4 + l] * b[c * 4 + k] for k in range(4))
    return fora


def _matriz_do_no(no: dict) -> list[float]:
    if "matrix" in no:
        return [float(v) for v in no["matrix"]]
    t = no.get("translation") or [0, 0, 0]
    r = no.get("rotation") or [0, 0, 0, 1]
    s = no.get("scale") or [1, 1, 1]
    x, y, z, w = (float(v) for v in r)
    rot = [
        1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w), 0,
        2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w), 0,
        2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y), 0,
        0, 0, 0, 1,
    ]
    for c in range(3):
        for l in range(3):
            rot[c * 4 + l] *= float(s[c])
    rot[12], rot[13], rot[14] = float(t[0]), float(t[1]), float(t[2])
    return rot


def _aplicar(m: list[float], p: tuple) -> tuple[float, float, float]:
    x, y, z = float(p[0]), float(p[1]), float(p[2])
    return (
        m[0] * x + m[4] * y + m[8] * z + m[12],
        m[1] * x + m[5] * y + m[9] * z + m[13],
        m[2] * x + m[6] * y + m[10] * z + m[14],
    )


def _girar(m: list[float], p: tuple) -> tuple[float, float, float]:
    x, y, z = float(p[0]), float(p[1]), float(p[2])
    return (
        m[0] * x + m[4] * y + m[8] * z,
        m[1] * x + m[5] * y + m[9] * z,
        m[2] * x + m[6] * y + m[10] * z,
    )


def percorrer_nos(doc: dict):
    """Gera (no, matriz de mundo) para a cena padrao, ja com a hierarquia."""
    nos = doc.get("nodes") or []
    cena = (doc.get("scenes") or [{}])[doc.get("scene", 0) or 0]
    pilha = [(i, _matriz_do_no(nos[i])) for i in reversed(cena.get("nodes") or [])]
    while pilha:
        indice, mundo = pilha.pop()
        no = nos[indice]
        yield no, mundo
        for filho in reversed(no.get("children") or []):
            pilha.append((filho, _multiplicar(mundo, _matriz_do_no(nos[filho]))))


def para_obj(doc: dict, binario: bytes, nome: str) -> tuple[str, str, dict[str, bytes]]:
    """(.obj, .mtl, texturas) do documento ja mesclado.

    Geometria com posicao, normal e UV; materiais com cor base, opacidade e
    mapa difuso. Malhas comprimidas com Draco sao puladas (nao ha decodificador
    puro em Python aqui) e ficam registradas em comentario no .obj.
    """
    linhas = [f"# ARCZ Earth · recorte {nome}", f"mtllib {nome}.mtl"]
    materiais_usados: dict[int, str] = {}
    texturas: dict[str, bytes] = {}
    puladas = 0
    base_v = base_vn = base_vt = 1

    for ordem, (no, mundo) in enumerate(percorrer_nos(doc)):
        if "mesh" not in no:
            continue
        malha = (doc.get("meshes") or [])[no["mesh"]]
        for prim in malha.get("primitives") or []:
            if (prim.get("extensions") or {}).get("KHR_draco_mesh_compression"):
                puladas += 1
                continue
            if prim.get("mode", 4) != 4:
                continue
            atributos = prim.get("attributes") or {}
            if "POSITION" not in atributos:
                continue

            posicoes = ler_acessor(doc, binario, atributos["POSITION"])
            normais = ler_acessor(doc, binario, atributos["NORMAL"]) if "NORMAL" in atributos else []
            uvs = ler_acessor(doc, binario, atributos["TEXCOORD_0"]) if "TEXCOORD_0" in atributos else []
            if "indices" in prim:
                indices = [i[0] for i in ler_acessor(doc, binario, prim["indices"])]
            else:
                indices = list(range(len(posicoes)))

            linhas.append(f"g {no.get('name', 'malha')}_{ordem}")
            material = prim.get("material")
            if material is not None:
                nome_mat = materiais_usados.setdefault(material, f"mat_{material}")
                linhas.append(f"usemtl {nome_mat}")

            for p in posicoes:
                x, y, z = _aplicar(mundo, p)
                linhas.append(f"v {x:.6f} {y:.6f} {z:.6f}")
            for n in normais:
                x, y, z = _girar(mundo, n)
                comprimento = math.sqrt(x * x + y * y + z * z) or 1.0
                linhas.append(f"vn {x / comprimento:.6f} {y / comprimento:.6f} {z / comprimento:.6f}")
            for uv in uvs:
                linhas.append(f"vt {uv[0]:.6f} {1.0 - uv[1]:.6f}")

            for i in range(0, len(indices) - 2, 3):
                face = []
                for k in range(3):
                    v = indices[i + k]
                    parte = str(base_v + v)
                    if uvs or normais:
                        parte += "/" + (str(base_vt + v) if uvs else "")
                    if normais:
                        parte += "/" + str(base_vn + v)
                    face.append(parte)
                linhas.append("f " + " ".join(face))

            base_v += len(posicoes)
            base_vn += len(normais)
            base_vt += len(uvs)

    if puladas:
        linhas.insert(1, f"# {puladas} malha(s) comprimida(s) com Draco nao entraram no OBJ")

    mtl = ["# ARCZ Earth"]
    for indice, nome_mat in sorted(materiais_usados.items()):
        material = (doc.get("materials") or [])[indice]
        pbr = material.get("pbrMetallicRoughness") or {}
        cor = pbr.get("baseColorFactor") or [1, 1, 1, 1]
        mtl.append(f"newmtl {nome_mat}")
        mtl.append(f"Kd {cor[0]:.4f} {cor[1]:.4f} {cor[2]:.4f}")
        mtl.append("Ka 0 0 0")
        mtl.append(f"Ns {max(1.0, (1.0 - float(pbr.get('roughnessFactor', 1.0))) * 250):.1f}")
        if len(cor) > 3 and cor[3] < 1:
            mtl.append(f"d {cor[3]:.4f}")
        textura = (pbr.get("baseColorTexture") or {}).get("index")
        if textura is not None:
            arquivo = _extrair_textura(doc, binario, textura, nome, texturas)
            if arquivo:
                mtl.append(f"map_Kd {arquivo}")
        mtl.append("")

    return "\n".join(linhas) + "\n", "\n".join(mtl) + "\n", texturas


def _extrair_textura(doc, binario, indice_textura, nome, texturas) -> str | None:
    try:
        textura = (doc.get("textures") or [])[indice_textura]
        fonte = textura.get("source")
        if fonte is None:
            for ext in (textura.get("extensions") or {}).values():
                if isinstance(ext, dict) and isinstance(ext.get("source"), int):
                    fonte = ext["source"]
                    break
        if fonte is None:
            return None
        imagem = (doc.get("images") or [])[fonte]
        view = (doc.get("bufferViews") or [])[imagem["bufferView"]]
    except (IndexError, KeyError, TypeError):
        return None
    inicio = view.get("byteOffset", 0)
    dados = binario[inicio : inicio + view.get("byteLength", 0)]
    ext = EXTENSAO_POR_MIME.get(imagem.get("mimeType", "image/png"), ".png")
    arquivo = f"{nome}_tex{fonte}{ext}"
    texturas[arquivo] = dados
    return arquivo


# ------------------------------------------------------------------- terreno
def geodetico_para_ecef(lat: float, lon: float, alt: float) -> tuple[float, float, float]:
    rlat, rlon = math.radians(lat), math.radians(lon)
    sen = math.sin(rlat)
    n = _A / math.sqrt(1 - _E2 * sen * sen)
    return (
        (n + alt) * math.cos(rlat) * math.cos(rlon),
        (n + alt) * math.cos(rlat) * math.sin(rlon),
        (n * (1 - _E2) + alt) * sen,
    )


def ecef_para_enu(p, origem, lat0: float, lon0: float) -> tuple[float, float, float]:
    dx, dy, dz = p[0] - origem[0], p[1] - origem[1], p[2] - origem[2]
    rlat, rlon = math.radians(lat0), math.radians(lon0)
    sl, cl = math.sin(rlon), math.cos(rlon)
    sf, cf = math.sin(rlat), math.cos(rlat)
    return (
        -sl * dx + cl * dy,
        -sf * cl * dx - sf * sl * dy + cf * dz,
        cf * cl * dx + cf * sl * dy + sf * dz,
    )


def tile_de(lon: float, lat: float, zoom: int) -> tuple[float, float]:
    """Coordenada (x, y) continua no esquema de tiles Web Mercator."""
    n = 2.0**zoom
    rlat = math.radians(max(-85.05, min(85.05, lat)))
    x = (lon + 180.0) / 360.0 * n
    y = (1.0 - math.log(math.tan(rlat) + 1.0 / math.cos(rlat)) / math.pi) / 2.0 * n
    return x, y


def ponto_no_poligono(lon: float, lat: float, poligono: list[dict]) -> bool:
    dentro = False
    quantidade = len(poligono)
    for i in range(quantidade):
        j = (i - 1) % quantidade
        xi, yi = poligono[i]["lon"], poligono[i]["lat"]
        xj, yj = poligono[j]["lon"], poligono[j]["lat"]
        if (yi > lat) != (yj > lat):
            corte = (xj - xi) * (lat - yi) / ((yj - yi) or 1e-12) + xi
            if lon < corte:
                dentro = not dentro
    return dentro


def altura_terrarium(pixel: tuple[int, int, int]) -> float:
    return (pixel[0] * 256 + pixel[1] + pixel[2] / 256.0) - 32768.0


def malha_terreno(
    poligono: list[dict],
    centro: dict,
    ler_tile,
    resolucao: int = 80,
    zoom: int = 14,
    nome: str = "relevo",
) -> dict | None:
    """Recorta o relevo dentro do perimetro e devolve um item para `mesclar`.

    `ler_tile(z, x, y)` deve devolver os bytes do PNG Terrarium (ou None).
    O resultado ja sai em Y-up: (leste, altura, -norte) relativo ao `centro`.
    Devolve None quando nao ha tile disponivel ou o perimetro e degenerado.
    """
    try:
        from PIL import Image
    except ImportError:
        return None
    if len(poligono) < 3 or resolucao < 2:
        return None

    lons = [p["lon"] for p in poligono]
    lats = [p["lat"] for p in poligono]
    passos = max(2, min(int(resolucao), 400))
    origem = geodetico_para_ecef(centro["lat"], centro["lon"], centro.get("alt", 0.0))

    tiles: dict[tuple[int, int], object] = {}

    def amostrar(lon: float, lat: float) -> float | None:
        tx, ty = tile_de(lon, lat, zoom)
        chave = (int(tx), int(ty))
        if chave not in tiles:
            dados = ler_tile(zoom, chave[0], chave[1])
            if not dados:
                tiles[chave] = None
            else:
                try:
                    import io

                    tiles[chave] = Image.open(io.BytesIO(dados)).convert("RGB")
                except Exception:
                    tiles[chave] = None
        imagem = tiles[chave]
        if imagem is None:
            return None
        largura, altura = imagem.size
        px = min(largura - 1, max(0, int((tx - chave[0]) * largura)))
        py = min(altura - 1, max(0, int((ty - chave[1]) * altura)))
        return altura_terrarium(imagem.getpixel((px, py)))

    vertices: list[tuple[float, float, float]] = []
    indice_de: dict[tuple[int, int], int] = {}

    for i in range(passos + 1):
        lat = min(lats) + (max(lats) - min(lats)) * i / passos
        for j in range(passos + 1):
            lon = min(lons) + (max(lons) - min(lons)) * j / passos
            altura = amostrar(lon, lat)
            if altura is None:
                continue
            leste, norte, cima = ecef_para_enu(
                geodetico_para_ecef(lat, lon, altura), origem, centro["lat"], centro["lon"]
            )
            indice_de[(i, j)] = len(vertices)
            vertices.append((leste, cima, -norte))

    if len(vertices) < 3:
        return None

    def celula_no_perimetro(i: int, j: int) -> bool:
        lat = min(lats) + (max(lats) - min(lats)) * (i + 0.5) / passos
        lon = min(lons) + (max(lons) - min(lons)) * (j + 0.5) / passos
        return ponto_no_poligono(lon, lat, poligono)

    indices: list[int] = []
    for i in range(passos):
        for j in range(passos):
            cantos = [(i, j), (i, j + 1), (i + 1, j + 1), (i + 1, j)]
            if not all(c in indice_de for c in cantos):
                continue
            if not celula_no_perimetro(i, j):
                continue
            a, b, c, d = (indice_de[k] for k in cantos)
            indices.extend([a, b, c, a, c, d])

    if not indices:
        return None

    posicoes = bytearray()
    for v in vertices:
        posicoes.extend(struct.pack("<fff", *v))
    corpo_indices = bytearray()
    formato = "<I" if len(vertices) > 65535 else "<H"
    for i in indices:
        corpo_indices.extend(struct.pack(formato, i))
    while len(corpo_indices) % 4:
        corpo_indices.append(0)

    binario = bytes(posicoes + corpo_indices)
    xs = [v[0] for v in vertices]
    ys = [v[1] for v in vertices]
    zs = [v[2] for v in vertices]

    doc = {
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": nome, "mesh": 0}],
        "meshes": [{"name": nome, "primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0}]}],
        "materials": [
            {
                "name": "terreno",
                "doubleSided": True,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.42, 0.45, 0.38, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.95,
                },
            }
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": len(vertices),
                "type": "VEC3",
                "min": [min(xs), min(ys), min(zs)],
                "max": [max(xs), max(ys), max(zs)],
            },
            {
                "bufferView": 1,
                "componentType": 5125 if len(vertices) > 65535 else 5123,
                "count": len(indices),
                "type": "SCALAR",
            },
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": len(posicoes), "target": 34962},
            {"buffer": 0, "byteOffset": len(posicoes), "byteLength": len(corpo_indices), "target": 34963},
        ],
        "buffers": [{"byteLength": len(binario)}],
    }
    return {"doc": doc, "bin": binario, "nome": nome, "triangulos": len(indices) // 3}
