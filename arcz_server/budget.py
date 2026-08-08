from __future__ import annotations
from dataclasses import dataclass,asdict,fields
import sqlite3,uuid
from pathlib import Path
from typing import Any
from .errors import ApiError
from .hardware import HardwareProfile,detect_hardware

@dataclass(slots=True)
class Resources:
    triangles:int=0;instances:int=0;draw_calls:int=0;geometry_mb:float=0;textures_mb:float=0;framebuffer_mb:float=0;materials:int=0;vegetation_overdraw:float=0;cpu_ms:float=0;gpu_upload_ms:float=0;cache_mb:float=0
    @classmethod
    def from_dict(cls,d:dict[str,Any]):return cls(**{f.name:d.get(f.name,0) for f in fields(cls)})
    def as_dict(self):return asdict(self)
    def add(self,o:'Resources')->'Resources':return Resources(**{f.name:getattr(self,f.name)+getattr(o,f.name) for f in fields(self)})
    def sub(self,o:'Resources')->'Resources':return Resources(**{f.name:max(0,getattr(self,f.name)-getattr(o,f.name)) for f in fields(self)})

class BudgetEngine:
    def __init__(self,db_path:Path,hardware:HardwareProfile|None=None):self.db_path=db_path;self.hardware=hardware or detect_hardware();db_path.parent.mkdir(parents=True,exist_ok=True);self._init()
    def _db(self):db=sqlite3.connect(self.db_path);db.row_factory=sqlite3.Row;return db
    def _init(self):
        with self._db() as db:db.execute('CREATE TABLE IF NOT EXISTS reservations(id TEXT PRIMARY KEY, resources_json TEXT NOT NULL, state TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)')
    def limits(self,profile:str)->Resources:
        profile=profile.upper();vram=self.hardware.vram_mb or min(2048,max(512,self.hardware.ram_mb//8));ram=self.hardware.ram_mb
        factors={'LEVE':0.30,'EQUILIBRADO':0.52,'ALTO':0.72,'CINEMATICO':0.90,'CUSTOM':0.52}
        if profile not in factors:raise ApiError('BUDGET_PROFILE_INVALID',profile,status=400)
        f=factors[profile]
        return Resources(triangles=int(15_000_000*f),instances=int(800_000*f),draw_calls=int(4000*f),geometry_mb=vram*.30*f,textures_mb=vram*.45*f,framebuffer_mb=vram*.18*f,materials=int(1600*f),vegetation_overdraw=4.0*f,cpu_ms=max(50,2000*f),gpu_upload_ms=max(25,600*f),cache_mb=max(1024,ram*.12*f))
    def reserved(self)->Resources:
        import json
        total=Resources()
        with self._db() as db:
            for row in db.execute("SELECT resources_json FROM reservations WHERE state='RESERVED'"):total=total.add(Resources.from_dict(json.loads(row[0])))
        return total
    def evaluate(self,requested:Resources,profile:str='EQUILIBRADO',reserve:bool=True)->dict[str,Any]:
        import json
        limits=self.limits(profile);available=limits.sub(self.reserved());exceeded=[]
        for f in fields(Resources):
            if getattr(requested,f.name)>getattr(available,f.name):exceeded.append({'resource':f.name,'requested':getattr(requested,f.name),'available':getattr(available,f.name)})
        decision='ACCEPT' if not exceeded else ('SPLIT' if all(x['resource'] in {'triangles','instances','draw_calls','geometry_mb','textures_mb','cache_mb'} for x in exceeded) else 'REJECT')
        rid=None
        if decision=='ACCEPT' and reserve:
            rid=uuid.uuid4().hex
            with self._db() as db:db.execute('INSERT INTO reservations(id,resources_json,state) VALUES(?,?,?)',(rid,json.dumps(requested.as_dict(),separators=(',',':')),'RESERVED'))
        return {'profile':profile.upper(),'limits':limits.as_dict(),'available':available.as_dict(),'requested':requested.as_dict(),'decision':decision,'reasons':[f"{x['resource']}: {x['requested']} > {x['available']}" for x in exceeded],'reservation_id':rid,'hardware':self.hardware.as_dict()}
    def release(self,rid:str,state:str='RELEASED')->bool:
        with self._db() as db:cur=db.execute('UPDATE reservations SET state=? WHERE id=? AND state=\'RESERVED\'',(state,rid));return cur.rowcount>0
