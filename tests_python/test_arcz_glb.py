"""Testes do processamento de GLB do ARCZ (conversao de material + LOD real)."""

import io
import json
import struct
import sys
import unittest
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ))

from PIL import Image  # noqa: E402

import arcz_glb  # noqa: E402


def _png(largura: int, altura: int, cor=(200, 30, 30)) -> bytes:
    buf = io.BytesIO()
    Image.new("RGB", (largura, altura), cor).save(buf, format="PNG")
    return buf.getvalue()


def _png_alfa(largura: int, altura: int) -> bytes:
    buf = io.BytesIO()
    Image.new("RGBA", (largura, altura), (10, 200, 10, 128)).save(buf, format="PNG")
    return buf.getvalue()


def montar_glb(imagens: list[bytes], malha: bytes = b"", com_sg: bool = True) -> bytes:
    """Monta um .glb minimo com N imagens embutidas e um bufferView de malha."""
    binario = bytearray()
    views = []

    if malha:
        views.append({"buffer": 0, "byteOffset": 0, "byteLength": len(malha)})
        binario.extend(malha)

    for img in imagens:
        resto = len(binario) % 4
        if resto:
            binario.extend(b"\x00" * (4 - resto))
        views.append({"buffer": 0, "byteOffset": len(binario), "byteLength": len(img)})
        binario.extend(img)

    inicio_img = 1 if malha else 0
    doc = {
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": len(binario)}],
        "bufferViews": views,
        "images": [
            {"bufferView": inicio_img + i, "mimeType": "image/png"} for i in range(len(imagens))
        ],
        "materials": [],
    }
    if com_sg:
        doc["extensionsUsed"] = ["KHR_materials_pbrSpecularGlossiness"]
        doc["materials"] = [
            {
                "name": "fachada",
                "extensions": {
                    "KHR_materials_pbrSpecularGlossiness": {
                        "diffuseFactor": [0.8, 0.6, 0.4, 1.0],
                        "diffuseTexture": {"index": 0},
                        "glossinessFactor": 0.25,
                    }
                },
            }
        ]
    return arcz_glb.empacotar(doc, bytes(binario))


class TesteEmpacotamento(unittest.TestCase):
    def test_ida_e_volta_preserva_json_e_binario(self):
        glb = montar_glb([_png(8, 8)], malha=b"\x01\x02\x03\x04" * 4, com_sg=False)
        doc, binario = arcz_glb.desempacotar(glb)
        self.assertEqual(doc["asset"]["version"], "2.0")
        self.assertEqual(binario[:4], b"\x01\x02\x03\x04")
        self.assertEqual(arcz_glb.desempacotar(arcz_glb.empacotar(doc, binario))[1], binario)

    def test_cabecalho_valido(self):
        glb = montar_glb([_png(8, 8)])
        magica, versao, tamanho = struct.unpack("<III", glb[:12])
        self.assertEqual(magica, arcz_glb.MAGICA_GLTF)
        self.assertEqual(versao, 2)
        self.assertEqual(tamanho, len(glb))

    def test_arquivo_invalido_levanta(self):
        with self.assertRaises(ValueError):
            arcz_glb.desempacotar(b"nao eh glb")


class TesteConversaoMaterial(unittest.TestCase):
    def test_converte_specular_glossiness_em_metallic_roughness(self):
        doc, _ = arcz_glb.desempacotar(montar_glb([_png(8, 8)]))
        self.assertEqual(arcz_glb.converter_specular_glossiness(doc), 1)
        pbr = doc["materials"][0]["pbrMetallicRoughness"]
        self.assertEqual(pbr["baseColorFactor"], [0.8, 0.6, 0.4, 1.0])
        self.assertEqual(pbr["baseColorTexture"], {"index": 0})
        self.assertEqual(pbr["metallicFactor"], 0.0)
        self.assertAlmostEqual(pbr["roughnessFactor"], 0.75)
        self.assertNotIn("extensions", doc["materials"][0])
        self.assertNotIn("extensionsUsed", doc)

    def test_sem_specular_glossiness_nao_muda_nada(self):
        doc, _ = arcz_glb.desempacotar(montar_glb([_png(8, 8)], com_sg=False))
        self.assertEqual(arcz_glb.converter_specular_glossiness(doc), 0)

    def test_processar_sem_mudanca_devolve_bytes_identicos(self):
        glb = montar_glb([_png(16, 16)], com_sg=False)
        self.assertEqual(arcz_glb.processar(glb, 2048), glb)


