// Browser UI fixture only. Not an application entry point or production build asset.
import React from "react";
import { createRoot } from "react-dom/client";
const mode=new URLSearchParams(location.search).get("mode");
const w=window as any;
w.isTauri=true;
w.updateCalls=[];
w.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
w.__TAURI_INTERNALS__={
  transformCallback:()=>1,
  invoke:async (command:string)=>{
    w.updateCalls.push(command);
    if(command==="update_info")return {version:"0.3.2",channel:mode==="development"?"development":"stable",enabled:mode!=="development",repository:"https://github.com/EnezMutluoglu/TermTerm",packageSupported:true};
    if(command==="update_check"){
      await new Promise(r=>setTimeout(r,100));
      if(mode==="error")throw Error("Offline: test network failure");
      return {available:true,version:"0.3.3",notes:"Synthetic UI test release"};
    }
    if(command==="update_install")throw Error("Test installer failure; no installer was executed");
    return 1;
  }
};
const {default:Updates}=await import("../../src/Updates");
createRoot(document.getElementById("root")!).render(<Updates/>);
