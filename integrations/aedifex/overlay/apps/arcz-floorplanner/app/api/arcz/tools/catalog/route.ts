import { listAedifexTools, verifyBridgeToken } from '@arcz/aedifex-tools'
import { NextResponse } from 'next/server'

export const dynamic = 'force-dynamic'
export async function GET(request: Request) {
  try {
    verifyBridgeToken(request.headers.get('authorization'))
    return NextResponse.json({ schema_version:1, tools:await listAedifexTools() }, {headers:{'Cache-Control':'no-store'}})
  } catch (caught: any) {
    return NextResponse.json({error:{code:caught?.code || 'AEDIFEX_TOOL_CATALOG_FAILED',message:caught?.message || String(caught),details:caught?.details}}, {status:caught?.status || 500})
  }
}
