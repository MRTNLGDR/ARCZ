"""Testes da exportacao de recorte de cena (juncao de glTF, OBJ e relevo)."""

import io
import json
import math
import struct
import sys
import tempfile
import unittest
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ))

from PIL import Image  # noqa: E402

import arcz_export  # noqa: E402
import arcz_glb  # noqa: E402


def _png(cor) -> bytes:
    buf = io.BytesIO()
    Image.new("RGB", (4, 4), cor).save(buf, format="PNG")
    return buf.getvalue()


def triangulo_glb(escala: float = 1.0, com_textura: bool = False) -> bytes:
    """GLB minimo: um triangulo com POSITION, TEXCOORD_0, indices e material."""
    posicoes = struct.pack(
        "<9f",
        0.0, 0.0, 0.0,
        escala, 0.0, 0.0,
        0.0, escala, 0.0,
    )
    uvs = struct.pack("<6f", 0.0, 0.0, 1.0, 0.0, 0.0, 1.0)
    indices = struct.pack("<3H", 0, 1, 2) + b"\x00\x00"
    binario = bytearray(posicoes + uvs + indices)

    views = [
        {"buffer": 0, "byteOffset": 0, "byteLength": len(posicoes)},
        {"buffer": 0, "byteOffset": len(posicoes), "byteLength": len(uvs)},
        {"buffer": 0, "byteOffset": len(posicoes) + len(uvs), "byteLength": 6},
    ]
    doc = {
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "tri", "mesh": 0}],
        "meshes": [
            {
                "primitives": [
                    {"attributes": {"POSITION": 0, "TEXCOORD_0": 1}, "indices": 2, "material": 0}
                ]
            }
        ],
        "materials": [
            {"name": "m", "pbrMetallicRoughness": {"baseColorFactor": [1, 0.5, 0.25, 1]}}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
             "min": [0, 0, 0], "max": [escala, escala, 0]},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"},
        ],
        "bufferViews": views,
        "buffers": [{"byteLength": len(binario)}],
    }
    if com_textura:
        imagem = _png((10, 20, 30))
        while len(binario) % 4:
            binario.append(0)
        views.append({"buffer": 0, "byteOffset": len(binario), "byteLength": len(imagem)})
        binario.extend(imagem)
        doc["images"] = [{"bufferView": len(views) - 1, "mimeType": "image/png"}]
        doc["samplers"] = [{"magFilter": 9729}]
        doc["textures"] = [{"source": 0, "sampler": 0}]
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = {"index": 0}
        doc["buffers"][0]["byteLength"] = len(binario)
    return arcz_glb.empacotar(doc, bytes(binario))


class TesteLeitura(unittest.TestCase):
    def test_carrega_glb_com_buffer_embutido(self):
        with tempfile.TemporaryDirectory() as tmp:
            caminho = Path(tmp) / "a.glb"
            caminho.write_bytes(triangulo_glb())
            doc, binario = arcz_export.carregar_documento(caminho)
        self.assertEqual(len(doc["buffers"]), 1)
        self.assertNotIn("uri", doc["buffers"][0])
        posicoes = arcz_export.ler_acessor(doc, binario, 0)
        self.assertEqual(posicoes[1], (1.0, 0.0, 0.0))

    def test_gltf_com_bin_e_imagem_externos_vira_embutido(self):
        with tempfile.TemporaryDirectory() as tmp:
            pasta = Path(tmp)
            glb = triangulo_glb()
            doc, binario = arcz_glb.desempacotar(glb)
            (pasta / "malha.bin").write_bytes(binario)
            (pasta / "cor.png").write_bytes(_png((1, 2, 3)))
            doc["buffers"] = [{"byteLength": len(binario), "uri": "malha.bin"}]
            doc["images"] = [{"uri": "cor.png"}]
            (pasta / "a.gltf").write_text(json.dumps(doc), encoding="utf-8")

            lido, bin_lido = arcz_export.carregar_documento(pasta / "a.gltf", pasta)

        self.assertNotIn("uri", lido["images"][0])
        self.assertEqual(lido["images"][0]["mimeType"], "image/png")
        self.assertEqual(arcz_export.ler_acessor(lido, bin_lido, 0)[1], (1.0, 0.0, 0.0))
        view = lido["bufferViews"][lido["images"][0]["bufferView"]]
        inicio = view["byteOffset"]
        self.assertEqual(bin_lido[inicio : inicio + 4], b"\x89PNG")

    def test_uri_fora_da_raiz_e_recusada(self):
        with tempfile.TemporaryDirectory() as tmp:
            pasta = Path(tmp) / "dentro"
            pasta.mkdir()
            doc = {"asset": {"version": "2.0"}, "buffers": [{"byteLength": 4, "uri": "../fora.bin"}]}
            (Path(tmp) / "fora.bin").write_bytes(b"1234")
            (pasta / "a.gltf").write_text(json.dumps(doc), encoding="utf-8")
            with self.assertRaises(ValueError):
                arcz_export.carregar_documento(pasta / "a.gltf", pasta)


