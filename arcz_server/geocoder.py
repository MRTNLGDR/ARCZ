from __future__ import annotations
import json, re, sqlite3, unicodedata
from pathlib import Path
from typing import Any, Iterable
from .errors import ApiError

SPACE=re.compile(r'\s+')
def normalize(text:str)->str:
    value=unicodedata.normalize('NFKD',text).encode('ascii','ignore').decode('ascii').lower()
    return SPACE.sub(' ',''.join(ch if ch.isalnum() or ch in ' -' else ' ' for ch in value)).strip()

class LocalGeocoder:
    def __init__(self,db_path:Path): self.db_path=db_path.resolve(); self.db_path.parent.mkdir(parents=True,exist_ok=True); self._fts=False; self._init_db()
    def _connect(self): db=sqlite3.connect(self.db_path);db.row_factory=sqlite3.Row;db.execute('PRAGMA journal_mode=WAL');return db
    def _init_db(self):
        with self._connect() as db:
            db.execute('''CREATE TABLE IF NOT EXISTS places(id TEXT PRIMARY KEY, display_name TEXT NOT NULL, normalized TEXT NOT NULL, lat REAL NOT NULL, lon REAL NOT NULL, bbox_json TEXT NOT NULL, scale TEXT NOT NULL, source_package TEXT NOT NULL, metadata_json TEXT NOT NULL DEFAULT '{}')''')
            db.execute('CREATE INDEX IF NOT EXISTS idx_places_normalized ON places(normalized)')
            try:
                # Tabela FTS independente. `content=''` torna colunas não recuperáveis
                # em algumas builds SQLite e quebra o JOIN por id.
                sql = db.execute(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name='places_fts'"
                ).fetchone()
                if sql and "content=''" in (sql[0] or ""):
                    db.execute("DROP TABLE places_fts")
                db.execute("CREATE VIRTUAL TABLE IF NOT EXISTS places_fts USING fts5(id UNINDEXED, display_name, normalized)")
                self._fts=True
            except sqlite3.OperationalError: self._fts=False
    def count(self)->int:
        with self._connect() as db:return int(db.execute('SELECT COUNT(*) FROM places').fetchone()[0])
    def import_records(self,records:Iterable[dict[str,Any]],source_package:str)->int:
        rows=[]
        for i,r in enumerate(records):
            try:
                rid=str(r['id']);name=str(r['display_name']);lat=float(r['lat']);lon=float(r['lon']);bbox=r.get('bbox_wgs84') or [lon,lat,lon,lat];scale=str(r.get('scale','endereco'))
                if not(-90<=lat<=90 and -180<=lon<=180):raise ValueError('coordinate out of range')
                if len(bbox)!=4:raise ValueError('bbox')
            except Exception as e: raise ApiError('GEOCODER_RECORD_INVALID',f'Registro {i}: {e}',status=400) from e
            rows.append((rid,name,normalize(name),lat,lon,json.dumps(bbox,separators=(',',':')),scale,source_package,json.dumps(r.get('metadata',{}),ensure_ascii=False,separators=(',',':'))))
        with self._connect() as db:
            db.executemany('INSERT OR REPLACE INTO places(id,display_name,normalized,lat,lon,bbox_json,scale,source_package,metadata_json) VALUES(?,?,?,?,?,?,?,?,?)',rows)
            if self._fts:
                for rid,name,norm,*_ in rows:
                    db.execute('DELETE FROM places_fts WHERE id=?',(rid,));db.execute('INSERT INTO places_fts(id,display_name,normalized) VALUES(?,?,?)',(rid,name,norm))
        return len(rows)
    def search(self,query:str,limit:int=8,scale:str|None=None)->list[dict[str,Any]]:
        q=normalize(query)
        if len(q)<2:return []
        if self.count()==0:raise ApiError('DATASET_NOT_INSTALLED','Índice geográfico local vazio. Importe um pacote de geocodificação.',status=503,retryable=False)
        limit=max(1,min(int(limit),50))
        with self._connect() as db:
            rows = []
            if self._fts:
                tokens=[t for t in q.split() if t];match=' AND '.join(f'"{t}"*' for t in tokens)
                sql='SELECT p.* FROM places_fts f JOIN places p ON p.id=f.id WHERE places_fts MATCH ?';params:[Any]=[match]
                if scale: sql+=' AND p.scale=?';params.append(scale)
                sql+=' ORDER BY length(p.display_name), p.display_name LIMIT ?';params.append(limit)
                try:
                    rows=db.execute(sql,params).fetchall()
                except sqlite3.OperationalError:
                    rows=[]
            # LIKE é também fallback de consistência quando um índice FTS foi
            # importado por versão antiga ou ainda não foi reconstruído.
            if not rows:
                sql='SELECT * FROM places WHERE normalized LIKE ?';params=[f'%{q}%']
                if scale: sql+=' AND scale=?';params.append(scale)
                sql+=' ORDER BY length(display_name), display_name LIMIT ?';params.append(limit)
                rows=db.execute(sql,params).fetchall()
        return [{'id':r['id'],'display_name':r['display_name'],'lat':r['lat'],'lon':r['lon'],'bbox_wgs84':json.loads(r['bbox_json']),'scale':r['scale'],'source_package':r['source_package'],'metadata':json.loads(r['metadata_json'])} for r in rows]
