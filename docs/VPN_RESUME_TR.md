> **4 Kasım 2026 devir notu:** Güncel durum ve açık Windows engeli [HANDOFF_VPN_TR.md](HANDOFF_VPN_TR.md) dosyasındadır. Aşağıdaki kayıtlar tarihseldir; özel rapor bağlantıları Git ile taşınmaz.

# VPN çalışma devri — 30 Ekim 2026, üçüncü Windows setup tanısı / ortam temiz

**Güncel durum: normal ve iki kayıp vakası PASS; inbound setup nedeni hâlâ
kanıtlanmadı, toplam Windows kabulü tamamlanmadı.**
[Tanı ve temiz kapanış checkpoint'i](../artifacts/vpn-lab/windows-wireguard-pending-20261030-v1-diagnosed-checkpoint.json),
SHA256 `4fcab03c95c9c21b775cf306798fecf49f06ab62e821f38f08398c4d6c311cc7`.

Kullanıcının “Hazırım, yeniden aç” yanıtından sonra birleşik **salt okunur**
UAC koleksiyonu tamamlandı: 14:13:23 UTC after-client-death audit **journal 0**,
native views ve helper/WireGuard süreçleri sıfır; recovery yapılmadı.
[Korumalı gerçek broker/watchdog tanıları](../artifacts/vpn-lab/windows-wireguard-pending-20261030-v1-diagnostics.json)
üçüncü session'ın yedi intent'i uyguladığını, private kernel peer'in
13:56:28 UTC'de taze handshake aldığını ve failure-finally owner'ı kapattıktan
sonra journal'ın normal yoldan kaldırıldığını gösterdi. Bu, connect reply ile
broker `connected`/adres readiness beklemesini ayırmaz; ürün veya IPC bug'ı
varsayılmaz ve 45 saniye körlemesine artırılmaz.

Özel fixture/relay yalnız doğrulanmış kendi stop marker'larıyla kapandı.
[14:16:10 UTC kapanış kanıtı](../artifacts/vpn-lab/windows-wireguard-pending-20261030-v1-closure.json):
tam Windows baseline aynı, native/journal/helper sayıları sıfır, Linux
namespace kaldırıldı, host ağı ve fixture kaynakları aynı. Bekleyen UAC veya
eski canlı fixture yoktur; özel `.lab` ve bütün başarısız/PASS kayıtları
korundu. Sonraki native çalışma taze fixture/pin kullanır.

Yalnız harness'e dar setup tracing eklendi:
`vpn-windows-broker/scripts/validate-wireguard-v1.ps1` SHA256
`0f23feeeedbc5fcca83d35ec88ee2768d178d960514e38c30d860d41ace0c314`.
`setupAttempts`, connect gönderiminden **önce** oluşur; bounded request/reply
ID, event/generation, stage, mevcut async reader ve owner-exit zamanlarını
tutar. Profil, anahtar veya ham frame yazmaz. İki tamamlanan eski session,
kuyrukta eski reply/stale generation ve önceden açılmış reader ile üçüncü
setup mock akışı dahil **23 self-check PASS**; budget'lar değişmedi. Tek
paylaşılan reader'ın reuse/reset ve monotonik request ID akışında statik
inceleme/self-test bug kanıtlamadı. Ürün dosyaları değiştirilmedi. Eski 53
source receipt test anına aittir; bu tanı editinden sonra yeni paket yeniden
freeze ve bağımsız incelenmeden native çalıştırılmaz.

Sıradaki sınırlı Windows WG kabulü, router'ın Team quiet-window GO'sundan
sonra **aynı reconnect sırasını** izlemelidir; yalnız fresh inbound koşusu
üçüncü reconnect sorununu gizlememelidir. Client-death ve ortak OpenConnect/
OpenVPN native cleanup regresyonları henüz tamamlanmadı. Windows Team kabulü
ayrı worker kapsamındadır; aynı anda çakışan fixture açılmaz. Kurulu stable,
Desktop 0.5, özel veriler ve release/main/tag/update feed korunur; yayın yok.

---

# Önceki checkpoint — 30 Ekim 2026, pending cleanup gerçek kabulü / UAC bekliyor

**Güncel durum: normal disconnect PASS, iki kayıp vakası PASS; bütün Windows
kabulü tamamlanmadı.**
[Son checkpoint](../artifacts/vpn-lab/windows-wireguard-pending-20261030-v1-checkpoint.json),
SHA256 `7030d0486c4a58da0b9f1217eb8203d0f4937fd4b03365597dc3e717c4811ad7`.

Yeni pending politika ayrı implementer ve bağımsız verifier tarafından
incelendi: exact-owned kimlik native kayıttan kalkana kadar journal,
rezervasyon ve asıl motor nedeni korunur; ürün için keyfi 5 dakika cutoff
yoktur. Harness'in 12 dakikalık sınırı yalnız test gözlem sınırıdır. İki
harness dosyası güncellendi, 22 self-check geçti; 3 taze frozen paket, 53
kaynak hash'i ve özel fixture bağımsız GO aldı. Ürün dosyaları bu kabul
worker'ı tarafından değiştirilmedi.

- [Normal kabul](../artifacts/vpn-lab/windows-wireguard-pending-20261030-v1-normal.json)
  **PASS**: gerçek kernel peer, 64 KiB TCP SHA256 ve DNS; pending sırasında
  Sessions **3 ms**, Close **2 ms** ve exact `cleanup_pending` cevabı,
  owner korunması. Disconnect 2.257 s, tam cleanup 2.682 s; authenticated
  journal/session sıfır, native inventory ve tam baseline temiz. Ayrı
  elevated after audit sıfır journal gördü; recovery yapılmadı.
- [İlk kayıp koşusu](../artifacts/vpn-lab/windows-wireguard-pending-20261030-v1-loss-attempt1.json)
  **toplam FAIL; iki tamamlanan vaka korunur.** Boşta 55 s ve gerçek 10 s
  kısa kayıp sonrası yeni el sıkışması/TCP geçti. Kalıcı çift yön kayıp
  41.948 s'de algılandı, 86.469 s'de exact
  `engine_failed: wireguard_peer_unresponsive` ile cleanup tamamlandı;
  Sessions/Close 14/41 ms. Outbound-only kayıp 31.349/80.034 s, Sessions/Close
  5/45 ms. Her iki vakada normal native cleanup/baseline geçti; recovery yok.
- Üçüncü **inbound bağlantı hazırlığı**, kayıp uygulanmadan ve üçüncü vaka
  context'i oluşmadan `Broker test deadline exceeded` ile durdu. Mevcut
  rapor connect reply/connected bekleme aşamalarını ayırmadığından neden
  kanıtlanmadı; 45 s setup sınırı körlemesine artırılmaz. Önceki iki PASS
  bu koşunun tamamını PASS yapmaz. Failure-finally yalnız exact owner'ı
  kapattı; son salt okunur native inventory sıfır ve tam baseline aynıydı.

**Mevcut engel yalnız sonraki salt okunur inceleme için kullanıcı UAC
erişilebilirliği:** after-failure audit UAC isteği iptal oldu/zaman aşımına
uğradı, audit dosyası oluşmadı. Son protected journal sayısı bu nedenle
**bilinmiyor**; sıfır veya artakalan journal var denmez. Yeni UAC isteği
açılmadı, recovery veya native tekrar yapılmadı. Hazır birleşik wrapper
`.lab/read-wireguard-pending-loss-diagnostics-v1.ps1`, SHA256
`efcda71384e6d9f8cd104ec79200b1fef3eec3999ea95c81ab58b3cfdb24036d`,
önce original read-only after-client-death audit'i, sonra sabit admin ACL
kökünden bounded broker/watchdog tanı snapshot'ını toplar; recovery/silme
yapmaz. `-SelfTest` yalnız pin ve korunmuş başarısız koşu context'ini
doğruladı. Actual çalışma kullanıcı uygun olduğunda ve router GO'suyla yapılır.

Windows broker/WireGuard süreçleri kapalıdır. Özel kernel fixture/relay
`.lab/wireguard-windows-pending-20261030-v1` hâlâ hazırlık için açıktır;
15:43:20 UTC'de sona erer. `stop` ve `relay-stop` yalnız bu dizinin sahipli
stop marker'larıdır. İşlemden önce taze PID/birth/fixture/pin doğrulanır;
eski kimlikler varsayılmaz. Client-death, inbound loss, ortak OpenConnect/
OpenVPN cleanup regresyonu ve Windows Team native kabulü henüz yapılmadı.
Yeni network test/UAC kullanıcı yanıtından önce açılmaz. Kurulu stable,
Desktop 0.5, özel veriler ve release/main/tag/update feed korunur; yayın yok.

---

# Önceki checkpoint — 30 Ekim 2026, Windows v3 gerçek temizlik engeli

**Test altyapısı ve son journal temiz; Windows kabulü hâlâ FAIL.**
[Son checkpoint](../artifacts/vpn-lab/windows-wireguard-liveness-20261030-final-checkpoint.json),
SHA256 `e73551dd1573fedb4efedea928e7b7f64ffff0f447b902d46db488baeb2b0967`.

**Güncel sonuç: Windows OS kayıt temizliği engeli; v3 native kabulü FAIL.**
[Son rapor](../artifacts/vpn-lab/windows-wireguard-liveness-20261030-v3-blocked.json),
SHA256 `a859a3bf5f8a04123ed6775d50abb1d3c1ef7f77360b3766eec43d60d093d59f`,
[sabit alanlı gerçek broker/watchdog tanıları](../artifacts/vpn-lab/windows-wireguard-liveness-20261030-v3-diagnostics.json),
SHA256 `6a7eb2196191b662491498f7761c0ecac9b2464d26b6942c043a3f7b7acb6a3c`.
WireGuard kayıp algılaması çalıştı; normal adaptör temizliği tamamlanmadı.
Kayıp ACK'sinden 43.202 saniye sonra broker'ın gerçek `stop_requested`
tanısı `wireguard_peer_unresponsive` nedenini kaydetti; quiesce ACK 17 ms
sonra gönderildi. Wintun handle kapandı ve devnode kayboldu, fakat aynı
GUID/LUID kayıtlı kalınca 30 saniyelik dual-absence kontrolü başarısız oldu.
Terminal sonuç 74.394 saniyede `cleanup_required` idi. Owner-exit tekrarı ve
üç watchdog denemesi de `owned_guid_present_without_devnode` korumasında
durdu. Bu başarısızlık test deadline'ı değildir; korumalar gevşetilmedi.

Tam ağ baseline'ı geri geldi; başarısız koşunun elevated salt okunur audit'i
**bir** journal ve kayıtlı sahipli interface buldu, device/süreç sıfırdı. O zamanki
kimlik: session `46cc1494-8346-4659-b27b-811e8de3e393`, generation 1,
GUID `65ffcf66-d2d0-411f-9e10-1399557186eb`, LUID `14918723588587520`.
Canlı/registered kimlik için kurtarma veya journal silme yapılmadı. V3 fixture/relay stop marker ile kapandı;
yeni kaynak testi için eski PID/pin/fixture yeniden kullanılmaz.

**12:55:33 UTC salt okunur kanıtında aynı GUID/LUID hâlâ kayıtlıydı:**
[geç native snapshot](../artifacts/vpn-lab/windows-wireguard-liveness-v3-native-late-20261030.json).
Raw handle kapanışından **7 dakikadan fazla** sonra devnode/süreç sıfır,
registered identity bir kaldı. Bu nedenle önerilen 5 dakikalık async
reconciler bu gerçek OS durumunu çözemeyeceğinden **ertelendi ve uygulanmadı**.
Yalnız doğrulanan harness/tanı değişiklikleri vardır; ürünün cleanup
politikası, süreleri ve ownership retleri korunur. Yeni native deneme yapılmaz.

**12:58:19 UTC son durumunda native kayıt kendiliğinden kayboldu:**
[son salt okunur baseline/native snapshot](../artifacts/vpn-lab/windows-wireguard-liveness-v3-final-state-20261030.json)
devices/registered interfaces sıfır, tam Windows ağ baseline'ı aynı,
helper/WireGuard süreci yok, fixture/relay kapalı, Linux namespace/host ve
kaynaklar temizdir. Reboot/recovery yapılmadı. Kayıt 12:55:33–12:58:19
aralığında kayboldu; bu sonuç v3'ün normal cleanup FAIL'ini değiştirmez.

**Son ayrı guarded remediation kullanıcı UAC onayıyla PASS:**
[makbuz](../artifacts/vpn-lab/windows-wireguard-liveness-20261030-v3-remediation.json),
SHA256 `40d25fe89790507b1921864ce5d73d1326d8203ccae1b9c97009759b81c36840`.
Bağımsız incelenen wrapper önce exact session/GUID, admin ACL/reparse,
orijinal tam baseline, süreç yokluğu ve iki native görünümün yokluğunu
yeniden doğruladı; taze journal/set hash'ini ayrı salt okunur snapshot'a yazdı.
Native helper aynı set digest'ini machine lock altında tekrar doğrulayarak
yalnız bir test journal'ını tamamladı: journal **1→0**, oluşturulan adaptör ve
kaldırılan phantom **0**; baseline ve sıfır native envanter değişmedi.
Remediation v3 FAIL kaydını değiştirmedi. Bekleyen test süreci, fixture,
owned interface veya journal kalmadı; reboot yapılmadı ve gerekmiyor.

75 Rust test, özellik kapalı normal derleme, 16 runner self-check, Go 12
test + bir opt-in skip/`vet`, baseline hook (8 hook, TypeScript, 32 Vitest)
geçti; 53 kaynak hash'i son test/temizlik sonrası aynı. Sonraki geliştirme
bu Windows unregister gecikmesi için güvenli strateji belirlemeli; taze fixture/paketle
üç yönlü kalıcı kayıp ve normal cleanup kabulü gerekir; Windows tek yönlü
vakalar bu turda çalışmadı. Fiziksel suspend, macOS ve Windows Team native
uçtan uca kapsamı da yoktur. V1/v2 FAIL ve ayrı remediation makbuzları korunur;
aşağıdaki hazırlık notları tarihseldir.
Kurulu stable, Desktop 0.5 ve özel veriler korunur; release/main/tag/update
yayını yapılmadı.

---

# Önceki checkpoint — 30 Ekim 2026, Windows v3 tanılı kabul hazırlığı

V1 başarısızlığı aşağıda tarihsel olarak korunur. Bağımsız incelenen,
exact-set SHA256 ve mevcut ikiliye sabitlenmiş ayrı remediation, **bir** eski
test journal'ını kaldırdı; adaptör oluşturmadı, phantom kaldırmadı. Özgün tam
Windows ağ baseline'ı ve sıfır native envanter korundu. Bu sonuç v1 kabulünü
PASS yapmaz; kök temizlik nedeni kanıtlanmadı ve ürünün 30 saniyelik bekleme
kodu değiştirilmedi.

**V2 de PASS değildir.** Sağlıklı 55 saniye, gerçek 10 saniyelik çift yön kayıp,
yeni el sıkışması ve tekrar 64 KiB TCP geçti. Kalıcı çift yön kayıpta
`disconnecting` geldi; eski harness algılama ve adaptör kapanışını aynı
50 saniyeye sığdırdığı için terminal neden alınmadan durdu. `disconnecting`
tek başına WireGuard liveness nedeni kanıtı sayılmaz. Son ağ baseline'ı geri
geldi; ayrı after audit bir journal buldu. 12:32:18 UTC'de devnode sıfırken
bir kayıtlı sahipli GUID vardı; 12:32:34 UTC'de ikisi de sıfırdı, süreç yoktu.
Journal'da yalnız adaptör `undone=false`; diğer altı intent tamamdır. V2'nin
exact-set korumalı ayrı remediation'ı henüz çalıştırılmadı; journal korunur.

Güçlü inceleme sonrasında yalnız `privileged-validation` derlemelerine, sabit
ayrı admin ACL kökünde en çok 64 KiB sabit-alanlı tanı eklendi. Ham hata,
profil, parola veya anahtar yazılmaz; broker/watchdog kapanış nedeni, gerçek
adaptör handle kapanışı ve native kimlik/journal aşamaları kaydedilir. Normal
derlemede modül/hook'lar yoktur; ürün kapanış yetkileri ve süreleri değişmedi.
Yeni runner pending vakayı önceden tutar; kayıp ACK, algılama, quiesce ACK,
terminal, owner kill ve native örneklerinin monotonik zamanını kaydeder.
50 saniyelik algılama ile mevcut aşamalardan türeyen 85 saniyelik kapanış
bütçesi ayrıdır; tam başarı yine exact terminal neden, baseline, sıfır native
envanter ve ayrı sıfır-journal audit gerektirir. 75 broker test, normal özellik
derlemesi ve 16 runner self-check geçti. Taze v3 paketi ve birleşik ayrı
remediation/before-audit wrapper'ı son bağımsız incelemeyi bekler; native v3
henüz başlamadı. V2 fixture/relay kapandı, namespace ve Linux host baseline'ı
korundu. Kurulu stable/0.5 ve kullanıcı verileri korundu; yayın yapılmaz.

---

# Önceki checkpoint — 30 Ekim 2026, Windows v1 liveness kabulü engelli

**Güncel Windows sonucu: PASS değildir.**
[Engel raporu](../artifacts/vpn-lab/windows-wireguard-liveness-20261030-v1-blocked.json),
SHA256 `2be4bf6808aea7379b9c7f5273d982c0177ee9a155e2342ff2063a0c3785fb93`:
taze sabitlenmiş broker + son Windows WireGuard yardımcısı, gerçek Wintun
adaptörü ve özel Linux kernel peer'iyle el sıkışması, 64 KiB TCP SHA256,
normal Windows DNS ve 55 saniye boşta taze el sıkışmaları geçti. İlk kısa
paket kaybı uygulanmadan, PowerShell okumasıyla çakışan atomik relay receipt
değiştirmesi `WinError 5` verdi; harness kapandı ve kabul güvenli biçimde durdu.
Kısa/kalıcı çift veya tek yönlü Windows kayıp vakası tamamlanmadı.

Relay receipt için yalnız `PermissionError` üzerinde en çok 100 deneme ve
99 × 10 ms bekleme eklendi; kalıcı kilit hata olmaya devam eder. Geçici/kalıcı
kilit self-test'i ve bağımsız kaynak incelemesi geçti. Bu düzeltmeyle yeni
native koşu henüz yapılmadı. Broker 73, Go 12 test + bir opt-in skip,
`go vet`, runner 11 self-check ve baseline hook (8 hook, TypeScript, 32 Vitest)
geçti; ürün broker/WireGuard kaynakları bu Windows turunda değiştirilmedi.

**Yeni kabulden önce çözülmesi gereken temizlik engeli:** tam Windows
adres/rota/interface/DNS baseline'ı geri geldi; bounded salt okunur kontrolde
native device ve kayıtlı sahipli interface sayısı sıfır, broker/WireGuard
süreçleri yok. Buna rağmen ayrı elevated salt okunur audit **bir protected
journal** buldu. Exact-owned journal'da yalnız adaptör intent'i `undone=false`;
adres, MTU, iki rota, metric ve DNS `undone=true`. Journal/set SHA256 ve
admin ACL/kimlik doğrulaması raporda korunur. Nedeni güçlü incelemeye
yönlendirildi; recovery veya journal silme yapılmadı, yeni native koşu
başlatılmaz. Recovery sonucu eski başarısız kabulün yerine geçirilemez.

Özel kernel fixture stop marker ile kapandı, namespace silindi ve Linux host
ağı aynı kaldı. Relay süreci kapandı; eski PID/pin yeniden kullanılmaz.
İlk başarısız fixture kapanmadan retry kaynağı değiştiği için kendi
`sourceUnchanged=false` kaydı korunur. Kurulu stable, Desktop 0.5 ve özel
veriler korundu; release/main/tag/update yayını yapılmadı.

Kalan doğrulamalar: bu journal engeli sonrası taze Windows liveness kabulü,
Windows Team SSH/SFTP uçtan uca testi, fiziksel suspend, macOS ve güncel
geliştirme paket/kurucu kabulü. El sıkışmasını geçirip yalnız veri paketini
engelleyen seçici filtre liveness sinyalinin kapsamı dışındadır.

**Ayrı Linux Team paylaşım/iptal kabulü bağımsız PASS.**
[Logout'tan önce cleanup'ı zorunlu kılan son bağımsız tekrar](../artifacts/vpn-lab/native-team-shared-revocation-cleanup-20261030-verifier.json)
SHA256 `a8632cd566b6e1af3d803121caf747cf72ee8d416d81ca6e68df27e23bf604fc`:
iptal kapanışı 3.496 saniye; `logout` öncesinde journal/TUN sıfır kanıtı
alındı. Önceki raporlar aşağıda korunur.

[İlk rapor](../artifacts/vpn-lab/native-team-shared-revocation-20261030.json)
SHA256 `d23809f403e998d77fa3685ef0a9806f238c859ae46f1702767fc299a4dad6ee`,
[bağımsız tekrar](../artifacts/vpn-lab/native-team-shared-revocation-20261030-verifier.json)
SHA256 `043375c7fcedabf77a6281e49a0d66d0659caa8bb61c4001ce960347caa397fc`:
gerçek Team alıcısının connect-only paylaşımıyla WireGuard/SSH/SFTP ve SHA256
geçti. Alıcı yetkisi iptal edilince SSH/SFTP ve duraklatılmış aktarım
3.133 saniyede kapandı; yeniden bağlanma reddedildi, mevcut hedef korundu,
`.part` kalmadı. Sahibin bağımsız paylaşılan kopyası erişilebilir kaldı.
Fiziksel hedef TCP paketi, journal ve sahipli TUN sıfır; namespace temizliği
ve host ağı eşitliği geçti. Bu Linux alıcı-iptal testidir; sessiz UDP kaybı
veya Windows Team uçtan uca kabulü değildir.

---

# Önceki checkpoint — 30 Ekim 2026, Linux kabul turu ve temizlik tamamlandı

**Güncel sonuç:** WireGuard kernel 12/12, Team sessiz kayıp bağımsız PASS,
Linux servis kurulumu 10 PASS ve son native full-tunnel kabulü 65 PASS + 1 tanı.
[Gerçek Linux GP son raporu](../artifacts/vpn-lab/linux-company-globalprotect-final-20261030.json),
SHA256 `adb05999d0dc0e5679805f57b5035b78443a9e6b9b72990ddd401ebcfd63dd69`:
normal geliştirme masaüstünde kayıtlı kullanıcı/parola ve kullanıcının girdiği
OTP ile gerçek bağlantı kuruldu. Açık tam tünelde 267 rota, 2 DNS ve alan ayarları
doğrulandı; VPN arayüzüne sabitlenmiş tek TCP bağlantısı izin verilen hedefin
SSH tanıtımını aldı. **SSH girişi/uzak komut veya Microsoft tarayıcı SSO testi yapılmadı.**

Normal bağlantı kesme sonrası engine/TUN/journal/sahipli rota sayıları sıfır,
özel DNS/rota baseline'ı geri geldi. Geliştirme arayüzü, broker, denetlenen slirp,
router namespace'i ve kimliği doğrulanan özel misafir kapandı. WSL ana ağ/DNS
ve PostgreSQL/resolved servis kimlikleri aynı kaldı. Şifreli test kasasının
tutarlı kopyası, aslı ve yerel test parola dosyası özel `.lab`/misafir alanında
korundu; eski PID, oturum veya pin yeniden kullanılmaz. Kurulu stable ve 0.5
korundu; release/main/tag/update yayını yapılmadı.

Kalan doğrulamalar: Windows adaptörde yeni liveness, gerçek fiziksel suspend,
macOS ve güncel masaüstü kurucu/paket kabulü. Tam kapsam/yayın hazır sayılmaz.
Aşağıdaki kayıtlar tarihsel checkpoint'lerdir.

---

# Önceki checkpoint — 30 Ekim 2026, WireGuard kayıp algılama ve Linux kurulum

**Güncel durum bu bölümdür.** Aşağıdaki ikinci kabul turu ve önceki kayıtlar
tarihsel olarak korunur. Şirket Linux GP portal/OTP girişi geçti; tünel, rota
ön kontrolünde durdu. Özel Linux misafiri devam testi için açıktır. Release/main/tag/update yayını yapılmadı.
Kurulu stable TermTerm, 0.5 çalışma ağacı ve kullanıcı verileri korunur.

- **WireGuard sessiz UDP kaybı uygulandı, son kernel kabulü 12/12 PASS.**
  [Son rapor](../artifacts/vpn-lab/wireguard-liveness-20261030-final.json):
  boşta sağlıklı bağlantı, kısa kesinti sonrası toparlanma, kalıcı çift/tek yönlü
  kayıp ve gerekli peer kaybı sınandı. Peer başına 15 saniyede handshake probe,
  45 saniyede başarısızlık kullanılır; tekrar eden scheduler gecikmeleri süreyi
  sınırsız uzatmaz. El sıkışması geçerken yalnız veri paketlerini engelleyen
  seçici filtre bu yöntemle algılanmaz. Fiziksel suspend ve Windows adaptör
  liveness testi bu koşunun kapsamı değildir.
- **Gerçek Team → VPN → SSH/SFTP sessiz kayıp bağımsız PASS.**
  [Bağımsız rapor](../artifacts/vpn-lab/native-team-ssh-silent-loss-20261030-verifier.json),
  SHA256 `bcc21c6c5f1ad8d45a1fdcf09489c100aa6aa1c85922f565d3b6b5f26c2e40a6`:
  UDP kesilince SSH/SFTP kapandı, duraklatılmış aktarım iptal edildi, mevcut
  hedef korundu, `.part` kalmadı. Fiziksel hedef TCP paketi, journal ve sahipli
  TUN sayısı sıfır; namespace temizliği ve host ağı eşitliği geçti.
  [Normal kesme regresyonu](../artifacts/vpn-lab/native-team-ssh-manual-20261030-final.json)
  ve [son kaynak/ikili makbuzu](../artifacts/vpn-lab/wireguard-liveness-20261030-source-receipt.json)
  ayrıca korunur.
- **Gerçek özel Linux systemd kurulum kontrolü 10 PASS.**
  [Kurulum raporu](../artifacts/vpn-lab/linux-installed-services-20261030.json),
  SHA256 `0a09d25bfcd4abfbc16fd0f76c321f06d030e0b6d10e3e1d798aa4a30e66a03f`:
  Ubuntu 24.04 nspawn misafirinde paketli VPN birimi, root olmayan sabitlenmiş
  istemci, yeniden başlatma/durdurma ve boş günlük doğrulandı. Yanlış UID,
  sabitlenmemiş aynı-UID istemcisi ve yazılabilir ikili reddedildi. Gerçek
  systemd kurulumunda görülen peer executable doğrulama hatası, yalnız
  `CAP_SYS_PTRACE` eklenerek giderildi; sandbox ve kimlik denetimleri korundu.
  Eksik `libjsoncpp25` dahil referans Linux çalışma zamanı bağımlılıkları
  hazırlama manifestine ve [kılavuza](VPN_UNIX_KURULUM_TR.md) eklendi.
  Ayrı PostgreSQL 16/5433 üzerinde 66 migration temiz kuruldu ve yeniden
  doğrulandı; dar servis rolüyle Team `/ready` şema 66/API 1 döndürdü, doğrudan
  tablo okuması reddedildi, yeniden başlatma/durdurma geçti. WSL host rotaları,
  DNS ve PostgreSQL 18/resolved servis kimlikleri değişmedi. Bu test tam Linux
  masaüstü paketi, HTTPS istemci girişi veya şirket kimlik doğrulaması değildir.
- **Linux tam tünel bağlı ağ düzeltmesi bağımsız PASS: 65 kontrol + 1 tanı.**
  [Son rapor](../artifacts/vpn-lab/native-unix-full-tunnel-20261031-verifier.json),
  SHA256 `7a8b7fe57c77cf499e5d3e6af5aa129035dfaf74d1431325198e0faaa16a9708`:
  yalnız ham IPv4 `/0` planında, uç noktaya giden Ethernet/`veth` arayüzünün
  gerçek adresiyle eşleşen daha dar `kernel/link` öneki korunur. Tünelin atanmış
  alt ağı, split/custom plan, elle eklenen rota, başka VPN ve belirsiz metadata
  için engeller sürer. Korunan fiziksel önekte VPN DNS bulunursa mutation öncesi
  `dns_conflicts_with_physical_underlay` ile reddedilir; ilk incelemenin bulduğu
  DNS kaçış riski giderilip gerçek sunucuyla negatif test edildi.
  Üç motor × normal/istemci-ölümü/broker-ölümü tam tünel, OpenConnect tam tünel +
  DNS'nin üç kapanış biçimi, gerçek FQDN/kısa ad ve kaynak/host/namespace temizliği
  geçti. Bridge/bond/VLAN/TUN/TAP istisnası ve macOS tam tünel doğrulaması yok.
  [Kapsam ve sınırlar](VPN_UNIX_KURULUM_TR.md#linux-tam-tünel-ve-fiziksel-ağın-korunması).
- **Şirket Linux GP girişi geçti; gerçek tünel henüz kurulmadı.**
  Normal Linux arayüzündeki ilk deneme, kayıtlı kullanıcı/parolaya rağmen boş
  giriş formu gösterdi; kurumsal doğrulama gönderilmeden iptal edildi. Unix
  broker yalnız OpenVPN'in `username/password` alanlarını tanıyordu. Artık
  Windows ile aynı Rust politika denetimi, OpenConnect'in `user/passwd`
  alanlarını da ilk sır formunda bir kez kullanır. OTP işaretleri ve sonraki
  formlar kayıtlı parola alamaz. Paylaşılan politika alt sürecinin istek/yanıtı
  tek kilitte tutulur; eşzamanlı WireGuard kurulumu yanıtları karıştıramaz.
  [Yerel odaklı kabul](../artifacts/vpn-lab/linux-saved-auth-20261030-implementation.json)
  gerçek Python oturum döngüsü/Rust politika sınırını, gizli verinin metadata
  isteğine girmemesini, hata halinde durmayı ve 25+25 eşzamanlı isteği doğrular;
  Rust çekirdeğinde 17 test geçti.
  Yeni broker betiği ile yeniden derlenmiş `vpn-policy-lab` birlikte
  hazırlanmalıdır; eski politika ikilisi yeni işlemi reddeder ve bağlantı
  güvenli biçimde durur. Bağımsız [native servis tekrarı](../artifacts/vpn-lab/native-unix-saved-auth-20261030-final.json)
  **55 PASS + 1 tanı**, [son WG motorlu tekrar](../artifacts/vpn-lab/native-unix-saved-auth-20261030-final-wg.json)
  da **55 PASS + 1 tanı** verdi. Bunlar sonraki motor tanılama ve devam eden
  Linux full-tunnel değişikliklerinden önceki kaynak checkpoint'ine aittir.
  [Hazırlık kaydı](../artifacts/vpn-lab/linux-company-preparation-20261030.json)
  sabitlenmiş OpenConnect 9.21 motorunu ve ayrı VPN geliştirme masaüstünü
  kaydeder. Gerçek WSLg masaüstü, test kasası ve şifreli GP profili ayrı
  geliştirme kimliğiyle açıldı; Windows kasası okunmadı.
  İlk yeniden denemedeki genel giriş hatasını ayırmak için motorun mevcut hata
  kodu öneki korunarak yalnız sayısal kütüphane sonucu, sabit callback evresi/
  nedeni ve form/yanıt sayıları eklendi. Ham sunucu mesajı, parola, OTP ve
  token tanılama çıktısına yazılmaz. Gerçek yerel callback testleri; normal
  parola ve OTP başarısı, eski/bozuk/kapalı kanal, sertifika/SSO reddi ve
  sır içermeyen sınırlı çıktı için bağımsız geçti.
  [Ön giriş tanısı](../artifacts/vpn-lab/linux-company-prelogin-diagnostic-20261030.json)
  `result=-5 stage=none callback=none forms=0 replies=0` gösterdi. Bu deneme
  kimlik bilgisi istemeden durmuştu; parola geçerliliğini sınamadı.
  Ayrı test ortamının denetimsiz slirp süreci kapanmış, `tap0` ve varsayılan
  rotası kaybolmuştu. Teste özel geçici systemd birimiyle egress denetlendi;
  tek DNS/TCP/TLS kontrolü sertifika doğrulaması açıkken geçti, HTTP veya kimlik
  bilgisi göndermedi. Ana sistem rota/DNS ayarları değiştirilmedi.
  [Gerçek şirket giriş checkpoint'i](../artifacts/vpn-lab/linux-company-portal-gateway-20261030.json):
  normal arayüzde kayıtlı kimlikle portal girişi, kullanıcının girdiği OTP ve
  gateway seçimi geçti; gelen ağ planı `existing_route_conflict` ön kontrolünde
  reddedildi. Sahipli TUN/journal oluşmadı. Başarısız plan önceden tutulmadığından
  gerçek şirket planının split/full türü veya çakışan öneki tahmin edilmez.
  Ayrı sentetik inceleme Unix full-tunnel ön kontrolündeki bağlı rota eksikliğini
  gösterdi; yukarıdaki sınırlı düzeltme ve bağımsız kabul tamamlandı. Bu giriş başarısı, tünel
  veya SSH veri yolu doğrulaması değildir; tahmin için yeni şirket girişi yapılmaz.
  [Sonraki özel test altyapısı](../artifacts/vpn-lab/linux-company-veth-underlay-20261030.json)
  misafire normal `veth` Ethernet alt ağı verir; slirp TUN ayrı router
  namespace'indedir. Yönlendirme/NAT/DNS yalnız bu iki özel alandadır; root
  ağ/çözümleyici baseline'ı aynı kaldı. DNS/TCP/TLS kontrolü doğrulandı,
  ürünün rota güvenlik denetimlerine test istisnası eklenmedi. Şirket tekrarına
  full-tunnel düzeltmesinin bağımsız kabulünden sonra geçilecek.
  Misafir/egress ve arayüz devam testi için açık, test kasası korunur. Sonraki
  görev kendi süreç/namespace ve geçici birim kimliğini doğrulamadan işlem yapmaz.

Kalan işler: doğrulanan Linux full-tunnel düzeltmesiyle gerçek şirket
Linux tüneli/erişim sonucu, bu özel misafirin son temizliği, Windows adaptörde yeni
liveness kabulü, ayrı güncel geliştirme paketleri ve kurucu doğrulaması.
macOS ortamı yok; Mac/ücretli CI çalıştırılmayacak. IPv6 ve gelişmiş OpenVPN
`DOMAIN` DNS politikası mevcut güvenli ret sınırlarında kalır. Yayın yapılmaz.

---

# Önceki checkpoint — 30 Ekim 2026, ikinci sınırlı kabul turu TAMAMLANDI

Bu ikinci turun test/fixture oturumları kapandı; bu sonuç VPN geliştirmesinin veya
yayın hazırlığının tamamlandığı anlamına gelmez. Güncel durum bu bölümdür;
aşağıdaki ilk 30 Ekim checkpoint'i, başarısız raporlar ve eski devirler tarihsel
olarak korunur. Canlı test/UAC/fixture oturumu kalmadı. Kurulu stable
uygulama, Desktop/termterm 0.5 kaynakları ve kullanıcı verileri korunur;
release/main/tag/update feed değişmedi, yayın veya dağıtım yapılmadı.

- **Linux kısa-ad düzeltmesi bağımsız doğrulandı: 54 PASS + 1 tanı.**
  [Bağımsız rapor](../artifacts/vpn-lab/native-unix-service-20261030-search-domains-verifier.json),
  SHA256 `c29283f58e79410629580767f4d0a4b5f317b427c733bcc850897bff071b453c`.
  `searchDomain` sonekleri Linux'ta düz arama alanı, yalnız `dnsDomains` içinde
  bulunanlar `~alan` biçiminde route-only alan olarak uygulanır; birleşik DNS
  çakışma politikası, eski journal temizliği ve macOS yardımcı payload'ı korunur.
  Özel network/mount/D-Bus alanında systemd-resolved 255 ile gerçek OpenConnect
  `default-domain + split-dns` ve OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX` tünellerinde
  nonroot `socket.getaddrinfo`, hem FQDN hem düz `probe` adını çözdü. Altı ayrı
  protokol × kapanış vakasının her birinde iki sorgu da otorite DNS cevap
  sayısını artırdı. Disconnect, istemci ölümü ve broker ölümü/yeniden başlatma
  sonrası DNS/rota baseline'ı, yabancı DNS ayarı, boş journal ve interface
  temizliği geçti; iki ad da tünel öncesinde ve sonrasında çözülemedi. Host
  ağı/resolver/servis kimliği ve kaynaklar değişmedi; namespace'ler temizlendi.
  Gerçek OpenVPN `DOMAIN` politikası hâlâ mutation öncesinde
  `advanced_dns_policy_unsupported` ile reddedilir. Önceki bölümlerdeki
  “Linux kısa-ad araması doğrulanmadı” sınırı bu dar laboratuvar kapsamı için
  kalktı; hosta kurulu servis, macOS, şirket Linux SSO ve mutlak OS DNS
  sızıntısızlığı doğrulanmış değildir.
- **Windows örtüşen 505 rota normal ve istemci-ölümü kabulü bağımsız PASS.**
  Doğrulanan native planlarda örtüşen rota uyumluluk engeli **kapandı**.
  [V4 normal kabul](../.tools/vpn-windows/nested505-acceptance-20261030-v4-normal/dns-v1-normal-full-tunnel-many-routes-result.json),
  SHA256 `39634b9e7d38f5297219746dce652ddc79dd247ce3efcb4dac2b080ea83fb263`,
  cleanup **10915 ms**; [V5 istemci-ölümü kabulü](../.tools/vpn-windows/nested505-acceptance-20261030-v5-client-death/dns-v1-client-death-full-tunnel-many-routes-result.json),
  SHA256 `67a66f88d5c0dc5ea205a616061cdedd13639212242921f331acefc77a0978ca`,
  cleanup **17232 ms**. Her koşuda 505 farklı exact rota doğrulandı: 128 tek
  sayılı `/32` `LOCAL (2)`, 377 `NETMGMT (3)`. Tek/çift nested hedefler
  `10.88.0.2`, `10.88.0.3` ve `10.88.0.54` için taze TCP/veri ve zorunlu DNS
  kontrolleri iki koşuda da çakışma öncesi/sonrası geçti; mevcut bağlantı korundu.
  İsteğe bağlı PowerShell `cmdletShortName` tanısı **FAIL** kalır.
  Final baseline ve boş native envanter iki koşuda da doğrulandı. Ayrı yükseltilmiş
  [normal before](../.tools/vpn-windows/nested505-acceptance-20261030-v4-normal/dns-journal-audit-before.json),
  [normal after](../.tools/vpn-windows/nested505-acceptance-20261030-v4-normal/dns-journal-audit-after-normal.json),
  [istemci-ölümü before](../.tools/vpn-windows/nested505-acceptance-20261030-v5-client-death/dns-journal-audit-before.json)
  ve [istemci-ölümü after](../.tools/vpn-windows/nested505-acceptance-20261030-v5-client-death/dns-journal-audit-after-client-death.json)
  audit'lerinin hepsi protected journal **0**, `recoveryPerformed: false`.
  Önceki UAC iptalleri tarihseldir; normal after audit sonradan yeniden çalışıp
  geçti. Açık audit/UAC yanıtı veya istemci-ölümü testi beklemiyor.
- **Bütün test/UAC/fixture oturumları kapandı; eski v4/v5 pinleri emekli.**
  [V5 fixture cleanup raporu](../artifacts/vpn-lab/nested505-fixture-20261030-v5.json),
  SHA256 `b1ff7ea4b523e757bc6655c319524d3ade250f899e512d14f2ad37f444f26596`:
  **35 lab PASS**, completed/hostNetworkUnchanged/namespacesRemoved/
  sourceUnchangedDuringTest=true, productionInstallationTouched=false.
  [V5 kapanış makbuzu](../artifacts/vpn-lab/nested505-fixture-retired-20261030-v5.json)
  controller 2009'un yokluğunu, boş fixture namespace listesini ve özel
  profil/CA/readiness dosyalarının kaldırıldığını doğruladı.
  [Son native envanter](../artifacts/vpn-lab/nested505-final-native-inventory-20261030-v5.json)
  kilitli helper `9ae110…`, devices/registered interfaces boş, safe/readOnly=true,
  recovery=false. [V4 kapanışı](../artifacts/vpn-lab/nested505-fixture-retired-20261030-v4.json)
  da korunur; bu fixture pinleri yeniden bağlantı için kullanılamaz.
- **Desktop readiness assertion'ları düzeltildi; GUI yeniden koşulmadı.**
  [Desktop profil testi](../tests/desktop/vpn-profiles.mjs) gerçek `available`
  durumunu/ret nedenini denetler; isteğe bağlı `TERMTERM_VPN_EXPECT_BUNDLE=1`
  bundle kapısı korunur. Node syntax kontrolleri geçti. Root'un bu son doküman
  güncellemesinden önceki hook **8**, tsc ve Vitest **32** kontrolleri geçti.
- **Önceki Windows sekiz iç içe rota native tanısı bağımsız PASS.**
  Üretim düzeltmesi bütün rotaları koruyup yalnız ekleme sırasını en özel prefix
  önce olacak biçimde değiştirir. Üretim undo/journal ve strict cleanup kuralları
  gevşetilmedi. Beklenen OS metadata farkı yalnız tanı doğrulamasında sınırlandı. [V4 gerçek native raporu](../.tools/vpn-windows/overlap-route-order-20261030-v4/privileged-v3-nested-routes-result.json),
  SHA256 `fa51bf0f648bae771566451b770c384b608ec354ebbb0a696e65ae464be9ab96`,
  bağımsız doğrulandı: sekiz rotanın tümü eklendi ve tam küme readback'i geçti.
  Tam sekiz undo kanıtı var: yedi `owned-NETMGMT`, bir `absent`; sonuncusu parent
  rota kaldırıldıktan sonra OS'nin broadcast `/32` satırını silmesidir. Final
  ağ baseline'ı geri döndü; önce/sonra native envanter boş ve journal sayısı 0.
  Bu sekiz rotalı tanı gerçek VPN kimlik doğrulaması veya veri trafiği kabulü
  değildir. Taze 505 rota normal/istemci-ölümü gerçek kabulü yukarıdadır.
  [Tanı v1](../.tools/vpn-windows/overlap-row-diagnostic-20261030-v1/privileged-v3-nested-routes-result.json)
  otomatik broadcast `/32` satırının `protocol=LOCAL (2)` kaynağını kanıtladı;
  önceki “broadcast kaynağı kanıtlanmadı” ifadesi bu kanıtla aşıldı.
  [V2 en özel prefix önce tanısı](../.tools/vpn-windows/overlap-route-order-20261030-v2/privileged-v3-nested-routes-result.json),
  SHA256 `c15fefe54ec0a513d6aee821fe164932fcbd9b2a3eb224744bca1bc74aadaf8f`,
  sekiz rotayı ekledi; diğer öznitelikler korunurken strict protokol readback'i
  `LOCAL` nedeniyle **FAIL** oldu. [V3 FAIL tanısı](../.tools/vpn-windows/overlap-route-order-20261030-v3/privileged-v3-nested-routes-result.json),
  parent kaldırıldıktan sonra broadcast satırının hâlâ bulunmasını yanlış
  varsaydı; bu bir **tanı assertion hatasıdır, üretim cleanup başarısızlığı
  değildir**. V2/V3 son baseline/native/journal temizliği geçti. Bütün eski
  başarısız raporlar korunur.

Kalan somut işler: gerçek macOS ve şirket Linux SSO, hosta kurulu üretim servisi
ve IPv6 broker doğrulaması/desteği; güncel ayrı geliştirme paketi/ekranları ve
kurucu doğrulaması. Yeni kurucu veya release üretilmedi/yayımlanmadı.
WireGuard sessiz UDP kaybı için peer başına 15 saniye handshake probe ve 45 saniye
başarısızlık eşiği uygulandı. Tek scheduler-grace sertleştirmesini içeren son ikiliyle
[kernel kabulü](../artifacts/vpn-lab/wireguard-liveness-20261030-final.json) **12/12** geçti;
kalıcı çift/tek yön ve çok-peer kayıpları 45.001–45.100 saniyede hata verdi.
[Gerçek native Team sessiz UDP kaybı](../artifacts/vpn-lab/native-team-ssh-silent-loss-20261030.json)
**PASS**: SSH/SFTP kapandı, duraklatılmış aktarım iptal edildi, mevcut hedef korundu,
`.part` kalmadı, fiziksel hedef TCP paketi 0; journal/TUN 0 ve namespace/host temizliği geçti.
[Normal kesme regresyonu](../artifacts/vpn-lab/native-team-ssh-manual-20261030-final.json)
da geçti. [Kaynak parmak izi](../artifacts/vpn-lab/wireguard-liveness-20261030-source-receipt.json)
son yardımcı ve native örnek SHA256 değerlerini taşır. Windows/WSL Go 12 PASS + bir opt-in skip,
vet PASS. Bağımsız native tekrar, Windows adaptör liveness, fiziksel suspend ve macOS bekliyor.
[Davranış ve sınırlar](VPN_WIREGUARD_TR.md). El sıkışması geçerken yalnız veri paketini
engelleyen seçici filtre bu ölçümle algılanmaz.
Team keyfi ağ kaybı ve mutlak OS DNS sızıntısızlığı da doğrulanmadı.
Bu turdaki native overlap uyumluluğu kapandı; VPN geliştirmesinin bütünü veya
yayın hazırlığı tamamlandı sayılmaz. Kurulu stable/0.5 ve release korumaları sürer.

---

# VPN çalışma devri — 30 Ekim 2026, sınırlı doğrulama turu TAMAMLANDI

`codex/feature-vpn-workspace` dalındaki bu sınırlı doğrulama turu tamamlandı;
**VPN geliştirmesinin bütünü veya yayın hazırlığı tamamlandı sayılmadı**.
Açık UAC/test/fixture oturumu kalmadı. Bu checkpoint güncel durumdur. Aşağıdaki
29 Ekim duraklatma ve daha eski kayıtlar tarihsel olarak korunur; o kayıtlardaki
eski engeller veya açık bağlantı ifadeleri güncel durum sayılmaz. Kurulu stable
uygulama, Desktop/termterm 0.5 kaynakları ve kullanıcı verileri korunur;
release/main/tag/update feed değişmedi, yayın veya dağıtım yapılmadı.
Bu turda kurucu üretilmedi/çalıştırılmadı; kurulu stable uygulamaya dokunulmadı.

Doğrulanmış ilerleme:

- **Unix journal yarışı giderildi ve bağımsız doğrulandı.** `tunnel-ready`
  yapılandırması mevcut broker kilidi altında çalışır; mutation, durable save
  ve ağ uygulaması cleanup ile aynı sıraya alınır. Eski kodda hata üreten
  deterministik add/add ve add/delete regresyonları, ağ uygulamasından önce
  intent kalıcılığı ve başarısız cleanup sonrası intent koruması geçti.
- **Gerçek Linux resolved DNS ve servis kabulü: 54 PASS + 1 tanı.**
  [Sakin tekrarın bağımsız raporu](../artifacts/vpn-lab/native-unix-service-20261030-openvpn-dns-verifier-quiet.json),
  özel network/mount/D-Bus alanında systemd-resolved 255 ve gerçek
  OpenConnect/AnyConnect ile **OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX`**
  tünellerinden ayrı taze DNS adları/cevap kayıtlarıyla nonroot `socket.getaddrinfo` çözümünü
  doğruladı. Disconnect, istemci ölümü ve broker SIGKILL/restart sonrası
  DNS/rota baseline'ı, yabancı DNS ayarı, boş journal ve interface temizliği
  geçti. Host resolver yapılandırması ve resolved servis kimliği değişmedi;
  kaynaklar değişmedi ve namespace'ler temizlendi. Gerçek eşzamanlı üç tünel,
  üç intent ile crash/recovery ve kernel WireGuard peer'i de kapsamda.
  Gerçek OpenVPN `DOMAIN` split-DNS planının ağ mutation'ından önce beklenen
  güvenli reddi de geçti. Önceki [50 kontrol raporu](../artifacts/vpn-lab/native-unix-service-20261030-resolved-verifier.json)
  korunur. [İlk 54 kontrollü bağımsız koşu](../artifacts/vpn-lab/native-unix-service-20261030-openvpn-dns-verifier.json),
  iç kontroller geçmesine rağmen dış host ağ snapshot'ı eşleşmediğinden toplam
  **FAIL** oldu ve korunur. Neden kanıtlanmadı; eşzamanlı Windows çalışması
  yalnız olasılıktır. Sakin tekrarın host/source/namespace kapıları geçti.
- **WebKit process-main fixture'ı geçti:** beş gerçek native yanıt kontrolü
  ve temiz **exit 0**; ephemeral view ve özel CA korunur. TLS/sandbox
  gevşetilmedi. [Bağımsız rapor](../artifacts/vpn-lab/native-webkit-20261030-independent.json).
- **Team kontrolleri geçti:** servis 7/7, client 42/42, Vitest 32/32, tsc,
  Team UI 11/11 ve toplam VPN UI 33/33. UI sonuçları **MOCK IPC** kapsamındadır. Gerçek PostgreSQL clean-66,
  65→66 korunmuş kayıtlar, backup/restore/checksum/crash-resume/readiness,
  eski VPN istemcisi reddi ve gerçek HTTPS/AEAD/offline/replay/revoke
  kontrolleri geçti. Hesap/RLS/auth kontrolleri ve açık API whitelist doğrulandı:
  111 imza, 110 farklı API adı; `api_chain_finish` iki açık overload içerir.
  şema 66/cache 9 ve uygulanmış 066 migration hash'i korunur.
  [Team kapsamı ve test kaynakları](VPN_TEAM_TR.md).
- **Gerçek Linux Team VPN → SSH/SFTP bağımsız kabulü geçti.** Gerçek HTTPS,
  şifreli Team profili/eşlemesi ve `requiredVPN` çözümüyle WireGuard üzerinden
  SSH oturumu ve SFTP SHA256 kontrolü doğrulandı. Duraklatılmış aktarım sırasında
  elle `vpn_disconnect` mevcut hedefi korudu; `.part` kalmadı ve bağlantılar
  kapandı. Fiziksel arayüzde hedefe TCP paketi **0**; journal/owned TUN boş,
  host ağı değişmedi ve namespace'ler temizlendi.
  [Bağımsız rapor](../artifacts/vpn-lab/native-team-ssh-20261030-verifier.json),
  SHA256 `1fe9ef908fc699c73108e4f78cbe7f3793ca88767802d466527d683d3b594947`.
- **Eski Windows owner-death kabulünün eksik audit'i tamamlandı:**
  30 Ekim salt okunur after-client-death audit'inde protected journal **0**,
  recovery yapılmadı.
  [Audit raporu](../.tools/vpn-windows/wireguard-acceptance-20261029-v2-client-death/dns-journal-audit-after-client-death.json).
- **Windows EOF hata önceliğinin kaynak/test düzeltmesi bağımsız geçti.**
  Dört dosyada Go motoru `failed` olayını paket kanalını kapatmadan önce iletir;
  broker typed EOF'ta aktarımı hemen durdurur ve yalnız son hata tanısı için
  en çok **1 saniye** bekler. Önceki hatalar, iptal, bozuk paket ve stale
  generation korunur; geç gelen readiness bağlantıyı yeniden canlandıramaz.
  Bağımsız **66 unit + 8 integration**, Go kontrolleri, UAC'siz gerçek
  yardımcı/kernel-peer kontrolü, tsc ve 32 Vitest geçti; frozen bundle incelemesi
  **GO**. Bu, yeni Wintun/rota/DNS kabulünün tamamlanması değildir.
  Kaynaklar: [Go motoru](../vpn-wireguard/main.go),
  [Go regresyonu](../vpn-wireguard/main_test.go),
  [typed EOF](../vpn-windows-broker/src/windows/pipe.rs),
  [broker hata sırası](../vpn-windows-broker/src/windows/runtime.rs).
- **Windows rota tanısı iyileştirmesi bağımsız geçti:** sorgu aşaması,
  native sonuç kodu ve destination artık güvenli hata bağlamında korunur.
  Kaynak incelemesi ve **67 unit + 8 integration** geçti; sahiplik/ret kapıları
  gevşetilmedi. Bu tanı değişikliği iç içe rota uyumluluğunu çözmüş sayılmaz.
- **Windows WireGuard v2 gerçek yanlış-anahtar kabulü PASS.** Doğru
  `engine_failed: wireguard_peer_handshake_timeout` alındı; hiç `connected`
  olmadı. Normal doğrulanmış cleanup, before/after native envanter **0**,
  son ağ baseline'ı ve ayrı before/after protected-journal audit'leri **0**;
  recovery yapılmadı.
  [V2 gerçek kabul](../.tools/vpn-windows/wireguard-acceptance-20261030-v2-wrong-key/wireguard-v1-wrong-key-result.json),
  [before audit](../.tools/vpn-windows/wireguard-acceptance-20261030-v2-wrong-key/dns-journal-audit-before.json),
  [after audit](../.tools/vpn-windows/wireguard-acceptance-20261030-v2-wrong-key/dns-journal-audit-after-normal.json).
- **Çakışmayan 505 rotalı Windows gerçek kapasite/cleanup kabulü PASS:**
  normal ve istemci-ölümü koşularında **505 owned rota**, tünel veri trafiği,
  DNS, zorunlu normal uygulama kısa-ad çözümü ve çakışmada mevcut bağlantının
  korunması geçti. Her iki koşuda son ağ baseline'ı geri döndü, native envanter **0** ve
  ayrı before/after protected-journal audit'leri **0**; recovery yapılmadı.
  Cleanup normalde **12559 ms**, istemci ölümünde **14221 ms** sürdü.
  İsteğe bağlı PowerShell `Resolve-DnsName` bare-shortname tanısı **FAIL**
  kalır; bütün DNS tanılarının geçtiği sonucu verilmez.
  [Normal kabul](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-normal/dns-v1-normal-full-tunnel-many-routes-result.json),
  [istemci-ölümü kabulü](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-client-death/dns-v1-client-death-full-tunnel-many-routes-result.json),
  [normal before audit](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-normal/dns-journal-audit-before.json),
  [normal after audit](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-normal/dns-journal-audit-after-normal.json),
  [istemci-ölümü before audit](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-client-death/dns-journal-audit-before.json),
  [ayrı istemci-ölümü after audit](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-client-death/dns-journal-audit-after-client-death.json).
  WireGuard ve bu iki 505 kabulünün bağımsız native kanıt incelemesi **GO**.
- **Son kaynak/hook kontrolleri PASS:** Git Bash üzerinden stdin `{}`,
  whitespace, 8 hook kontrolü, tsc ve Vitest 32/32 geçti.
- **Son fixture temizliği doğrulandı; canlı oturum kalmadı.** Yalnız sahip
  olunan fixture marker'larıyla cleanup yapıldı; yabancı süreç öldürülmedi.
  Launcher/relay/helper süreçleri yok, Windows salt okunur native envanterinde
  devices/registered interfaces **0**, izlenen Linux controller'ları ve fixture
  namespace'leri yok. Disjoint fixture raporu completed/hostUnchanged/
  namespacesRemoved/sourceUnchanged=true ve **35 lab PASS**; kurulu stable'a
  dokunulmadı.
  [Son cleanup özeti](../artifacts/vpn-lab/windows-final-cleanup-summary-20261030.json),
  [Windows salt okunur envanter](../artifacts/vpn-lab/windows-final-native-cleanup-20261030.json),
  [Linux son kontrol](../artifacts/vpn-lab/windows-fixture-linux-final-20261030.json),
  [kapanmış disjoint fixture raporu](../artifacts/vpn-lab/disjoint505-fixture-20261030-v1.json).

Açık işler ve sınırlar:

- **İç içe/örtüşen 505 rotalı Windows planı: açık uyumluluk sınırı, normal
  gerçek kabul FAIL.** Policy planı kabul ederken apply/preflight,
  `route_already_exists_or_unqueryable` ile intent öncesinde duruyor. V3'te
  owned arayüzde **`10.88.0.1/32` exact sorgusu `code=0`** döndürdü.
  Sorgunun longest-prefix sonucu döndürdüğü hipotezi salt okunur probe ile
  çürütüldü; satırın otomatik broadcast kaynaklı olduğu **kanıtlanmadı**.
  Bu yalnız fixture kusuru olarak kapatılamaz. Mevcut satır benimsenmedi veya
  atlanmadı; hiçbir sahiplik/güvenlik kapısı gevşetilmedi.
  V3 sonrası ağ baseline'ı/native envanter doğrulandı ve ayrı after audit
  protected journal sayısı **0**; **örtüşen planın istemci-ölümü koşusu
  çalıştırılmadı**.
  [V3 başarısız normal kabul](../.tools/vpn-windows/many-routes-acceptance-20261030-v3-normal/dns-v1-normal-full-tunnel-many-routes-result.json),
  [V3 after audit](../.tools/vpn-windows/many-routes-acceptance-20261030-v3-normal/dns-journal-audit-after-normal.json).
  [İlk V1 başarısız kabul](../.tools/vpn-windows/many-routes-acceptance-20261030-v1-normal/dns-v1-normal-full-tunnel-many-routes-result.json)
  korunur. Geçen **çakışmayan 505 rota** normal/istemci-ölümü stresi ayrı
  kapasite/cleanup kanıtıdır; **örtüşen planın uyumluluk sınırını kapatmaz**.
  Önceki [WireGuard v1 yanlış-anahtar FAIL raporu](../.tools/vpn-windows/wireguard-acceptance-20261030-v1-wrong-key/wireguard-v1-wrong-key-result.json)
  `packet_size_invalid` tanısıyla tarihsel kanıt olarak korunur.
  Geliştirme paketlerindeki `distributionReady: false`
  uyarısı korunur; kaynak/bundle incelemesi yayın veya kurulum onayı değildir.
- Team SSH/SFTP kabulü elle `vpn_disconnect` kapsamındadır; **keyfi ağ
  kesintisi ve Windows/macOS Team uçtan uca testleri yapılmadı**.
- OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX` FQDN çözümü geçti; **`DOMAIN`
  split-DNS politikası** `advanced_dns_policy_unsupported` ile beklenen biçimde
  ağ mutation'ından önce reddedilir, desteklenmiş sayılmaz.
  [İlk başarısız DNS koşusu](../artifacts/vpn-lab/native-unix-service-20261030-resolved-v1.json)
  korunur. Geçen DNS kapsamı OpenConnect ve belirtilen OpenVPN FQDN alt kümesidir; Linux kısa-ad araması,
  hosta kurulu üretim servisi ve mutlak OS DNS sızıntısızlığı doğrulanmış değildir.
- Gerçek macOS ve şirket Linux SSO doğrulaması yok; IPv6 broker desteği yok.
  Son ayrı geliştirme paketi/ekranları ve kalan kabul/bağımsız kontroller
  tamamlanmadan yayın hazırlığı sonucu verilmeyecek.

Sonraki çalışma öncelikleri: örtüşen planın policy kabulü ile prepare reddi
arasındaki açık uyumluluk sınırını çözmek veya
açık ürün kararıyla sınırlandırmak; ardından kapsamda kalan geliştirme paketi
ve platform kontrollerini tamamlamak. Disjoint kabul, örtüşen satırları benimseme,
atlama veya başka semantik workaround için onay değildir.

---

# VPN çalışma devri — 29 Ekim 2026, kullanıcı isteğiyle DURAKLATILDI

Worktree: `<VPN_WORKTREE>`, dal
`codex/feature-vpn-workspace`. Bütün kaynaklar kaydedilmiş, commit edilmemiş durumda.
Kullanıcı durdurdu: yeniden devam isteği olmadan kod, derleme, test, ağ veya UAC
işlemi yapılmayacak. Kurulu stable uygulama başlatılmadı, kaldırılmadı veya
değiştirilmedi; Desktop/termterm 0.5 kaynakları, kullanıcı verileri ve release /
main / tag / update feed korunur. Yayın veya dağıtım yapılmadı.

**İlk devam işi, yayın hazırlığını engelleyen Unix journal yarışıdır; henüz
düzeltilmedi.** Bağımsız verifier, `scripts/vpn-unix-broker.py:160–170` içindeki
`Journal.save` ve `configure:450–453` ile `scripts/vpn-lab-broker.py:279–281`
yolunda mutation+save işlemlerinin aynı broker kilidi altında olmadığını yeniden
üretti: A'nın eski serialized snapshot'ı bekletilir; B `{A,B}` kaydını yazar;
A devam edip diski `{A}` ile ezer. Bellekte iki intent kalırken diskte biri
kaybolur; çökme sonrası sahip olunan ağ temizliği eksik kalabilir. Journal
**mutation ve save birlikte** serileştirilecek, bu sıra deterministik regresyon
testiyle korunacak, eşzamanlı servis/çökme-recovery kontrolleri yeniden koşulacak.
Geçen Linux raporu bu bağımsız bulguyu geçersiz kılmaz; hazır sonucu verilmez.

## Gerçek Windows WireGuard kabulü ve durdurma

Root, bağımsız Windows verifier'ın GO sonucundan sonra protokol 2 broker +
Wintun + resmi WireGuard motoru ile gerçek Linux kernel peer'ine bağlandı:

- `.tools/vpn-windows/wireguard-acceptance-20261029-v2-normal/wireguard-v1-normal-result.json`:
  `ok=true`; 64 KiB TCP SHA256, taze DNS UDP/TCP, normal uygulama kısa ad
  çözümü, owned interface, explicit disconnect ve son ağ baseline'ı geçti.
  Ayrı yükseltilmiş **before ve after-normal audit'lerinde journal sayısı 0**.
- Sibling `wireguard-acceptance-20261029-v2-client-death/wireguard-v1-client-death-result.json`:
  `ok=true`; aynı gerçek veri/DNS kanıtları ve owner-death sonrası baseline ile
  kesin boş native envanter geçti. Before audit 0; **ayrı after-client-death
  protected-journal audit'i kullanıcı durdurduğu için yapılmadı**. Şimdi UAC istenmeyecek.
- İsteğe bağlı PowerShell `cmdletShortName` tanısı başarısız kaldı; zorunlu
  gerçek uygulama resolver'ı geçti. **Yanlış anahtarın gerçek Wintun kabulü
  çalıştırılmadı; 505 rotalı normal/kesintili kabul de hâlâ bekliyor.**

Fixture yalnız `.lab/wireguard-windows-20261029-v2/stop` marker'ıyla kapatıldı.
`stopped.json`: namespaceRemoved, hostNetworkUnchanged ve sourceUnchanged=true;
relay receipt stopped=true. Son salt okunur kontrolde broker/WireGuard süreci
yok; relay 15504 çıkmış. Controller'ın statik `windowsBrokerAdapterTested:false`
alanı toplu kabul sonucu değildir; gerçek kabul yukarıdaki iki ayrı rapordadır.
**V2 fixture/pinleri tekrar bağlantı için artık geçersizdir; taze fixture ve
yeni dondurulmuş paket gerekir.** Eski kanıtlar korunacak, tekrar çalıştırılmayacak.

Kullanıcı yeniden devam ettiğinde eksik salt okunur after-client-death audit'i
için repository'deki script yolu kullanılmalı; frozen audit kopyası yalnız
hash/review referansıdır (göreli tools guard'ı nedeniyle kopya çalıştırılmaz).
Önce repository script hash'i frozen kopyayla karşılaştırılır; sonra yalnız
açıkça onaylanmış yükseltilmiş shell'de aşağıdaki işlem yapılabilir; **şimdi yapılmaz**:

```powershell
powershell.exe -NoProfile -File vpn-windows-broker/scripts/validate-dns-journal-audit-v1.ps1 -Helper .tools/vpn-windows/wireguard-acceptance-20261029-v2-client-death/termterm-vpn-broker.exe -ExpectedHelperSha256 b209fcfd1ba1bb076a6a68c21392091f833979ee875a6d83afcc034ac0856451 -Phase after-client-death
```

## Linux, macOS kaynağı ve WebKit sınırları

`artifacts/vpn-lab/native-unix-service-20261029-wg-ipv4fix.json`: **44 PASS +
1 tanı**, sourceUnchanged/hostUnchanged/namespacesRemoved=true. Gerçek kernel
WireGuard peer'i ve owned TUN/pump; OpenConnect/OpenVPN ile paralellik, UID/hash
doğrulaması, quiesce, çakışma, ölüm/yeniden başlatma ve yabancı kaynakları koruma
geçti. WireGuard'ı kesen otomatik IPv6 RS, yalnız yeni sahip olunan TUN'da UP
öncesi `disable_ipv6` yazılıp okunarak giderildi; global IPv6 kapatılmadı.
Yukarıdaki bağımsız journal yarışı yine de servis hazırlığını engelliyor.

Kaynak paketleri `.tools/vpn-unix-linux-source-20261029-r2` ve
`.tools/vpn-unix-macos-source-20261029-r2`: distributionReady=false,
installed=false, sourceSubset=true. Kurulum notu `docs/VPN_UNIX_KURULUM_TR.md`.
Gerçek Linux resolved DNS testi bekliyor; systemd-resolved 255 ve D-Bus mevcut,
ancak test yalnız özel network/mount/D-Bus alanında yapılacak, host servislerine
dokunulmayacak. Gerçek macOS testi yok; kullanıcıda Mac olmadığı açıkça belirtildi.

WebKit özel CA fixture düzeltmesinden sonra beş gerçek native assertion geçti,
**fakat süreç exit 134 verdiği için toplam sonuç FAIL**. Escalation, GTK init'in
libtest worker'ında, C++ main-exit `WebsiteDataStore::allDataStores` kontrolünün
UI thread'inde olduğunu kanıtladı. Rapor
`artifacts/vpn-lab/native-webkit-teardown-review.json`, trace
`/tmp/termterm-webkit-teardown.trace`. Sonraki düzeltme: feature-gated,
`harness=false`, process main üzerinde çalışan özel native fixture; aynı üretim
hook'u, ephemeral view, beş assertion ve özel CA güveni korunacak. Henüz
uygulanmadı. `_exit`, 134'ü yok sayma, sandbox/TLS gevşetme kullanılmayacak;
üretim TLS doğrulaması zayıflatılmadı.

## Team VPN ve bağlantı kapsamı

Team şema **66**, cache **9** uygulandı; migration 066 SHA256
`5a30dc383defcdd1e41264835e7a0cf2f00fee7c5460f3b64ecfd8b932793234`.
Gerçek PostgreSQL clean-66 ve 65→66 retained-data/backup-restore/checksum/
crash-resume/HTTPS/capability kontrolleri geçti; case
`/var/lib/termterm-team-upgrade-tests/835833d626414d9f93f3dda30b487afb`.
Gerçek client/DesktopState HTTPS kripto testi; kişisel/remember auth, bağımsız
kopya/ACL, logout-reopen offline, cached connect, gerçek kesintide edit ve tekrar
teslim, lost-reply receipt, stale shared conflict/no-overwrite geçti. Kanıt
`/var/lib/termterm-team-upgrade-tests/vpn-crypto-730afbfa16df4a8fa8662fe2b5dbfd00/public-report.json`;
066 hash'i eşleşiyor. Ayrıntılı devir `docs/VPN_TEAM_TR.md`.

Client 42 unit ve tsc geçti; Team service unit sonucu **6 PASS / 1 FAIL**:
`team-service/src/lib.rs:507` pozitif fixture hâlâ schemaVersion 65'i kabul
bekliyor, gate 66. Kullanıcı durdurduğunda düzeltilmemişti; bütün servis unit'leri
geçti denmeyecek. UI 6 kontrol geçti; `artifacts/vpn-ui/team-vpn-permissions.png`
**MOCK IPC** kanıtıdır. Connect-only operatör kişisel VPN binding arayüzüne
erişemiyor: TeamVpnBinding yalnız resource edit dialog'unda; account-scoped API
hazır, context menu/personal profile girişi eklenecek. Şifreli çatışan/iptal
edilmiş taslaklar yerel karantinada kalır; yöneticiye otomatik teslim desteklenmez.
`docs/TEAM_UPGRADES_TR.md` içindeki eski 65/8 bilgisi 66/9'a güncellenecek;
`tests/team_v2_accounts.py` yeni VPN API exposure whitelist'i gözden geçirilecek.
Kişisel VPN UI regresyonu, format/hook son kontrolü ve bağımsız Team incelemesi bekliyor.

Bağlantı katmanı generation başına BoundVpn lease, ilk TCP/proxy soketinde
interface pin, SSH/SFTP/tunnel/paused-copy iptali ve staging/atomic-commit
korumasını içerir. Recursive copy boyunca aynı SFTP Arc korunur. Her hop için
required-VPN kanıtı yalnız yetkili dependency'ye izin verir; keyfi host veya
uyumsuz VPN planı reddedilir. Required-VPN Mosh güvenli ret verir. IPv6 broker
desteği yok; OS DNS için mutlak sızıntısızlık garantisi verilmez. Gerçek Windows
adapter cutover/socket-reuse ve gerçek Team SSH/SFTP-over-VPN uçtan uca testi
henüz yeterli kapsamda yapılmadı; ayrı bileşen testleri bunların yerine geçmez.

Bağımsız Windows doğrulaması: 199 hash; broker 61, packet 3, parent 1,
readiness 4; WG Go/core kontrolleri ve core 2; runner 8/35/13, native 3+6,
tsc/Vitest 32 geçti. V5 paralel timeout sonrası izole koşuda geçti; ilk sonuç
gizlenmez. Native Windows 6/Linux 7/SSO-common 5 ve bound-lifecycle Windows 8 /
Linux 8 + namespace 2 geçti. Bu sayılar macOS, gerçek şirket Linux SSO veya
tam Team fiziksel tünel kabulü değildir.

## Yeniden devam sırası

1. Unix journal mutation+save yarışını düzelt; deterministik regresyon ve eşzamanlı servis/recovery testleri.
2. WebKit process-main fixture'ını uygula; beş assertion ile **temiz process exit** iste.
3. Özel alanlarda gerçek Linux resolved DNS; bağımsız Team/platform incelemesi ve etkilenen kontroller.
4. Eksik after-client-death audit'i; kapsam hâlâ isteniyorsa **taze** fixture/paketle Windows yanlış anahtar ve 505 rota kabulü.
5. Son ayrı geliştirme uygulaması paketi/ekranları; gerçek Team SSH/SFTP-over-VPN kapsamını ayrıca tamamla.

Gerçek şirket Linux SSO ve macOS doğrulanmadı. Aşağıdaki 28 Ekim kayıtları
**tarihsel** olarak korunur; açık bağlantı veya eski “bekliyor” ifadelerinde
bu 29 Ekim duraklatma checkpoint'i önceliklidir.

---

# VPN çalışma devri — 28 Ekim 2026

Dal/worktree: `codex/feature-vpn-workspace`,
`<VPN_WORKTREE>`.
Kurulu TermTerm ve Desktop/termterm 0.5 çalışmasına yazılmadı. Kurucu/release/tag/main/update yayını yok.
IPv4 tam tünelin izole Windows kabulü geçti. Yeni şirket GlobalProtect bağlantısı
Desktop V9 ile kuruldu; kullanıcının verdiği SSH hedefinde parola istemine erişildi.
Kimlik doğrulama, shell ve SFTP henüz sınanmadı.

## Güncel kapsam checkpoint'i — 28 Ekim 2026: Authenticator tamamlandı

Kişisel/Team TOTP Authenticator uygulandı; Kasa/SFTP yanında üst sekme, yerel
QR/kurulum anahtarı girişi, geri sayım ve güncel kodu kopyalama hazır. Manuel VPN
OTP/`secret` challenge formundan Authenticator'a geçip aynı bekleyen challenge'a
dönme arayüz testleri geçti; otomatik OTP doldurma veya bağlantı iptali yapılmaz.
Şifreli saklama, ayrı kod/sır izinleri, kurtarma, kasa/yedek formatı 3 ve Team
şema 65 kontrolleri geçti. Gerçek Windows GUI'de son VPN Dev executable ile yeni
disposable kasa, public RFC QR, gerçek pano ve kilit/yeniden açma doğrulandı.
Ekran kanıtı `artifacts/authenticator-ui/native-windows.png`; ayrıntılar ve
doğrulama sınırları [AUTHENTICATOR_TR.md](AUTHENTICATOR_TR.md) içinde.

Son executable ProductVersion **0.4.3-dev.1**, ayrı `.vpndev` kimliğinde; alt
bilgideki Cargo kaynaklı `v0.4.2` mevcut kozmetik farktır. Kurucu/yayın yapılmadı.
Yeni VPN veya gerçek sağlayıcı OTP testi bu kontrolün parçası değildir.

VPN'in Windows/Linux/macOS ürünleştirmesi, Team VPN kayıtları/izinleri ve
host/klasör ilişkileri, gerçek WireGuard motoru ile 505 rotalı normal/kesintili
native kabul hâlâ bekliyor. Authenticator'ın Team migration'ı **065**'i kullanır;
eski VPN planındaki 065+ ifadesi tarihsel kalmıştır, sonraki yeni migration en az
**066** olmalıdır. Mevcut VPN kapanış ve başarısız rota kabul geçmişi aşağıda
korunur. Kurulu stable/0.5, kullanıcı verileri ve release/update akışı değişmedi.

## Son kapanış ve büyük rota testi checkpoint'i — 28 Ekim 2026

Kullanıcı VPN'i kapatmayı istedi. V9 gerçek bağlantısında yaklaşık 500 rota geri
alınırken IPC 10 saniyelik bekleme sınırına ulaştı; ardından native cleanup,
devnode kaybolduğu halde GUID/LUID hâlâ görünür olduğu için 2 saniyelik adapter
kapanış sınırında güvenli biçimde durdu. Adaptör ve engine sonradan kayboldu;
eski broker'ın normal doğrulanmış kapatma yolu tamamlandı ve yeni test öncesi
audit sıfır journal doğruladı. Aşağıdaki açık bağlantı kaydı artık tarihsel bilgidir.

Kaynak düzeltmesi: adapter kapanışı GUID ve devnode yokluğunu birlikte en fazla
30 saniye bekler; değişen LUID veya bilinmeyen sorgu sonucu hemen reddedilir.
Tekrar denemede kapatılan LUID korunur. Tauri yalnız disconnect için 60 saniye,
üç bağlantının close işlemi için 185 saniye ayırır; diğer istekler 10 saniyedir.
Durable journal ve phantom sahiplik kontrolleri gevşetilmedi. Odaklı kontroller:
broker 57/57, validation 58/58, Tauri VPN 5/5 ve PS5/PS7 runner 35 kontrol geçti.

ManyRoutes senaryosu yalnız izole `10.88.0.0/24` altında 504 ek CIDR, toplam 505
owned rota ve aynı TCP/DNS kanıtlarını ister. İlk native deneme
`broker-close-validation-20261028-v1` normal koşusunda
`network_prepare_before_intent` ile başarısız oldu; mevcut ağ baseline'ı geri
döndü. Bu koşu kapanış düzeltmesinin native kanıtı değildir. Alttaki güvenli
prepare nedenini görünür yapan küçük tanı değişikliği/testi eklendi; guard aynı.
`broker-close-validation-20261028-v2` tanı paketi hazırlandı, fakat UAC/native
koşu yapılmadı. V1 başarısız raporu korundu. İki paketin de eski CA/profil pinleri
fixture kapandığı için yeniden kullanılamaz; sonraki devamda taze fixture/paket gerekir.

Sahip olunan WSL fixture 52564 yalnız kendi stop marker'ıyla kapandı, exit 0.
`many-routes-fixture-20261028-v1.json`: completed, hostNetworkUnchanged,
namespacesRemoved ve sourceUnchangedDuringTest true; 35 passed, 1 prepared.
`ip netns list` boş. Şu an 505 rotalı normal/kesintili native kabul **bekliyor**;
yayın hazır sonucu verilmedi. Kurulu stable/0.5 ve release değişmedi.

## Önceki canlı uygulama checkpoint'i — 28 Ekim 2026 (tarihsel)

Root Desktop V9 geliştirme uygulamasında normal Bağlan yolunu kullandı. Kullanıcı
UAC ve OTP adımlarını tamamladı; gateway seçimi sonrasında generation 2 backend
durumu `connected` oldu. Arayüz aktif bağlantı 1 ve Bağlı gösterdi. Windows'ta
TermTerm adaptörü Up, interface index 68, 500 IPv4 rotası ve iki DNS sunucusu
görüldü; gönderilen/alınan sayaçlar sıfırdan büyüktü. Böylece gerçek şirket VPN
tüneli kurulmuş olarak gözlendi. Bağımsız salt okunur doğrulama sürüyor.

Kullanıcının izin verdiği hedef için seçili rotanın aktif VPN'e ait olduğu
doğrulandı. Sistem OpenSSH ile port 22 SSH handshake tamamlandı ve hedef
kullanıcının parola istemine erişildi. Parola gönderilmedi; Ctrl+C ile istemci
kapatıldı. Bu sonuç VPN üzerinden SSH parola istemine erişim kanıtıdır; hesapla
giriş, shell, TermTerm SSH arayüz yolu veya SFTP başarısı değildir. Test `-F NUL`
ile kullanıcı config'ini, agent ve public-key denemelerini kullanmadı. TOFU
`accept-new` kaydı yalnız ignored özel `.lab` known-hosts dosyasına yazıldı;
kullanıcının known_hosts dosyası değiştirilmedi. Hedef IP/kullanıcı/sır kaydedilmedi.

Bağlantı kullanıcının denemesi için AÇIK BIRAKILDI. Geliştirme uygulamasını,
broker'ı veya aktif adaptörü durdurmayın; temizlik/sweep işlemi çalıştırmayın.
Son bilinen uygulama PID 4320 ve pencere 527140'tır; devamda canlı kimlikler
yeniden doğrulanacak. Önceki disposable test kasası kullanıldı. Kasa parolası,
endpoint, kullanıcı adı veya diğer sırlar bu nota alınmadı. Kurulu stable,
0.5 ve release/tag/main/update değiştirilmedi.

## Güncel native tam tünel sonucu — 28 Ekim 2026

Yeni `broker-full-validation-20261028-v3` paketinde hem normal kapanış hem istemci
ölümü senaryosu geçti (`ok=true`). Orijinal `/0` sunucu planı, sunucu exclusion'ları
ile yalnız izole hedef ve atanan tünel ağını kapsadı; hostun interneti yönlendirilmedi.
Explicit hedef rotası gönderilmeden türetilmiş subnet üzerinden 256 KiB TCP
verisinin SHA256 eşitliği, DNS UDP/TCP ve normal uygulama kısa ad çözümü geçti.
Peer ve exclusion için önceki yol korundu, ikinci tam tünel reddedildi ve ilk
bağlantı kullanılabilir kaldı. Normal/kesintili kapanışta ağ baseline'ı ve kesin
boş native envanter doğrulandı; ayrı yükseltilmiş audit sıfır journal buldu,
recovery çalıştırılmadı. Bu kanıt keyfi internet egress'i veya şirket tüneli
doğrulaması değildir. `Resolve-DnsName -DnsOnly` çıplak kısa ad tanısı bilinen
isteğe bağlı başarısız ölçüm olarak raporda durur; uygulama resolver'ı geçti.

Native manifest SHA256 `11088c9a0d00e2556ad9cc3e8c9b01d9cc6184f6a0363968f135a902827a3571`.
Kanıtlar paket altındaki `dns-v1-normal-full-tunnel-result.json`,
`dns-v1-client-death-full-tunnel-result.json` ve ayrı journal audit dosyalarıdır.
Default broker 56/56, validation 57/57, readiness 4/4 ve bağımsız inceleme geçti.

`desktop-gp-sso-e2e-v9` geliştirme paketi 31 dosyayla hazırlandı ve bağımsız
doğrulandı. `vpn-dev-bundle.json` SHA256
`79b192b385cd138033b22aa00b7c7553d77d6d0b902bf398b72bb35d9eb79882`.
Yalnız default broker değişti; masaüstü, motorlar ve DLL'ler V8 ile aynıdır.
Root yeni şirket bağlantısını bu paketle, OTP'yi kullanıcı girerek kurdu;
güncel bağlantı kanıtı ve açık bırakma durumu üstteki checkpoint'tedir.
Kurulu TermTerm, diğer VPN'ler, 0.5, release/tag/main/update korundu.

WSL fixture `94622` kendi stop marker'ıyla kapandı (exit 0). Rapor
`artifacts/vpn-lab/full-fixture-20261028-v2.json`: 35 geçti, bir fixture hazırlandı;
completed, hostNetworkUnchanged, namespacesRemoved ve sourceUnchangedDuringTest
doğrudur. `ip netns list` boştur. Özel CA/profiller kaldırıldı; native paketleri
yeniden çalıştırmak için yeni fixture ve yeni manifest gerekir. Eski başarısız
v1 UAC deadline kanıtı ve kullanılamayan eski fixture bağlı v2 paket korunur.

## Önceki tam tünel hazırlık checkpoint'i — 28 Ekim 2026

IPv4 tam tünel kodu uygulandı; gerçek Windows kabulü henüz geçmedi. Kaynak ve
iki broker derlemesi donduruldu. Default 56/56, privileged-validation 57/57 ve
readiness 4/4 test geçti; bağımsız kaynak incelemesi geçti. Bu sonuçlar gerçek
şirket tüneli veya native tam tünel testi başarısı değildir.

Plan orijinal IPv4 `/0` isteğini açık `fullTunnel` alanıyla korur. Sunucu
exclusion'ları, sayısal transport peer `/32` ve loopback/non-unicast aralıkları
bounded CIDR çıkarımıyla tünel dışında kalır. Fiziksel rotalara yazılmaz. Yalnız
daha özel ve gerçek bağlı/on-link yerel ağ rotaları korunur; yabancı yönlendirme,
ikinci tam tünel ve aktif sanal varsayılan rota reddedilir. Son kural sanal
adaptörden internete çıkan VM/Hyper-V ortamlarını da sınırlayabilir; bu ortamlar
desteklenmiş sayılmaz. IPv6 tam tünel, split DNS ve PAC kapsamı açılmadı.

Peer yolu `GetBestRoute2` ile önce/sonra doğrulanır. DNS uygulanmadan önce gerçek
seçili yolun sahip olunan VPN adaptörüne ait olması gerekir; uyuşmazlıkta bağlantı
kapanıp journal temizliği çalışır. Yalnız `/0` guard'ı kaldırılmadı.

İlk native tam tünel koşusu UAC onayı beklenirken 120 saniyelik deadline'a ulaştı;
engine/bağlantı testi başlamadı. Koşu başarısız olarak korunur; sonraki salt okunur
kontrolde engine/adaptör kalmadı. Yeni kaynakla şirket GlobalProtect yeniden
DENENMEDİ. Root kullanıcı hazır olduğunda yeni native koşu ve sonra OTP ile gerçek
bağlantıyı yürütecek. Hazırlanmış v2 native paket eski fixture CA/profillerine
bağlı olduğundan yeniden kullanılmaya HAZIR DEĞİLDİR; yeni fixture/pin gerekir.

İzole WSL full fixture kendi stop marker'ıyla kapandı: rapor
`artifacts/vpn-lab/full-fixture-20261028-v1.json`, completed/hostNetworkUnchanged/
namespacesRemoved/sourceUnchangedDuringTest=true; 35 kontrol geçti, bir desktop
fixture hazırlandı. Namespace listesi boş; ana WSL/PostgreSQL korunur.

Derlemeler `.tools/vpn-windows/broker-default-build/debug/termterm-vpn-broker.exe`
SHA256 `a5c6b05cfe2eb30c28c513def58faec48258ead0ad2685462eda79b4abceea68`, validation
`.tools/vpn-windows/broker-validation-build/debug/termterm-vpn-broker.exe`
SHA256 `8202ceba6b4571cce86afe274ba9b5752e6de333da79288fa56d62fcbc6a67b5`.
Kurulu TermTerm, kullanıcı kasaları ve 0.5 kaynakları değiştirilmedi; release yok.
## Tam tünel geliştirmesinden önceki DNS/rota kabulü — 28 Ekim 2026

### Son gerçek bağlantı sonucu

Yeni şirket GlobalProtect hedefinde normal Bağlan yolu kullanıldı; parola,
kullanıcının girdiği OTP ve tek gateway seçimi geçti. Broker adaptör oluşturmadan
sunucunun planını şu değer içermeyen özetle reddetti:
`dnsServers=2 dnsDomains=0 searchDomain=true proxyPac=false excludedRoutes=10
dnsOptions=false defaultRoutePresent=true`. Eksik yetenek IPv4 full-tunnel'dır;
bu sonuç kimlik doğrulama hatası veya kurulmuş tünel değildir. Son salt okunur
kontrolde engine/TermTerm VPN adaptörü kalmadı. V8 geliştirme uygulaması failed
durumda açık bırakıldı; kullanıcı vault'u veya kurulu uygulama değiştirilmedi.

Full-tunnel guard'ı gevşetilmedi. Devam için yalnız IPv4'te açık full-tunnel planı,
sayısal gateway peer için tünel dışı transport yolu, exclusion sonrası sınırlı
rota üretimi, mevcut fiziksel/diğer VPN rota öncelikleri ve tek full-tunnel/DNS
sahibi politikası birlikte tasarlanıp test edilmelidir. Sadece `/0` reddini
kaldırmak veya iki `/1` yazmak yeterli değildir: mevcut endpoint-capture ve host
overlap kontrolleri bunu güvenle tamamlamaz. Bu aşamada full-tunnel kodu yazılmadı.

Kurulu TermTerm, diğer VPN uygulamaları/adaptörleri ve Desktop/termterm 0.5
çalışması korunur. Dış VPN istemcileri için başlatma, durdurma veya kaldırma
işlemi yapılmadı. Release/tag/main/update yayını yoktur.

V6 native doğrulamasında uygulamaların kullandığı `.NET GetHostAddressesAsync`
ile kısa ad iki kez doğru adrese çözüldü; her çağrıda tüneldeki yetkili DNS
fixture'ında yeni olumlu cevap ve genişletilmiş ad sorgusu sayıldı. FQDN UDP/TCP,
adaptöre özel DNS/suffix/search-list geri okuması, gerçek CIDR exclusion sonrası
rota seçimi, ikinci DNS sahibi VPN'in reddi ve doğrulanmış normal kapanış geçti.
Fiziksel/global ağ baseline'ı eşitti; son salt okunur envanterde adaptör/devnode
kalıntısı yoktu. Excluded hedefe paket gönderilmedi; kanıt rota seçimi/geri
okumasıdır. Kanıt:
`artifacts` dışındaki yerel `.tools/vpn-windows/broker-dns-validation-20261028-v6/dns-v1-normal-exclusions-result.json`.

`Resolve-DnsName` ile çıplak kısa ad aynı fixture'da suffix eklemeden sorgulandı
ve başarısız oldu. Bu tanı sonucu saklanır; V6 raporunun `ok=false` değeri
değiştirilmedi. Aynı tünelde normal uygulama resolver'ı doğru suffix ile yeni
cevap aldı. Bağımsız inceleme ve sınırlı escalation sonrasında yeni runner'da
yalnız bu cmdlet kısa-ad ölçümü isteğe bağlı tanı olarak ayrıldı; uygulama
resolver'ı ve FQDN ölçümleri hâlâ doğru IP **ve yeni tünel cevabı** gerektirir.
Genel Windows/kurumsal DNS politikalarının tamamı doğrulandı denmez; global
suffix veya resolver cache'i değiştirilmedi.

Kaynak düzeltmeleri: OpenVPN fixture'ı pinned core'un split-DNS anlamına gelen
`DOMAIN` yerine açık `ADAPTER_DOMAIN_SUFFIX` kullanır. Broker, yalnız kendi
GUID/LUID bağlı adaptörünün DNS `SearchList` değerini diğer DNS değerleriyle
birlikte günlüğe alır, geri okur ve sahiplik/drift kontrolüyle geri yükler.
Eski günlükte bu alan boş listeye açılır. Sınırlı int/int64 JSON kontrolü
PowerShell 5/7 farkını karşılar. Varsayılan broker 52, validation broker 53;
gerçek pinned DNS parser'ı 4 olumlu/19 ret; fixture 12; runner iki PowerShell
sürümünde 30 self-check geçti. Bu sayılar gerçek şirket bağlantısı kanıtı değildir.

V7 native `client-death + RouteExclusions` tüm zorunlu DNS/rota/çakışma ölçümlerini
geçti; ağ baseline'ı eşitti. Kapanışın tek seferlik native envanter okuması henüz
boş olmadığından koşu başarısız korundu; hemen sonraki salt okunur envanter boş,
ayrı yükseltilmiş journal audit'i sıfır bulundu. Recovery yapılmadı. Yeni runner
aynı 30 saniyede hem ağ baseline'ını hem kesin boş native envanteri bekler;
her salt okunur denemenin güvenli sayıları saklanır. Kontrol gevşetilmedi.

Yeni `broker-dns-validation-20261028-v8` yalnız test paketidir; manifest SHA256
`bd60fd058bc53b58d0880120526e32da2342b82dd0b33e12b1da25322935ff64`,
runner SHA256 `ecfabd92d8dfc5e48e0c08f4e509109e9723caeed4da140e01972c319f911099`.
V8 native `client-death + RouteExclusions` **geçti** (`ok=true`): zorunlu uygulama
resolver'ı/FQDN DNS, rota exclusion, ikinci DNS sahibinin reddi, owner-death
kapanışı ve ağ baseline'ı doğrulandı. Kapanışın ilk salt okunur örneğinde bir
registered interface, ikinci örneğinde sıfır görüldü; son devices/interfaces
listeleri boştur. Böylece IP ağ durumu ile PnP kayıt silinmesinin ayrı zamanlarda
bitmesi doğrudan ölçüldü; recovery yapılmadı. Ayrı yükseltilmiş kapanış journal
audit'i root tarafından tamamlanmadan şirket denemesine geçilmez.

V8 masaüstü paketi `.tools/vpn-windows/desktop-gp-sso-e2e-v8` altında 31 dosyayla
hazırlandı. Manifest SHA256
`dcd85d64edd454563bfb40dad807c5e5495ac929d49e6ee1f273973dc1ad3902`.
Masaüstü/OpenConnect önceki V7 ile aynı; default broker
`c05919e9fadd4faedcecf43bfdb08e3f511789bed30d49d8b07db4dec6f00213`,
OpenVPN `20be8bb865c5687f52a3fc0aad3e3735a50772805fa1c8bb3c841e679f9ebfe4`.
V1–V7 paketleri ve başarısız test kanıtları korunur. Yeni şirket GlobalProtect
denemesi yukarıdaki full-tunnel eksikliğiyle sonuçlandı; gerçek tünel kurulmadı.

İzole WSL fixture'ı `43179`, kendi `.lab/vpn-desktop-dns-20261028-v3/stop`
marker'ıyla kapandı (exit 0). Son rapor
`artifacts/vpn-lab/dns-fixture-20261028-v3.json`: 35 geçti, bir desktop fixture
hazırlandı; completed, hostNetworkUnchanged, namespacesRemoved ve
sourceUnchangedDuringTest doğrudur. `ip netns list` boştur. Eski V1/V2 fixture'ları
da kendi marker'larıyla temiz kapandı. Yalnız ignored özel `.lab` altında özel ağ
baseline kanıtları ve stop marker'ı kaldı; profil/sertifika dosyaları temizlendi.

## Durma checkpoint'i — 27 Ekim 2026

Fiziksel Escape ile çalışma, V7 uygulamasının kapanışı ve yeni native DNS kabul
kontrolleri yapılmadan durdu. Yeni kaynak donduruldu ve bağımsız inceleme geçti:
default broker 51/51, privileged-validation broker 52/52; OpenVPN DNS projection
3 desteklenen/18 reddedilen durum; disposable DNS runner 14 kontrol geçti.
Bu kayıt için yeni test veya derleme çalıştırılmadı.

Son sorumluluk dağılımı: builder yeni OpenVPN motoru, default broker ve validation
broker derleme çıktılarını yönetiyordu; WSL fixture oturumu 21478 tutuluyordu.
Builder'dan yalnız sahip olunan fixture'ı güvenli durdurması ve mevcut derleme
sonuçlarını/hash'lerini kaydetmesi istendi. Buradan yeni paket tamamlandı, fixture
kapandı veya V7 kapandı sonucu çıkarılmamalı; sonraki devamda builder'ın son
raporu esas alınmalı. V7 arayüzü failed durumda açık kalmış olabilir; son
bilinen denemede yeni VPN adaptörü yoktu, bu checkpoint canlı envanter okumadı.

Native Windows DNS readback, normal resolver ile FQDN/shortname, gerçek tünelden
DNS erişimi, competing DNS premutation reddi ve normal/broker-death cleanup
kabul testleri **ÇALIŞTIRILMADI**. Yeni şirket bağlantısı da **DENENMEDİ**.
Devamda root mevcut çıktıları doğrulayıp disposable native kabul adımını yönetir;
bu implementer'a yeni kaynak değişikliği ancak somut test bulgusu ile döner.
Kurulu stable/0.5 ve kullanıcı verilerine işlem, release veya yayın yoktur.

## Güncel çalışma — sınırlı Windows DNS ve exclusion desteği, 27 Ekim 2026

V7 normal bağlantıda kullanıcı OTP'yi arayüzde girdikten sonra motor network-plan
sınırına ulaştı. Broker adaptör/rota/DNS oluşturmadan şu güvenli özeti reddetti:
`dnsServers=2 dnsDomains=0 searchDomain=true proxyPac=false excludedRoutes=10
dnsOptions=false`. Ağ baseline'ı değişmedi; gerçek şirket tüneli kurulmuş sayılmaz.

Yeni kaynak yalnız sahip olunan adaptöre IPv4 DNS sunucuları ve tek bağlantı
suffix'i uygular. DNS ve interface metric'i GUID/LUID bağlı before/after journal
ile kalıcı intent'ten sonra yazılır ve geri okunur. Cleanup yalnız mevcut değer
owned after ile eşleşirse eski değeri geri yazar; drift/kimlik değişimi journal ve
rezervasyonu korur. İkinci DNS-bearing VPN pending aşamasında
`competing_dns_session` ile reddedilir; disjoint DNS'siz VPN'ler üç bağlantı sınırı
içinde kalır. Bu standart Windows resolver politikasıdır; bütün interface'lerde
DNS fallback'ini engelleyen kill-switch veya split DNS/NRPT/WFP garantisi değildir.

Exclusion'lar yalnız include CIDR'lerinden sınırlı parçaya ayrılarak çıkarılır;
fiziksel interface'e bypass rotası yazılmaz. Assigned connected prefix, DNS ve
VPN peer erişimi ile başka VPN rezervasyonları korunur. Orijinal `/0` subtraction
öncesinde reddedilir. PAC, dnsDomains ve enhanced DNS kapalı kalır; unsupported
özeti artık `defaultRoutePresent` de bildirir.

Broker kontrolleri bağımsız olarak default 51/51 ve privileged-validation 52/52
geçti. Bunlar gerçek DNS paket/UAC adapter ayarı kanıtı değildir. OpenVPN basic
legacy DNS projection pinned DnsOptions üzerinden dar dönüşümle tamamlandı;
self-check 3 desteklenen ve 18 reddedilen policy ile geçti. Pinned boş suffix
vector'ünün JSON'da alanı atlaması escalation ile doğrulandı; yalnız sentetik
fixture düzeltildi, üretim guard'ı gevşetilmedi. Disposable gerçek
OpenVPN DNS fixture'ında normal resolver FQDN/shortname, DNS reachability,
competing DNS premutation reddi ve normal/crash cleanup henüz çalıştırılmadı.
Get/SetInterfaceDnsSettings için Windows 10 build 19041+ gerekir.
Bu kanıtlar ve inceleme tamamlanmadan yeni şirket bağlantısı denenmeyecek.

## Önceki canlı deneme — V6, 27 Ekim 2026

V6 paketi oluşturuldu ve 31 dosyasının hash kontrolü geçti. Önceki baseline hook
TypeScript, 31 Vitest ve 8 hook testiyle geçti; bu checkpoint için yeni test
çalıştırılmadı. Kurulu stable/0.5 ve kullanıcı verileri korunur; yayın yoktur.

V6'da normal `Bağlan` gerçekten denendi; bu auth-only denemesi değildir.
Generation 2'de portal SSO tamamlandı, gateway seçildi ve challenge 3
`stage=gateway` aşamasına geçildi. Gateway panelinde Microsoft oturum açma ekranı
görülüyor; gateway girişinin/MFA'nın tamamlanıp tamamlanmadığına dair kullanıcı
açıklaması bekleniyor.

Gateway challenge'ı için 240 saniyelik güvenli ölçümün sonucu:

```json
{"responseObserved":41,"acsMatched":0,"status":null,"headerPresence":{"samlUsername":false,"preloginCookie":false,"portalUserauthcookie":false,"samlAuthStatus":false},"validationOutcome":"awaitingResponse","completionOutcome":"waiting"}
```

Bu sonuç o ölçümde eşleşen gateway ACS yanıtının gözlenmediğini gösterir. False
header presence alanları sunucunun başlıkları göndermediği anlamına gelmez;
eşleşen ACS yanıtı henüz okunmadı. Döngü, protokol hatası veya sağlayıcı politikası
tanısı konmadı. Wintun adaptörü oluşturulmadı; gerçek tünel/SSH kanıtı yoktur.
Sonraki adım gateway girişinin/MFA'nın tamamlandığını kullanıcıyla netleştirmektir.
Bu kayda müşteri adı, endpoint, kimlik bilgisi veya özel profil yolu alınmadı.

## Güncel düzeltme — mantıksal DNS origin'i, 26 Ekim 2026

V5 normal `Bağlan` denemesi auth-only değildi; privileged broker başladı ve ilk SSO
challenge'ı arayüze gelmeden `sso_origin_changed` ile durdu. Kaynakta kök neden
doğrulandı: OpenConnect `get_hostname()` TLS sonrasında bağlanılan sayısal peer
IP'sini döndürür. Browser ACS origin'i için URL/TLS hostname'ini döndüren
`get_dnsname()` gerekir. Önceki IP-literal fixture'larında bu iki değer aynıydı.
Default 443 zaten doğru biçimde origin'den çıkarılıyordu; port kontrolü gevşetilmedi.

Motor şimdi her challenge için güncel `get_dnsname()` ve port'u kullanır; küçük
ortak formatlayıcı mevcut küçük-harf/default443/IPv6 bracket davranışını korur.
Windows DLL API resolver yeni getter'ı yükler. Rust canonical origin kontrolü,
gerçek origin değişikliği reddi ve yalnız ilan edilmiş gateway seçimi allowlist'i
değişmedi. Önceki ACS başarılı-sayfa/pending gözlemi DNS/IP uyuşmazlığıyla
uyumludur; tek başına server header veya body biçimi tanısı yapılmadı.

Eski motor (`68c2…`) yeni DNS regresyonunda beklenen `wrong_fixture_challenge`
reddini verdi. Yeni motor TLS 17/17 (aynı native motor 10, DNS 3), lab contract
10/10, C++ SSO boundary self-check ve Rust core SSO testi geçti. DNS kapsamı gerçek
loopback TLS bağlantısında DNS adı ile numeric peer ayrımını ve iki aşamalı
portal/gateway port değişimini kapsar. Pure testler implicit/default443, nondefault
port, case, IPv6 origin ve ASCII punycode biçimini kapsar; IPv6 tüneli kanıtı değildir.
Kanıt `artifacts/vpn-auth/gp-sso-dns-origin-20261026.json`.

Yeni motor SHA256 `6ca6549f1bd6cf1f49c6ace85781f5f00c6745b8317c19ba0ba82b175533c6bf`.
V6 paketi 31 dosyayla oluşturuldu ve doğrulandı; güncel gerçek denemenin sonucu
ve kalan sınırları üstteki 27 Ekim checkpoint'indedir. Kurulu stable/0.5, kullanıcı
verileri ve yayın sınırları korunur.

## Kaynak ek tanısı — ACS yanıtı, 26 Ekim 2026

Kullanıcı artık gerçek VPN tüneli istiyor; auth-only başarı bu hedefin tamamlanması
değildir. V3'te gerçek portal ACS sayfasında `Login Successful!` görüldüğü sırada
panel portal SSO challenge'ında kaldı; daha sonraki boş pencere envanteri güncel
backend durumunun kanıtı değildir. Yanıt başlıkları veya HTML biçimi okunmadı ve
bu görüntüden eksik başlık/HTML-comment formatı sonucu çıkarılmaz.

Kaynakta yalnız Windows debug+e2e için `vpn_sso_test_diagnostics` eklendi:

```javascript
await invoke("vpn_sso_test_diagnostics", { id, generation, challengeId });
```

Komut yalnız yerel `main` view, yalıtılmış development kimliği ve açık kişisel
kasa içindir. Tam session/kasa/generation/challenge eşleşmesini denetler;
terminal/timeout sonrasında da aynı rapor okunabilir. Son 32 rapor yalnız native
bellekte tutulur ve kasa lifecycle temizliğinde silinir. Dönen alanlar yalnız
`responseObserved`, `acsMatched`, nullable HTTP `status`, dört `headerPresence`
boolean'ı, `validationOutcome` ve `completionOutcome` enum kodlarıdır. URL, body,
DOM, kullanıcı adı, token, cookie veya başlık değeri rapora/log'a alınmaz.

`responseObserved=0` callback teslimi görülmediğini; `acsMatched=0` kesin challenge
ACS eşleşmesi görülmediğini gösterir. `responseReadFailed` native yanıt okuma
hatasıdır; bu durumda false header presence, eksik sunucu başlığı kanıtı değildir.
`headersRejected` doğrulama reddidir. `headersAccepted` + `queued` native completion
işini, `submitted` yalnız yanıtın motora iletildiğini gösterir; VPN/tünel başarısı
değildir. Timeout `expired`, iptal `cancelled` koduyla ayrılır. Geç callback'ler
önceki terminal completion kodunu silemez.

Event handler navigasyondan önce kaydedilir; lifetime/delivery hatası henüz
kanıtlanmadı. TLS/ACS/navigation/message/capability kuralları değiştirilmedi;
HTML/body parser veya yeni canlı giriş/ağ denemesi eklenmedi. Bu kaynak değişikliği
çalışan V3'e yazılmadı; V4 build ve güvenli canlı tanı router tarafından yapılacak.

## Kaynak ek düzeltmesi — çift SSO iptali, 26 Ekim 2026

V3 paketinin 31 dosyası doğrulanarak hazırlandı; gerçek yeni gateway testi router
tarafından yürütülür. Aşağıdaki ek iptal düzeltmesi yalnız kaynakta bulunur ve
çalışan `desktop-gp-sso-e2e-v3` paketine yazılmadı.

SSO panelinin unmount kapanışı ile açık iptal komutu aynı auth-only motorunu
kapatabilir. İlk komut motoru sildiğinde ikinci komut eski kodda normal broker'a
düşebiliyor veya terminal session için `stale` hatası veriyordu. Auth-only route
artık motor kaldırılsa da cache'teki tam session ID, geçerli kasa ve generation
ile doğrulanır; tekrarlanan iptal başarılı ve etkisizdir. Terminal başarısızlık
kaydı korunur. Eski generation/yanlış kasa reddedilir; geç cevaplar normal broker'a
iletilmez ve normal VPN session route'u korunur.

Isolated dev config ile focused Rust auth-only testleri 4/4 geçti; yeni test
engine'siz aktif ve terminal cache durumlarında iki iptal sırasını, başarısızlık
kaydının korunmasını, eski generation/yanlış kasa ve geç cevapları kapsar.
Whitespace kontrolü geçti. Bu pure backend testidir; native GUI'de eşzamanlı iptal
yeniden denenmedi. Yeni EXE/paket oluşturulmadı; root'un v3 gerçek testi,
kurulu stable, 0.5 ve `readiness=false` korunur.

## Güncel devir — portal/gateway SSO geçişi, 26 Ekim 2026

Kullanıcının gerçek gözlemi: portal Microsoft girişi ve MFA sonrasında gateway
seçimi göründü; seçimden sonra ayrı gateway SSO ekranı açıldı. Son gateway girişi
ve VPN bağlantısı doğrulanmadı. Önceki dört deneme `authOnly=true` ile başarısız
bitti; hiçbirinde tünel beklenmez. Bu gözlem bir kimlik sağlayıcı politikası tanısı değildir.

Kaynakta doğrulanan hata: OpenConnect gateway'e yönlendikten sonra motor callback
origin'ini ilk portalda sabit tutuyordu. Her SSO challenge artık kütüphanenin o
andaki hedef host/port'una bağlıdır. Broker/lab origin değişimi yalnız portalın
sunduğu, boş olmayan seçenekler içinden doğrulanan gateway yanıtıyla yapılır.
Kaydedilmiş GP gateway seçimi arayüzde ön seçilir ve açıkça onaylanır; diğer
protokollerin realm davranışı korunur. Token adı her callback'in kendi yanıtından
alınır; portal token'ını gateway yerine zorla kullanma veya otomatik SSO tekrarı yoktur.

Panel portal SSO, gateway seçimi ve gateway SSO aşamalarını ayrı gösterir;
auth-only bekleme/başarı metinleri tünel açılmayacağını söyler. Aynı generation'ın
cleanup olayı hata metnini silemez; yeni açık deneme eski hatayı temizler.
Kullanıcının iptali auth-only reader'ın son gate'i altında sınıflandırılır ve
kimlik doğrulama hatası gibi raporlanmaz. Uzak SSO pencere boşluğuna tıklamak
girişi iptal etmez; açık iptal düğmesi vardır.

Yerel TLS 14/14 (aynı C++ motoru/job/private-pipe yolunda 8 auth-only), Python lab
contract 10/10, UI IPC mock 9/9, C++ SSO self-check ve Stop hook
(8 hook + TypeScript + 31 Vitest) geçti. TLS kapsamı farklı origin/port ve aynı
origin portal/gateway, kaydedilmiş seçim, stale/off-origin cevap ve iptaldir;
son gateway formunun doğru cookie alanı doğrulandı. Rapor
`artifacts/vpn-auth/gp-sso-transition-20261026.json`.
Yeni motor SHA256 `68c2a89d367e87c3ddc748913e6cc0440b90c7b48b63a9088f1573e57547afde`;
önceki paketler korundu. V3 masaüstü/default broker paketi 31 dosyayla hazırlandı
ve hash'leri doğrulandı; yukarıdaki çift iptal ek düzeltmesi bu pakette yoktur.
Gerçek yeni gateway kabulü router'da bekliyor;
tünel/SSH, macOS ve hardware key test edilmedi. Readiness/yayın kapısı açılmadı.

## Güncel devir — native SSO fixture düzeltmesi, 26 Ekim 2026

Native fixture 1/1 geçti; gerçek Microsoft SSO girişi değildir. WebView2'nin
`NavigateToString` için bildirdiği tam `data:text/html;charset=utf-8;base64,...`
adresi HTTPS kuralıyla iptal ediliyordu: native kanıt `cancelled=true`,
`NavigationCompleted success=false`, `WebErrorStatus=14`. Başlangıç `about:blank`
beklemesi de kaldırıldı; pinned Tauri runtime bu başlangıç navigasyonunu başlatmaz.
Tanı: `artifacts/vpn-ui/native-sso-diagnostic-20261026.json`.

Düzeltme yalnız debug/e2e fixture'ında sabit inert belgenin tam adresine izin verir.
Üretim web mesajlarının kapalı olduğu native API ile doğrulandıktan sonra yalnız bu
fixture'da Tauri'nin kendi authenticated postMessage transport'u açılır; opaque
fixture origin'i normal fetch IPC yolunu kullanamaz. Gerçek auth HTTPS/frame
kuralları, TLS, capability kapsamı ve `SetIsWebMessageEnabled(false)` korunur.
Fixture `vault_info` ve `window.inner_size` için gerçek Tauri ACL retlerini aldı;
eksik API, transport hatası, izin verilen çağrı ve timeout başarı sayılmaz.
Bu koşu üretim transport'unda komut denemesi veya app label guard'ına ulaşma kanıtı değildir.

Debug/e2e build, denial sınıflandırıcısı 6/6 ve native fixture 1/1 geçti. Bağımsız
hook: whitespace, 8 hook testi, TypeScript ve 30 Vitest geçti. Vault kimliği korundu,
sessions boş, child view kapandı; implementer sonrası GUI/engine/broker envanteri boş.
Kanıt `artifacts/vpn-ui/native-sso-isolation-implementer-20261026.json`.
Bağımsız native fixture 1/1 ve paketin 31 dosya hash kontrolü de geçti; kanıt
`artifacts/vpn-ui/native-sso-isolation-verifier-20261026.json`.
Bağımsız son Rust tekrarı da 8/8 geçti (0 failed/ignored): isolated dev `TAURI_CONFIG`
ile `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features e2e vpn_sso::`.

Yeni paket `.tools/vpn-windows/desktop-gp-sso-e2e-v2` (31 dosya):
EXE SHA256 `3970ce057e6ae7f7356554da60b1ceffa4fdd2ee44a3f13a8fbc84d0bd62b47c`;
manifest SHA256 `130d4d2100aa2c35f5f80536ad8441f10a91aa9685736eb125883e91eb123ea8`.
V1 ve doğrulanmış helper/DLL envanteri korundu; yalnız yeni EXE hash'i değişti.
Gerçek Microsoft hesap/MFA, HTTPS header teslimi, tünel/SSH, macOS ve hardware key
test edilmedi. Stable/0.5 ve kullanıcı verilerine yazılmadı; kurulum/yayın yok,
`readiness=false` korunur.

## Önceki tanı denemeleri — 26 Ekim 2026

Kullanıcının devam talebiyle yalnız inert native fixture tanısı yapıldı. İlk
`ExecuteScript` literal kontrolü `42`, `readyState=complete`, boş başlık ve
`invoke=undefined` döndürdü; `fixtureResult` yoktu. Bu sonuç callback yolunun
çalıştığını, fixture belgesinin gözlenmediğini gösterir; IPC izolasyonu değildir.
İkinci sınırlı deneme başlangıç `about:blank` için `on_page_load Finished`
bekledi ve 15 saniyede `Native VPN SSO initial document timed out` ile başarısız
oldu. Başarısız bekleme kaynak değişikliği geri alındı; daha derin inceleme gerekir.

Tanı ve kesin host ACL/guard hata metni zorunluluğu kaynakta kaldı;
`artifacts/vpn-ui/native-sso-diagnostic-20261026.json` iki denemenin kanıtıdır.
Log dizinleri `.tools/wdio-sso-diagnostic1` ve `.tools/wdio-sso-diagnostic2`.
İki isolated debug/e2e build geçti; son bekleme geri alındıktan sonra yeni build
ve fixture koşusu yapılmadı. `src-tauri/target/debug/termterm.exe` ikinci başarısız
denemenin geçici binary'sidir; doğrulanmış paket değildir. Korunan v1 değişmedi.
Hooks/izolasyon 13/13 ve bağımsız baseline hook (TypeScript, Vitest 30/30) geçti.
Gerçek Microsoft giriş/MFA, tünel ve SSH test edilmedi; auth view güvenlik
ayarları, stable/0.5, kullanıcı verileri ve `readiness=false` korunur; yayın yok.

## Önceki duraklatma kaydı — kullanıcının talebi, 25 Ekim 2026

Çalışma durduruldu; yeni test, düzeltme veya ağ/giriş denemesi yapılmaz. Native
debug/e2e build geçti. Son kaynakta bağımsız kontroller: TypeScript 0 hata,
Vitest 30/30, UI 8/8, hooks/izolasyon 13/13, native VPN 13/13; TLS 8/8
(aynı C++ motoruyla 2 auth-only koşu) ve lab contract 7/7 geçti.

**Native GUI izolasyon fixture'ı başarısız:** `SSO native isolation check timed out`.
Bu sonuç IPC izolasyonu veya gerçek Microsoft SSO başarısı değildir. Gerçek
Microsoft parolası gönderilmedi; hesap girişi/MFA ve tünel/SSH doğrulanmadı.
Sonraki sınırlı adım yalnız inert fixture'da host `ExecuteScript` literal nesne,
`readyState`/başlık/`typeof invoke` kanıtı alarak belge yükü, inline script/CSP ve
polling callback olasılıklarını ayırmaktır; bunlar henüz hipotezdir. Gerçek auth
webview güvenlik ayarları gevşetilmez.

Korunan paket: `.tools/vpn-windows/desktop-gp-sso-e2e-v1` (31 dosya).
EXE SHA256 `24218a7e4d98150aff9395e3aa4a709c666540a8e619f4c9966f18e12d2e4414`;
manifest SHA256 `a7daeb131e71741f76d0135e8e3db3fc1d88a9eaa66d3e4cabbfc041ed41b1ae`.
V1 build sonrasında kaynak/config değişmedi; yalnız bu checkpoint ve üretilen
fixture çıktıları vardır. Son task-owned GUI/engine/broker süreç envanteri boş;
build/test oturumları tamamlandı. Kurulu stable/0.5, kullanıcı verileri, yayın ve
`readiness=false` korunur. Ayrıntılar aşağıdaki tarihsel kanıtta saklıdır.

## Güncel devir — GlobalProtect SSO motoru, 25 Ekim

Kimliksiz prelogin yanıtı geçerli GlobalProtect XML ve SAML REDIRECT içeriyor;
aynı OpenConnect library API'si Microsoft giriş yönlendirmesine ulaştı. Önceki
CLI `protocol_response_unrecognized` sonucu XML bozukluğu değildi: browser callback
olmayınca dönen `EINVAL`, XML parse hatası diye raporlanıyordu. Motorun SSO reddi
yerine native private challenge/result akışı eklendi; küçük LGPL patch'i, browser
token'ını `passwd` yerine sunucunun döndürdüğü cookie alanıyla gönderiyor.

Yerel TLS fixture 8/8 geçti; bunun 2 koşusu aynı C++ motoru ve job/private-pipe
yoluyla `authenticated` sonucuna ulaştı. Auth-only modu CSTP/tünel pazarlığından önce
durur; network-plan, adaptör, rota veya DNS değişikliği yoktur. Kanıt
`artifacts/vpn-auth/gp-sso-protocol-20261025.json`; bağımsız verifier 8 TLS/2 motor/
7 lab contract kontrolünü ve staged hash'lerini doğruladı.

Gerçek Microsoft hesap girişi/MFA, native masaüstü SSO akışı ve üretici VPN tüneli/SSH
henüz doğrulanmadı. `readiness=false` korunur; Wintun sahiplik ve ağ politikası sınırları
açılmadı. Callback ve test kapsamı için [güncel protokol sınırları](VPN_DEVELOPMENT_STATUS_TR.md#güncel-protokol-sınırları).
Kurulu stable, kullanıcı verileri ve 0.5 çalışması korunur; yayın yok.

## Önceki devir — üretici auth-only deneyi, 25 Ekim

**Fortinet: Çalışıyor — giriş ve OTP aşaması doğrulandı.**
Tek parola formu sonrası gerçek OTP challenge, diagnostic native exit `42`;
OTP gönderilmeden durdu. GlobalProtect: kimlik bilgisi olmadan TLS + HTTP 200,
mevcut CLI prelogin yanıtı `protocol_response_unrecognized`; SSO URL gözlenmedi.
Bu sonuç GP sunucusunun SSO desteklemediğini göstermez. Tarayıcı/Microsoft girişi,
tam authentication ve Fortinet/GP tüneli test edilmedi; ek ağ denemesi yapılmaz.
Sanitize kanıt `artifacts/vpn-auth/vendor-auth-20261025.json`. Kimlikler yalnız
private stdin pipe/bellekte kaldı; şirket hedefi/hesabı/parolası/cookie raporda yok.
Wrapper selftest 36 ve native origin/form/OTP guard 10 kontrol geçti. Uygulama,
kurulu stable, 0.5 çalışması ve `readiness=false` değiştirilmedi; yayın yok.

## Önceki devir — scoped full-v9 PASS, fixture kapalı

Gerçek Windows broker: iki OpenVPN + Cisco/ocserv, üç hedefte 24 payload ICMP,
orta oturum stop/reconnect; authenticated cleanup 0/0, native/process/device boş,
host baseline aynı. Bağımsız verifier 24 payload ve 30 bundle hash'ini doğruladı.
Rapor `.tools/vpn-windows/broker-full-validation-v9/full-broker-result.json`, SHA256
`4dcea13314856c7f15c76066e9bd8fc83ba8c38af5f31b544be6ced6b1d27907`.
Default/validation broker suite 44/44 ve 46/46, runner selftest 11/11.

WSL session95604 stop marker ile exit0 kapatıldı. Final Linux raporu
`artifacts/vpn-lab/desktop-broker-resume-20261022-1425.json`, SHA256
`9013841fbfc4c2ceeb0914bdb420dbbccf7b76d51c2217e5607221c632920200`:
35/35, host/source aynı, namespace listesi boş. Private CA/profil/ready/stop dosyaları temizdir.
Yeni ağ koşusu gerekirse fixture yeni CA/profillerle yeniden üretilir; eski hash kullanılmaz.

Sonraki işler: global Wintun sweep ve create-before-LUID sahiplik sınırlarını çözmek;
güncel default helper ile yeni isolated desktop bundle üretmek; ancak doğrulanan kapsamın
kapısı bilinçli açıldıktan sonra gerçek panel connect/challenge/disconnect/lock/exit testi.
DNS/full-tunnel/net30/SSO ve gerçek Fortinet/GP sunucu doğrulaması ayrıca kalır.
Readiness kapalıdır; mevcut UI kanıtı profil import/edit/persistence içindir, VPN connect değildir.
Güncel ayrıntı [VPN_DEVELOPMENT_STATUS_TR.md](VPN_DEVELOPMENT_STATUS_TR.md).

## Tarihsel önceki sınırlar — aşağıdaki ara durumlar v9 ile güncellendi

- Native kişisel VPN profil akışı gerçek debug uygulamada geçti: import/opaque token,
  metadata-only liste, görünür UI edit, parola/config korunması, lock/reopen. Kanıt
  `artifacts/vpn-ui/native-profile-workflow.json` ve `.png`; test `tests/desktop/vpn-profiles.mjs`.
  Son staged e2ev2 native UI implementer/bağımsız tekrar 1/1: profiller 379/530 ms'de görünür,
  31 dosyalı envanter tam, readiness=false. Ayrı raporlar
  `artifacts/vpn-ui/native-preflight-v2-{implementer,verifier}.json`.
  Windows broker'ın no-UAC suite'i lifecycle/kernel trafik kanıtı sayılmaz.
- Gerçek Windows Core/OpenConnect motorlarında 2 OpenVPN + ocserv ve ayrı 3 OpenVPN
  driverless kontrolleri geçti. Her tünelde 128 KiB checked ICMP payload; orta tünel kapanınca
  kalan ikisinde 32'şer KiB. Wintun/host network API'si kullanılmadı.
- Linux bağımsız broker laboratuvarı 35/35; Windows native vault 7 pass/1 helper ignore,
  VPN 2/2; Linux native VPN 3/3; UI mock testleri son bağımsız koşuda 5/5. Stop gate: 8 hook + TypeScript +29 Vitest.
- Sonraki bir Linux TCP sender exit1 nedeni kesinleşmedi; stderr tanılamasından sonra koşu
  geçti, ancak aralıklı sorunun çözüldüğü iddia edilmiyor.
- Son normal `pnpm vpn:build` geçti: test sürücüsü feature'ı kapalı, ayrı EXE
  `.tools/vpn-windows/desktop-dev/termterm.exe`, ürün `TermTerm VPN Dev 0.4.3-dev.1`.
  SHA256 `340bcb2164ef61755aab2c1431c6caab2d01861632e9793598dadb864944a9d6`;
  target/debug ile aynıdır. Güncel default helper bu EXE yanına henüz stage edilmedi;
  eski diagnostic helper hazır uygulama gibi taşınmaz. Native odaklı testler 4/4 geçti.

## Tarihsel kullanıcı kontrollü UAC denemeleri — v4–v7

Kullanıcının yeni UAC onayıyla exact-owned recovery ve normal v4 test **geçti**.
`broker-validation-v4/privileged-v3-normal-result.json` kanıttır. Ayrı v4 crash testi
`privileged-v3-broker-death-result.json` başarısız: host baseline/adaptörler geri geldi,
üç journal kaldı. v5 recovery fail-closed reddetti; raw GUID/LUID envanteri ve PnP Code45
üç exact-owned phantom kimliğin kaldığını gösterdi. Son v6 combined koşusunda native exact
recovery → normal → broker-death ve bağımsız sonuç incelemesi geçti: raw GUID/devnode yok, journal/broker listesi boş,
host baseline aynı. Kanıt
`.tools/vpn-windows/recovery-v3-result-20261022T152447808-b08bc3a749214f0d909836ea2816b441/recovery-result.json`,
SHA256 `a7d6acdb55e843785dd40738a563ae1c6caf30eb9095c0758ed080098dbd9e0d`.
Bu scoped lifecycle deneyi gerçek VPN authentication/kernel paket kanıtı değildir.
Yeni fresh-CA full-v4 (2 OpenVPN + Cisco/ocserv) hello/connect öncesinde
`different_user_logon_or_session` ile reddedildi: VPN trafiği yok, cihaz/süreç kalıntısı yok,
host baseline aynı. Linked UAC token/owner kontrolü inceleniyor; güvenlik koşulları gevşetilmez.
Rapor `.tools/vpn-windows/broker-full-validation-v4/full-broker-result.json`,
SHA256 `1f946125e6b7c1ecb9038d45f90691625a7b1b31655700d7f074dff3b02f2154`.
Bu hata için yeni fixed-source/bundle incelemesi olmadan eski frozen runner tekrar çalıştırılmaz.
Son full-v5: handshake geçti; connect `os error 1314` eksik ayrıcalıkla reddedildi;
spawn yolu inceleniyor. Events=0/pings=0, broker/adaptör kalmadı, host baseline aynı.
Rapor `.tools/vpn-windows/broker-full-validation-v5/full-broker-result.json`,
SHA256 `d3c0a4e9c154d0510bcb12a5cd8ad3fe0583467a9ba21404f900790d4c4c631a`.
Özgün manager token'ıyla güvenli spawn kararı bekleniyor; elevated engine fallback yok.
Full kernel trafiği ve gerçek panel bağlantısı tamamlanmış sayılmaz.
Full-v6: özgün düşük token spawn çalıştı, iki OpenVPN `connected`; Cisco/ocserv auth
reddi nedeniyle pings=0. Rapor `.tools/vpn-windows/broker-full-validation-v6/full-broker-result.json`,
SHA256 `087055ba9fbb3716b679963cd81406674adfdef40bb2676e4cdc1d1cb940fb72`.
Baseline aynı fakat authenticated cleanup `owned_adapter_still_present_cleanup_required`
ile doğrulanamadı; ayrı cleanup incelemesi sürüyor. Auth nedeni runner'ın bilerek boş
nested profile.password alanını kullanmasıdır; doğru credential kilitli fixture'ın üst alanındadır.
Yeni v4 runner ile bu düzeltme test edildi; motor/private profiller/frozen bundle aynı kaldı.
Full-v7: üç auth/connected ve OpenVPN .2/.3 için 6 kernel ICMP payload geçti; Cisco/ocserv
.1 ilk Ping.Send exception nedeniyle test başarısız. Authenticated cleanup geçti: journal/session0,
broker/adaptör yok, host baseline aynı. Rapor `.tools/vpn-windows/broker-full-validation-v7/full-broker-result.json`,
SHA256 `a37a8788e1b20164f5a14541eb7bc28be6fa15d57ef675abe536d778ce2cd560`.
Güncel diagnostic runner v5 inner NativeErrorCode/status ve owned adres DadState/route/interface
snapshot'ını cleanup öncesi ekler; blind wait/retry yok. Snapshot atomik değildir.
Otomatik elevation yok; global driver kaldırma yapılmadı, readiness kapalıdır.
Pinned Wintun Create/Close global orphan sweep de kuyruğa alır; DLL kullanmayan native
exact recovery ayrı sınırdır. Lifecycle/full deneyinden önce ilgisiz orphan yokluğu salt-okunur
envanterle kanıtlanmalıdır. Bu deney guard'ı genel üretim garantisi sayılmaz; global sweep
davranışı ve create-before-LUID crash penceresi ayrıca çözülmeden readiness açılamaz.
No-global-sweep gelecek userspace yolu henüz uygulanmadı: resmi signed DLL binary patch'i
ve embedded signed driver extract/repack yapılmaz. [Prebuilt lisansı](https://github.com/WireGuard/wintun/blob/0.14.1/prebuilt-binaries-license.txt),
[GPL kaynak](https://github.com/WireGuard/wintun/blob/0.14.1/COPYING) ve
[stock SYS/INF/CAT resources](https://github.com/WireGuard/wintun/blob/0.14.1/api/resources.rc)
ayrı dağıtım/driver sağlama kararı gerektirir. Kaynaktan ayrı userspace API + önceden
sağlanmış uyumlu resmi driver yalnız adaydır; kaynak/patch/recipe/hash ve yeni test kanıtı gerekir.

Tarihsel v4 helper: `.tools/vpn-windows/broker-validation-v4/termterm-vpn-broker.exe`.
SHA256 `f5bbfbc514f1faf0c1e8a74ba774662c008ae1525cbcdfdcf15bc1e71a824e24`.
Resmi Wintun DLL SHA256 `e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce`.
Doğru normal runner `validate-privileged-v3.ps1`, SHA256
`ed7aa11bd344ed06f3a6b9299c18dc7e6bf7ccee81157e230be7a97a403bab77`.
Eski `validate-privileged.ps1` kullanılmaz; hata kanıtını kaybedebilir.
Aşağıdaki tarihsel v4 komut yeni cleanup düzeltmesinin yerine kullanılmaz. Güncel frozen
paketi ve hash'i router/verifier yeniden inceler. Yalnız ayrıca onaylanan elevated PowerShell:

```powershell
& ./vpn-windows-broker/scripts/validate-privileged-v3.ps1 `
  -Helper (Resolve-Path .tools/vpn-windows/broker-validation-v4/termterm-vpn-broker.exe).Path `
  -ExpectedHelperSha256 f5bbfbc514f1faf0c1e8a74ba774662c008ae1525cbcdfdcf15bc1e71a824e24 `
  -Mode normal
```

Bu üç yeni sahipli adaptör, TEST-NET /32 adres/rota ve Wintun ring lifecycle testidir;
VPN authentication/kernel packet proof sayılmaz. Scoped normal/broker-death v6 geçti;
sonraki adım yeni fixture/bundle ile `validate-full-broker.ps1` gerçek kernel/VPN paketleri,
ardından genel sahiplik engelleri çözülünce gerçek masaüstü panelidir.
Her yeni ayrıcalıklı kapsam için kesin frozen bundle/hash ve ön/son snapshot incelemesi korunur.

Masaüstü IPC transport → authenticated `VerifiedClient` → broker/runtime kodu bağlıdır.
Gerçek geliştirme kullanımına geçmek için ayrı EXE yanında sabit helper/audited engine/DLL
staging/preflight'in güncel helper ile yeniden üretimi, privileged proof sonrası dar readiness açılması ve panelden
connect/challenge/disconnect/lock/exit temizliğinin gerçek testi kalır. Şu an yalnız sınırlı
split routes; DNS/proxy/exclusions/full-tunnel/net30/SSO desteklenmez. Fortinet/GP için
protokol eşlemesi ve derlenmiş library desteği vardır; gerçek üretici sunucusu sağlanmadı.

## Tarihsel fixture açılışı — artık kapalı, ready 14:26:01 UTC

Yeni fixture sahibi worker session95604, WSL PID1947; private ready.json süre7200 saniye,
14:26:01 UTC yazıldı, deadline yaklaşık 19:26:01 yereldir.
35 kontrol tamamlandı, fixture hazırdır. Final cleanup/source/host snapshot kontrolü bu
aktif koşu için henüz yapılmadı. Rapor hedefi
`artifacts/vpn-lab/desktop-broker-resume-20261022-1425.json`.
Yeni CA SHA256 `42366ce749f37626f0535b349d851789027097ca9a89ceaf3268d0035d4c8475`;
private profiller SHA256 `4423287099f65d68aa0aebf11028930afa5cb3869c356d7c3bcd0986b06e4ea6`.
Yeni full-broker bundle/manifest gerekir; önceki v2 paketi tarihsel kanıttır.
Fixture yalnız aktif test işi için tutulur; parent talebiyle aşağıdaki stop marker kullanılır,
sonra sahibi session95604 finally sonucunu ve raporu doğrular.

### Önceki kapalı fixture kanıtı

Tutulan fixture parent talebiyle stop marker kullanılarak kapatıldı; session52352 exit0.
Son raporda completed=true, 35/35, hostNetworkUnchanged/namespacesRemoved/sourceUnchangedDuringTest=true.
`ip netns list` boş; private profil/CA/ready dosyaları ve stop marker temizlendi, dizin boş.
Yeni fixture yalnız test/karar işi aktifken tutulur: `.lab/vpn-desktop/ready.json` PID ve süreyi taşır.
Loopback TCP 31194→10.88.0.2, 31195→.3, 31198→.4 OpenVPN; 31443→ocserv .1.
Profiller/kimlikler private `.lab` içinde; log/Git/mesaja kopyalanmaz.

Güvenli sonlandırma, worktree'de:

```powershell
New-Item -ItemType File -Path .lab/vpn-desktop/stop -Force | Out-Null
```

Çalıştıran worker yeni session'ın final cleanup'ını bekler; namespace/process temizliği,
ilgili koşunun raporunda completed/tests/snapshot kontrol edilir.
Sonra yalnız bu stop marker silinir. WSL global terminate veya başka namespace silme yapılmaz.
Yeniden başlatma için boş/private ACL'li `.lab/vpn-desktop` ve ana durum belgesindeki
laboratuvar komutuna `--desktop-fixture .lab/vpn-desktop --desktop-duration 7200` eklenir.
Fixture durduysa stale profillerle UAC/full-broker testi başlatılmaz. Yeni CA ve profiller yeniden
stage edilir, fixture manifest ve bundle hash'i tekrar oluşturulup incelenir. Önceki fixture
hash'i yeni manifest'e kopyalanmaz. Eski full runner `vpn-windows-broker/scripts/validate-full-broker.ps1`
v1 SHA256 `e96fa70cb894546b6998a0293aad90d00a7c7c58dc7019c2b0219e8c8deadab2`
tarihsel kalır. Yeni incelenmiş runner `vpn-windows-broker/scripts/validate-full-broker-v3.ps1`,
SHA256 `7af38bbebb79a808629bdad5ea8c448ba23cfeb5396067806739c69c4ca40e25`.
PS5.1 `-SelfTest` implementer/bağımsız 6/6 geçti; önceki connect/session hataları kapanışta
kaydedilirken yalnız exact Close makbuzu beklenir, ok=false/asıl hata korunur ve deadline
aşılmaz. Sonraki auth düzeltmesi için güncel runner
`vpn-windows-broker/scripts/validate-full-broker-v4.ps1`, SHA256
`e3b6f2a45e10ead573c652ef45b41004e19b18c4d334f82b93b089e271840b2a`.
PS5.1 implementer/bağımsız 10/10: boş nested profile yerine kilitli üst credential,
eksik credential/OTP/unknown field reddi dâhil. v4 ara koşusu bu incelenmiş pin ile üretildi;
eski v1/v2/v3 paketleri değiştirilmez.
Sonraki diagnostic koşu için güncel runner
`vpn-windows-broker/scripts/validate-full-broker-v5.ps1`, SHA256
`4bed56f81c0173684a6238ba59a96c4e9524e58ebc9ab3fc47366697d7459236`;
PS5.1 implementer/bağımsız selftest 11/11 geçti. Yeni bundle bu yeni pin ile incelenir;
eski frozen runner/bundle dosyaları değiştirilmez.

Önceki kapalı fixture için hazırlanıp bağımsız hash kontrolü yapılan tarihsel tam bundle:
`.tools/vpn-windows/broker-full-validation-v2` (30 üye). Helper SHA256
`84d009349e4f31224f9da8e1485d99bff8cb4c477454e8c61f03a5a897818c8f`, manifest SHA256
`a3086427949909c6e403805c7ad6fc5aabdd138a50f41fba7cfdee7c0f63ae3c`.
Feature-enabled unelevated suite 30/30, default suite 31/31 geçti. Bu bundle'ın CA/fixture
kimliği kapanan laboratuvara aittir: yalnız tarihsel kanıt, yeniden test onayı değildir.
Yeni fixture CA'sıyla yeni manifest/bundle hazırlanıp tekrar incelenmeden kullanılmaz.
