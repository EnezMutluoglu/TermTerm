import { useEffect, useState } from "react";
import { call } from "./api";
import Updates from "./Updates";

type Profile = { serviceUrl: string; teamId: string; caPem: string | null };
type Policy = {
  required: boolean;
  minimumClientVersion: string;
  apiVersion: number;
};
export function useTeamCompatibility(profile: Profile, sessionPhase: string) {
  const scope = JSON.stringify([profile.serviceUrl, profile.teamId, profile.caPem]);
  const [blocked, setBlocked] = useState<{ scope: string; policy: Policy } | null>(null);
  useEffect(() => {
    let stopped = false, checking = false, recheck = false;
    const check = async () => {
      if (stopped || !profile.serviceUrl.startsWith("https://") || !/^[0-9a-f-]{36}$/i.test(profile.teamId)) return;
      if (checking) { recheck = true; return; }
      checking = true;
      try {
        const policy = await call<Policy>("team_v2_compatibility", { profile });
        if (!stopped) setBlocked(policy.required ? { scope, policy } : null);
      } catch (error) {
        // Only this profile's native check can block it. Global errors are hints.
        const match = /^team_update_required:([0-9A-Za-z.+-]{1,80}):(\d+)$/.exec(String(error instanceof Error ? error.message : error));
        if (!stopped && match) setBlocked({ scope, policy: {
          required: true, minimumClientVersion: match[1], apiVersion: Number(match[2]),
        } });
        // Network/TLS errors cannot clear a previously verified denial.
      } finally {
        checking = false;
        if (recheck && !stopped) { recheck = false; void check(); }
      }
    };
    const refresh = () => void check();
    const first = setTimeout(refresh, 500);
    const timer = setInterval(refresh, 30000);
    window.addEventListener("focus", refresh);
    window.addEventListener("team-update-required", refresh);
    return () => {
      stopped = true;
      clearTimeout(first);
      clearInterval(timer);
      window.removeEventListener("focus", refresh);
      window.removeEventListener("team-update-required", refresh);
    };
  }, [scope, sessionPhase]);
  return blocked?.scope === scope ? blocked.policy : null;
}

export default function TeamCompatibility({
  policy,
  onBack,
}: {
  policy: Policy;
  onBack: () => void;
}) {
  return (
    <main className="team-service">
      <section className="ts-login" aria-label="Zorunlu Team güncellemesi">
        <h1>Team için güncelleme gerekli</h1>
        <p role="alert">
          Bu Team servisi en az TermTerm {policy.minimumClientVersion} ve API{" "}
          {policy.apiVersion} istiyor.
        </p>
        <p>
          Güncelleme tamamlanana kadar Team kullanımı durduruldu. Şifreli
          kayıtların ve gönderilmemiş değişikliklerin korunur; uyumlu sürümle
          girişte yetkiler yeniden doğrulanır ve eşitleme devam eder.
        </p>
        <p>
          Uygun paket henüz yayımlanmadıysa yöneticinle iletişime geç.
          Geliştirme paketlerinde kurucuyu yöneticinden al; kararlı sürüme
          otomatik geçilmez.
        </p>
        <Updates minimumVersion={policy.minimumClientVersion} />
        <button onClick={onBack}>Kişisel kasaya dön</button>
      </section>
    </main>
  );
}
