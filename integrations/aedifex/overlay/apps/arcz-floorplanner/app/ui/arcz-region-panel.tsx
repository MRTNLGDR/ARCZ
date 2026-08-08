'use client'

import type { ModelingContext } from '@arcz/aedifex-bridge'

export function ArczRegionPanel({ context }: { context: ModelingContext | null }) {
  if (!context) return <div className="arcz-empty-state">Carregando Região Ativa…</div>
  const parcel = context.selection.parcel_polygon_aedifex_xyz_m
  return (
    <div className="arcz-aedifex-panel">
      <header className="arcz-panel-copy">
        <strong>Contexto territorial ARCZ</strong>
        <span>Referência bloqueada; a modelagem continua autoritativa no scene graph Aedifex.</span>
      </header>
      <dl className="arcz-context-list">
        <div><dt>Escala</dt><dd>{context.scale}</dd></div>
        <div><dt>Região</dt><dd>{context.region_id}</dd></div>
        <div><dt>Vértices do lote</dt><dd>{parcel.length}</dd></div>
        <div><dt>Norte</dt><dd>{context.geo_anchor.north_rotation_deg.toFixed(2)}°</dd></div>
        <div><dt>Bioma</dt><dd>{String(context.environment?.biome || 'não informado')}</dd></div>
        <div><dt>Context hash</dt><dd><code>{context.context_hash.slice(0, 16)}…</code></dd></div>
      </dl>
      {context.warnings.length > 0 && (
        <section className="arcz-context-warning">
          <h3>Alertas de entrada</h3>
          <pre>{JSON.stringify(context.warnings, null, 2)}</pre>
        </section>
      )}
    </div>
  )
}