class TesteMescla(unittest.TestCase):
    def _dois_itens(self):
        with tempfile.TemporaryDirectory() as tmp:
            a = Path(tmp) / "a.glb"
            b = Path(tmp) / "b.glb"
            a.write_bytes(triangulo_glb(1.0, com_textura=True))
            b.write_bytes(triangulo_glb(2.0, com_textura=True))
            doc_a, bin_a = arcz_export.carregar_documento(a)
            doc_b, bin_b = arcz_export.carregar_documento(b)
        deslocado = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 10, 0, 0, 1]
        return [
            {"doc": doc_a, "bin": bin_a, "nome": "predio", "matriz": deslocado},
            {"doc": doc_b, "bin": bin_b, "nome": "peca"},
        ]

    def test_mescla_mantem_geometria_e_texturas_de_cada_item(self):
        doc, binario, _ = arcz_export.mesclar(self._dois_itens())

        self.assertEqual(len(doc["scenes"][0]["nodes"]), 2)
        self.assertEqual(len(doc["meshes"]), 2)
        self.assertEqual(len(doc["materials"]), 2)
        self.assertEqual(len(doc["textures"]), 2)
        # Cada material aponta para a SUA textura (indices remapeados).
        alvos = [m["pbrMetallicRoughness"]["baseColorTexture"]["index"] for m in doc["materials"]]
        self.assertEqual(sorted(alvos), [0, 1])
        self.assertEqual([doc["textures"][i]["source"] for i in alvos], [0, 1])

        # Geometria do segundo item continua com escala 2 depois do deslocamento.
        prim = doc["meshes"][1]["primitives"][0]
        posicoes = arcz_export.ler_acessor(doc, binario, prim["attributes"]["POSITION"])
        self.assertEqual(posicoes[1], (2.0, 0.0, 0.0))
        indices = arcz_export.ler_acessor(doc, binario, prim["indices"])
        self.assertEqual([i[0] for i in indices], [0, 1, 2])

    def test_matriz_do_item_vai_para_o_no_raiz(self):
        doc, _, _ = arcz_export.mesclar(self._dois_itens())
        embrulho = doc["nodes"][doc["scenes"][0]["nodes"][0]]
        self.assertEqual(embrulho["name"], "predio")
        self.assertEqual(embrulho["matrix"][12], 10)
        self.assertEqual(len(embrulho["children"]), 1)

    def test_glb_resultante_e_valido(self):
        doc, binario, _ = arcz_export.mesclar(self._dois_itens())
        glb = arcz_export.para_glb(doc, binario)
        relido, bin_relido = arcz_glb.desempacotar(glb)
        self.assertEqual(relido["asset"]["version"], "2.0")
        self.assertEqual(len(relido["meshes"]), 2)
        self.assertGreaterEqual(len(bin_relido), doc["buffers"][0]["byteLength"])

    def test_gltf_separado_aponta_para_o_bin(self):
        doc, binario, _ = arcz_export.mesclar(self._dois_itens())
        json_bytes, bin_bytes = arcz_export.para_gltf(doc, binario, "recorte.bin")
        lido = json.loads(json_bytes.decode("utf-8"))
        self.assertEqual(lido["buffers"][0]["uri"], "recorte.bin")
        self.assertEqual(len(bin_bytes), len(binario))


class TesteObj(unittest.TestCase):
    def test_obj_traz_vertices_no_lugar_do_mundo(self):
        with tempfile.TemporaryDirectory() as tmp:
            caminho = Path(tmp) / "a.glb"
            caminho.write_bytes(triangulo_glb(1.0, com_textura=True))
            doc, binario = arcz_export.carregar_documento(caminho)
        item = {"doc": doc, "bin": binario, "nome": "tri",
                "matriz": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 5, 0, 0, 1]}
        doc, binario, _ = arcz_export.mesclar([item])

        obj, mtl, texturas = arcz_export.para_obj(doc, binario, "recorte")
        vertices = [l for l in obj.splitlines() if l.startswith("v ")]
        self.assertEqual(len(vertices), 3)
        self.assertTrue(vertices[0].startswith("v 5.000000 0.000000 0.000000"))
        self.assertIn("f 1/1 2/2 3/3", obj)
        self.assertIn("newmtl mat_0", mtl)
        self.assertIn("Kd 1.0000 0.5000 0.2500", mtl)
        self.assertEqual(len(texturas), 1)
        arquivo, dados = next(iter(texturas.items()))
        self.assertIn(f"map_Kd {arquivo}", mtl)
        self.assertEqual(dados[:4], b"\x89PNG")


