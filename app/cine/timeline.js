import { evaluateTrack } from "./keyframes.js";
export class Timeline{constructor({fps=30,durationFrames=300,tracks=[]}={}){if(!(fps>0))throw new Error("fps inválido");this.schema_version=1;this.fps=fps;this.duration_frames=durationFrames;this.tracks=tracks;this.frame=0;this.playing=false;this._raf=null;this._last=null;this.onFrame=null;}
 evaluate(frame=this.frame){return Object.fromEntries(this.tracks.map(t=>[t.id,evaluateTrack(t,frame)]));}
 seek(frame){this.frame=Math.max(0,Math.min(this.duration_frames,frame));const values=this.evaluate();this.onFrame?.({frame:this.frame,time:this.frame/this.fps,values});return values;}
 play(){if(this.playing)return;this.playing=true;this._last=performance.now();const tick=now=>{if(!this.playing)return;const delta=(now-this._last)/1000;this._last=now;this.seek(this.frame+delta*this.fps);if(this.frame>=this.duration_frames){this.pause();return;}this._raf=requestAnimationFrame(tick);};this._raf=requestAnimationFrame(tick);}
 pause(){this.playing=false;if(this._raf)cancelAnimationFrame(this._raf);this._raf=null;}
 serialize(){return{schema_version:1,fps:this.fps,duration_frames:this.duration_frames,tracks:this.tracks};}
 static fromJSON(data){return new Timeline({fps:data.fps,durationFrames:data.duration_frames,tracks:data.tracks});}}
