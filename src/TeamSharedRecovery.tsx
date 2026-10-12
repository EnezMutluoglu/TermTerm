import {useEffect,useRef,useState} from "react";
import {call,chooseFile,errorText} from "./api";
type Preview={label:string;revision:number;recipients:{accountId:string;displayName:string;connect:boolean;reveal:boolean;export:boolean}[]};
export default function TeamSharedRecovery({id,onClose,onSaved}:{id:string;onClose:()=>void;onSaved:()=>void}) {
  const [path,setPath]=useState(""),[password,setPassword]=useState(""),[reauth,setReauth]=useState(""),[reason,setReason]=useState("");
  const [preview,setPreview]=useState<Preview|null>(null),[error,setError]=useState(""),[busy,setBusy]=useState(false);
  const live=useRef(true),gate=useRef(false);
  const rpc=<T,>(action:Record<string,unknown>)=>call<T>("team_v2_action",{action});
  useEffect(()=>{live.current=true;return()=>{live.current=false;void rpc({kind:"sharedRecoveryFinish",cancel:true}).catch(()=>{});};},[]);
  async function run(task:()=>Promise<void>) {
    if(gate.current)return;gate.current=true;setBusy(true);setError("");
    try{await task();}catch(e){if(live.current)setError(errorText(e));}
    finally{gate.current=false;if(live.current)setBusy(false);}
  }
  return <div className="ts-overlay"><section className="ts-dialog" role="dialog" aria-modal="true" aria-label="Ortak kimliği kurtar">
    <h2>Ortak kimliği şirket dosyasıyla kurtar</h2>
    <p>Dosya yalnız bu bilgisayarda açılır. Erişim gerekçesi denetim kaydına yazılır. Kurtarma yeni bir revizyon oluşturur; sır yalnız güncel izinli alıcılara sarılır.</p>
    {error&&<p role="alert">{error}</p>}
    {!preview?<form onSubmit={e=>{e.preventDefault();const p=password,r=reauth;setPassword("");setReauth("");void run(async()=>{
      const result=await rpc<Preview>({kind:"sharedRecoveryOpen",id,path,password:p,reauthentication:r,reason});if(live.current)setPreview(result);
    });}}>
      <label>Şirket kurtarma dosyası<input required value={path} onChange={e=>setPath(e.target.value)}/></label>
      <button type="button" disabled={busy} onClick={()=>void run(async()=>{const p=await chooseFile(["ttrecovery"]);if(p&&live.current)setPath(p);})}>Dosyayı seç</button>
      <label>Dosya parolası<input required type="password" autoComplete="off" value={password} onChange={e=>setPassword(e.target.value)}/></label>
      <label>Team hesap parolan<input required type="password" autoComplete="off" value={reauth} onChange={e=>setReauth(e.target.value)}/></label>
      <label>Kurtarma gerekçesi<textarea required minLength={8} maxLength={2000} value={reason} onChange={e=>setReason(e.target.value)}/></label>
      <button className="ts-primary" disabled={busy}>Dosyayı aç ve alıcıları önizle</button>
    </form>:<>
      <p>{preview.label} · sürüm {preview.revision} → {preview.revision+1}</p>
      <table><thead><tr><th>Güncel alıcı</th><th>Bağlan</th><th>Sır göster</th><th>Dışa aktar</th></tr></thead>
        <tbody>{preview.recipients.map(r=><tr key={r.accountId}><td>{r.displayName}</td><td>{r.connect?"Evet":"Hayır"}</td><td>{r.reveal?"Evet":"Hayır"}</td><td>{r.export?"Evet":"Hayır"}</td></tr>)}</tbody>
      </table>
      <button className="ts-primary" disabled={busy} onClick={()=>void run(async()=>{await rpc({kind:"sharedRecoveryFinish",cancel:false});if(live.current){onSaved();onClose();}})}>Yeni revizyonla kurtarmayı uygula</button>
    </>}
    <button disabled={busy} onClick={onClose}>İptal</button>
  </section></div>;
}
