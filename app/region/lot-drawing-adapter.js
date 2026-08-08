function cross(a,b,c) { return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]); }
function intersects(a,b,c,d) {
  const o1=cross(a,b,c), o2=cross(a,b,d), o3=cross(c,d,a), o4=cross(c,d,b);
  return ((o1>0&&o2<0)||(o1<0&&o2>0)) && ((o3>0&&o4<0)||(o3<0&&o4>0));
}

export function validateLotPolygon(points) {
  if (!Array.isArray(points) || points.length < 3) throw new Error("Lote precisa de pelo menos 3 vértices");
  const normalized = points.map((p,i) => {
    const lon = Number(p.lon ?? p[0]), lat = Number(p.lat ?? p[1]);
    if (!Number.isFinite(lon)||!Number.isFinite(lat)) throw new Error(`Vértice ${i} inválido`);
    return [lon,lat];
  });
  if (normalized.length > 1 && normalized[0][0] === normalized.at(-1)[0] && normalized[0][1] === normalized.at(-1)[1]) normalized.pop();
  for (let i=0;i<normalized.length;i++) for (let j=i+1;j<normalized.length;j++) {
    const i2=(i+1)%normalized.length, j2=(j+1)%normalized.length;
    if (i===j||i2===j||j2===i) continue;
    if (intersects(normalized[i],normalized[i2],normalized[j],normalized[j2])) throw new Error("Lote possui auto-interseção");
  }
  let area=0; for (let i=0;i<normalized.length;i++) { const a=normalized[i],b=normalized[(i+1)%normalized.length]; area+=a[0]*b[1]-b[0]*a[1]; }
  if (Math.abs(area)<1e-14) throw new Error("Lote possui área nula");
  return [...normalized, normalized[0]];
}

export class LotDrawingAdapter {
  constructor({ recorteApp, estadoApp }) { this.recorteApp=recorteApp; this.estadoApp=estadoApp; }
  read() {
    const raw=this.estadoApp.obter()?.recorte?.perimetro || [];
    return validateLotPolygon(raw);
  }
  lockAsActiveLot({ source="user_drawn", license="user_owned" }={}) {
    const polygon=validateLotPolygon(this.estadoApp.obter()?.recorte?.perimetro || []);
    const lons=polygon.map(p=>p[0]), lats=polygon.map(p=>p[1]);
    return { id:`lot:user:${Date.now()}`, polygon_wgs84:polygon, bbox_wgs84:[Math.min(...lons),Math.min(...lats),Math.max(...lons),Math.max(...lats)], source, license, confidence:1, locked:true };
  }
}
