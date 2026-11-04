# Kodlama yönlendirmesi

2026-10-22 doğrulaması: Kurulu Codex app-server `config/read` çıktısı global varsayılanı `gpt-6.1-sol` / `medium` olarak doğruladı; bu varsayılan yeni oturumlar içindir, mevcut ana ajanın modelinin anında değiştiği anlamına gelmez. `hooks/list`, proje Stop hook'unu 300 saniyeyle, `errors: []`, `warnings: []` ve `trustStatus: trusted` olarak gösterdi; güven durumunu uygulama bağımsız kaydetti, bu çalışma güven onayı üretmedi. Gerçek Windows komutu elle çalıştırıldığında ana repoda 52, VPN worktree'sinde 27 birim testi; her ikisinde typecheck geçti. Son hata-yolu düzeltmesinin ardından her iki kökte 7 hook testi ayrıca geçti. Yeni bir UI oturumunda otomatik Stop tetiklenmesi ayrıca denenmedi.

Ana ajan yönlendiricidir: önce `triage` ajanına görevi sınıflandırır, sonra seçilen uygulayıcıya devreder. Uygulayıcılar yeniden triage yapmaz ve alt ajan açmaz. Sonuç bağımsız `verifier` tarafından kontrol edilir; ana ajan değişiklikleri, kontrol sonuçlarını ve kalan sınırları özetler.

| Ajan | Model | Akıl yürütme | Görev |
| --- | --- | --- | --- |
| triage | gpt-6-luna | low | Salt okuma, yalnız JSON sınıflandırma |
| quick | gpt-6-luna | medium | Küçük, açık ve düşük riskli değişiklik |
| build | gpt-6.1-sol | medium | Normal uygulama ve odaklı kontroller |
| deep | gpt-6.1-sol | xhigh | Karmaşık veya yüksek riskli iş |
| escalate | gpt-6-astra | medium | Kanıtla sınırlandırılmış tıkanıklık |
| verifier | gpt-6.1-sol | high | Bağımsız kontroller ve inceleme |

Triage önce `ROUTING-LESSONS.md` dosyasını okur ve yalnız şu biçimde çıktı verir: `{"route":"build","risk":"medium","checks":["tests","typecheck","review"]}`. Rota `quick|build|deep`, risk `low|medium|high` olmalıdır; kontrol listesi yalnız gereken `tests|typecheck|lint|review` değerlerini içerir.

Aynı tıkanıklık için en fazla iki başarısız odaklı denemeden sonra veya çözülemeyen yüksek riskli belirsizlikte yönlendirici somut kanıtlarla yükseltir: quick → build → deep → escalate. Her işi pahalı modele taşımaz; çözülemeyen kararı açıkça bildirir. Doğrulayıcı bulguları uygulayıcıya geri gider ve ilgili kontroller tekrarlanır.

Ajan dosyaları görev talimatlarıdır; triage/verifier için yazmama talimatı gerçek bir dosya sistemi sandbox'ı oluşturmaz. Özel ajandaki `sandbox_mode` üst ajanın izinlerini değiştirmez. Bkz. [resmî alt ajan yapılandırması](https://learn.chatgpt.com/docs/agent-configuration/subagents).

Global yönlendirme/ajan ayarı değişikliklerini yeni bir Codex oturumunda doğrulayın; mevcut oturumun yeniden yüklendiğini varsaymayın. Yerel hook'ları çalıştırmadan önce `/hooks` üzerinden güven/onay durumunu kontrol edin. Windows'ta PATH üzerindeki `bash` WSL başlatıcısı olabilir; shell hook'u için doğrulanmış Git Bash kullanın. Stop hook stdout'u yalnız protokol JSON'u içermelidir.

Yerel kontrol kapısının kapsamı yalnız TypeScript (`tsc`), Vitest ve `git diff --check` tabanıdır. Bu kapı native derleme, SSH sunucusu, donanım anahtarı, macOS, üretim kimlik doğrulaması veya VPN laboratuvarını doğrulamaz; gereken ek kontroller ayrıca çalıştırılıp raporlanır. Lint yalnız projede karşılığı varsa çalıştırılır. Bu yapılandırma uygulama/paket sürümünü değiştirmez ve yayın izni vermez.

Stop kapısı `check.sh` üzerinden `check.mjs` çalıştırır: staged/unstaged diff kontrolü, hook öz testleri, yerel `tsc --noEmit` ve `vitest run --pool=threads --maxWorkers=2`. Toplam kontrol bütçesi 275 saniyedir; 300 saniyelik hook süresinin altında kalır. Başarısızlıkta tek düzeltme turu ister, tekrar başarısızsa engeli bildirir. Windows `commandWindows` yolu PowerShell üzerinden `C:/Program Files/Git/bin/bash.exe` kullanır; repo kökü Git ile bulunur. Otomatik kapının gerçekten çalıştığını yeni oturumda ayrıca doğrulayın.
