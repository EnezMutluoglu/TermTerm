# TermTerm — bilgisayar değişimi için devir teslim

## 6 Kasım 2026 — güncel 0.4.3 yerel kabul ve teslim

**Özgün geliştirme verileri geri yüklendi; bekleyen Restore/Prepare işlemi yoktur.** Aşağıdaki 5 Kasım ve daha eski özel vaka komutları tarihsel kanıttır; yeniden çalıştırılmamalıdır. Kullanıcı gerçek sağlayıcı VPN tekrarını kapsamdan çıkardı; önceki başarısız bağlantı PASS sayılmadı.

Yetkili Windows 0.4.2 → 0.4.3 yerinde yükleyici testi geçti; özgün AppData/kasa/yan dosyalar/yedekler korunup bağımsız doğrulandı, kullanıcı verileri taşınmadı. Bu Windows kabulü GUI/VPN testi içermez. Linux broker v3, gerçek root AppDir AppImage kurulumu, UID1000 ile test kasası ve kimliği doğrulanmış IPC üzerinden kabul edildi; bağlı AppImage/DEB istemcileriyle temiz servis kapanışı ve aynı sürümde değiştirme/koruma geçti. Worker-thread SIGTERM kapanış hatası düzeltildi; ilk başarısız kanıtlar korundu.

