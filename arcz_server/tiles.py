from __future__ import annotations
import math
from typing import Any
from .errors import ApiError

def lonlat_to_tile(lon:float,lat:float,z:int)->tuple[int,int]:
    if not(0<=z<=22):raise ApiError('TILE_Z_INVALID',str(z),status=400)
    lat=max(-85.0511287798066,min(85.0511287798066,lat));n=1<<z
    x=int(math.floor((lon+180)/360*n));rad=math.radians(lat);y=int(math.floor((1-math.asinh(math.tan(rad))/math.pi)/2*n))
    return max(0,min(n-1,x)),max(0,min(n-1,y))
def tile_center(x:int,y:int,z:int)->tuple[float,float]:
    n=1<<z;lon=(x+.5)/n*360-180;lat=math.degrees(math.atan(math.sinh(math.pi*(1-2*(y+.5)/n))));return lon,lat
def distance_approx_m(a:tuple[float,float],b:tuple[float,float])->float:
    lon1,lat1=a;lon2,lat2=b;latm=math.radians((lat1+lat2)/2);return math.hypot((lon2-lon1)*111132*math.cos(latm),(lat2-lat1)*111132)
class TilePlanner:
    def plan(self,focus:dict[str,float],radius_m:float,zoom:int,generation_epoch:int,ring_m:dict[str,float]|None=None)->dict[str,Any]:
        rings=ring_m or {'HERO':150,'NEAR':600,'MEDIUM':1800,'DISTANT':max(radius_m,3000)}
        cx,cy=lonlat_to_tile(float(focus['lon']),float(focus['lat']),zoom);n=1<<zoom
        center_lon,center_lat=tile_center(cx,cy,zoom);tile_m=max(1,distance_approx_m((center_lon,center_lat),tile_center(min(n-1,cx+1),cy,zoom)))
        span=max(1,math.ceil(radius_m/tile_m));tiles=[]
        for y in range(max(0,cy-span),min(n-1,cy+span)+1):
            for x in range(max(0,cx-span),min(n-1,cx+span)+1):
                d=distance_approx_m((focus['lon'],focus['lat']),tile_center(x,y,zoom))
                if d>radius_m+tile_m:continue
                ring=next((name for name,limit in rings.items() if d<=limit),'DISTANT')
                tiles.append({'key':f'local/v1/{zoom}/{x}/{y}','z':zoom,'x':x,'y':y,'ring':ring,'priority':round(d,3),'state':'MISSING'})
        tiles.sort(key=lambda t:(t['priority'],t['y'],t['x']))
        return {'schema_version':1,'generation_epoch':generation_epoch,'zoom':zoom,'tiles':tiles,'tile_size_m':tile_m}
