"""Testes das rotas HTTP do servidor do ARCZ, contra um servidor de verdade.

Sobe o `Handler` real numa porta efemera e fala HTTP com ele. Os arquivos de
estado do usuario (teste/projeto.json, lib_thumbs/) sao trocados por temporarios
durante os testes para nao sujar o projeto.
"""

import base64
import http.server
import io
import json
import shutil
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ))

from PIL import Image  # noqa: E402

import arcz_glb  # noqa: E402
import servidor  # noqa: E402
from test_arcz_glb import montar_glb  # noqa: E402
from test_arcz_export import triangulo_glb  # noqa: E402


class BaseServidor(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), servidor.Handler)
        cls.porta = cls.httpd.server_address[1]
        cls.thread = threading.Thread(target=cls.httpd.serve_forever, daemon=True)
        cls.thread.start()
        cls.tmp = Path(tempfile.mkdtemp(prefix="arcz_teste_"))
        # Area temporaria DENTRO da raiz: as rotas de GLB so aceitam caminho do projeto.
        cls.tmp_raiz = RAIZ / "tests_python" / "_tmp"
        cls.tmp_raiz.mkdir(parents=True, exist_ok=True)

    @classmethod
    def tearDownClass(cls):
        cls.httpd.shutdown()
        cls.httpd.server_close()
        shutil.rmtree(cls.tmp, ignore_errors=True)
        shutil.rmtree(cls.tmp_raiz, ignore_errors=True)

    def url(self, rota):
        return f"http://127.0.0.1:{self.porta}{rota}"

    def get(self, rota):
        with urllib.request.urlopen(self.url(rota), timeout=15) as r:
            return r.status, r.read(), r.headers  # Message: chave insensivel a caixa

    def get_json(self, rota):
        codigo, corpo, _ = self.get(rota)
        return codigo, json.loads(corpo.decode("utf-8"))

    def post_json(self, rota, obj):
        req = urllib.request.Request(
            self.url(rota),
            data=json.dumps(obj).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=15) as r:
            return r.status, json.loads(r.read().decode("utf-8"))


class TesteRotasBasicas(BaseServidor):
    def test_raiz_entrega_o_index_do_visualizador(self):
        codigo, corpo, cabecalhos = self.get("/")
        self.assertEqual(codigo, 200)
        self.assertIn("text/html", cabecalhos["Content-Type"])
        self.assertIn(b"cesiumContainer", corpo)
        self.assertIn(b"./app/main.js", corpo)

    def test_modulos_do_app_sao_servidos_como_javascript(self):
        for modulo in ("main", "estado", "ui", "cena", "camera", "ambiente", "lib", "gizmo", "historico", "icones"):
            codigo, corpo, cabecalhos = self.get(f"/app/{modulo}.js")
            self.assertEqual(codigo, 200, modulo)
            self.assertEqual(cabecalhos["Content-Type"], "text/javascript", modulo)
            self.assertTrue(corpo, modulo)

    def test_rota_post_desconhecida_devolve_404(self):
        with self.assertRaises(urllib.error.HTTPError) as ctx:
            self.post_json("/api/inexistente", {})
        self.assertEqual(ctx.exception.code, 404)


class TesteProjeto(BaseServidor):
    def setUp(self):
        self.projeto_original = servidor.PROJETO
        servidor.PROJETO = self.tmp / "projeto.json"

    def tearDown(self):
        servidor.PROJETO = self.projeto_original

    def test_projeto_ausente_devolve_esqueleto_com_takes_em_lista(self):
        codigo, dados = self.get_json("/api/projeto")
        self.assertEqual(codigo, 200)
        self.assertIsInstance(dados["takes"], list)
        self.assertIsInstance(dados["pecas"], list)

    def test_salvar_e_reler_preserva_pecas_e_takes(self):
        estado = {
            "versao": 1,
            "posicao": {"lat": -27.1545, "lon": -48.5022, "rumo": 119},
            "pecas": [{"id": "peca_1", "nome": "Sofa", "url": "/biblioteca/sofa/sofa.glb"}],
            "takes": [{"id": "take_1", "nome": "Fachada"}],
        }
        codigo, resposta = self.post_json("/api/projeto", estado)
        self.assertEqual(codigo, 200)
        self.assertTrue(resposta["ok"])

        _, lido = self.get_json("/api/projeto")
        self.assertEqual(lido["pecas"][0]["nome"], "Sofa")
        self.assertEqual(lido["takes"][0]["id"], "take_1")
        self.assertEqual(lido["posicao"]["rumo"], 119)
        self.assertIn("atualizado_em", lido)

    def test_json_invalido_devolve_400(self):
        req = urllib.request.Request(
            self.url("/api/projeto"), data=b"{isso nao e json", method="POST"
        )
        with self.assertRaises(urllib.error.HTTPError) as ctx:
            urllib.request.urlopen(req, timeout=15)
        self.assertEqual(ctx.exception.code, 400)


