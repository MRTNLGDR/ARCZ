import { structuredApiError } from "../core/budget-client.js";
export class SheetClient {
  constructor({baseUrl="/api/v2",fetchImpl=globalThis.fetch?.bind(globalThis)}={}){if(!fetchImpl)throw new Error("fetch indisponível");this.fetch=fetchImpl;this.baseUrl=baseUrl.replace(/\/$/,"");}
  async compose(specification,{signal}={}){const response=await this.fetch(`${this.baseUrl}/sheets/compose`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(specification),signal});const data=await response.json();if(!response.ok)throw structuredApiError(data,response.status);return data;}
}