class TesteTerreno(unittest.TestCase):
    @staticmethod
    def _tile_plano(altura_m: float) -> bytes:
        valor = altura_m + 32768.0
        r = int(valor // 256)
        g = int(valor - r * 256)
        b = int(round((valor - r * 256 - g) * 256)) % 256
        buf = io.BytesIO()
        Image.new("RGB", (32, 32), (r, g, b)).save(buf, format="PNG")
        return buf.getvalue()

    def test_altura_terrarium(self):
        self.assertAlmostEqual(arcz_export.altura_terrarium((128, 0, 0)), 0.0)
        self.assertAlmostEqual(arcz_export.altura_terrarium((128, 100, 0)), 100.0)

    def test_ponto_no_poligono(self):
        quadrado = [
            {"lon": 0, "lat": 0}, {"lon": 1, "lat": 0},
            {"lon": 1, "lat": 1}, {"lon": 0, "lat": 1},
        ]
        self.assertTrue(arcz_export.ponto_no_poligono(0.5, 0.5, quadrado))
        self.assertFalse(arcz_export.ponto_no_poligono(1.5, 0.5, quadrado))

    def test_malha_do_perimetro_sai_em_y_up_e_no_terreno(self):
        centro = {"lat": -27.1545, "lon": -48.5022, "alt": 40.0}
        d = 0.0009  # ~100 m
        poligono = [
            {"lon": centro["lon"] - d, "lat": centro["lat"] - d},
            {"lon": centro["lon"] + d, "lat": centro["lat"] - d},
            {"lon": centro["lon"] + d, "lat": centro["lat"] + d},
            {"lon": centro["lon"] - d, "lat": centro["lat"] + d},
        ]
        item = arcz_export.malha_terreno(
            poligono, centro, lambda z, x, y: self._tile_plano(40.0), resolucao=8
        )
        self.assertIsNotNone(item)
        self.assertGreater(item["triangulos"], 0)

        doc, binario = item["doc"], item["bin"]
        posicoes = arcz_export.ler_acessor(doc, binario, 0)
        # Terreno plano na mesma cota do centro: y (cima) ~ 0 e extensao ~200 m.
        self.assertLess(max(abs(p[1]) for p in posicoes), 1.0)
        # 0.0018 grau de longitude a -27° de latitude ~ 178 m de leste a oeste.
        largura = max(p[0] for p in posicoes) - min(p[0] for p in posicoes)
        self.assertTrue(170 < largura < 190, largura)
        profundidade = max(p[2] for p in posicoes) - min(p[2] for p in posicoes)
        self.assertTrue(190 < profundidade < 210, profundidade)

    def test_sem_tile_nao_gera_malha(self):
        centro = {"lat": 0.0, "lon": 0.0, "alt": 0.0}
        poligono = [{"lon": 0, "lat": 0}, {"lon": 0.001, "lat": 0}, {"lon": 0.001, "lat": 0.001}]
        self.assertIsNone(
            arcz_export.malha_terreno(poligono, centro, lambda z, x, y: None, resolucao=4)
        )


class TesteGeodesia(unittest.TestCase):
    def test_enu_do_proprio_centro_e_zero(self):
        p = arcz_export.geodetico_para_ecef(-27.1545, -48.5022, 12.0)
        e, n, u = arcz_export.ecef_para_enu(p, p, -27.1545, -48.5022)
        self.assertAlmostEqual(math.sqrt(e * e + n * n + u * u), 0.0)

    def test_um_grau_de_longitude_vai_para_leste(self):
        origem = arcz_export.geodetico_para_ecef(0.0, 0.0, 0.0)
        alvo = arcz_export.geodetico_para_ecef(0.0, 0.001, 0.0)
        e, n, u = arcz_export.ecef_para_enu(alvo, origem, 0.0, 0.0)
        self.assertGreater(e, 100)
        self.assertLess(abs(n), 0.01)


if __name__ == "__main__":
    unittest.main()
