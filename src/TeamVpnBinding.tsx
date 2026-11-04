import {useEffect,useRef,useState} from "react";
import {call,errorText} from "./api";
import type {VpnProfile} from "./vpnState";

type Binding={vpnId:string|null;revision:number};
export default function TeamVpnBinding({resourceId,canEdit}:{resourceId:string;canEdit:boolean}) {
  const [context,setContext]=useState("");
  const [profiles,setProfiles]=useState<VpnProfile[]>([]);
  const [shared,setShared]=useState<Binding>();
  const [personal,setPersonal]=useState<Binding>();
  const [error,setError]=useState("");
  const [busy,setBusy]=useState(false);
  const generation=useRef(0);
  const rpc=(action:Record<string,unknown>,scope=context)=>call<any>("team_v2_action",{action:{kind:"vpn",context:scope,action}});
  useEffect(()=>{
    const stamp=++generation.current;
    let active=true;
    setContext("");setProfiles([]);setShared(undefined);setPersonal(undefined);setError("");setBusy(false);
    void (async()=>{
      const snapshot=await call<{context:string;profiles:VpnProfile[]}>("team_v2_action",{action:{kind:"vpn",action:{kind:"list"}}});
      const rows=await Promise.allSettled([canEdit?rpc({kind:"binding",resourceId},snapshot.context):Promise.resolve(undefined),rpc({kind:"binding",resourceId,personal:true},snapshot.context)]);
      if(!active)return;
      setContext(snapshot.context);setProfiles(snapshot.profiles);
      if(rows[0].status==="fulfilled")setShared(rows[0].value);
      if(rows[1].status==="fulfilled")setPersonal(rows[1].value);
      if(rows[1].status==="rejected" && (!canEdit || rows[0].status==="rejected"))setError("VPN bağlama izinleri doğrulanamadı.");
    })().catch(e=>{if(active)setError(errorText(e));});
    return()=>{active=false;if(generation.current===stamp)generation.current++;};
  },[resourceId,canEdit]);
  async function save(personalMode:boolean,binding:Binding) {
    if(busy || !context || (!personalMode && !canEdit))return;setBusy(true);setError("");
    const stamp=generation.current;
    try {
      const result=await rpc({kind:"bind",resourceId,expectedRevision:binding.revision,vpnId:binding.vpnId,personal:personalMode});
      if(stamp===generation.current){if(personalMode)setPersonal(result);else setShared(result);}
    } catch(e){if(stamp===generation.current)setError(errorText(e));} finally {if(stamp===generation.current)setBusy(false);}
  }
  return <fieldset><legend>Bağlantı için VPN</legend>
    <p>Host ayarı en yakın klasörün VPN ayarından önce gelir. Erişimi kaldırılan VPN olmadan bağlantı açılmaz.</p>
    {error && <p role="alert">{error}</p>}
    {([false,true] as const).map(personalMode=>{
      const binding=personalMode?personal:shared;if(!binding || (!personalMode && !canEdit))return null;
      const update=(vpnId:string|null)=>personalMode?setPersonal({...binding,vpnId}):setShared({...binding,vpnId});
      return <div key={String(personalMode)}><label>{personalMode?"Yalnız benim hesabım":"Ortak VPN"}<select aria-label={personalMode?"Kişisel VPN bağlantısı":"Ortak VPN bağlantısı"} disabled={busy} value={binding.vpnId??""} onChange={e=>update(e.target.value||null)}>
        <option value="">Üst klasörden devral</option>
        {binding.vpnId && !profiles.some(p=>p.id===binding.vpnId) && <option value={binding.vpnId}>Erişim doğrulanamadı · mevcut VPN</option>}
        {profiles.filter(p=>(personalMode || p.visibility==="shared") && (p.visibility!=="shared" || p.permissions?.includes("connect"))).map(p=><option key={p.id} value={p.id}>{p.label}</option>)}
      </select></label><button type="button" disabled={busy} onClick={()=>void save(personalMode,binding)}>VPN ayarını kaydet</button></div>;
    })}
  </fieldset>;
}
