import { structuredApiError } from "./budget-client.js";

/** Resolve somente entradas materializadas em pacotes locais. */
export class LocalInputClient {
  constructor({baseUrl="/api/v2",fetchImpl=globalThis.fetch?.bind(globalThis)}={}) {
    if(!fetchImpl) throw new Error("fetch indisponível");
    this.baseUrl=baseUrl.replace(/\/$/,""); this.fetch=fetchImpl;
  }
  async resolve(kind,{region,params={},signal}={}) {
    const response=await this.fetch(`${this.baseUrl}/generation/inputs/resolve`,{
      method:"POST",headers:{"Content-Type":"application/json"},
      body:JSON.stringify({kind,region,params}),signal
    });
    const data=await response.json();
    if(!response.ok) throw structuredApiError(data,response.status);
    return data;
  }
  asServices(){return {inputResolve:this.resolve.bind(this)};}
}