class TesteGlb(BaseServidor):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.mini = cls.tmp_raiz / "mini.glb"
        cls.mini.write_bytes(montar_glb([_imagem(1024, 1024)]))
        cls.rel = "/" + cls.mini.relative_to(RAIZ).as_posix()

    def test_glb_lod_reduz_o_arquivo_de_verdade(self):
        alvo = urllib.parse.quote(self.rel, safe="")
        codigo, pequeno, cabecalhos = self.get(f"/glb-lod?arquivo={alvo}&tex=128")
        self.assertEqual(codigo, 200)
        self.assertEqual(cabecalhos["Content-Type"], "model/gltf-binary")
        self.assertLess(len(pequeno), self.mini.stat().st_size)

        _, grande, _ = self.get(f"/glb-lod?arquivo={alvo}&tex=512")
        self.assertLess(len(pequeno), len(grande))

        doc, binario = arcz_glb.desempacotar(pequeno)
        view = doc["bufferViews"][doc["images"][0]["bufferView"]]
        with Image.open(
            io.BytesIO(binario[view["byteOffset"] : view["byteOffset"] + view["byteLength"]])
        ) as im:
            self.assertEqual(max(im.size), 128)

    def test_glb_lod_converte_o_material_para_metallic_roughness(self):
        alvo = urllib.parse.quote(self.rel, safe="")
        _, corpo, _ = self.get(f"/glb-lod?arquivo={alvo}&tex=256")
        doc, _ = arcz_glb.desempacotar(corpo)
        self.assertIn("pbrMetallicRoughness", doc["materials"][0])
        self.assertNotIn("extensions", doc["materials"][0])

    def test_glb_corrigido_mantem_a_textura_original(self):
        alvo = urllib.parse.quote(self.rel, safe="")
        _, corpo, _ = self.get(f"/glb-corrigido?arquivo={alvo}")
        doc, binario = arcz_glb.desempacotar(corpo)
        view = doc["bufferViews"][doc["images"][0]["bufferView"]]
        with Image.open(
            io.BytesIO(binario[view["byteOffset"] : view["byteOffset"] + view["byteLength"]])
        ) as im:
            self.assertEqual(im.size, (1024, 1024))

    def test_caminho_fora_do_projeto_e_recusado(self):
        for tentativa in ("../../Windows/win.ini", "/../servidor.py", "..%2F..%2Fsecret.txt"):
            with self.assertRaises(urllib.error.HTTPError) as ctx:
                self.get(f"/glb-lod?arquivo={urllib.parse.quote(tentativa, safe='')}")
            self.assertEqual(ctx.exception.code, 404, tentativa)

    def test_parametro_tex_invalido_devolve_400(self):
        alvo = urllib.parse.quote(self.rel, safe="")
        with self.assertRaises(urllib.error.HTTPError) as ctx:
            self.get(f"/glb-lod?arquivo={alvo}&tex=abc")
        self.assertEqual(ctx.exception.code, 400)


class TesteExportacaoDeRecorte(BaseServidor):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        cls.modelo = cls.tmp_raiz / "recorte_a.glb"
        cls.modelo.write_bytes(triangulo_glb(1.0, com_textura=True))
        cls.rel = cls.modelo.relative_to(RAIZ).as_posix()

    def setUp(self):
        self.exportacoes_original = servidor.EXPORTACOES
        servidor.EXPORTACOES = self.tmp / "exportacoes"
        servidor.EXPORTACOES.mkdir(parents=True, exist_ok=True)

    def tearDown(self):
        servidor.EXPORTACOES = self.exportacoes_original

    def carga(self, formato="glb", itens=None, relevo=None):
        return {
            "nome": "teste-recorte",
            "formato": formato,
            "itens": itens if itens is not None else [
                {"nome": "predio", "arquivo": self.rel,
                 "matriz": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 7, 0, 0, 1]},
                {"nome": "peca", "arquivo": self.rel},
            ],
            "relevo": relevo or {"incluir": False},
        }

    def test_glb_do_recorte_junta_os_modelos_com_a_matriz_de_cada_um(self):
        codigo, resposta = self.post_json("/api/exportar", self.carga())
        self.assertEqual(codigo, 200)
        self.assertTrue(resposta["ok"], resposta)
        self.assertEqual(resposta["modelos"], 2)

        destino = servidor.EXPORTACOES / resposta["arquivos"][0]
        self.assertTrue(destino.is_file())
        doc, binario = arcz_glb.desempacotar(destino.read_bytes())
        self.assertEqual(len(doc["scenes"][0]["nodes"]), 2)
        self.assertEqual(len(doc["meshes"]), 2)
        raiz = doc["nodes"][doc["scenes"][0]["nodes"][0]]
        self.assertEqual(raiz["matrix"][12], 7)
        self.assertTrue(binario)

    def test_gltf_separado_grava_json_e_bin(self):
        _, resposta = self.post_json("/api/exportar", self.carga("gltf"))
        nomes = sorted(resposta["arquivos"])
        self.assertEqual(nomes, ["teste-recorte.bin", "teste-recorte.gltf"])
        for nome in nomes:
            self.assertTrue((servidor.EXPORTACOES / nome).is_file())

    def test_obj_grava_geometria_material_e_textura(self):
        _, resposta = self.post_json("/api/exportar", self.carga("obj"))
        obj = (servidor.EXPORTACOES / "teste-recorte.obj").read_text(encoding="utf-8")
        mtl = (servidor.EXPORTACOES / "teste-recorte.mtl").read_text(encoding="utf-8")
        self.assertIn("mtllib teste-recorte.mtl", obj)
        self.assertEqual(len([l for l in obj.splitlines() if l.startswith("v ")]), 6)
        self.assertIn("newmtl", mtl)
        self.assertIn("map_Kd", mtl)
        self.assertTrue(any(a.endswith(".png") for a in resposta["arquivos"]), resposta["arquivos"])

    def test_arquivo_fora_do_projeto_vira_aviso_e_nao_exporta(self):
        codigo, resposta = None, None
        try:
            codigo, resposta = self.post_json(
                "/api/exportar", self.carga(itens=[{"nome": "x", "arquivo": "../../Windows/win.ini"}])
            )
        except urllib.error.HTTPError as e:
            codigo, resposta = e.code, json.loads(e.read().decode("utf-8"))
        self.assertEqual(codigo, 400)
        self.assertFalse(resposta["ok"])
        self.assertTrue(resposta["avisos"])

    def test_formato_desconhecido_devolve_400(self):
        with self.assertRaises(urllib.error.HTTPError) as ctx:
            self.post_json("/api/exportar", self.carga("stl"))
        self.assertEqual(ctx.exception.code, 400)

    def test_relevo_sem_tile_avisa_mas_exporta_os_modelos(self):
        original = servidor.obter_tile_dem
        servidor.obter_tile_dem = lambda z, x, y: None
        try:
            _, resposta = self.post_json("/api/exportar", self.carga(relevo={
                "incluir": True,
                "poligono": [{"lon": 0, "lat": 0}, {"lon": 0.001, "lat": 0}, {"lon": 0.001, "lat": 0.001}],
                "centro": {"lon": 0.0005, "lat": 0.0005, "alt": 0},
                "resolucao": 8,
            }))
        finally:
            servidor.obter_tile_dem = original
        self.assertTrue(resposta["ok"])
        self.assertTrue(any("relevo" in a for a in resposta["avisos"]), resposta["avisos"])


