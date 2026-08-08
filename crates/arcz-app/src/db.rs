//! Safe Project Store: persistência transacional SQLite (`project.sqlite`) com modo WAL e zero data loss.
//!
//! Tabela autoritativa de nós da cena (`scene_nodes`), registro de projeto (`projects`),
//! catalogo de assets (`assets`) e diario de comandos (`journal_entries`).

use std::path::{Path, PathBuf};
use rusqlite::{params, Connection, Result};
use crate::cena::{SceneNode, NodeType, JournalEntry};

pub struct SafeProjectStore {
    db_path: PathBuf,
}

impl SafeProjectStore {
    /// Abre ou cria o banco SQLite do projeto com modo WAL ativado para concorrencia e tolerancia a falhas.
    pub fn abrir<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        let store = Self { db_path };
        store.inicializar_esquema()?;
        Ok(store)
    }

    fn conectar(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    /// Inicializa o esquema de 4 tabelas se ainda nao existir.
    pub fn inicializar_esquema(&self) -> Result<()> {
        let conn = self.conectar()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS projects (
                project_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                crs TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                altitude REAL NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                revision INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS scene_nodes (
                node_id TEXT PRIMARY KEY,
                parent_id TEXT,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL,
                transform_json TEXT NOT NULL,
                georef_json TEXT NOT NULL,
                visibility INTEGER NOT NULL,
                locked INTEGER NOT NULL,
                selectable INTEGER NOT NULL,
                layer TEXT NOT NULL,
                material_refs_json TEXT NOT NULL,
                asset_ref TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                revision INTEGER NOT NULL,
                metadata_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS assets (
                asset_id TEXT PRIMARY KEY,
                uri TEXT NOT NULL,
                hash TEXT NOT NULL,
                asset_type TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                license TEXT NOT NULL,
                stac_provenance TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS journal_entries (
                sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                command_name TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Salva atomicamente toda a arvore de nos da cena dentro de uma transacao SQLite.
    pub fn salvar_cena(&self, nodes: &[SceneNode]) -> Result<()> {
        let mut conn = self.conectar()?;
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM scene_nodes", [])?;

        for node in nodes {
            let transform_json = serde_json::to_string(&node.transform).unwrap_or_default();
            let georef_json = serde_json::to_string(&node.georeference).unwrap_or_default();
            let mat_refs_json = serde_json::to_string(&node.material_refs).unwrap_or_default();
            let metadata_json = serde_json::to_string(&node.metadata).unwrap_or_default();
            let asset_ref_str = node.asset_ref.as_deref().unwrap_or_default();

            tx.execute(
                "INSERT INTO scene_nodes (
                    node_id, parent_id, name, node_type, transform_json, georef_json,
                    visibility, locked, selectable, layer, material_refs_json, asset_ref,
                    source, confidence, created_at, updated_at, revision, metadata_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    node.id,
                    node.parent_id,
                    node.name,
                    format!("{:?}", node.node_type),
                    transform_json,
                    georef_json,
                    if node.visibility { 1 } else { 0 },
                    if node.locked { 1 } else { 0 },
                    if node.selectable { 1 } else { 0 },
                    node.layer,
                    mat_refs_json,
                    asset_ref_str,
                    node.source,
                    node.confidence.value(),
                    node.created_at,
                    node.updated_at,
                    node.revision,
                    metadata_json,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Carrega todos os nos autoritativos da cena do SQLite.
    pub fn carregar_cena(&self) -> Result<Vec<SceneNode>> {
        let conn = self.conectar()?;
        let mut stmt = conn.prepare(
            "SELECT node_id, parent_id, name, node_type, transform_json, georef_json,
                    visibility, locked, selectable, layer, material_refs_json, asset_ref,
                    source, confidence, created_at, updated_at, revision, metadata_json
             FROM scene_nodes",
        )?;

        let node_rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let parent_id: Option<String> = row.get(1)?;
            let name: String = row.get(2)?;
            let node_type_str: String = row.get(3)?;
            let transform_json: String = row.get(4)?;
            let georef_json: String = row.get(5)?;
            let visibility: i32 = row.get(6)?;
            let locked: i32 = row.get(7)?;
            let selectable: i32 = row.get(8)?;
            let layer: String = row.get(9)?;
            let mat_refs_json: String = row.get(10)?;
            let asset_ref_str: String = row.get(11)?;
            let source: String = row.get(12)?;
            let confidence_val: f64 = row.get(13)?;
            let created_at: String = row.get(14)?;
            let updated_at: String = row.get(15)?;
            let revision: u64 = row.get(16)?;
            let metadata_json: String = row.get(17)?;

            let transform = serde_json::from_str(&transform_json).unwrap_or_default();
            let georeference = serde_json::from_str(&georef_json).unwrap_or_default();
            let material_refs = serde_json::from_str(&mat_refs_json).unwrap_or_default();
            let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();
            let confidence = crate::cena::NodeConfidence::from_value(confidence_val);
            let asset_ref = if asset_ref_str.is_empty() { None } else { Some(asset_ref_str) };

            Ok(SceneNode {
                id,
                parent_id,
                name,
                node_type: NodeType::do_texto(&node_type_str),
                transform,
                georeference,
                visibility: visibility != 0,
                locked: locked != 0,
                selectable: selectable != 0,
                layer,
                material_refs,
                asset_ref,
                source,
                confidence,
                created_at,
                updated_at,
                revision,
                metadata,
            })
        })?;

        let mut nodes = Vec::new();
        for node in node_rows {
            nodes.push(node?);
        }
        Ok(nodes)
    }

    /// Próximo número livre do diário.
    ///
    /// Sem isto a sessão nova recomeçaria em 1 e sobrescreveria as entradas da
    /// anterior — o diário perderia justamente o histórico que existe para
    /// guardar.
    pub fn proxima_sequencia(&self) -> Result<u64> {
        let conn = self.conectar()?;
        let maior: Option<u64> = conn.query_row(
            "SELECT MAX(sequence_id) FROM journal_entries",
            [],
            |r| r.get(0),
        )?;
        Ok(maior.unwrap_or(0) + 1)
    }

    /// Registra atomicamente uma lista de JournalEntry no banco.
    pub fn registrar_journal(&self, entries: &[JournalEntry]) -> Result<()> {
        let mut conn = self.conectar()?;
        let tx = conn.transaction()?;

        for entry in entries {
            let payload_str = serde_json::to_string(&entry.payload_json).unwrap_or_default();
            tx.execute(
                "INSERT INTO journal_entries (sequence_id, timestamp, command_name, payload_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![entry.sequence_id, entry.timestamp, entry.command_name, payload_str],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cena::{SceneNode, NodeType, NodeConfidence, Transform64, Georeference64, JournalEntry};

    #[test]
    fn salva_e_carrega_cena_sqlite_com_sucesso() {
        let dir = std::env::temp_dir().join(format!("arcz-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("project.sqlite");

        let store = SafeProjectStore::abrir(&db_path).unwrap();

        let mut node = SceneNode::novo("node_1", "Zenite Tower", NodeType::Building);
        node.georeference = Some(Georeference64 {
            latitude: -27.595,
            longitude: -48.548,
            altitude: 15.4,
            heading: 145.0,
        });
        node.transform = Transform64 {
            position: [100.0, 200.0, 50.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        node.confidence = NodeConfidence::Observed;

        store.salvar_cena(&[node.clone()]).unwrap();

        let carregados = store.carregar_cena().unwrap();
        assert_eq!(carregados.len(), 1);
        assert_eq!(carregados[0].id, "node_1");
        assert_eq!(carregados[0].name, "Zenite Tower");
        assert_eq!(carregados[0].confidence, NodeConfidence::Observed);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_entries_registradas_corretamente() {
        let dir = std::env::temp_dir().join(format!("arcz-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("project.sqlite");

        let store = SafeProjectStore::abrir(&db_path).unwrap();

        let entry = JournalEntry {
            sequence_id: 1,
            timestamp: "2026-07-30T09:40:00Z".into(),
            command_name: "Mover".into(),
            payload_json: serde_json::json!({ "id": 1, "offset": 10.0 }),
        };

        store.registrar_journal(&[entry]).unwrap();

        let conn = store.conectar().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests_ciclo_completo {
    use super::*;
    use crate::cena::SceneNode;
    use arcz_model::Placement;

    fn banco(nome: &str) -> (SafeProjectStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("arcz-ciclo-{}-{nome}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("project.sqlite");
        (SafeProjectStore::abrir(&p).unwrap(), dir)
    }

    fn placement() -> Placement {
        Placement {
            lat_deg: -27.1544967,
            lon_deg: -48.5022653,
            heading_deg: 59.98,
            escala: 1.25,
            offset_leste_m: 12.5,
            offset_norte_m: -7.25,
            offset_vertical_m: 3.0,
            assentar_no_terreno: true,
        }
    }

    #[test]
    fn a_posicao_sobrevive_ao_fechar_e_reabrir() {
        // O teste que fecha T140: gravar, largar o store, abrir de novo e
        // conferir que a posicao voltou igual.
        let (store, dir) = banco("posicao");
        let p = placement();
        store.salvar_cena(&[SceneNode::do_placement("zenite", &p)]).unwrap();
        drop(store);

        let store2 = SafeProjectStore::abrir(dir.join("project.sqlite")).unwrap();
        let nos = store2.carregar_cena().unwrap();
        assert_eq!(nos.len(), 1);
        let voltou = nos[0].para_placement().expect("sem georreferencia");

        assert_eq!(voltou.lat_deg, p.lat_deg);
        assert_eq!(voltou.lon_deg, p.lon_deg);
        assert_eq!(voltou.heading_deg, p.heading_deg);
        assert!((voltou.escala - p.escala).abs() < 1e-6);
        assert!((voltou.offset_leste_m - p.offset_leste_m).abs() < 1e-6);
        assert!((voltou.offset_norte_m - p.offset_norte_m).abs() < 1e-6);
        assert!((voltou.offset_vertical_m - p.offset_vertical_m).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_latitude_nao_perde_precisao_no_ciclo() {
        // f32 nesta latitude erra ~1 m. O ciclo inteiro tem de ser f64.
        let (store, dir) = banco("precisao");
        let p = placement();
        store.salvar_cena(&[SceneNode::do_placement("z", &p)]).unwrap();
        let voltou = store.carregar_cena().unwrap()[0].para_placement().unwrap();
        // Igualdade exata, nao tolerancia: qualquer f32 no caminho quebraria.
        assert_eq!(voltou.lat_deg, -27.1544967);
        assert_eq!(voltou.lon_deg, -48.5022653);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reabrir_nao_reassenta_o_modelo() {
        // `assentar_no_terreno` na volta moveria o modelo para a cota do DEM,
        // desfazendo o ajuste vertical que o usuario fez.
        let (store, dir) = banco("assentar");
        store.salvar_cena(&[SceneNode::do_placement("z", &placement())]).unwrap();
        let voltou = store.carregar_cena().unwrap()[0].para_placement().unwrap();
        assert!(!voltou.assentar_no_terreno);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_tipo_do_no_sobrevive_ao_ciclo() {
        // O tipo era gravado e descartado na leitura: tudo voltava como
        // Building, entao um terreno reabria como edificacao.
        let (store, dir) = banco("tipo");
        let nos: Vec<SceneNode> = [
            ("t", crate::cena::NodeType::Terrain),
            ("v", crate::cena::NodeType::Vegetation),
            ("c", crate::cena::NodeType::Camera),
        ]
        .iter()
        .map(|(id, t)| SceneNode::novo(*id, "x", *t))
        .collect();
        store.salvar_cena(&nos).unwrap();

        let mut voltou = store.carregar_cena().unwrap();
        voltou.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(voltou[0].node_type, crate::cena::NodeType::Camera);
        assert_eq!(voltou[1].node_type, crate::cena::NodeType::Terrain);
        assert_eq!(voltou[2].node_type, crate::cena::NodeType::Vegetation);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn o_diario_continua_de_onde_parou() {
        // Recomecar em 1 sobrescreveria o historico da sessao anterior.
        let (store, dir) = banco("sequencia");
        assert_eq!(store.proxima_sequencia().unwrap(), 1);

        store
            .registrar_journal(&[
                JournalEntry {
                    sequence_id: 1,
                    timestamp: String::new(),
                    command_name: "A".into(),
                    payload_json: serde_json::json!({}),
                },
                JournalEntry {
                    sequence_id: 2,
                    timestamp: String::new(),
                    command_name: "B".into(),
                    payload_json: serde_json::json!({}),
                },
            ])
            .unwrap();
        drop(store);

        let store2 = SafeProjectStore::abrir(dir.join("project.sqlite")).unwrap();
        assert_eq!(store2.proxima_sequencia().unwrap(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_sem_georreferencia_nao_vira_placement() {
        // Sem lat/lon nao ha onde colocar; inventar o centro seria pior.
        let no = SceneNode::novo("x", "sem geo", crate::cena::NodeType::GenericModel);
        assert!(no.para_placement().is_none());
    }
}
