import { invokeAedifexTool, verifyBridgeToken } from '@arcz/aedifex-tools'
import { NextResponse } from 'next/server'

export const dynamic = 'force-dynamic'
export async function POST(request: Request) {
  try {
    verifyBridgeToken(request.headers.get('authorization'))
    const body = await request.json()
    const result = await invokeAedifexTool({
      name:String(body.name || ''), arguments:body.arguments && typeof body.arguments === 'object' ? body.arguments : {},
      projectId:String(body.project_id || ''), expectedRevision:Number(body.expected_revision),
      dryRun:body.dry_run !== false, approvalId:body.approval_id ? String(body.approval_id) : null,
    })
    return NextResponse.json(result, {headers:{'Cache-Control':'no-store'}})
  } catch (caught: any) {
    return NextResponse.json({error:{code:caught?.code || 'AEDIFEX_TOOL_INVOKE_FAILED',message:caught?.message || String(caught),details:caught?.details}}, {status:caught?.status || 500})
  }
}
