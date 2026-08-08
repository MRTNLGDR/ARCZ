from __future__ import annotations
import json
from pathlib import Path
from typing import Any
from .errors import ApiError
from .hashing import canonical_json_hash
from .schema_validation import SchemaRegistry

def deep_merge(base:Any,layer:Any)->Any:
    if isinstance(base,dict) and isinstance(layer,dict):
        out={k:deep_merge(v,{}) if isinstance(v,dict) else v for k,v in base.items()}
        for k,v in layer.items():out[k]=deep_merge(out[k],v) if k in out else v
        return out
    return layer

def normalize_weights(values:dict[str,float],path:str)->dict[str,float]:
    total=sum(max(0,float(v)) for v in values.values())
    if total<=0:raise ApiError('PROFILE_DISTRIBUTION_EMPTY',path,status=400)
    return {k:max(0,float(v))/total for k,v in values.items()}

class ProfileStore:
    def __init__(self,roots:list[Path],schemas:SchemaRegistry):self.roots=[p.resolve() for p in roots];self.schemas=schemas
    def _files(self):
        seen=set()
        for root in self.roots:
            if not root.is_dir():continue
            for p in sorted(root.rglob('*.json')):
                if p.name.endswith('.schema.json'):continue
                try:d=json.loads(p.read_text(encoding='utf-8'))
                except Exception:continue
                if isinstance(d,dict) and 'architecture' in d and 'roofs' in d and d.get('id') not in seen:
                    self.schemas.validate('regional-profile.schema.json',d);seen.add(d['id']);yield d,p
    def list(self)->list[dict[str,Any]]:return [d for d,_ in self._files()]
    def get(self,profile_id:str)->dict[str,Any]:
        for d,_ in self._files():
            if d['id']==profile_id:return d
        raise ApiError('PROFILE_NOT_FOUND',profile_id,status=404)
    def compose(self,profile_ids:list[str],override:dict[str,Any]|None=None)->dict[str,Any]:
        if not profile_ids:raise ApiError('PROFILE_REQUIRED','Ao menos um perfil é obrigatório',status=400)
        result={};applied=[]
        for pid in profile_ids:
            layer=self.get(pid);result=deep_merge(result,layer);applied.append(f"{layer['id']}@{layer['version']}")
        if override:result=deep_merge(result,override);applied.append('user_override')
        result['architecture']['building_mix']=normalize_weights(result['architecture']['building_mix'],'architecture.building_mix')
        result['roofs']['types']=normalize_weights(result['roofs']['types'],'roofs.types')
        result['roofs']['materials']=normalize_weights(result['roofs']['materials'],'roofs.materials')
        result['facades']['materials']=normalize_weights(result['facades']['materials'],'facades.materials')
        result['resolution_report']={'applied':applied,'profile_hash':canonical_json_hash(result)}
        return result
