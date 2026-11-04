import React,{useState} from "react";
import {createRoot} from "react-dom/client";
import "../../src/style.css";
const w=window as any;
const params=new URLSearchParams(location.search);
const workspace=params.has("workspace"),role=params.get("role")??"operator";
w.isTauri=true;
const callbacks=new Map<number,(event:any)=>void>(),listeners=new Map<number,{event:string;handler:number}>();
let sequence=0;
const base={version:1,family:"fortinet",source:"manual",server:"https://vpn.example.test",groupId:"Team",username:"",hasPassword:false,hasConfig:false,realm:"",remember:false,revision:1};
const profiles=[
  {...base,id:"private",label:"Private office",visibility:"private",permissions:["read","connect","edit","reveal","export"],canRemove:true,canUpdate:true,username:"my-account",hasPassword:true,remember:true},
  {...base,id:"shared",label:"Shared connect only",visibility:"shared",permissions:["read","connect"],canRemove:false,canUpdate:false},
  {...base,id:"observer",label:"Shared view only",visibility:"shared",permissions:["read"],canRemove:false,canUpdate:false},
  {...base,id:"wireguard",label:"WireGuard office",family:"wireGuard",hasConfig:true,visibility:"private",permissions:["read","connect","edit","reveal","export"],canRemove:true,canUpdate:true},
];
const fixture=w.fixture={scope:"team-a:vpn:1",accountId:"account-a",required:false,bindConflict:false,bindings:{} as Record<string,any>,calls:[] as any[],profiles,offline:params.has("offline"),journal:{pending:0,conflicts:0,quarantined:0},sessions:[] as any[],emit:(session:any)=>{for(const l of listeners.values())if(l.event==="vpn-event")callbacks.get(l.handler)?.({payload:session});}};
const serviceProfile={serviceUrl:"https://team.example.test",teamId:"12345678-1234-1234-1234-123456789012",caPem:null};
const permissions=role==="observer"?["read"]:role==="editor"?["read","connect","edit"]:["read","connect"];
const resources=["host","folder"].map(kind=>({id:kind,revision:1,permissions,content:{kind,parentId:null,name:kind==="host"?"Lab host":"Operations",address:kind==="host"?"host.example.test":null,port:kind==="host"?22:null,protocol:kind==="host"?"ssh":null,usernamePolicy:"personal",username:null,deleted:false}}));
const snapshot=()=>({context:fixture.scope,profiles:profiles.map(p=>({...p,label:fixture.scope.startsWith("team-b")?"Account B "+p.label:p.label})),offline:fixture.offline,journal:fixture.journal});
w.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
w.__TAURI_INTERNALS__={metadata:{currentWindow:{label:"main"},currentWebview:{label:"main"}},transformCallback:(callback:any)=>{const id=++sequence;callbacks.set(id,callback);return id;},unregisterCallback:(id:number)=>callbacks.delete(id),invoke:async(command:string,args:any={})=>{
  fixture.calls.push({command,args});
  if(command==="plugin:event|listen"){const id=++sequence;listeners.set(id,{event:args.event,handler:args.handler});return id;}
  if(command==="plugin:event|unlisten"){listeners.delete(args.eventId);return;}
  if(command==="team_v2_saved_login")return {profile:serviceProfile,username:"fixture",remembered:true};
  if(command==="team_v2_login_remembered")return {activationRequired:false};
  if(command==="team_v2_logout")return;
  if(command==="team_v2_compatibility")return {required:fixture.required,minimumClientVersion:"0.4.2",apiVersion:1};
  if(command==="update_info")return {version:"0.4.2",channel:"dev",enabled:false,repository:"",packageSupported:false};
  if(command==="app_info")return {home:"/disposable"};
  if(command==="file_list")return [];
  if(command==="vpn_status")return {available:true,platform:"Windows",reason:"Mock IPC only; no tunnel opened",labOnly:true,maxConnections:3,supportedFamilies:["fortinet","openVpn","globalProtect","wireGuard"]};
  if(command==="vpn_sessions")return fixture.sessions;
  if(command==="vpn_connect"){
    if(args.teamContext!==fixture.scope)throw Error("Team session changed");
    const session={id:"session-"+(++sequence),vaultId:fixture.scope,profileId:args.profileId,generation:sequence,state:"connecting"};fixture.sessions.push(session);fixture.emit(session);return session;
  }
  if(command==="vpn_profile_save"){
    if(args.teamContext!==fixture.scope)throw Error("Team session changed");
    fixture.saved=args;return {...args.profile,revision:args.revision+1,visibility:"private",hasPassword:!!args.profile.password,hasConfig:false,permissions:["read","connect","edit","reveal","export"],pending:fixture.offline};
  }
  if(command!=="team_v2_action")throw Error("Unexpected fixture command: "+command);
  const envelope=args.action;
  if(envelope.kind==="overview")return {teamName:"Disposable Team",displayName:"Fixture",role,accountId:fixture.accountId,offline:fixture.offline};
  if(envelope.kind==="vaults")return [{id:"vault",name:"Disposable vault"}];
  if(envelope.kind==="resources")return {records:resources,nextAfter:null};
  if(envelope.kind==="historySelections")return [];
  if(envelope.kind==="journalStatus")return {...fixture.journal,editRemainingMs:0};
  if(envelope.context && envelope.context!==fixture.scope)throw Error("Team session changed; reopen VPN");
  if(envelope.kind==="vpn"){
    if(envelope.action.kind==="authenticate"){fixture.authentication=envelope.action;return snapshot();}
    if(envelope.action.kind==="binding" || envelope.action.kind==="bind"){
      const action=envelope.action;
      if(!permissions.includes(action.personal?"connect":"edit"))throw Error("permission_denied");
      const key=fixture.accountId+":"+action.resourceId+":"+!!action.personal;
      const binding=fixture.bindings[key]??{vpnId:null,revision:0};
      if(action.kind==="bind"){
        if(fixture.bindConflict || binding.revision!==action.expectedRevision)throw Error("revision_conflict");
        return fixture.bindings[key]={vpnId:action.vpnId,revision:binding.revision+1};
      }
      return binding;
    }
    if(envelope.action.kind==="list")return snapshot();
    throw Error("Unsupported VPN fixture action");
  }
  if(envelope.kind==="vpnShare"){
    const action=envelope.action;
    if(action.kind==="shareCandidates")return [{accountId:"member",displayName:"Member"}];
    if(action.kind==="prepareShare"){fixture.share=action;return {id:"plan",recipients:action.recipients};}
    if(action.kind==="finishShare"){fixture.finished=true;return {credentialId:"copy",revision:1};}
    if(action.kind==="cancelShare")return {status:"cancelled"};
  }
  throw Error("Unsupported fixture action");
}};
const {default:VpnWorkspace}=await import("../../src/VpnWorkspace");
const {default:TeamService}=await import("../../src/TeamService");
function Harness(){const [account,setAccount]=useState("team-a"),[left,setLeft]=useState(false);fixture.switchAccount=()=>{fixture.scope="team-b:vpn:2";setAccount("team-b");};return <main style={{height:"100vh"}}><div><strong>Mock Team VPN · no real tunnel</strong><button onClick={()=>fixture.switchAccount()}>Switch account</button></div><div style={{height:"calc(100vh - 32px)"}}>{workspace?(left?<p>Team session closed</p>:<TeamService onBack={()=>setLeft(true)}/>):<VpnWorkspace key={account} team vaultId={account} scopes={[{id:"folder",name:"Operations"}]}/>}</div></main>;}
createRoot(document.getElementById("root")!).render(<Harness/>);