Team 0.4.3 istemci kataloğunun son ELF/imaj/API kabulü geçti; yerel paket ve kabul kanıtları tamamlandı. Bu kayıt yerel kabul sonuçlarını kapsar; dağıtılan sürüm ve kurulum dosyaları [GitHub Releases](https://github.com/EnezMutluoglu/TermTerm/releases) sayfasında görülür. Ayrı SQL0.5 çalışması korunur. [Güncel paket/kapsam rehberi](artifacts/vpn-delivery-20261106/candidate-0.4.3/DURUM_VE_KURULUM_TR.md), [AppImage kabulü ve yönetici güncelleme adımları](artifacts/vpn-delivery-20261106/candidate-0.4.3/appimage-validation/README_TR.md). Bu belge eki paketlerden sonra yazıldı; paket kaynak kapanışı kendi receipt/hash kayıtlarıyla sabittir.

---

## 5 Kasım 2026 — VPN dev.4 ve önce veri geri yükleme

**Önce mevcut test vakasını güvenle kapatın; yeni Prepare/test açmayın.** Dev.4 portable PID 30648 ile başlatıldı; ilk computer-use çağrısı fiziksel Escape ile durduruldu. GUI gözlemi/vault/import/Save/Connect/UAC yapılmadı. İki özgün dev AppData dizini `gui-import-diagnostic-dev4-bdd69050f7c14c4cbf86726d9f212fbe` özel vakasında yedekli; **Restore pending**. Kullanıcı veya gelecekte izinli CU uygulamayı kapattıktan, app/WebView/VPN süreçleri quiet doğrulandıktan sonra reviewed Restore gerekir. Bu turda CU veya alternatif close/kill yapmayın. Exact komut ve son kanıtlar [VPN devrinin dev.4 bölümündedir](docs/HANDOFF_VPN_TR.md). Eski restored vakalar bu yeni vakanın durumu değildir.

Kullanıcının açıkça seçtiği sertifika-only örneğin değişmeyen native Rust parser kabulü geçti. Dev.4 compatibility/auth düzeltmesi ve odaklı kontroller bağımsız GO aldı; yeni Windows/Linux paketleri `artifacts/vpn-delivery-20261105/dev4` altında, nihai statik paket/source incelemesi bağımsız GO aldı. Admin/UAC/Connect testleri kullanıcı dönene kadar **WAIT**. Stable ve ayrı SQL0.5 korundu; yayın/main/tag/feed onayı yok. Aşağıdaki dev.3 ve eski FAIL kayıtları tarihseldir.

---

## 5 Kasım 2026 — VPN dev.3 güncel devir

Kullanıcı isteğiyle çalışma sürdürüldü. **0.4.3-dev.3 yerel Windows/Linux paketleri ve Linux teknik kaynak kapanışı bağımsız incelemeden geçti; gerçek import hatasının nedeni hâlâ çözülmedi.** Kurulu stable uygulama ve ayrı SQL0.5 çalışması korundu; main/tag/feed/yayın onayı yok. Son durum, veri geri yükleme kaydı ve güvenli devam komutları [VPN devir teslimindedir](docs/HANDOFF_VPN_TR.md). Yerel teslim `artifacts/vpn-delivery-20261105/dev3`; ignored paketler/özel kanıtlar Git ile gelmez.

Portable import-only pencere kapatıldı ve iki özgün dev AppData dizini geri yüklendi; profil seçimi/import sonucu alınamadı, VPN/Save/Connect yapılmadı. Kurulu dev.3 GUI/VPN-close kabulü **pending** kalır. Aşağıdaki 4 Kasım bilgisayar-devir kaydı ve önceki FAIL'ler tarihseldir, silinmedi.

---

## 4 Kasım 2026 — önceki bilgisayar-devir kaydı

4 Kasım 2026. Önce bu dosyayı, ardından çalışacağınız dalın belgesini okuyun. Bu teslim kaynak çalışmasını korur; kararlı sürüm veya yayın onayı değildir.

| Çalışma | Ayrı dal | Başlangıç noktası | Devam belgesi |
|---|---|---|---|
| VPN + Authenticator | `codex/feature-vpn-workspace` | Devir öncesi HEAD `9a7fd9b63671fa28c43233d446cea0e4a44dde60`; ortak develop tabanı `b749b35c66670a47941b572db44cd66097e88ef3` | [VPN devir teslimi](https://github.com/EnezMutluoglu/TermTerm/blob/codex/feature-vpn-workspace/docs/HANDOFF_VPN_TR.md) |
| Database Workspace / SQL | `codex/feature-database-workspace` | Devir öncesi HEAD ve dal tabanı `59652374014cd8f65bb34673b9d9d900bb836bb8` | [SQL devir teslimi](https://github.com/EnezMutluoglu/TermTerm/blob/codex/feature-database-workspace/docs/HANDOFF_DATABASE_TR.md) |
| Team servis teslim kapısı | `develop` | Devir öncesi HEAD `fcf3775`; önceki uzak develop `b749b35` | [develop devir teslimi](https://github.com/EnezMutluoglu/TermTerm/blob/develop/docs/HANDOFF_DEVELOP_TR.md) |

VPN ile SQL farklı tarihli tabanlarda ilerledi. Birbirine veya `main` dalına birleştirmeyin. Dal başına son kaynak checkpoint'ini `git log -1 --oneline` ile görün; yukarıdaki SHA'lar teslim belgesi yazılmadan önceki noktalardır.

## Yeni bilgisayarda kaynakları açma

Kaynak checkpoint'leri uzak depoya gönderildikten sonra PowerShell'de, kendi çalışma klasörünüzde:

```powershell
git ls-remote --heads https://github.com/EnezMutluoglu/TermTerm.git develop codex/feature-vpn-workspace codex/feature-database-workspace
git clone --branch develop https://github.com/EnezMutluoglu/TermTerm.git TermTerm
Set-Location TermTerm
git fetch origin develop codex/feature-vpn-workspace codex/feature-database-workspace
git worktree add ../TermTerm-VPN --track -b codex/feature-vpn-workspace origin/codex/feature-vpn-workspace
git worktree add ../TermTerm-SQL --track -b codex/feature-database-workspace origin/codex/feature-database-workspace
git status --short
git worktree list
```

`TermTerm` develop, `TermTerm-VPN` VPN, `TermTerm-SQL` SQL içindir. Boş bir hedef klasör kullanın; mevcut checkout'un üstüne açmayın. Bu komutlar dal yoksa durur; yerine `main` kullanmayın. İtme yetkisi için yeni bilgisayarda kendi GitHub oturumunuzu açın. Paketler ve özel laboratuvar verileri clone ile gelmez.

## Ortam ve veri sınırı

- Her dalda `AGENTS.md` dosyasını okuyun. `ROUTING-LESSONS.md` ve `.codex` proje dosyaları VPN/SQL kaynak checkpoint'lerinde vardır; develop devir commit'i bu dosyaları eklemez. VPN/SQL üzerinde ayrıca bu talimatları okuyun. `.codex/agents` ve hook kaynakları proje talimatlarıdır; global ayar, güven onayı veya kullanıcı oturumu taşınmaz. Hook'taki Windows Git Bash yolunu yeni makinede kontrol edin.
- Git, Node/pnpm, Rust/Cargo, Python 3; Windows masaüstü için Visual Studio C++ Build Tools/Windows SDK ve WebView2 gerekir. Dalın kilit dosyalarıyla kurun. VPN ayrıca Go, CMake/Ninja ve ayrı WSL/Linux laboratuvarı; SQL Java 21/JDBC hazırlığı ister.
- `.lab`, `.tools`, `artifacts`, build cache, kurucular, indirilmiş runtime/DLL/JAR, kasalar, yedekler, host exportları, özel anahtarlar, imzalama sırları ve ham ağ/DB raporları Git'e dahil değildir. Eski makinedeki özgün dosyalar yerinde korunur; bu teslim onları yedeklemiş veya taşımış sayılmaz.
- Yeni test kasası, Team test hesabı, DB/VPN test profilleri, CA güveni ve gerekli parolalar/OTP yeni makinede yeniden girilir. Gerçek sunucu sırlarını belgeye veya Git'e koymayın. Eski fixture GUID/PID/pin/baseline değerleri yeni çalıştırma yetkisi değildir; taze laboratuvar kurun.
- VPN dalındaki 22 tek seferlik `.codex/*.py` kod üretim betiği kaynak teslimine dahil edilmedi; özgünleri ve ignored `.tools/handoff-local-source-20261104/codex-oneoff` kopyası yalnız eski makinededir. Uygulama kaynakları ve gerekli test betikleri ayrı korunur.
- Yalnız açıkça seçilmiş genel kaynak/test/fixture/lisans dosyaları kaynak checkpoint'ine alınır. `tests/ssh-keys` içindeki mevcut herkese açık disposable fixture'lar hariç özel SSH anahtarı taşınmaz.

Bu devir sırasında uygulama testi, native ağ testi, yeni derleme veya paket kabulü yapılmadı. Dal belgeleri önceki sonuçları tarih ve kapsamıyla ayırır; eski özel raporlara bağlantılar yeni bilgisayarda dosya bulunacağını göstermez.

Kararlı uygulamayı/kurucusunu çalıştırmayın; yalnız dalın ayrı geliştirme kimliğini kullanın. `main`, sürüm etiketi, GitHub Release ve `latest.json` değişikliği bu teslimin kapsamı dışındadır.
