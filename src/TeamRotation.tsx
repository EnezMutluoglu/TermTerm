import { useEffect, useRef, useState } from "react";
import { call, chooseFile, errorText } from "./api";
type Status={operationId:string;status:string;total:number;remaining:number};
export default function TeamRotation({onClose}:{onClose:()=>void}) {
  const [oldPath,setOldPath]=useState(""),[newPath,setNewPath]=useState("");
  const [oldPassword,setOldPassword]=useState(""),[newPassword,setNewPassword]=useState(""),[reauth,setReauth]=useState("");
  const [createNew,setCreateNew]=useState(true),[reason,setReason]=useState("");
  const [status,setStatus]=useState<Status|null>(null),[error,setError]=useState(""),[busy,setBusy]=useState(false),[done,setDone]=useState(false);
  const live=useRef(true),gate=useRef(false);
  const rpc=<T,>(action:Record<string,unknown>)=>call<T>("team_v2_action",{action});
  useEffect(()=>{live.current=true;return ()=>{live.current=false;void rpc({kind:"rotationClose"}).catch(()=>{});};},[]);
  async function run(task:()=>Promise<void>) {
    if(gate.current)return;gate.current=true;setBusy(true);setError("");
    try{await task();}catch(e){if(live.current)setError(errorText(e));}
    finally{gate.current=false;if(live.current)setBusy(false);}
  }
  return <div className="ts-overlay"><section className="ts-dialog" role="dialog" aria-modal="true" aria-label="Kurtarma anahtarını değiştir">
    <h2>Şirket kurtarma anahtarını değiştir</h2>
    <p>Özel ve ortak kimlikler ile kurtarma zarfı bulunan kişisel kullanıcı adı sürümleri yeni dosyayla doğrulanır. İşlem sırasında yazılan kayıtlar da son kontrole dahil edilir. Eski anahtar, sen sonlandırana kadar etkin kalır.</p>
    {error&&<p role="alert">{error}</p>}
    {!status?<form onSubmit={e=>{e.preventDefault();const fields={oldPassword,newPassword,reauthentication:reauth};setOldPassword("");setNewPassword("");setReauth("");void run(async()=>{
      const result=await rpc<Status>({kind:"rotationOpen",oldPath,newPath,createNew,reason,...fields});
      if(live.current){setStatus(result);setDone(result.status==="completed");}
    });}}>
      <label><input type="checkbox" checked={createNew} onChange={e=>setCreateNew(e.target.checked)} disabled={busy}/> Yeni dosya oluştur (devam etmek için işareti kaldır)</label>
      <label>Eski kurtarma dosyası<input required value={oldPath} onChange={e=>setOldPath(e.target.value)} /></label>
      <button type="button" disabled={busy} onClick={()=>void run(async()=>{const p=await chooseFile(["ttrecovery"]);if(p&&live.current)setOldPath(p);})}>Eski dosyayı seç</button>
      <label>Eski dosyanın parolası<input required type="password" autoComplete="off" value={oldPassword} onChange={e=>setOldPassword(e.target.value)}/></label>
      <label>Yeni kurtarma dosyası<input required value={newPath} onChange={e=>setNewPath(e.target.value)}/></label>
      {!createNew&&<button type="button" disabled={busy} onClick={()=>void run(async()=>{const p=await chooseFile(["ttrecovery"]);if(p&&live.current)setNewPath(p);})}>Devam dosyasını seç</button>}
      <label>Yeni dosyanın parolası<input required minLength={12} type="password" autoComplete="new-password" value={newPassword} onChange={e=>setNewPassword(e.target.value)}/></label>
      <label>Team hesap parolan<input required type="password" autoComplete="off" value={reauth} onChange={e=>setReauth(e.target.value)}/></label>
      <label>Değişim gerekçesi<textarea required minLength={8} maxLength={2000} value={reason} onChange={e=>setReason(e.target.value)}/></label>
      <small>Yeni dosya sunucu isteğinden önce kaydedilip yeniden açılarak doğrulanır. Bağlantı kesilirse dosyayı koru ve “Yeni dosya oluştur” kapalıyken aynı iki dosyayla devam et. Var olan dosya üzerine yazılmaz.</small>
      <button className="ts-primary" disabled={busy}>Dosyaları doğrula ve aç</button>
    </form>:<>
      <p role="status">{done?"Yeni kurtarma anahtarı etkinleştirildi.":`${status.total-status.remaining} / ${status.total} zarf doğrulandı; ${status.remaining} kaldı.`}</p>
      {!done&&<div className="ts-actions">
        <button disabled={busy} onClick={()=>void run(async()=>{const result=await rpc<Status>({kind:"rotationStep"});if(live.current)setStatus(result);})}>Sonraki parçayı doğrula</button>
        <button className="ts-primary" disabled={busy||status.remaining!==0} onClick={()=>void run(async()=>{await rpc({kind:"rotationFinish"});if(live.current)setDone(true);})}>Eski anahtarı emekliye ayır ve bitir</button>
      </div>}
    </>}
    <button disabled={busy} onClick={onClose}>{done?"Kapat":"Kapat; daha sonra devam et"}</button>
  </section></div>;
}