class TesteLodDeTextura(unittest.TestCase):
    def test_reduz_textura_grande_e_encolhe_o_arquivo(self):
        glb = montar_glb([_png(1024, 1024)], com_sg=False)
        saida = arcz_glb.processar(glb, 256)
        self.assertLess(len(saida), len(glb))

        doc, binario = arcz_glb.desempacotar(saida)
        view = doc["bufferViews"][doc["images"][0]["bufferView"]]
        bruto = binario[view["byteOffset"] : view["byteOffset"] + view["byteLength"]]
        with Image.open(io.BytesIO(bruto)) as im:
            self.assertEqual(im.size, (256, 256))

    def test_textura_menor_que_o_limite_nao_e_tocada(self):
        glb = montar_glb([_png(128, 128)], com_sg=False)
        self.assertEqual(arcz_glb.processar(glb, 1024), glb)

    def test_aspecto_nao_quadrado_e_preservado(self):
        glb = montar_glb([_png(800, 400)], com_sg=False)
        doc, binario = arcz_glb.desempacotar(arcz_glb.processar(glb, 200))
        view = doc["bufferViews"][doc["images"][0]["bufferView"]]
        bruto = binario[view["byteOffset"] : view["byteOffset"] + view["byteLength"]]
        with Image.open(io.BytesIO(bruto)) as im:
            self.assertEqual(im.size, (200, 100))

    def test_textura_com_alfa_continua_png(self):
        glb = montar_glb([_png_alfa(512, 512)], com_sg=False)
        doc, binario = arcz_glb.desempacotar(arcz_glb.processar(glb, 64))
        self.assertEqual(doc["images"][0]["mimeType"], "image/png")
        view = doc["bufferViews"][doc["images"][0]["bufferView"]]
        bruto = binario[view["byteOffset"] : view["byteOffset"] + view["byteLength"]]
        with Image.open(io.BytesIO(bruto)) as im:
            self.assertEqual(im.mode, "RGBA")

    def test_bufferview_de_malha_sobrevive_intacto(self):
        malha = bytes(range(64))
        glb = montar_glb([_png(512, 512)], malha=malha, com_sg=False)
        doc, binario = arcz_glb.desempacotar(arcz_glb.processar(glb, 64))
        view = doc["bufferViews"][0]
        recuperada = binario[view["byteOffset"] : view["byteOffset"] + view["byteLength"]]
        self.assertEqual(recuperada, malha)

    def test_offsets_ficam_alinhados_e_dentro_do_buffer(self):
        glb = montar_glb([_png(512, 512), _png(600, 600, (10, 10, 200))], malha=bytes(40))
        doc, binario = arcz_glb.desempacotar(arcz_glb.processar(glb, 128))
        for view in doc["bufferViews"]:
            self.assertEqual(view["byteOffset"] % 4, 0)
            self.assertLessEqual(view["byteOffset"] + view["byteLength"], len(binario))
        self.assertEqual(doc["buffers"][0]["byteLength"], len(binario))

    def test_lod_menor_gera_arquivo_menor_que_lod_maior(self):
        glb = montar_glb([_png(1024, 1024)], com_sg=False)
        self.assertLess(len(arcz_glb.processar(glb, 128)), len(arcz_glb.processar(glb, 512)))

    def test_converte_material_e_reduz_textura_na_mesma_passada(self):
        glb = montar_glb([_png(1024, 1024)])
        doc, _ = arcz_glb.desempacotar(arcz_glb.processar(glb, 128))
        self.assertIn("pbrMetallicRoughness", doc["materials"][0])
        self.assertNotIn("extensions", doc["materials"][0])
        self.assertEqual(doc["images"][0]["mimeType"], "image/jpeg")


class TesteModeloReal(unittest.TestCase):
    """Roda so quando o zenite.glb esta no disco."""

    def setUp(self):
        self.modelo = RAIZ / "modelos" / "zenite.glb"
        if not self.modelo.is_file():
            self.skipTest("modelos/zenite.glb ausente")

    def test_zenite_perde_specular_glossiness_e_encolhe_com_lod(self):
        bruto = self.modelo.read_bytes()
        doc_original, _ = arcz_glb.desempacotar(bruto)
        materiais_sg = [
            m
            for m in doc_original.get("materials", [])
            if "KHR_materials_pbrSpecularGlossiness" in m.get("extensions", {})
        ]
        self.assertGreater(len(materiais_sg), 0, "o modelo real deveria ter materiais SG")

        saida = arcz_glb.processar(bruto, 256)
        doc, _ = arcz_glb.desempacotar(saida)
        for mat in doc.get("materials", []):
            self.assertNotIn("KHR_materials_pbrSpecularGlossiness", mat.get("extensions", {}))
            self.assertIn("pbrMetallicRoughness", mat)
        self.assertLess(len(saida), len(bruto))


if __name__ == "__main__":
    unittest.main(verbosity=2)
