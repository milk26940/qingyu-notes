import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
export type WikiItem = { pageId:string; title:string; kind:"topic"|"concept"; summary:string; sources:{pageId:string;title:string}[] };
type Catalog = { items:WikiItem[]; hiddenPageIds:string[] };
export const useWikiStore=create<Catalog & {error:string;refresh:()=>Promise<void>}>((set)=>({items:[],hiddenPageIds:[],error:"",refresh:async()=>{try{const result=await invoke<Catalog>("knowledge_catalog");set({...result,error:""});}catch(e){set({error:e && typeof e==="object" && "message" in e ? String(e.message):String(e)});}}}));