class TesteBibliotecaEThumbs(BaseServidor):
    def setUp(self):
        self.thumbs_original = servidor.LIB_THUMBS
        servidor.LIB_THUMBS = self.tmp / "lib_thumbs"
        servidor.LIB_THUMBS.mkdir(parents=True, exist_ok=True)

    def tearDown(self):
        servidor.LIB_THUMBS = self.thumbs_original

    def test_thumb_salvo_responde_url_servivel(self):
        png = io.BytesIO()
        Image.new("RGB", (32, 32), (0, 120, 255)).save(png, format="PNG")
        b64 = base64.b64encode(png.getvalue()).decode("ascii")

        codigo, resposta = self.post_json(
            "/api/thumb", {"nome": "sofa modular", "png": f"data:image/png;base64,{b64}"}
        )
        self.assertEqual(codigo, 200)
        self.assertTrue(resposta["ok"])
        self.assertTrue(resposta["url"].startswith("/lib_thumbs/"))
        self.assertFalse(resposta["url"].startswith("/teste/"))
        self.assertTrue((servidor.LIB_THUMBS / Path(resposta["url"]).name).is_file())


class TesteCatalogoReal(BaseServidor):
    """Sem monkeypatch: valida o catalogo e os thumbs que existem no disco."""

    def test_catalogo_da_biblioteca_lista_glb_com_url_valida(self):
        codigo, itens = self.get_json("/api/biblioteca")
        self.assertEqual(codigo, 200)
        self.assertIsInstance(itens, list)
        if not itens:
            self.skipTest("biblioteca/ vazia nesta maquina")
        for item in itens:
            self.assertTrue(item["url"].endswith((".glb", ".gltf")), item["url"])
            self.assertTrue((RAIZ / item["url"].lstrip("/")).is_file(), item["url"])
            if item["thumb"]:
                self.assertTrue(item["thumb"].startswith("/lib_thumbs/"))

    def test_thumbs_do_catalogo_sao_baixaveis(self):
        _, itens = self.get_json("/api/biblioteca")
        com_thumb = [i for i in itens if i.get("thumb")]
        if not com_thumb:
            self.skipTest("nenhum thumb gerado nesta maquina")
        codigo, corpo, _ = self.get(com_thumb[0]["thumb"])
        self.assertEqual(codigo, 200)
        self.assertEqual(corpo[:8], b"\x89PNG\r\n\x1a\n")


class TesteModelos(BaseServidor):
    def test_lista_de_modelos_aponta_para_arquivos_reais(self):
        codigo, itens = self.get_json("/api/modelos")
        self.assertEqual(codigo, 200)
        for item in itens:
            self.assertTrue((RAIZ / item["url"].lstrip("/")).is_file(), item["url"])


def _imagem(largura, altura):
    buf = io.BytesIO()
    Image.new("RGB", (largura, altura), (180, 90, 40)).save(buf, format="PNG")
    return buf.getvalue()


if __name__ == "__main__":
    unittest.main(verbosity=2)
