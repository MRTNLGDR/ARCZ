import type { ReactNode } from 'react'
import './globals.css'
export const metadata = { title: 'ARCZ Floorplanner', description: 'Aedifex authoring kernel for ARCZ Earth' }
export default function RootLayout({children}:{children:ReactNode}) { return <html lang="pt-BR"><body>{children}</body></html> }
