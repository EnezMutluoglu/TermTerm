// Disposable UI fixture; connections are separately verified in native tests.
import React from "react";
import { createRoot } from "react-dom/client";
import "../../src/style.css";
const w = window as any;
w.isTauri = true;
w.calls = [];
const callbacks=new Map<number,(e:any)=>void>();
const listeners=new Map<number,{event:string,handler:number}>();
let sequence=0;
w.emitEvent=(event:string,payload:any)=>{for(const [id,l] of listeners)if(l.event===event)callbacks.get(l.handler)?.({event,id,payload});};
w.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(_event:string,id:number)=>listeners.delete(id)};
w.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main" },
  },
  transformCallback:(fn:(e:any)=>void)=>{const id=++sequence;callbacks.set(id,fn);return id;},
  unregisterCallback:(id:number)=>callbacks.delete(id),
  invoke: async (command: string, args: any = {}) => {
    w.calls.push({ command, ...args });
    if (command === "plugin:event|listen") {const id=++sequence;listeners.set(id,{event:args.event,handler:args.handler});return id;}
    if (command === "file_list") {
      if (args.endpoint.path === "/denied") throw Error("Permission denied");
      if (args.endpoint.path === "/pending")
        await new Promise((resolve) => {
          w.releaseDirectory = resolve;
        });
      return [
        {
          name: args.endpoint.connection + ".txt",
          path: args.endpoint.path + "/file.txt",
          isDir: false,
          size: 42,
        },
      ];
    }
    if (command === "sftp_connect") {
      if (args.hostId === "failed") throw Error("Connection refused");
      if (args.hostId === "slow")
        await new Promise((resolve) => {
          w.releaseConnection = resolve;
        });
      return "/home/test";
    }
    return null;
  },
};
const records = [
  { id: "prod", kind: "group", data: { label: "Üretim" } },
  {
    id: "db",
    kind: "group",
    data: { label: "Veritabanları", groupId: "prod" },
  },
  {
    id: "root",
    kind: "host",
    data: { label: "Kök makine", address: "127.0.0.1", protocol: "ssh" },
  },
  {
    id: "first",
    kind: "host",
    data: {
      label: "İstanbul PostgreSQL",
      address: "127.0.0.2",
      groupId: "db",
      protocol: "ssh",
    },
  },
  {
    id: "second",
    kind: "host",
    data: { label: "Ankara backup", groupId: "db", protocol: "mosh" },
  },
  {
    id: "failed",
    kind: "host",
    data: { label: "Başarısız makine", protocol: "ssh" },
  },
  {
    id: "slow",
    kind: "host",
    data: { label: "Yavaş makine", protocol: "ssh" },
  },
  {
    id: "telnet",
    kind: "host",
    data: { label: "Telnet only", protocol: "telnet" },
  },
].map((r) => ({ ...r, updatedAt: 0 }));
const { default: Sftp } = await import("../../src/Sftp");
createRoot(document.getElementById("root")!).render(
  <Sftp
    records={records as any}
    home="C:/test"
    notify={(s) => {
      w.notice = s;
    }}
  />,
);
