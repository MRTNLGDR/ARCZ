#!/usr/bin/env python3
"""Processamento de GLB do ARCZ: conversao de material e LOD real de textura.

Funcoes puras, sem HTTP, para poderem ser testadas isoladamente
(`python -m unittest tests_python.test_arcz_glb`).

Duas operacoes:

1. `converter_specular_glossiness` — o zenite.glb (SketchUp) so tem cor em
   `KHR_materials_pbrSpecularGlossiness`, extensao aposentada que o CesiumJS
   ignora; sem a conversao o predio aparece branco.
2. `reduzir_texturas` — LOD de verdade: reamostra as imagens embutidas no
   chunk BIN e reescreve os bufferViews. Antes desta versao a rota /glb-lod
   recebia `tex=` e devolvia o arquivo do mesmo tamanho.
"""

from __future__ import annotations

import io
import json
import struct

try:
    from PIL import Image

    PIL_DISPONIVEL = True
except ImportError:  # pragma: no cover - ambiente sem Pillow
    PIL_DISPONIVEL = False

MAGICA_GLTF = 0x46546C67
CHUNK_JSON = 0x4E4F534A
CHUNK_BIN = 0x004E4942


def desempacotar(glb: bytes) -> tuple[dict, bytes]:
    """Le um .glb binario e devolve (documento JSON, chunk BIN)."""
    if len(glb) < 12 or glb[:4] != b"glTF":
        raise ValueError("nao e um .glb")
    pedacos: list[tuple[int, bytes]] = []
    off = 12
    while off + 8 <= len(glb):
        tam, tipo = struct.unpack("<II", glb[off : off + 8])
        off += 8
        pedacos.append((tipo, glb[off : off + tam]))
        off += tam
    if not pedacos or pedacos[0][0] != CHUNK_JSON:
        raise ValueError("primeiro pedaco nao e JSON")
    doc = json.loads(pedacos[0][1].decode("utf-8"))
    binario = b""
    for tipo, dados in pedacos[1:]:
        if tipo == CHUNK_BIN:
            binario = dados
            break
    return doc, binario


def empacotar(doc: dict, binario: bytes) -> bytes:
    """Monta um .glb valido a partir do documento JSON e do chunk BIN."""
    json_bytes = json.dumps(doc, separators=(",", ":")).encode("utf-8")
    json_bytes += b" " * (-len(json_bytes) % 4)
    corpo = struct.pack("<II", len(json_bytes), CHUNK_JSON) + json_bytes
    if binario:
        bin_bytes = binario + b"\x00" * (-len(binario) % 4)
        corpo += struct.pack("<II", len(bin_bytes), CHUNK_BIN) + bin_bytes
    return struct.pack("<III", MAGICA_GLTF, 2, 12 + len(corpo)) + corpo


def converter_specular_glossiness(doc: dict) -> int:
    """KHR_materials_pbrSpecularGlossiness -> pbrMetallicRoughness (no lugar).

    Devolve quantos materiais foram convertidos.
    """
    convertidos = 0
    for mat in doc.get("materials", []):
        extensoes = mat.get("extensions")
        if not extensoes:
            continue
        sg = extensoes.pop("KHR_materials_pbrSpecularGlossiness", None)
        if sg is None:
            continue
        pbr = mat.setdefault("pbrMetallicRoughness", {})
        pbr["baseColorFactor"] = sg.get("diffuseFactor", [1, 1, 1, 1])
        if "diffuseTexture" in sg:
            pbr["baseColorTexture"] = sg["diffuseTexture"]
        pbr["metallicFactor"] = 0.0
        pbr["roughnessFactor"] = round(1.0 - float(sg.get("glossinessFactor", 0.5)), 4)
        if not extensoes:
            del mat["extensions"]
        convertidos += 1

    if convertidos:
        usadas = [e for e in doc.get("extensionsUsed", []) if e != "KHR_materials_pbrSpecularGlossiness"]
        if usadas:
            doc["extensionsUsed"] = usadas
        else:
            doc.pop("extensionsUsed", None)
        requeridas = [
            e for e in doc.get("extensionsRequired", []) if e != "KHR_materials_pbrSpecularGlossiness"
        ]
        if requeridas:
            doc["extensionsRequired"] = requeridas
        else:
            doc.pop("extensionsRequired", None)
    return convertidos


def _reamostrar(bruto: bytes, max_tex: int) -> tuple[bytes, str] | None:
    """Reduz uma imagem se ela passar de `max_tex` no maior lado."""
    if not PIL_DISPONIVEL:
        return None
    try:
        with Image.open(io.BytesIO(bruto)) as im:
            largura, altura = im.size
            if max(largura, altura) <= max_tex:
                return None
            fator = max_tex / float(max(largura, altura))
            novo_tam = (max(1, round(largura * fator)), max(1, round(altura * fator)))
            tem_alfa = im.mode in ("RGBA", "LA") or (im.mode == "P" and "transparency" in im.info)
            reduzida = im.convert("RGBA" if tem_alfa else "RGB").resize(novo_tam, Image.LANCZOS)
    except Exception:
        return None

    saida = io.BytesIO()
    if tem_alfa:
        reduzida.save(saida, format="PNG", optimize=True)
        return saida.getvalue(), "image/png"
    reduzida.save(saida, format="JPEG", quality=85, optimize=True)
    return saida.getvalue(), "image/jpeg"


def reduzir_texturas(doc: dict, binario: bytes, max_tex: int) -> tuple[bytes, int]:
    """Reamostra as imagens embutidas e reescreve o chunk BIN.

    Devolve (novo binario, quantidade de texturas reduzidas). O documento e
    alterado no lugar (bufferViews, mimeType e byteLength do buffer 0).
    """
    imagens = doc.get("images") or []
    views = doc.get("bufferViews") or []
    if not imagens or not views or not binario or max_tex <= 0:
        return binario, 0

    substituicoes: dict[int, bytes] = {}
    for img in imagens:
        indice = img.get("bufferView")
        if indice is None or indice >= len(views):
            continue
        view = views[indice]
        if view.get("buffer", 0) != 0:
            continue
        inicio = view.get("byteOffset", 0)
        bruto = binario[inicio : inicio + view.get("byteLength", 0)]
        resultado = _reamostrar(bruto, max_tex)
        if resultado is None:
            continue
        substituicoes[indice] = resultado[0]
        img["mimeType"] = resultado[1]

    if not substituicoes:
        return binario, 0

    saida = bytearray()
    for i, view in enumerate(views):
        if view.get("buffer", 0) != 0:
            continue
        dados = substituicoes.get(i)
        if dados is None:
            inicio = view.get("byteOffset", 0)
            dados = binario[inicio : inicio + view.get("byteLength", 0)]
        resto = len(saida) % 4
        if resto:
            saida.extend(b"\x00" * (4 - resto))
        view["byteOffset"] = len(saida)
        view["byteLength"] = len(dados)
        saida.extend(dados)

    if doc.get("buffers"):
        doc["buffers"][0]["byteLength"] = len(saida)
    return bytes(saida), len(substituicoes)


def processar(glb: bytes, max_tex: int | None = None) -> bytes:
    """Converte material e, quando `max_tex` for dado, aplica LOD de textura.

    Devolve o .glb original quando nada muda, para nao pagar reserializacao.
    """
    doc, binario = desempacotar(glb)
    convertidos = converter_specular_glossiness(doc)
    reduzidas = 0
    if max_tex:
        binario, reduzidas = reduzir_texturas(doc, binario, max_tex)
    if not convertidos and not reduzidas:
        return glb
    return empacotar(doc, binario)
