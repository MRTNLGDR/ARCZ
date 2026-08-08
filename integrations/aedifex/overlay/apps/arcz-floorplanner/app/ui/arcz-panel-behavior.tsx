'use client'

import { useSidebarStore } from '@aedifex/editor'
import { useEffect, useState } from 'react'

const STORAGE_KEY = 'arcz:aedifex-sidebar:v1'

function readPinned(): boolean {
  try { return JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}').pinned === true } catch { return false }
}

export function ArczPanelBehavior() {
  const collapsed = useSidebarStore((state) => state.isCollapsed)
  const setCollapsed = useSidebarStore((state) => state.setIsCollapsed)
  const width = useSidebarStore((state) => state.width)
  const [pinned, setPinned] = useState(false)

  useEffect(() => {
    const initial = readPinned()
    setPinned(initial)
    setCollapsed(!initial)
  }, [setCollapsed])

  useEffect(() => {
    let openTimer: ReturnType<typeof setTimeout> | null = null
    let closeTimer: ReturnType<typeof setTimeout> | null = null
    const onMove = (event: PointerEvent) => {
      if (pinned) return
      if (event.clientX <= 70) {
        if (closeTimer) clearTimeout(closeTimer)
        if (!openTimer) openTimer = setTimeout(() => { setCollapsed(false); openTimer = null }, 140)
      } else if (!collapsed && event.clientX > Math.max(90, width + 82)) {
        if (openTimer) clearTimeout(openTimer)
        if (!closeTimer) closeTimer = setTimeout(() => { setCollapsed(true); closeTimer = null }, 260)
      }
    }
    window.addEventListener('pointermove', onMove, { passive: true })
    return () => {
      if (openTimer) clearTimeout(openTimer)
      if (closeTimer) clearTimeout(closeTimer)
      window.removeEventListener('pointermove', onMove)
    }
  }, [collapsed, pinned, setCollapsed, width])

  const toggle = () => {
    const next = !pinned
    setPinned(next)
    setCollapsed(false)
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ pinned: next }))
  }

  return (
    <button
      aria-pressed={pinned}
      className="arcz-aedifex-pin"
      onClick={toggle}
      title={pinned ? 'Desafixar painel lateral' : 'Fixar painel lateral aberto'}
      type="button"
    >
      {pinned ? 'Painel fixado' : 'Fixar painel'}
    </button>
  )
}
