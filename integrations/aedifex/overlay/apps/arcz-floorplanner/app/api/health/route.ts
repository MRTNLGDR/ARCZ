import { NextResponse } from 'next/server'

const UPSTREAM_COMMIT = '5319368bae16500ca5267f6f8d68b36c9586d5bb'

export async function GET() {
  return NextResponse.json(
    { ok: true, service: 'arcz-aedifex-floorplanner', upstream_commit: UPSTREAM_COMMIT },
    { headers: { 'Cache-Control': 'no-store' } },
  )
}
