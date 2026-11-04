> **4 Kasım 2026 devir notu:** Güncel durum ve açık Windows engeli [HANDOFF_VPN_TR.md](HANDOFF_VPN_TR.md) dosyasındadır. Aşağıdaki kayıtlar tarihseldir; özel rapor bağlantıları Git ile taşınmaz.

# VPN geliştirmesi — 30 Ekim 2026, son kabul ve temizlik

[Son gerçek Linux GP raporu](../artifacts/vpn-lab/linux-company-globalprotect-final-20261030.json)
normal masaüstünde kayıtlı kullanıcı/parola + kullanıcının OTP'siyle tam tüneli,
267 rota ve DNS ayarlarını, VPN'e bağlı tek SSH tanıtım bağlantısını doğruladı.
SSH kimlik doğrulaması/uzak komut ve Microsoft tarayıcı SSO testi yapılmadı.
Normal kesmede engine/TUN/journal/sahipli rota sıfır; tüm özel test süreçleri,
router namespace'i ve misafir kapandı. Ana ağ/DNS/PostgreSQL/resolved aynı kaldı,
şifreli test bilgileri yerelde korundu. WireGuard 12/12, Team sessiz kayıp,
Linux kurulum 10 ve native full-tunnel 65 + 1 tanı kabulü geçti.
Kurulu stable/0.5 korunur; yayın yapılmadı. Windows native liveness, fiziksel
suspend, macOS ve güncel paket kabulü açık kalır. Ayrıntı ve kanıtlar
[güncel çalışma devrindedir](VPN_RESUME_TR.md). Aşağıdaki bölümler tarihseldir.

---

# Önceki checkpoint — 30 Ekim 2026, WireGuard/Linux

Güncel durum [çalışma devrinin üst bölümündedir](VPN_RESUME_TR.md).
WireGuard kernel kabulü **12/12**, gerçek Team sessiz UDP kaybı bağımsız **PASS**,
özel Linux systemd VPN/Team kurulumu **10 PASS** durumundadır.
[Kurulum raporu](../artifacts/vpn-lab/linux-installed-services-20261030.json)
root olmayan istemci doğrulaması için eksik `CAP_SYS_PTRACE` düzeltmesini,
dar Team servis rolüyle şema 66 kurulumunu ve host ağ/servislerinin korunmasını
belgeler. Şirket Linux GP portal/OTP girişi ve gateway seçimi normal arayüzde
geçti; ağ planı `existing_route_conflict` ön kontrolünde durdu. Açık özel
misafir bu işin sonunda temizlenecek. Tam Linux masaüstü kurucusu, şirket tüneli, Windows
adaptörde yeni liveness ve macOS geçti sayılmaz. Release/kurulu stable/0.5
korumaları sürer. Normal Linux denemesinde bulunan kayıtlı GP parolasını
yeniden sorma hatası, Windows ile ortak Rust alan denetimi ve ilk sır formunda
tek deneme kuralıyla düzeltildi. Politika alt sürecindeki bütün istek/yanıtlar
kilitlenerek eşzamanlı WireGuard/kimlik doğrulama yanıtlarının karışması önlendi.
[Odaklı yerel kabul](../artifacts/vpn-lab/linux-saved-auth-20261030-implementation.json)
OTP/tekrar/hata/gizli metadata sınırları ve 25+25 eşzamanlı isteği doğrular;
sonraki iki native servis tekrarı **55 PASS + 1 tanı** verdi. Ön girişteki
`-EIO` hatasının test ortamında kapanan slirp egress kaynaklı olduğu bulundu;
geçici systemd denetimi sonrasında sertifikası doğrulanmış bağlantı ve
kullanıcının OTP'siyle gerçek portal girişi ilerledi.
[Şirket checkpoint'i](../artifacts/vpn-lab/linux-company-portal-gateway-20261030.json)
gerçek tünel kurulmadığını açıkça kaydeder. Sabit enum/sayılarla motor tanısı
eklendi ve bağımsız yerel callback testleri geçti; sır/sunucu metni yazılmaz.
Linux full-tunnel bağlı rota ön kontrolündeki eksiklik giderildi; [bağımsız son kabul](../artifacts/vpn-lab/native-unix-full-tunnel-20261031-verifier.json)
**65 PASS + 1 tanı** verdi. Yalnız açık ham IPv4 `/0` ve uç noktaya giden
Ethernet/`veth` üzerinde gerçek adresle eşleşen daha dar `kernel/link` rota
istisnası vardır. Atanmış tünel alt ağı, split/custom/elle eklenen rota ve başka
VPN engelleri korunur; fiziksel önekteki VPN DNS mutation öncesi reddedilir.
Üç motorun üç kapanış biçiminde tam tünel, OpenConnect tam tünel + DNS ve gerçek
DNS çakışma reddi doğrulandı; host/kaynak/namespace temizliği geçti.
[Sınırlar](VPN_UNIX_KURULUM_TR.md#linux-tam-tünel-ve-fiziksel-ağın-korunması)
bridge/bond/VLAN/TUN/TAP, aynı alt ağdaki uç noktanın yetersiz istisnası ve
macOS doğrulamasını açık tutar. Gerçek şirket planı tutulmadığından split/full
olduğu veya kesin çakışan önek tahmin edilmez; şirket tüneli yeni kabul gerektirir.
Aşağıdaki kayıtlar önceki checkpoint'lerdir.

---

# Önceki checkpoint — 30 Ekim 2026, ikinci sınırlı kabul turu TAMAMLANDI

**Bu kabul turu kapandı; VPN geliştirmesi/yayın hazırlığı tamamlanmadı.** Güncel durum
bu bölüm ve [çalışma devrinin üstündedir](VPN_RESUME_TR.md). Aşağıdaki ilk 30 Ekim
checkpoint'i ve eski/başarısız raporlar tarihsel olarak korunur. Canlı test/UAC/fixture oturumu kalmadı. Kurulu stable uygulama, Desktop/termterm 0.5
kaynakları ve kullanıcı verileri korunur; release/main/tag/update feed, yayın ve
dağıtım değişmedi.

- **Linux kısa-ad düzeltmesi bağımsız PASS: 54 kontrol + 1 tanı.**
  [Bağımsız rapor](../artifacts/vpn-lab/native-unix-service-20261030-search-domains-verifier.json),
  SHA256 `c29283f58e79410629580767f4d0a4b5f317b427c733bcc850897bff071b453c`.
  Özel network/mount/D-Bus alanında gerçek resolved 255 ile OpenConnect
  `default-domain + split-dns` ve OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX` üzerinden
  nonroot `socket.getaddrinfo`, FQDN ve düz `probe` adını çözdü; her sorgu otorite
  cevap sayısını artırdı. İki protokolün disconnect/istemci-ölümü/broker-ölümü
  olmak üzere altı vakasında DNS/rota baseline'ı, yabancı DNS, boş journal ve
  interface temizliği doğrulandı; iki ad da tünel öncesi ve sonrası çözülemedi.
  Host ağı/DNS/servis kimliği ve kaynaklar değişmedi, namespace'ler temizlendi.
  Route-only DNS alanları, çakışma politikası ve gerçek OpenVPN `DOMAIN` güvenli
  reddi korundu. Önceki kısa-ad sınırı bu laboratuvar kapsamı için kalktı;
  hosta kurulu servis veya macOS doğrulaması değildir.
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
  [V4 gerçek native raporu](../.tools/vpn-windows/overlap-route-order-20261030-v4/privileged-v3-nested-routes-result.json),
  SHA256 `fa51bf0f648bae771566451b770c384b608ec354ebbb0a696e65ae464be9ab96`.
  Üretim düzeltmesi bütün rotaları koruyarak yalnız en özel prefix önce ekleme
  sırasını değiştirir; undo/journal ve strict cleanup gevşetilmedi. Beklenen OS metadata farkı yalnız tanı doğrulamasında sınırlandı. Sekiz rotanın
  tam küme/readback kontrolü geçti. Sekiz undo kanıtı yedi `owned-NETMGMT` ve
  parent rota kaldırılınca OS'nin sildiği broadcast `/32` için bir `absent`
  sonucudur. Final baseline geri döndü; native önce/sonra envanter boş, journal 0.
  Bu tanı VPN kimlik doğrulaması/veri trafiği değildir; taze 505 rota normal ve
  istemci-ölümü gerçek kabulü yukarıdadır.
  [V1](../.tools/vpn-windows/overlap-row-diagnostic-20261030-v1/privileged-v3-nested-routes-result.json)
  otomatik broadcast `LOCAL (2)` nedenini kanıtladı.
  [V2](../.tools/vpn-windows/overlap-route-order-20261030-v2/privileged-v3-nested-routes-result.json)
  sekiz ekleme sonrası strict protokol readback'inde **FAIL** oldu; SHA256
  `c15fefe54ec0a513d6aee821fe164932fcbd9b2a3eb224744bca1bc74aadaf8f`.
  [V3 FAIL](../.tools/vpn-windows/overlap-route-order-20261030-v3/privileged-v3-nested-routes-result.json)
  parent kaldırılınca OS'nin sildiği broadcast satırının varlığını yanlış
  varsayan tanı assertion'ıdır; **üretim cleanup başarısızlığı değildir**.
  V2/V3 son baseline/native/journal temizliği geçti; eski FAIL raporları korunur.

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

# VPN geliştirmesi — 30 Ekim 2026 tamamlanan sınırlı doğrulama checkpoint'i

**Bu sınırlı test turu bitti; VPN geliştirmesinin bütünü/yayın hazırlığı bitmedi.**
Açık UAC/test/fixture oturumu kalmadı. Güncel dal
`codex/feature-vpn-workspace`. Aşağıdaki 29 Ekim duraklatma ve önceki kayıtlar
tarihsel olarak korunur; güncel durum bu bölüm ve [çalışma devri](VPN_RESUME_TR.md)
üstündedir. Kurulu stable uygulama, Desktop/termterm 0.5 kaynakları ve kullanıcı
verileri korunur; release/main/tag/update feed, yayın ve dağıtım değişmedi.
Bu turda kurucu üretilmedi/çalıştırılmadı; kurulu stable uygulama değiştirilmedi.

- **Unix journal düzeltmesi bağımsız doğrulandı.** Ortak broker kilidi,
  yapılandırmanın mutation+durable save+ağ uygulamasını cleanup ile serileştirir.
  Eski kodda hatayı üreten deterministik add/add ve add/delete regresyonları,
  durable-before-apply ve cleanup hatasında recovery intent koruması geçti.
- **Gerçek Linux servis + resolved DNS: 54 PASS + 1 tanı.** Özel
  network/mount/D-Bus alanında systemd-resolved 255, OpenConnect ve
  **OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX`** üzerinden ayrı taze DNS kanıtlarıyla
  nonroot uygulama FQDN çözümü, normal/istemci-ölümü/broker-ölümü temizliği,
  üç eşzamanlı tünel ve üç intent ile recovery geçti. Host ağı/resolver/servis
  kimliği ve kaynaklar değişmedi; namespace'ler temizlendi.
  Gerçek `DOMAIN` split-DNS planı mutation öncesinde beklenen biçimde reddedildi.
  [Sakin bağımsız tekrar](../artifacts/vpn-lab/native-unix-service-20261030-openvpn-dns-verifier-quiet.json).
  [İlk bağımsız 54 kontrol koşusu](../artifacts/vpn-lab/native-unix-service-20261030-openvpn-dns-verifier.json)
  dış host ağ snapshot'ı eşleşmediğinden toplam **FAIL** olarak korunur;
  neden kanıtlanmadı, eşzamanlı Windows çalışması yalnız olasılıktır.
- **WebKit düzeltmesi geçti:** process-main fixture, beş gerçek native yanıt
  kontrolü ve **exit 0**. Ephemeral view, özel CA ve üretim TLS doğrulaması
  korunur. [Bağımsız rapor](../artifacts/vpn-lab/native-webkit-20261030-independent.json).
- **Team:** servis 7/7, client 42/42, Vitest 32/32, tsc, Team UI 11/11 ve
  toplam VPN UI 33/33 geçti; UI **MOCK IPC** kapsamındadır. Gerçek PG clean-66 ve 65→66 veri koruma,
  backup/restore/checksum/crash-resume/readiness, hesap/RLS/auth, API whitelist
  ve HTTPS/AEAD/offline/replay/revoke kontrolleri geçti. Whitelist 111 imza/
  110 farklı addır; `api_chain_finish` iki açık overload içerir. Şema 66/cache 9 ve
  migration hash'i korunur. [Team kapsamı](VPN_TEAM_TR.md).
- **Gerçek Linux Team VPN → SSH/SFTP bağımsız geçti:** gerçek HTTPS,
  şifreli Team profili/eşlemesi ve `requiredVPN` ile WireGuard üzerinden SSH
  ve SFTP SHA256 doğrulandı. Duraklatılmış aktarımda elle `vpn_disconnect`
  mevcut hedefi korudu; `.part` kalmadı, bağlantılar kapandı. Fiziksel hedef
  TCP paketi 0, journal/owned TUN boş; host ağı değişmedi, namespace'ler temiz.
  [Bağımsız rapor](../artifacts/vpn-lab/native-team-ssh-20261030-verifier.json).
  Keyfi ağ kesintisi ve Windows/macOS Team uçtan uca testi yapılmadı.
- **Eski Windows client-death sonrası audit açığı kapandı:** salt okunur
  protected journal **0**, recovery yok.
  [30 Ekim audit'i](../.tools/vpn-windows/wireguard-acceptance-20261029-v2-client-death/dns-journal-audit-after-client-death.json).
- **Windows EOF kaynak/test düzeltmesi bağımsız geçti:** dört dosyada Go
  `failed` olayını kapanıştan önce iletir; broker typed EOF'ta aktarımı hemen
  durdurup yalnız tanı için en çok 1 saniye bekler. Önceki hata/iptal,
  bozuk paket ve stale generation korunur; readiness bağlantıyı canlandıramaz.
  Bağımsız 66 unit + 8 integration, Go, UAC'siz gerçek yardımcı/kernel-peer,
  tsc ve 32 Vitest geçti; frozen bundle incelemesi **GO**. Bu sonuç yeni
  native Wintun kabulü değildir. [Kaynak ve kontroller](VPN_RESUME_TR.md).
- **Windows rota tanısı kaynak/bağımsız kontrolleri geçti:** sorgu aşaması,
  native kod ve destination hata bağlamında korunur; **67 unit + 8 integration**
  geçti. Sahiplik kapıları değişmedi; iç içe rota uyumluluğu çözülmedi.
- **Windows WireGuard v2 gerçek yanlış-anahtar kabulü PASS:** doğru
  `engine_failed: wireguard_peer_handshake_timeout`, hiç `connected` yok;
  before/after native envanter 0, final baseline ve ayrı before/after audit
  journal sayıları **0**. Recovery yapılmadı.
  [V2 kabul raporu](../.tools/vpn-windows/wireguard-acceptance-20261030-v2-wrong-key/wireguard-v1-wrong-key-result.json).
  [İlk v1 FAIL raporu](../.tools/vpn-windows/wireguard-acceptance-20261030-v1-wrong-key/wireguard-v1-wrong-key-result.json) korunur.
- **Çakışmayan 505 rota gerçek normal + istemci-ölümü kabulü PASS:** 505 owned
  rota, veri/DNS, zorunlu uygulama kısa-ad çözümü ve çakışmada mevcut bağlantıyı
  koruma geçti. İki koşuda final baseline geri döndü, native envanter **0**, ayrı
  before/after audit journal **0**, recovery yok. Cleanup **12559/14221 ms**.
  İsteğe bağlı PowerShell `Resolve-DnsName` bare-shortname tanısı **FAIL** kalır.
  [Normal rapor](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-normal/dns-v1-normal-full-tunnel-many-routes-result.json),
  [istemci-ölümü raporu](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-client-death/dns-v1-client-death-full-tunnel-many-routes-result.json),
  [ayrı after-client-death audit'i](../.tools/vpn-windows/many-routes-disjoint-acceptance-20261030-v1-client-death/dns-journal-audit-after-client-death.json).
  WireGuard + iki 505 kabulünün bağımsız native kanıt incelemesi **GO**.
- **Son kaynak/hook PASS:** Git Bash stdin `{}`, whitespace, 8 hook kontrolü,
  tsc ve Vitest 32/32.
- **Son cleanup doğrulandı:** yalnız owned marker cleanup, yabancı süreç
  öldürülmedi; launcher/relay/helper yok, Windows readOnly/safe native
  devices/registered interfaces **0**, Linux izlenen controller/fixture
  namespace'i yok. Disjoint fixture **35 lab PASS**, completed/host/source/
  namespace temizliği true; kurulu stable değişmedi.
  [Cleanup özeti](../artifacts/vpn-lab/windows-final-cleanup-summary-20261030.json),
  [native envanter](../artifacts/vpn-lab/windows-final-native-cleanup-20261030.json),
  [Linux kontrolü](../artifacts/vpn-lab/windows-fixture-linux-final-20261030.json),
  [disjoint fixture kapanışı](../artifacts/vpn-lab/disjoint505-fixture-20261030-v1.json).
- **Örtüşen/iç içe 505 rota planı açık uyumluluk sınırı; normal kabul FAIL.**
  Policy kabul ederken apply/preflight intent öncesinde reddediyor. V3'te owned
  `10.88.0.1/32` exact sorgusu `code=0`; longest-prefix hipotezi salt okunur
  probe ile çürütüldü, otomatik broadcast kaynağı kanıtlanmadı. Yalnız fixture
  kusuru diye kapatılmaz. Satır benimsenmedi/atlanmadı, kapılar gevşetilmedi.
  V3 sonrası baseline/native envanter doğrulandı, ayrı after audit journal **0**;
  **örtüşen planın istemci-ölümü koşusu çalıştırılmadı**.
  [V3 başarısız kabul](../.tools/vpn-windows/many-routes-acceptance-20261030-v3-normal/dns-v1-normal-full-tunnel-many-routes-result.json),
  [V3 after audit](../.tools/vpn-windows/many-routes-acceptance-20261030-v3-normal/dns-journal-audit-after-normal.json).
  Geçen **çakışmayan 505 rota** normal/istemci-ölümü stresi ayrı kapasite/cleanup
  kanıtıdır; **örtüşen planı doğrulamaz**, semantik workaround uygulanmadı.
  Son ayrı geliştirme paketi/ekranları ve etkilenen bağımsız kontroller bekliyor.
  `distributionReady: false` korunur; kaynak/bundle incelemesi yayın/kurulum onayı değildir.

OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX` FQDN alt kümesi geçti; `DOMAIN` split-DNS
politikası `advanced_dns_policy_unsupported` ile beklenen güvenli ret verir
([ilk başarısız koşu korunur](../artifacts/vpn-lab/native-unix-service-20261030-resolved-v1.json)).
Geçen Linux DNS kapsamı OpenConnect ve belirtilen OpenVPN alt kümesidir; kısa-ad araması ve hosta
kurulu üretim servisi doğrulanmadı. Gerçek macOS ve şirket Linux SSO yok;
IPv6 broker desteklenmiyor, mutlak OS DNS sızıntısızlık garantisi verilmiyor.

Sonraki öncelikler: policy kabulü/prepare reddi arasındaki örtüşen-plan
uyumluluk sınırı; ardından kalan ayrı geliştirme
paketi/ekranları ve platform kontrolleri. Hosta kurulu üretim servisi ve Team'de
keyfi ağ kaybı da doğrulanmış değildir. Bu belge yayın/kurucu onayı değildir.

---

# VPN geliştirmesi — 29 Ekim 2026 duraklatma checkpoint'i

**Kullanıcı isteğiyle duraklatıldı; yayın hazır değil.** Kaynaklar
`codex/feature-vpn-workspace` dalında,
`<VPN_WORKTREE>` içinde kaydedilmiş ve
commit edilmemiştir. Yeniden devam isteği olmadan kod/test/derleme/ağ/UAC yok.
Kurulu stable uygulama ve Desktop/termterm 0.5 kaynakları korunur; kurulu uygulama
başlatılmadı/değiştirilmedi/kaldırılmadı. Release/main/tag/update yayını yapılmadı.

**Öncelikli engel:** bağımsız Unix verifier, `scripts/vpn-unix-broker.py:160–170`
`Journal.save`, `configure:450–453` ve `scripts/vpn-lab-broker.py:279–281`
yolunda journal mutation+save yarışını yeniden üretti. A'nın eski snapshot'ı,
B'nin `{A,B}` disk kaydını `{A}` ile ezebiliyor; bellekte iki intent kalmasına
rağmen crash cleanup kaydı kayboluyor. Düzeltme henüz yapılmadı. İlk iş mutation
ve save'i aynı kilit altında serileştirmek, deterministik regresyon eklemek ve
eşzamanlı servis/recovery kontrollerini yeniden koşmaktır.

- **Windows WireGuard gerçek kabulü:** protokol 2 broker/Wintun/resmi motor ile
  gerçek Linux kernel peer'inde normal ve owner-death raporları `ok=true`.
  64 KiB TCP SHA256, taze DNS UDP/TCP ve uygulama kısa ad çözümü, owned interface
  ve kapanış baseline/native inventory geçti. Normalde explicit disconnect ve
  yükseltilmiş before+after-normal journal audit'leri 0. Owner-death before 0;
  **ayrı after-client-death protected-journal audit'i duraklatma nedeniyle yapılmadı**.
  İsteğe bağlı cmdlet kısa-ad tanısı başarısız; gerçek uygulama resolver'ı geçti.
  Raporlar `.tools/vpn-windows/wireguard-acceptance-20261029-v2-{normal,client-death}/wireguard-v1-*-result.json`.
- Fixture kendi stop marker'ıyla kapandı: namespace/host ağ/kaynak eşitliği
  true, relay stopped=true; son kontrolde broker/WG yok ve relay 15504 çıkmış.
  Controller'ın statik `windowsBrokerAdapterTested:false` alanı iki gerçek UAC
  raporunun toplam sonucu değildir. **Eski v2 pinleri tekrar bağlantı için
  geçersiz; yeni fixture ve frozen bundle gerekir.** Gerçek Wintun yanlış
  anahtar ve 505 rotalı normal/kesintili kabul çalıştırılmadı. Eksik audit için
  repository script yolu kullanılır; frozen audit kopyası yalnız review/hash
  referansıdır. Komut ve kesin kapsam `VPN_RESUME_TR.md` üstündedir; şimdi UAC yok.
- **Linux servis:** `artifacts/vpn-lab/native-unix-service-20261029-wg-ipv4fix.json`
  44 PASS + 1 tanı; source/host değişmedi, namespace'ler temiz. Gerçek WG kernel
  peer/owned TUN, OC/OV paralelliği, UID/hash, quiesce, çakışma ve ölüm/restart
  geçti. IPv6 RS düzeltmesi yalnız owned TUN'da UP öncesi `disable_ipv6` ve
  readback'tir; global IPv6 değişmedi. Bu rapor bağımsız journal engelini kaldırmaz.
- **WebKit toplam FAIL:** özel CA düzeltmesinden sonra beş gerçek native assertion
  geçti, ancak exit 134 kaldı. GTK libtest worker init / C++ main-exit
  `WebsiteDataStore::allDataStores` UI-thread assert nedeni kanıtlandı. Rapor
  `artifacts/vpn-lab/native-webkit-teardown-review.json`; trace
  `/tmp/termterm-webkit-teardown.trace`. Process-main, feature-gated `harness=false`
  fixture henüz uygulanmadı; üretim hook'u, ephemeral view, beş assertion ve
  özel CA korunacak. `_exit`, 134'ü yok sayma, sandbox/TLS gevşetme yok.
- **Team:** şema 66/cache 9; gerçek PG clean-66 ve 65→66 veri/backup-restore/
  checksum/crash-resume/HTTPS kontrolleri ve gerçek client/DesktopState kripto,
  offline-edit/reconnect/lost-reply/conflict-no-overwrite geçti. 066 SHA256
  `5a30dc383defcdd1e41264835e7a0cf2f00fee7c5460f3b64ecfd8b932793234`.
  Client 42 unit ve tsc geçti; **service unit 6 PASS / 1 FAIL**: lib.rs:507
  pozitif schema-65 fixture'ı gate-66 ile eski kalmış, duraklatmada düzeltilmedi.
  UI 6 kontrol ve `artifacts/vpn-ui/team-vpn-permissions.png` MOCK IPC kanıtıdır.
  Connect-only kullanıcının kişisel binding UI girişi eksik; API account-scoped,
  mevcut UI yalnız resource edit dialog'unda. Şifreli çakışan/iptal taslaklar
  yerel karantinada; yöneticiye otomatik teslim yok. Team bağımsız incelemesi,
  kişisel VPN UI regresyonu/format/hook, API exposure whitelist güncellemesi ve
  `TEAM_UPGRADES_TR.md` eski 65/8 → 66/9 düzeltmesi bekliyor. Devir: `VPN_TEAM_TR.md`.
- Bağımsız Windows review/testleri: 199 hash, broker 61 + packet 3 + parent 1 +
  readiness 4, WG Go/core, runner 8/35/13, native 3+6, tsc/Vitest 32. V5 paralel
  timeout sonrası izole geçti. Native Win 6/Linux 7/SSO-common 5; bound-lifecycle
  Win 8/Linux 8 + namespace 2 geçti. Generation lease/ilk TCP-proxy pin,
  SSH/SFTP/tunnel/copy iptali ve staging koruması var; gerçek Windows adapter
  cutover/socket-reuse ve Team SSH/SFTP-over-VPN uçtan uca kapsamı henüz tamamlanmadı.

Devam sırası: **journal yarışı → WebKit main fixture → özel alanlarda gerçek
Linux resolved DNS → bağımsız Team/platform incelemesi ve etkilenen kontroller →
eksik audit ve taze Windows yanlış-anahtar/505 kabulü → son dev uygulama paketi/ekranları**.
resolved 255/D-Bus mevcut; testte host servislerine dokunulmaz. Unix linux/macos
kaynak paketleri `.tools/vpn-unix-{linux,macos}-source-20261029-r2`:
distributionReady=false, installed=false, sourceSubset=true; kurulum notları
`VPN_UNIX_KURULUM_TR.md`. Gerçek macOS yok (kullanıcıda Mac yok); gerçek şirket
Linux SSO, IPv6 broker ve mutlak OS DNS sızıntısızlık garantisi yok.

Aşağıdaki 28 Ekim kayıtları tarihsel olarak korunur; güncel durum için bu
duraklatma checkpoint'i ve `VPN_RESUME_TR.md` üst bölümü önceliklidir.

---

# VPN geliştirmesi — 0.4.3-dev.1

Tarih: 28 Ekim 2026. Dal: `codex/feature-vpn-workspace`.
Temel: v0.4.2 (`b749b35`), PostgreSQL servis kurulum kiti kontrolü `d9d9671` ile taşındı.

## Güncel kapsam — 28 Ekim 2026: Authenticator tamamlandı, VPN devam ediyor

Kişisel/Team TOTP Authenticator üst çalışma alanı sekmesi, yerel QR/anahtar
girişi, kod/geri sayım/kopyalama ve manuel VPN challenge'ını koruyan sekme geçişi
tamamlandı. Şifreli kasa/yedek formatı 3, Team şema 65, gerçek PostgreSQL
yükseltme ve HTTPS şifreli paylaşım/kurtarma kontrolleri geçti. Son ayrı VPN Dev
executable ile gerçek Windows GUI'de yeni disposable kasa, public RFC QR, Tauri
panosu ve kilit/yeniden açma da doğrulandı; bu adım yeni VPN bağlantısı veya
gerçek sağlayıcı OTP testi değildir. Kanıt ve sınırlar
[AUTHENTICATOR_TR.md](AUTHENTICATOR_TR.md) içinde; yalnız public test hesabını
gösteren ekran `artifacts/authenticator-ui/native-windows.png`.

VPN'in üç platformda ürünleştirmesi, Team VPN kayıt/izin/host-klasör entegrasyonu,
gerçek WireGuard motoru ve 505 rotalı normal/kesintili native kabul henüz
tamamlanmadı. Authenticator **065** migration'ını kullandığından sonraki yeni
Team migration en az **066** olmalıdır; alttaki 065+ VPN planı tarihsel bilgidir.
Son executable ProductVersion **0.4.3-dev.1** ve ayrı `.vpndev` kimliğindedir;
Cargo kaynaklı alt bilgi `v0.4.2` mevcut kozmetik farktır. Kurulu stable/0.5,
kullanıcı verileri ve release/update feed korunur; yayın yapılmadı.

## Son kapanış durumu — 28 Ekim 2026

Kullanıcının kapatma isteğinde gerçek V9 bağlantısının yaklaşık 500 rotası geri
alındı; Tauri 10 saniyede timeout verdi ve adapter'ın devnode/GUID görünümü
arasındaki gecikme native 2 saniyelik beklemeyi aştı. Adapter/engine kayboldu,
normal doğrulanmış kapatma tamamlandı; sonraki audit sıfır journal gösterdi.
Aşağıdaki açık bağlantı ifadesi tarihsel kayıttır.

Kaynakta strict GUID+devnode yokluk beklemesi 30 saniye; yanlış LUID/sorgu
hatası hemen reddedilir. Cleanup RPC bütçeleri disconnect 60, üç bağlantı close
185 saniye, diğer istekler 10 saniye. Journal/phantom güvenlik kuralları korunur.
Broker 57/57, validation 58/58, Tauri VPN 5/5 ve runner PS5/7 35 kontrol geçti.
İzole 505 owned rota senaryosunun ilk native koşusu prepare aşamasında başarısız
oldu, baseline geri döndü; kapatma düzeltmesinin native kabulü sayılmadı. Safe
prepare nedenini gösteren tanı paketi V2 henüz UAC/native çalıştırılmadı.
ManyRoutes fixture kendi marker'ıyla exit 0 kapandı: host ağı değişmedi,
namespace'ler temiz, 35 passed + 1 prepared. Eski fixture pinleri artık geçersiz;
505 rotalı normal/kesintili kabul sonraki devamda taze fixture ile bekliyor.
Detay ve paket yolları `VPN_RESUME_TR.md` içinde. Release/kurulu stable değişmedi.

## Önceki canlı uygulama durumu — 28 Ekim 2026 (tarihsel)

Desktop V9 ile gerçek şirket GlobalProtect tüneli kuruldu. Kullanıcı UAC/OTP'yi
tamamladı; gateway sonrası generation 2 backend `connected`, arayüz aktif 1/Bağlı
gösterdi. Root Windows'ta Up TermTerm adaptörü, 500 IPv4 rotası, iki DNS sunucusu
ve pozitif gönderilen/alınan sayaçlar gözledi. Bağımsız salt okunur doğrulama
sürüyor. Kullanıcının izin verdiği SSH hedefinde seçili VPN rotası doğrulandı;
sistem OpenSSH port 22 handshake sonrası doğru kullanıcının parola istemine
ulaştı. Parola gönderilmeden Ctrl+C ile kapatıldı. VPN üzerinden SSH parola
istemine erişim geçti; kimlik doğrulama/shell/TermTerm SSH arayüzü/SFTP henüz
test edilmedi. Kullanıcı config'i/agent/public-key kullanılmadı; TOFU accept-new
kaydı yalnız özel `.lab` dosyasına alındı, kullanıcı known_hosts değiştirilmedi.
Hedef IP/kullanıcı veya sırlar bu rapora alınmadı.

Aktif bağlantı kullanıcının denemesi için AÇIK BIRAKILDI; uygulama/broker/adaptör
durdurulmayacak, temizlik/sweep çalıştırılmayacak. Native kabul aşağıdaki sınırlı
izole kapsamda geçti; şirket tünelinin kurulması bütün şirket makinelerine erişim
kanıtı değildir. Stable/0.5 ve release değiştirilmedi; sırlar kayda alınmadı.

## Güncel native tam tünel sonucu — 28 Ekim 2026

`broker-full-validation-20261028-v3` normal ve istemci ölümü kabul senaryoları
`ok=true` ile geçti. Orijinal IPv4 `/0` + güvenli sunucu exclusion'larından üretilen
izole subnet üzerinden 256 KiB TCP SHA256, DNS UDP/TCP, uygulama kısa ad çözümü,
peer/exclusion yolunun korunması, ikinci tam tünel reddi ve ilk bağlantının
devamı doğrulandı. Her iki kapanışta ağ baseline'ı ve kesin boş native envanter,
ayrı yükseltilmiş audit'te sıfır journal görüldü; recovery yapılmadı. Host interneti
yönlendirilmedi; bu sonuç keyfi internet egress veya gerçek şirket tüneli testi
sayılmaz. `Resolve-DnsName -DnsOnly` çıplak kısa ad tanısı isteğe bağlı başarısız
ölçüm olarak korunur; normal uygulama resolver'ı yeni tünel cevabıyla geçti.

Desktop V9, 31 dosya ve ayrı VPN geliştirme kimliğiyle hazır/bağımsız doğrulandı;
manifest SHA256 `79b192b385cd138033b22aa00b7c7553d77d6d0b902bf398b72bb35d9eb79882`.
Kaynak kontrolleri default 56/56, validation 57/57, readiness 4/4 geçti.
Yeni şirket GlobalProtect bağlantısı bu paketle kuruldu; güncel kanıt ve açık
bırakma durumu üstteki checkpoint'tedir. Release, kurulu stable ve 0.5 değişmedi.

İzole fixture `94622` kendi marker'ıyla exit 0 kapandı; 35 test geçti, bir fixture
hazırlandı; host ağı/kaynaklar değişmedi, namespace'ler temiz. Rapor
`artifacts/vpn-lab/full-fixture-20261028-v2.json`. CA/profiller temizlendiğinden
native tekrar için yeni fixture/paket gerekir. Eski başarısız UAC koşusu korunur.

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

Son gerçek GlobalProtect denemesinde normal parola/OTP ve gateway seçimi geçti.
Sunucunun IPv4 `/0` planı `defaultRoutePresent=true` ile adaptör oluşturulmadan
reddedildi (iki DNS, bir suffix, 10 exclusion; diğer gelişmiş DNS/PAC yok).
Full-tunnel henüz desteklenmiyor; gerçek tünel kurulmadı. Son salt okunur
kontrolde engine/TermTerm VPN adaptörü kalmadı. Guard gevşetilmedi; full-tunnel
ve transport peer/fiziksel rota önceliği için ayrı geliştirme ve kabul gerekir.

V6 native testinde normal uygulama resolver'ıyla kısa ad, FQDN UDP/TCP ve her
çağrıda yeni tünel DNS cevabı doğrulandı. Adaptöre özel DNS/suffix/search-list,
CIDR exclusion sonrası rota seçimi, ikinci DNS sahibinin reddi, normal kapanış
ve başlangıç ağ durumuna dönüş geçti. Excluded adrese paket gönderilmedi.
`Resolve-DnsName` çıplak kısa-ad tanısı başarısız kaldığı için V6 raporu
`ok=false` olarak korunur; aynı tünelde uygulama resolver'ı suffix ekleyerek
başarılıdır. Bağımsız inceleme/escalation ile V7 runner'ında yalnız cmdlet kısa-ad
ölçümü isteğe bağlı tanı yapıldı; uygulama resolver'ı, doğru IP ve yeni tünel
cevabı zorunlu kalır. Global DNS/suffix/cache değiştirilmedi.

Broker 52/53 test, pinned DNS parser'ı 4 olumlu/19 ret, fixture 12 ve runner
PowerShell 5/7 üzerinde 30 kontrol geçti. V7 native client-death/exclusion'da
zorunlu DNS/rota/çakışma ölçümleri ve ağ baseline'ı geçti; tek seferlik native
envanter henüz boş olmadığından koşu başarısız korundu. Sonraki salt okunur
envanter boş ve ayrı journal audit sıfırdır; recovery yapılmadı. Yeni native V8
runner'ı 30 saniyede ağ baseline'ı ve kesin boş native envanteri birlikte bekledi
ve geçti (`ok=true`); ilk örnekte bir registered interface, sonrakinde sıfır
ölçüldü. DNS/rota/çakışma ve owner-death temizliği doğrulandı. Ayrı kapanış journal
audit'i bitmeden V8 şirket denemesi yapılmaz. 31 dosyalık V8 geliştirme masaüstü
paketi hazır; gerçek yeni GlobalProtect tüneli full-tunnel guard'ında durdu. İzole WSL
fixture'ı kendi marker'ıyla kapandı: host ağı aynı, namespace listesi boş,
35 lab kontrolü geçti; bir desktop fixture yalnız hazırlandı olarak kayıtlıdır.
Kurulu TermTerm, dış VPN istemcileri ve 0.5 çalışması değiştirilmedi; release yok.
Paket/hash/fixture devri [güncel çalışma notunda](VPN_RESUME_TR.md) bulunur.

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

V6 oluşturuldu ve 31 dosyasının hash kontrolü geçti. Önceki baseline hook
TypeScript, 31 Vitest ve 8 hook testiyle geçti; bu doküman checkpoint'i için yeni
test çalıştırılmadı. Normal `Bağlan` denendi; auth-only değildir. Generation 2'de
portal SSO tamamlandı, gateway seçildi ve challenge 3 `stage=gateway` aşamasına
geçildi. Gateway'de Microsoft oturum açma ekranı görülüyor; kullanıcının gateway
girişini/MFA'yı tamamlayıp tamamlamadığı açıklama bekliyor.

240 saniyelik güvenli ölçüm: `responseObserved=41`, `acsMatched=0`, `status=null`,
dört `headerPresence=false`, `validationOutcome=awaitingResponse`,
`completionOutcome=waiting`. Eşleşen gateway ACS yanıtı henüz gözlenmedi; bu,
başlıkların eksik olduğu veya uygulamanın/protokolün döngüye girdiği tanısı değildir.
Wintun adaptörü oluşturulmadı ve gerçek tünel/SSH kanıtı yoktur. Sonraki adım
kullanıcının gateway giriş/MFA tamamlanması hakkındaki açıklamasıdır. Müşteri adı,
endpoint, kimlik bilgisi veya özel profil yolu kaydedilmedi; stable/0.5 ve yayın
sınırları korunur. Tam güvenli metadata [devirde](VPN_RESUME_TR.md) bulunur.

## Güncel düzeltme — mantıksal DNS origin'i, 26 Ekim 2026

V5 gerçek normal `Bağlan` denemesi auth-only değildi; privileged broker başladı,
ilk SSO challenge'ından önce `sso_origin_changed` ile durdu. Kök neden C++ origin
kaynak getter'ıdır: OpenConnect `get_hostname()` TLS sonrasında numeric peer IP'sini,
`get_dnsname()` ise URL/TLS hostname'ini verir. Motor browser callback origin'ini
artık her challenge'ın güncel DNS adı/port'undan üretir. Default443 zaten doğruydu.
Rust canonical kontrolü, başka gerçek origin reddi ve doğrulanmış gateway allowlist'i
değişmedi. Header/body formatı hakkında gözlem dışı varsayım yapılmadı.

Eski motor yeni DNS fixture'ında beklenen origin reddini verdi; yeni motorla TLS
17/17 (native motor 10, DNS 3), lab contract 10/10, C++ self-check ve Rust core SSO
testi geçti. DNS/numeric-peer ayrımı ve iki aşamalı DNS portal/gateway port değişimi
gerçek loopback TLS ile doğrulandı. Pure default/nondefault443, case, IPv6-origin ve
ASCII punycode kontrolleri tünel desteği kanıtı değildir. Motor SHA256
`6ca6549f1bd6cf1f49c6ace85781f5f00c6745b8317c19ba0ba82b175533c6bf`;
kanıt `artifacts/vpn-auth/gp-sso-dns-origin-20261026.json`. V6 paketleme/doğrulama
tamamlandı; gerçek denemenin güncel sınırları üstteki 27 Ekim checkpoint'indedir.
Bu yerel fixture VPN/tünel kabulü değildir.

## Kaynak ek tanısı — ACS yanıtı, 26 Ekim 2026

Hedef gerçek tüneldir. V3 portal ACS'de `Login Successful!` görünürken panel portal
SSO challenge'ında kaldı; bu görüntü tek başına başlık/HTML biçimini veya VPN
kimlik doğrulaması başarısını kanıtlamaz. Yeni Windows debug+e2e kaynak komutu
`vpn_sso_test_diagnostics({id,generation,challengeId})`, yalnız yerel development
main view ve aynı açık kişisel kasadaki tam kimlik için okunabilir. Son 32 native
bellek raporu terminal/timeout sonrasında korunur, kasa lifecycle ile silinir.
Yalnız response/ACS sayaçları, HTTP status, dört başlığın görülen presence
boolean'ları ve validation/completion enum kodları döner; URL/body/DOM/başlık
değerleri/hesap/token/log içeriği dışarı çıkmaz. `submitted` motor yanıt teslimidir,
tünel başarısı değildir. Handler kaydı navigasyondan öncedir; delivery/lifetime
hatası henüz kanıtlanmadı. Authentication kuralları değişmedi, body parser veya
canlı ağ denemesi eklenmedi. V4 build/canlı tanı router'da bekler; çalışan V3'e
yazılmadı. Ayrıntılı komut ve kod yorumları [devirde](VPN_RESUME_TR.md).

## Kaynak ek düzeltmesi — çift SSO iptali, 26 Ekim 2026

V3 masaüstü/default broker paketi 31 dosyayla hazırlandı ve hash'leri doğrulandı.
Canlı v3 testi devam ederken yalnız kaynakta ek iptal düzeltmesi yapıldı: SSO pane
kapanışı motoru önce kaldırmışsa ikinci iptal, tam auth-only session/kasa/generation
kimliğiyle etkisiz başarı döner; terminal durumu için `stale` üretmez ve normal
broker'a düşmez. Eski generation/yanlış kasa ve geç authentication cevapları
reddedilir, önceki hata kaydı korunur. Focused Rust auth-only 4/4 ve whitespace
kontrolü geçti; native GUI yarış testi yapılmadı. Ek düzeltme çalışan v3 paketinde
yoktur; yeni build/paket yapılmadı. Gerçek gateway sonucunu router kaydeder.

## Güncel durum — portal/gateway SSO geçişi, 26 Ekim 2026

Portal Microsoft girişi/MFA sonrası gerçek gateway seçimi gözlendi; seçim sonrası
ikinci SSO isteği tek başına döngü kanıtı değildir. Son gateway kabulü doğrulanmadı;
önceki dört deneme auth-only başarısız bitti, tünel açılmadı. Callback origin'inin
ilk portalda sabit tutulduğu kaynak hatası düzeltildi: her challenge OpenConnect'in
güncel host/port'una bağlanır. Broker/lab origin'i yalnız portalın sunduğu, boş
olmayan gateway seçeneklerinden doğrulanan yanıtla değişir; kaydedilmiş seçim
ön seçilir fakat GP gateway seçimi açıkça onaylanır. Her aşamanın kendi token adı
korunur. Arayüz aşamayı/tünelsiz sonucu gösterir, cleanup son hatayı silemez ve
iptal auth-only hata sayılmaz. Uzak pencerenin dışına tıklamak SSO'yu kapatmaz.

TLS 14/14 (aynı motor auth-only 8), lab contract 10/10, UI mock 9/9 ve baseline
hook (8 hook, TypeScript, 31 Vitest) geçti; ayrı origin/port, aynı origin,
kayıtlı seçim, stale/off-origin ve iptal kapsamı vardır. Kanıt
`artifacts/vpn-auth/gp-sso-transition-20261026.json`; ayrıntılı devir
[VPN_RESUME_TR.md](VPN_RESUME_TR.md#güncel-devir--portalgateway-sso-geçişi-26-ekim-2026).
V3 paket doğrulaması geçti; ek çift iptal kaynak düzeltmesi henüz paketlenmedi. Gerçek yeni gateway kabulü,
tünel/SSH, macOS ve hardware key kapsamı yok. `readiness=false`, stable/0.5 ve
yayın korumaları sürer.

## Önceki durum — native SSO fixture, 26 Ekim 2026

Native negatif IPC fixture'ı 1/1 geçti; gerçek Microsoft SSO başarısı değildir.
`NavigateToString` inert belgeyi `data:` URI olarak bildiriyordu ve HTTPS navigasyon
kuralı bunu iptal ediyordu (native `cancelled=true`, error 14). Yalnız debug/e2e
fixture'ında sabit belgenin tam URI'sine izin verildi; keyfi data URI, değiştirilmiş
payload, query/fragment, MIME/charset değişikliği kabul edilmez. Başlangıç blank wait
kaldırıldı: pinned Tauri bu navigasyonu hiç başlatmıyor.

Native üretim web mesajı ayarının kapalı olduğu önce assert edilir. Opaque fixture
origin'i fetch IPC için uygun olmadığından yalnız inert testte Tauri'nin kendi
postMessage transport'u açılır. Gerçek auth sayfalarının HTTPS/frame/TLS kuralları,
web mesajı yasağı ve main-only capability'leri değişmedi. `vault_info` ve
`window.inner_size` gerçek Tauri ACL hatalarıyla reddedildi; bu, üretim transport'unda
komut denemesi veya application label guard kapsamı olarak raporlanmaz.

Debug/e2e build, strict denial self-check 6/6, native fixture 1/1; bağımsız hook'ta
whitespace, 8 hook, TypeScript ve 30 Vitest geçti. Disposable vault korundu, sessions
boş, child kapalı; implementer sonrası GUI/engine/broker süreçleri boş. Bağımsız
native fixture 1/1, paketin 31 dosya hash kontrolü ve son focused Rust SSO tekrarı
8/8 geçti (0 failed/ignored; isolated dev `TAURI_CONFIG`, `--lib --features e2e vpn_sso::`).
Kanıtlar `artifacts/vpn-ui/native-sso-diagnostic-20261026.json`
ve `artifacts/vpn-ui/native-sso-isolation-implementer-20261026.json`; bağımsız tekrar
`artifacts/vpn-ui/native-sso-isolation-verifier-20261026.json`.

Yeni `.tools/vpn-windows/desktop-gp-sso-e2e-v2` 31 üyeli pakettir; v1 korundu.
EXE SHA256 `3970ce057e6ae7f7356554da60b1ceffa4fdd2ee44a3f13a8fbc84d0bd62b47c`;
manifest SHA256 `130d4d2100aa2c35f5f80536ad8441f10a91aa9685736eb125883e91eb123ea8`.
Gerçek Microsoft hesap/MFA, HTTPS header teslimi, tünel/SSH, macOS/hardware key henüz
test edilmedi. `readiness=false`; stable, 0.5, kullanıcı verileri ve yayın sınırları korundu.

## Önceki duraklatma kaydı — kullanıcının talebi, 25 Ekim 2026

Yeni çalışma/test/ağ denemesi yapılmaz. Native debug/e2e build geçti; son bağımsız
kontroller TypeScript 0 hata, Vitest 30/30, UI 8/8, hooks/izolasyon 13/13,
native VPN 13/13, yerel TLS 8/8 (aynı motorla 2 auth-only) ve lab contract 7/7.

Native GUI negatif IPC fixture'ı `SSO native isolation check timed out` ile
**başarısızdır**; GUI izolasyonu henüz kanıtlanmadı. Gerçek Microsoft parolası
gönderilmedi; hesap/MFA, gerçek HTTPS header teslimi ve VPN tüneli/SSH doğrulanmadı.
Devamda ilk sınırlı teşhis inert fixture için host `ExecuteScript` literal nesne,
`readyState`/başlık/`typeof invoke` ölçümüdür; belge yükü, inline script/CSP veya
polling callback yalnız olası nedenlerdir. Gerçek auth güvenliği değiştirilmez.

Paket `.tools/vpn-windows/desktop-gp-sso-e2e-v1`, 31 dosya; EXE SHA256
`24218a7e4d98150aff9395e3aa4a709c666540a8e619f4c9966f18e12d2e4414`, manifest
SHA256 `a7daeb131e71741f76d0135e8e3db3fc1d88a9eaa66d3e4cabbfc041ed41b1ae`.
V1 sonrası kaynak/config değişikliği yoktur; bu checkpoint ve fixture çıktıları
korundu. Task-owned GUI/engine/broker süreç envanteri boş; bilinen build/test
oturumları sona erdi. Stable/0.5 ve kullanıcı verileri değişmedi; yayın yok,
`readiness=false` korunur.

## Güncel durum — GlobalProtect SSO motoru, 25 Ekim

Kimliksiz gerçek prelogin geçerli GlobalProtect XML (`Success`, SAML `REDIRECT`)
döndürüyor; aynı OpenConnect library API'si Microsoft giriş yönlendirmesine ulaştı.
Önceki CLI `protocol_response_unrecognized` sınıflandırmasının nedeni eksik browser
callback'inin `EINVAL` sonucunun XML parse hatası olarak sunulmasıdır. Native private
SSO challenge/result akışı, origin/session/generation/expiry kontrolleri ve auth-only
durak noktası eklendi. Küçük LGPL library patch'i, SSO token'ını gerçek
`prelogin-cookie`/`portal-userauthcookie` alan adıyla gönderir; eski DLL aynı fixture'da
yanlış `passwd` alanı nedeniyle başarısız olur.

Yerel doğrulama: 8 TLS koşusu, bunun içinde aynı C++ motoruyla 2 auth-only başarı;
ayrıca core 13, broker protocol 4, packet API 11, diagnostic 38, native boundary
self-check ve lab contract 7 kontrol geçti. Bağımsız verifier 8 TLS/2 motor/7 contract
koşusunu tekrar geçti ve staged kaynak/binary hash'lerini doğruladı. Rapor
`artifacts/vpn-auth/gp-sso-protocol-20261025.json`; staged manifest
`.tools/vpn-windows/gp-sso-engine-v1/engine-sso-manifest.json`.

Gerçek Microsoft hesap girişi/MFA ve native masaüstü SSO doğrulaması bekliyor.
Bu auth-only sonuçları üretici VPN tüneli/SSH veya host ağı üzerinden trafik kanıtı
değildir: motor CSTP'den önce durdu; network-plan/adaptör/rota/DNS değişikliği yapmadı.
`readiness=false`, Wintun sahiplik ve mevcut ağ politikası kapıları korunur.
Kurulu stable, kullanıcı verileri ve 0.5 çalışması değiştirilmedi; yayın yok.

### Güncel protokol sınırları

Tamamlama, native gözlenen başarılı HTTPS yanıtında challenge'ın güncel VPN origin'i ve
belgelenmiş `/SAML20/SP/ACS` yolu, `saml-auth-status: 1`, kullanıcı ve tek onaylı
cookie başlığı gerektirir. Gateway origin'i yalnız portalın sunduğu seçeneklerden
doğrulanan seçimle değişir; farklı portal/gateway origin ve port'u yerel TLS
fixture'ında doğrulandı. Challenge dışı origin veya callback yolu reddedilir.
Yerel fixture REDIRECT
akışını doğrular; browser POST akışı, macOS, hardware key ve allocation-failure
injection test edilmedi. Gerçek portal Microsoft/MFA görünümü gözlendi; son gateway
kabulü ve gerçek tünel ayrıca kanıt gerektirir. Başarılı sayfa metni veya body içeriği
tamamlama sinyali olarak kullanılmaz.

## Kurulu uygulamanın korunması

Kullanıcının açık talebi: kurulu kararlı TermTerm silinmeyecek veya değiştirilmeyecek.
Bu çalışmada TermTerm kurucusu/kaldırıcısı çalıştırılmadı, kısayollar ve kullanıcı kasaları değiştirilmedi.
VPN kaynakları ayrı worktree'de; Desktop/termterm altındaki 0.5 SQL geliştirmesine yazılmadı.
VPN geliştirme kimliği `local.termterm.desktop.vpndev`; kurucu üretimi ve güncelleme uçları kapalı.
Bu kimlikle ayrı debug masaüstü EXE'si derlendi (`pnpm vpn:build`); kurucu üretilmedi.
Bu derleme Windows VPN bağlantısının çalıştığı anlamına gelmez.

## Önceki 25 Ekim deneyi — gerçek üretici kimlik doğrulama sınırı

**Fortinet: Çalışıyor — giriş ve OTP aşaması doğrulandı.**
Tek parola formu gönderildi; gerçek OTP challenge alındı ve yerel diagnostic
CLI güvenilir `42` çıkış koduyla durdu. OTP gönderilmedi, tünel açılmadı. Kimlik bilgileri
yalnız private stdin pipe ile bellekte taşındı; hedef/hesap/parola/cookie kaydedilmedi.
Callback mevcut hostname/port'u ilk origin ile eşleştirir; ikinci formu yanıtlamaz.
`--authenticate --non-inter --no-http-keepalive` tünel ve credential POST tekrarını engeller.

GlobalProtect keşfinde kimlik bilgisi gönderilmedi: TLS kuruldu, HTTP 200 alındı;
mevcut OpenConnect CLI prelogin yanıtını tanıyamadı (`protocol_response_unrecognized`).
SSO uç noktası gözlenmedi; bu sonuç sunucunun SSO desteklemediği anlamına gelmez.
Tarayıcı/Microsoft girişi, tam authentication ve gerçek Fortinet/GP tüneli test edilmedi.
Sanitize rapor `artifacts/vpn-auth/vendor-auth-20261025.json`; wrapper selftest 36,
native origin/form/OTP guard 10 kontrol geçti. Kurulu stable, uygulama ve
`readiness=false` aynı kaldı; release/installer/update yayını yapılmadı.

## Önceki sonuç — Windows full-v9 geçti, masaüstü kapısı kapalı

**Sınırlı Windows broker/kernel testi v9 PASS; geliştirme masaüstü `readiness=false`.**
İki OpenVPN + bir Cisco/ocserv oturumu aynı anda bağlandı; üç hedefte 24 ICMP payload
doğrulandı, orta OpenVPN kapatılıp yeniden açıldı ve kalan oturumların trafiği korundu.
Authenticated cleanup 0 journal/0 session döndürdü; native devnode/GUID, broker ve adaptör
kalıntısı yok, adres/route/interface/DNS baseline aynı. Kanıt
`.tools/vpn-windows/broker-full-validation-v9/full-broker-result.json`, SHA256
`4dcea13314856c7f15c76066e9bd8fc83ba8c38af5f31b544be6ced6b1d27907`.
Bağımsız verifier 24 payload'ı, 30 bundle hash'ini, boş native/device/process envanterini
ve aynı baseline'ı doğruladı. Son broker suite: default 44/44, validation 46/46;
runner selftest 11/11. Bu, Fortinet/GlobalProtect üretici sunucusu testi değildir.

WSL fixture stop marker ile kapatıldı (session95604 exit0). Final rapor
`artifacts/vpn-lab/desktop-broker-resume-20261022-1425.json`, SHA256
`9013841fbfc4c2ceeb0914bdb420dbbccf7b76d51c2217e5607221c632920200`:
35/35, host network aynı, namespace'ler silindi, test edilen kaynak hash'leri güncel kaynakla
aynı. `ip netns list` boş; private profil/CA/ready ve stop marker temizlendi.

Gerçek native profil UI testi implementer/bağımsız 1/1 (379/530 ms), native testler 4/4,
UI Playwright 5/5 geçti. Güncel default helper ile son masaüstü bundle henüz üretilmedi;
panelden gerçek connect/challenge/disconnect/lock/exit akışı henüz doğrulanmadı.
Global Wintun orphan sweep ve create-before-LUID crash penceresi, DNS/proxy/exclusion/
full-tunnel/net30/SSO ve gerçek Fortinet/GP profilleri kalan sınırlardır. Readiness açılmadı,
kurulu TermTerm değiştirilmedi ve release/installer/update yayını yapılmadı.

## Tarihsel ara denemeler ve uygulama kanıtları — v4–v7

Aşağıdaki hata ve “inceleniyor/henüz geçmedi” ifadeleri o aşamanın tarihsel durumudur;
v9'un yukarıdaki scoped başarı kanıtını veya kalan üretim/masaüstü sınırlarını değiştirmez.

**Windows masaüstü gerçek bağlantı için henüz hazır değildir; `readiness=false` korunuyor.**
Kullanıcının yeni UAC onayıyla exact-owned recovery ve normal v4 Windows ağ nesnesi testi
geçti. Ayrı v4 broker-death testinde üç journal kaldı; v5 recovery CIM'de görünmeyen raw
GUID/LUID ve PnP Code45 kimlikleri nedeniyle güvenli biçimde reddetti. Son v6 combined
recovery → normal → broker-death koşusu ve bağımsız sonuç incelemesi geçti: raw GUID/devnode yokluğu doğrulandı,
journal/broker listeleri boş ve host baseline aynı. Kanıt
`.tools/vpn-windows/recovery-v3-result-20261022T152447808-b08bc3a749214f0d909836ea2816b441/recovery-result.json`,
SHA256 `a7d6acdb55e843785dd40738a563ae1c6caf30eb9095c0758ed080098dbd9e0d`.
Global driver kaldırma yapılmadı; readiness bu sonuçlarla açılmadı. Bu lifecycle deneyi
VPN authentication veya kernel paket trafiği kanıtı değildir. Yeni CA/profillerle frozen full-v4
iki OpenVPN + bir Cisco/ocserv denemesi hello/connect öncesi
`different_user_logon_or_session` ile reddedildi. Trafik oluşmadı; cihaz/süreç kalmadı ve
host baseline aynı. UAC linked-token/owner eşleşmesi inceleniyor; kimlik kontrolü gevşetilmedi.
Başarısız denemenin raporu `.tools/vpn-windows/broker-full-validation-v4/full-broker-result.json`,
SHA256 `1f946125e6b7c1ecb9038d45f90691625a7b1b31655700d7f074dff3b02f2154`.
Son full-v5 denemesinde handshake geçti, connect `os error 1314` (eksik ayrıcalık) ile
reddedildi; spawn yolu inceleniyor. Rapor events=0/pings=0, broker/adaptör listesi boş ve baseline
aynı gösterir: `.tools/vpn-windows/broker-full-validation-v5/full-broker-result.json`,
SHA256 `d3c0a4e9c154d0510bcb12a5cd8ad3fe0583467a9ba21404f900790d4c4c631a`.
Capture edilmiş özgün kullanıcı token'ıyla motor oluşturma yolu inceleniyor; elevated motor
fallback'i eklenmedi. Tam broker/kernel trafik deneyi henüz geçmedi.
Cleanup kanıtının önceki failed-connect yanıtıyla kesilmesini önleyen yeni
`vpn-windows-broker/scripts/validate-full-broker-v3.ps1` eklendi; eski frozen runner'lar
korundu. SHA256 `7af38bbebb79a808629bdad5ea8c448ba23cfeb5396067806739c69c4ca40e25`.
PS5.1 `-SelfTest` implementer/bağımsız 6/6: exact Close makbuzuna kadar önceki hatalar
kaydedilir, asıl hata/ok=false korunur, timeout/başarısız close geçmez ve kapanırken challenge
yanıtı verilmez. Bu test UAC/ağ kullanmaz; yeni full paket için ayrıca pinlenir.
Sonraki full-v6'da özgün düşük token ile motor başlatma yolu çalıştı ve iki OpenVPN
oturumu `connected` oldu. Cisco/ocserv auth reddi nedeniyle WaitAllConnected tamamlanmadı;
pings=0. Rapor `.tools/vpn-windows/broker-full-validation-v6/full-broker-result.json`,
SHA256 `087055ba9fbb3716b679963cd81406674adfdef40bb2676e4cdc1d1cb940fb72`.
IP/route/DNS baseline aynı raporlansa da authenticated cleanup makbuzu başarısızdır
(`owned_adapter_still_present_cleanup_required`); cleanup ayrıca inceleniyor.
Auth reddi runner kaynaklıdır: nested profile.password bilerek boşken gerçek test parolası
kilitli fixture'ın üst alanındadır; eski driverless probe doğru alanı kullanıyordu.
Yeni `validate-full-broker-v4.ps1` yalnız explicit fixture username/password alanlarını üst
kimlik bilgilerine eşler; OTP/unknown alanları reddeder. SHA256
`e3b6f2a45e10ead573c652ef45b41004e19b18c4d334f82b93b089e271840b2a`, PS5.1
implementer/bağımsız selftest 10/10. Motor/private profil/frozen bundle değiştirilmedi.
Bu auth düzeltmesi tek başına üçlü kernel trafik kanıtı sayılmaz.
Son full-v7'de üç oturum auth/connected geçti ve OpenVPN .2/.3 hedeflerinde altı gerçek
Windows kernel ICMP payload kontrolü geçti. Cisco/ocserv .1 ilk `Ping.Send` çağrısı exception
verdi; test başarısız, tam üçlü trafik kanıtı yok. Authenticated cleanup bu kez doğrulandı:
0 protected journal, 0 session, broker/adaptör listesi boş, host baseline aynı.
Rapor `.tools/vpn-windows/broker-full-validation-v7/full-broker-result.json`, SHA256
`a37a8788e1b20164f5a14541eb7bc28be6fa15d57ef675abe536d778ce2cd560`.
Yeni diagnostic runner `validate-full-broker-v5.ps1`, SHA256
`4bed56f81c0173684a6238ba59a96c4e9524e58ebc9ab3fc47366697d7459236`, implementer/bağımsız
PS5.1 selftest 11/11. Per-ping inner NativeErrorCode/status ile sahipli adaptörlerin
AddressState/DadState, route/interface durumunu cleanup öncesi kaydeder; wait/retry eklemez.
Bu snapshot atomik değildir; sorgu gecikmesinde DAD ilerleyebilir. Readiness yarışı inceleniyor.
Pinned Wintun DLL'in Create/Close yolu da global orphan sweep kuyruğa alır. Native exact
recovery bu DLL'i yüklemez; yeni lifecycle deneyi öncesi ilgisiz orphan olmadığını salt-okunur
envanter kanıtlamalıdır. Bu deney koruması genel üretim sahipliği garantisi değildir.
Global sweep davranışı ve create→LUID kaydı arasındaki crash penceresi çözülmeden genel
Windows runtime hazır sayılamaz; yalnız normal/full testinin geçmesi readiness açmaya yetmez.
Salt-okunur kaynak incelemesinde signed DLL'i patch/extract/repack etme yolu benimsenmedi:
[resmi prebuilt lisansı](https://github.com/WireGuard/wintun/blob/0.14.1/prebuilt-binaries-license.txt)
ile [GPL kaynak lisansı](https://github.com/WireGuard/wintun/blob/0.14.1/COPYING) farklıdır.
Stock [API resource derlemesi](https://github.com/WireGuard/wintun/blob/0.14.1/api/resources.rc)
SYS/INF/CAT gömer; resmi imzalı driver'ın değiştirilmiş userspace DLL'e taşınabileceği izni
varsayılmadı. İleride ayrı GPL userspace API + önceden sağlanmış uyumlu resmi driver yolu
değerlendirilebilir; kaynak/lisans/build/hash paketi ve yeni makine driver sağlama planı gerekir.
Bu aday geliştirilmedi/derlenmedi; mevcut DLL pin'i ve driver değişmedi.

Gerçek native uygulama profil akışı 1/1 geçti: disposable şifreli kasa oluşturma, native `.ovpn`
import/list/save, görünür UI'dan iki profilin metadata düzenlemesi, config/parola korunması,
kasa kilidi ve yeniden açma; bağımsız tekrar da 1/1 geçti. Kanıt [gerçek uygulama ekranı](../artifacts/vpn-ui/native-profile-workflow.png)
ve `artifacts/vpn-ui/native-profile-workflow.json`; ekran 0 aktif bağlantı ve hazır olmayan runtime'ı
açıkça gösterir. Bu test mock IPC kullanmaz. Olağan debug EXE
`.tools/vpn-windows/desktop-dev/termterm.exe`; yalnız native test sürücülü debug kopya
`.tools/vpn-windows/desktop-e2e/termterm.exe`. Test sürücüsü normal `vpn:build` yolunda yoktur.
Son implementer native UI koşusu, yeni `desktop-preflight-e2e-v2` paketinde 1/1 geçti;
31 dosyanın salt-okunur envanteri tam, runtime kapalı, profiller refresh sonrası 379 ms'de
göründü; bağımsız aynı native tekrar 1/1 ve 530 ms geçti. İki sonuç
`artifacts/vpn-ui/native-preflight-v2-{implementer,verifier}.json` içinde korunur.
Önceki debug hash kontrolü 16.75 saniye ölçüldüğü için profil/session yükleme bu
kontrolden ayrıldı; test timeout'u artırılarak gizlenmedi. Yeni paketin manifest SHA256'i
`298942967ee2a4972ca28a715881ef38d54f3bb3ff5bfbfe5a2b0e54554616bd`.
Bu paket test sürücülüdür, dağıtım değildir; içindeki default helper son privileged düzeltmeden
önce derlendiği için hazır uygulama kanıtı sayılmaz. Güncel helper ile yeni paket gerekir.
Son normal `pnpm vpn:build` de geçti; `.tools/vpn-windows/desktop-dev/termterm.exe`
SHA256 `340bcb2164ef61755aab2c1431c6caab2d01861632e9793598dadb864944a9d6`,
ürün metadata'sı `TermTerm VPN Dev 0.4.3-dev.1`. Test sürücüsü feature'ı kapalıdır;
güncel helper henüz bu EXE yanına taşınmadı. Son native odaklı testler 4/4 geçti.
Stop baseline kapısı da geçti: CRLF gerçek whitespace hatası sayılmıyor, gerçek trailing-space,
blank-EOF ve space-before-tab hataları staged/unstaged testlerde reddediliyor (8 hook testi);
TypeScript ve 29 Vitest testi geçti. Per-command Git ayarı kullanıldı; repository/global ayar değişmedi.
Windows broker'ın sınırlı native/policy/pipe kontrolleri ayrı kanıttır; bunlar UAC/driver
yaşam döngüsü veya gerçek broker kernel trafiği kanıtı değildir. WSL fixture yeni
CA/profillerle tekrar başlatıldı; ready.json 14:26:01 UTC'de yazıldı. 35 kontrolün ardından hazır ve yalnız aktif test işi için
tutuluyor. Final host/source/namespace temizliği henüz bu koşuda yapılmadı. Eski fixture
hash'i yeni full-broker paketine taşınamaz; önceki kapalı koşunun cleanup kanıtı korunuyor.
Sabit normal v4 hash'leri ve kesin devam adımları [VPN_RESUME_TR.md](VPN_RESUME_TR.md) içindedir.

Devam sırası: (1) yeni fixture hash'leriyle loopback sunucularında tam broker/Wintun üçlü gerçek
kernel paket trafiği ve host snapshot geri yükleme; (2) genel sahiplik/global sweep ve
create-before-LUID engellerini çözme; (3) güncel default helper/motor/DLL'lerle
ayrı masaüstü EXE'sinin salt-okunur preflight paketini yeniden üretme; (4) yalnız doğrulanan geliştirme
kapsamının readiness kapısını bilinçli açma ve gerçek panelden connect/challenge/disconnect,
kasa kilidi/uygulama kapanışı temizliğini tekrar doğrulama. DNS/full-tunnel/net30/SSO ve gerçek
Fortinet/GlobalProtect sunucuları bu sıranın dışında ayrıca geliştirme/ortam doğrulaması gerektirir.

Önizlemeden ayrı gerçek `VpnWorkspace` paneli Tauri komutlarına bağlandı. Kişisel kasada
OpenVPN dosyasını native tarafta içe alma, profil oluşturma/düzenleme/silme, parola hatırlama,
oturum/challenge olayları ve en çok üç oturum vardır. Import sonucu yalnız tek kullanımlık
opaque token ve uyarıdır; anahtar/config/cookie webview'e gönderilmez. Profil metadata JSON'unda
`config` ve `password` anahtarları bulunmaz. OTP/challenge yanıtları kalıcı kayda alınmaz.
Panelin beş Playwright testi bağımsız tekrar dahil geçti; uzun salt-okunur bundle hash
kontrolü beklerken profiller görüntülenir/düzenlenir, Connect durum gelene kadar kapalıdır.
Windows status bu immutable kontrol sırasında profil/kasa operation gate'ini tutmaz;
connect yolunun gate ve yeni kontrolü korunur. Ayrıca metadata düzenlemenin saklı config/parolayı koruduğu ve
açıkça parola hatırlamayı kapatmanın parolayı sildiği de sınandı. Bunlar native ağ testi değildir.

VPN kayıtları diğer kayıtları listeleyen veya dışa aktaran API'den ayrı şifreli tabloda saklanır.
İlk VPN yazımı tablo ve kasa sürümü 2'yi atomik yazar; VPN eklenmemiş v1 kasa v1 kalır.
Tam şifreli yedek, SQL profili seçeneği kapalıyken de VPN verisini korur. Kopya, yeniden açma,
backup/restore ve eski kasa uyumluluğu native testlerde geçti. Kasa kilidi/değişiminde bağlantılar,
bekleyen import sırları ve oturum önbelleği kapatılır; eski kasa olayları yeni kasaya taşınmaz.
Bağımsız native `vault` kontrolü 7 test geçti, 1 `crash_child` yardımcı testi tek başına koşulmak
üzere ignore edildi; bağımsız Windows `vpn` kontrolleri 2 test geçti. Son Linux native koşuda
aynı iki teste ek olarak broker namespace eşleşme/reddetme kontrolü geçti (3/3).

Linux laboratuvar broker'ı gerçek UID/socket doğrulaması, sabit root-owned motorlar, bağlantı
nesli, ortak üçlü rezervasyon, süreç ve istemci ölümü temizliği ile çalışır. Tauri Linux istemcisi
broker'ın ağ namespace kimliğini karşılaştırır; başka namespace'den kullanımı reddeder. Üretim
namespace kilidi kaldırılmadı. Bağımsız tam koşu 35/35 geçti:
`artifacts/vpn-lab/desktop-broker-verifier-20261022.json`. Yanlış UID reddi, broker SIGKILL,
üç gerçek tünel/TCP-SHA256, ortadaki bağlantı kapanınca kalan trafik, kaynak hash'leri ve host
rota/DNS değişmemesi bu raporda yer alır. Sonraki bir koşuda TCP gönderici alt süreci exit1 verdi;
eski harness stderr'i saklamadığı için neden kesinleşmedi. Sınırlı stderr tanılaması eklendi ve
sonraki tam koşu geçti; tek geçiş bu aralıklı sorunun çözüldüğü iddiası değildir.

Windows tarafında gerçek MSVC OpenVPN Core ExternalTun ve OpenConnect motorları derlendi.
Motor yalnız broker'ın verdiği message-mode packet pipe'ına ham IP paketi taşır; adaptör/rota
API'si çağırmaz. OpenConnect 9.21'in normal Windows TUN yolu mevcut adaptörü benimseyebildiği
ve başka kapalı adaptörlerden adres silebildiği için kullanılmaz. Eşleşen LGPL kaynak patch'i
yalnız packet-handle API'si ekler; DLL bu export olmadan yüklenmeyi reddeder. Patch'li DLL SHA256:
`59d60129ea53ca903448a17d61a25034a38849ceb64c13d2733be1b647f363bf`.
Bağımsız 11/11 no-network ownership/API kontrolü ve gerçek ayrı süreç pipe testleri geçti.
Ardından gerçek Windows motorları iki OpenVPN ve bir AnyConnect/ocserv sunucusuna aynı anda
bağlandı. Her biri 128 × 1024 bayt ICMP payload taşıdı; IP/ICMP checksum ve her payload
karşılaştırıldı. Ortadaki OpenVPN kapatılınca kalan ikisi 32'şer paket daha taşıdı. Bu driverless
kontrol yalnız inherited packet pipe kullandı, Wintun/adapter/rota/DNS API'si çağırmadı.
Kanıt: `artifacts/vpn-windows/engines-real-packet-20261022.json`; motor, DLL ve kaynak hash'leri
rapordadır. Normal OpenConnect dotted-netmask rota çıktısını reddeden politika hatası bu gerçek
koşuda yakalandı; contiguous IPv4 maskesinin CIDR'a çevrilmesi ve bozuk maske reddi testleri eklendi.
Bu trafik Windows Wintun veya masaüstü broker bağlantısı kanıtı değildir; ocserv Fortinet/GP değildir.

Windows broker kodunda karşılıklı PID/süreç-doğum/token/logon doğrulaması, özel DACL,
machine lock, üç rezervasyon, sahipli adaptör/rota journal'ı ve watchdog vardır. Normal runtime
hazırlık kapısı henüz kapalıdır: panel açılışı UAC istemez ve hazırmış gibi bağlantı göstermez.
İmzalı resmi Wintun 0.14.1 yerel geliştirme bağımlılığı olarak doğrulandı. Yükseltilmiş gerçek
ağ nesnesi/çökme temizliği testi ayrıca incelenmektedir; sonucundan önce Windows desteği
tamamlandı sayılmaz. İlk kapsam DNS/proxy/exclusion/full-tunnel/net30 politikalarını açıkça
reddeder; yaygın sunucu yapılandırmalarının tümünü kapsamaz.

Windows hazırlık/source/hash/lisans kanıtı: `artifacts/vpn-windows/preparation-20261022.json` ve
`docs/VPN_WINDOWS_PREPARATION_TR.md`. Gerçek motor CMake projesi `vpn-engine/windows`;
bağımlılıkların mevcut kurulumu `.tools/vpn-windows/core-build/vcpkg_installed` altında yeniden
kullanıldı. Derleme çıktısı `artifacts/vpn-windows/engines-build.log`. Yeni driverless gerçek trafik
kontrolü `tests/vpn-engine-probe` içindedir; yalnız disposable WSL loopback fixture kullanır.

Aşağıdaki önceki bölümler tarihsel aşama kanıtlarıdır; güncel masaüstü veya Windows kapsamını
genişletmez. Fortinet ve GlobalProtect için gerçek üretici sunucu profili sağlanmadı.

## Son ekleme: gerçek eşzamanlı motor bağlantıları — 22 Ekim

**Masaüstü ürünü henüz gerçek VPN kullanımına hazır değildir.** Bu aşamada protokol motorları
ve gerçek ağ testleri geliştirildi; aşağıdaki kanıtlar Windows panelinden bağlantı anlamına gelmez.

- OpenVPN 3 de OpenConnect gibi önce ağ planını bildiriyor; onaydan önce TUN/rota oluşturulmuyor.
  OpenVPN Core'un otomatik Linux rota düzenlemesi yerine TunBuilder kullanılıyor. Sunucunun
  tam tünel, hariç rota, DNS, MTU ve adres bilgileri saklanıyor; desteklenmeyen politika gizlenmiyor.
- İki OpenVPN 3 ve bir OpenConnect/AnyConnect tüneli **aynı istemcide eşzamanlı** kuruldu.
  Her hedefin doğru arayüzü kullandığı, ICMP trafiği ve tünel başına eşzamanlı 1 MiB TCP aktarımının
  SHA256 bütünlüğü doğrulandı. Bu, Fortinet + GlobalProtect + OpenVPN üçlüsünün testi değildir.
- Paylaşılan Rust rezervasyon politikası bağlantı tamamlanmadan ağı ayırıyor. Çakışan dördüncü
  bağlantı ağ değişmeden reddedildi. Ortadaki tünelin kapatılması diğer ikisinin trafiğini kesmedi.
- Kontrol kanalının kapanması ve motorun çökmesinden sonra tünele ait rotalar temizlendi.
  TUN yalnız bu sürece ait ve kalıcı değil; mevcut bir arayüze bağlanmak yerine özel oluşturuluyor.
- VPN sunucusu kapatıldığında yalnız etkilenen bağlantı durdu; diğer tünelin trafiği sürdü.
  OpenVPN taşıma hatasında beş saniyelik otomatik yeniden denemeyi beklemiyor. Yeniden bağlantı
  için yeni oturum ve yeni ağ rezervasyonu gerekli; eski onay sessizce tekrar kullanılmıyor.
- OpenVPN şifreli özel anahtar parolası, statik OTP ve gerçek sunucunun istediği CRV1 dinamik OTP
  uygulandı. Doğru/yanlış parola veya OTP ve iptal yolları gerçek OpenVPN sunucusunda sınandı.
  Challenge kimliği karşılaştırmasında doğru yanıtı iptal sayan JSON sayı türü hatası düzeltildi.
  Dinamik oturum çerezi UI olaylarına gönderilmiyor; OTP kalıcı profil alanı değil.
- OpenConnect realm/grup seçimi ve izin verilen seçenek kontrolü eklendi. Kayıtlı grup ve
  bilinmeyen grupta açık seçim isteme, gerçek ocserv grubuyla doğrulandı. Derlenen kütüphanenin
  `fortinet` ve `gp` protokollerini içerdiği doğrulandı; bu ürünlerin sunucu testi hâlâ ortam bekliyor.
- Rapor kaynak/ikili SHA256 değerlerini ve test sırasında kaynağın değişmediğini içeriyor.
- Laboratuvarın anlık ürettiği sertifikalarda görülen “certificate not yet activated” hatası için
  test sertifikalarının başlangıcına beş dakikalık pay eklendi. Geçerlilik/CA kontrolü kapatılmadı;
  güvenilmeyen CA testi hâlâ reddediliyor. Bu yalnız geçici laboratuvar sertifikalarının üretimidir.

Kanıt: `artifacts/vpn-lab/concurrent-auth-20261022.json` ve aynı kaynakla tekrar koşusu
`artifacts/vpn-lab/concurrent-auth-repeat-20261022.json`: her ikisinde 28/28 geçti.
Dinamik OTP bir ara koşuda sunucudan AUTH_FAILED aldı; nedeni kesinleştirilemedi. Sır içermeyen
aşama tanılaması eklendikten sonra iki tam koşu geçti. Bu geçmiş bulgu gizlenmemeli;
gerçek üretici doğrulaması ve masaüstü bağlantı katmanı tamamlanmadan kararlılık garantisi verilmez.
Kaynak ve testler bu dalda;
laboratuvar parolaları, anahtarlar ve rapor/artifact dosyaları Git'e eklenmedi.

## Önceki adım: öncelikli üç VPN için arayüz — 12 Ekim

OpenVPN, Fortinet ve GlobalProtect önceliklendirildi. `src/VpnPreview.tsx` uygulamanın geliştirme
gezinmesine bağlanan etkileşimli **önizlemedir**. Profil türü, dosya adı seçimi/sürükleme, HTTPS
adres kontrolü, klasör/arama, taslak ekleme/düzenleme/silme ve birden fazla profili hazırlama vardır.
Dosya içeriği okunmaz; parola/OTP alınmaz; taslaklar yalnız bellekte yaşar. Bağlantı kontrolü
motorun hazır olmadığını gösterir; hiçbir profil bağlıymış gibi sunulmaz. Üretim menüsüne eklenmedi.

Windows Edge üzerinde 4 Playwright arayüz testi geçti; 1600×1000 ve 1366×768 kontrol edildi.
Görüntüler `artifacts/vpn-ui/` altında: genel ekran, OpenVPN dosya seçimi, Fortinet formu,
GlobalProtect formu ve üç profilli bağlantı hazırlığı. Bunlar native VPN bağlantı kanıtı değildir.
Rust çekirdeğinde üçüncü VPN'in iki mevcut VPN'in rotalarını değiştirmeden tümüyle karşılaştırılması
eklendi: o aşamada 7 test geçti. Bu bölüm tarihsel önizleme sonucudur; güncel sayılar aşağıdadır.

Önizleme: `pnpm dev --port 1427`, ardından `http://127.0.0.1:1427/?preview&previewVpn`.
Tekrar kontrol: `pnpm exec playwright test --config tests/ui/vpn.config.ts`.

## Tamamlanan bölüm

- Ubuntu 24.04 WSL üzerinde iki ayrı ağ namespace'i: VPN sunucusu ve istemci.
- Dış ağa varsayılan rota/NAT verilmeden, her çalıştırmada oluşturulan test sertifikaları ve kimlikleri.
- libopenconnect ile gerçek AnyConnect uyumlu ocserv bağlantısı ve tünel içinden trafik.
- OpenVPN 3 Core ile gerçek OpenVPN 2 sunucusuna sertifikalı bağlantı ve tünel içinden trafik.
- Ayrı parola/OTP formları; doğru ve yanlış OTP, yanlış parola, güvenilmeyen sertifika ve iptal.
- Motorların zorla öldürülmesinden sonra laboratuvar rotalarının temizliği.
- Motorların ana Linux ağ namespace'inde çalışmayı reddetmesi.
- On aileli profil veri modeli, parola hatırlama doğrulaması ve kalıcı OTP alanının reddi.
- OpenVPN/WireGuard import ön ayrıştırıcıları: kaynak değişmeden, komut/hook yürütmeden;
  OpenVPN'de göreli ek dosyaları birleştirme ve klasör dışına erişimi engelleme.
- IPv4/IPv6 rota ve DNS alan çakışmalarını reddeden saf politika testleri.
- Sabit Core kaynak/ruhsat hash'leri, lisans metinleri ve tekrar çalıştırılabilir laboratuvar hazırlığı.

İlk zorla kapatma testi, vpnc-script'in VPN sunucusuna eklediği rotayı geride bıraktığını yakaladı.
OpenConnect artık bu betiği çalıştırmıyor: tüneli açıp ağ planını özel IPC'den bildiriyor;
laboratuvar sürücüsü yalnız tünele bağlı test rotalarını kuruyor. Tünel dosya tanımlayıcısı
kapandığında bu rotalar da kalkıyor. **Bu, üretim için yetkili ağ servisinin tamamlandığı anlamına gelmez.**

## Son doğrulama

| Kontrol | Sonuç | Kanıt / sınır |
|---|---|---|
| VPN Rust çekirdeği | Windows ve Linux'ta 10'ar test geçti | `cargo test --locked --manifest-path vpn-core/Cargo.toml`; eşzamanlı rezervasyon, eski oturum kapanışı, DNS doğrulaması |
| Geliştirme kimliği ve kaynak sabitleme | 2 test geçti | `node --test scripts/test-vpn-isolation.mjs` |
| Gerçek laboratuvar | Aynı kaynakta iki kez 28/28 geçti | `artifacts/vpn-lab/concurrent-auth-20261022.json` ve `concurrent-auth-repeat-20261022.json`; protokol envanteri, kimlik doğrulama, üç tünel, TCP bütünlüğü, çakışma, realm ve temizlik |
| PostgreSQL kurulum kiti kontrolü | Önceki koşuda 1 test geçti | İlgili kaynak değişmedi; bu adımda tekrar çalıştırılmadı |
| Linux motor derlemesi | Geçti | CMake, kilitli Rust politika yardımcısı; upstream kaynaklar değiştirilmedi |
| Ana Linux rota/DNS değişmemesi | Geçti | Önce/sonra IPv4/IPv6 rota ve resolv.conf hash karşılaştırması |
| Geçici namespace/süreç temizliği | Geçti | Testin finally temizliği; testten sonra namespace listesi boş |

Trafik kontrolleri kısa ICMP ve eşzamanlı 1 MiB TCP/SHA256 testleridir. TermTerm SSH/SFTP
bütünlüğü, yüksek trafik, IPv6 tünel trafiği ve uyku/uyanma bu sonuçlardan çıkarılamaz.
DNS çakışma mantığı birim testidir;
gerçek DNS değişimi/geri yükleme laboratuvarda henüz uygulanmadı.

## Sıradaki işler — tamamlandı sayılmayacak

1. Linux/Windows yetkili ağ servisi: istemci doğrulama, sabit işlem listesi, hesap/oturum nesli,
   rota/DNS sahipliği, tam tünel, süreç ölümü ve yeniden başlatma temizliği. Laboratuvar kilidi
   bunlar doğrulanmadan kaldırılmayacak.
2. OpenConnect realm/grup değişimi, yeniden giriş ve sunucuya özel formların Fortinet/GlobalProtect testleri.
   OpenVPN statik/dinamik OTP ve şifreli anahtar testleri geçti; üretici OTP sağlayıcısı testi değildir.
   Ocserv testi Fortinet/GlobalProtect/F5 testi sayılmaz.
3. Windows OpenConnect/OpenVPN motorları ve imzalı sürücü gereksinimleri; WireGuard gerçek
   adaptörü; IKEv2, SonicWall ve Check Point için destek matrisi. Mevcut on aile yalnız veri modelidir.
4. VPN arayüz önizlemesini gerçek Tauri komutları/olaylarına bağlama; son kullanılanlar, şifreli
   profil/ek saklama ve import, hatırlanan kimlikler ve OTP ekranı. Önizleme kalıcı kayıt değildir.
5. Kişisel kasa/backup sürüm geçişi ve eski istemcinin yeni VPN verisini değiştirmeden reddi.
6. Team 065+ migration, ortak/kişisel kimlik ayrımı, yetkiler, geçmiş, çevrimdışı günlük,
   host/klasör VPN ilişkileri ve yetki iptali. Mevcut Team şeması değiştirilmedi.
7. SSH/chain/SFTP ön bağlantı bağımlılığı; aynı VPN işini paylaşma; VPN kopunca oturum/aktarım
   durdurma. Çoklu VPN'de çakışma ve DNS'in gerçek işletim sistemi doğrulaması.
8. Dağıtılacak her motorun kesin sürümü, eşleşen kaynakları, ikili hash'leri ve tüm bağımlılık
   lisansları; Windows/Linux geliştirme paketleri ve gerçek ekran görüntüleri.

Release/tag/main/güncelleme yayını yapılmadı. macOS/ücretli CI başlatılmadı.
Gerçek şirket VPN profilleri sağlanmadı; bu ürün/platform kombinasyonları ortam bekliyor.

## Laboratuvarı tekrar çalıştırma

Komutlar Ubuntu 24.04 WSL içinde, ayrı derleme dizininde çalışır. İlk hazırlık için bağımlılık
kurulumunu ayrıca seçin; mevcut kaynak veya paket hash'i uyuşmazsa araç durur, körlemesine güncellemez.

```bash
cd /mnt/c/Users/sonx/.codex/worktrees/vpn-workspace/termterm
sudo python3 scripts/prepare-vpn-lab.py --build-dir /root/termterm-vpn-build --install-dependencies
sudo python3 scripts/test-vpn-lab.py \
  --engines /root/termterm-vpn-build/engine \
  --ocserv-root /root/termterm-vpn-build/ocserv \
  --policy /root/termterm-vpn-build/policy-target/debug/vpn-policy-lab \
  --report artifacts/vpn-lab/concurrent-auth.json
```

Hazırlık için proje Rust araç zinciri de kurulu olmalıdır. Tekrar derlemede
`--install-dependencies` gerekmez. Betik ocserv paketini yalnız kendi dizinine
açar; ocserv sistem servisi kurmaz. Test kimlikleri/anahtarları geçici dizinden silinir, rapora
parola/OTP yazılmaz. OpenVPN 2 yalnız laboratuvar sunucusudur; ürün istemcisi Core 3 kullanır.

Windows çekirdek/config kontrolleri:

```powershell
& "$env:USERPROFILE/.cargo/bin/cargo.exe" test --locked --manifest-path vpn-core/Cargo.toml
node --test scripts/test-vpn-isolation.mjs
python tests/team_setup_package.py
```

Masaüstü geliştirme için yalnız `pnpm vpn:dev` / `pnpm vpn:build` kullanılacak. Debug EXE,
gerçek Windows ayrıcalıklı bağlantı kapısı doğrulanmadan çalışan VPN ürününün teslimi sayılmaz.

## Windows gerçek motor derlemesi ve driverless trafik tekrarı

Aşağıdaki komutlar mevcut sabit kaynaklar ve vcpkg önbelleğini kullanır; kurucu, driver yükleme
ve UAC çalıştırmaz. Önce `docs/VPN_WINDOWS_PREPARATION_TR.md` içindeki source/DLL hazırlığı
tamamlanmış olmalıdır. OpenConnect DLL'si ve MinGW bağımlılıkları helper ile aynı dizine konur;
Core OpenSSL/jsoncpp DLL'leri CMake Release dizinindedir. Sırlar `.lab` dışına kopyalanmaz.

```powershell
$root = (Get-Location).Path.Replace('\','/')
$cmake = 'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe'
$env:PKG_CONFIG_PATH = "$root/.tools/vpn-windows/core-build/vcpkg_installed/x64-windows/lib/pkgconfig;$root/.tools/vpn-windows/core-build/vcpkg_installed/x64-windows/share/pkgconfig"
& $cmake -S vpn-engine/windows -B .tools/vpn-windows/engine-build `
  -G 'Visual Studio 17 2022' -A x64 `
  "-DOPENVPN_CORE=$root/.tools/vpn-windows/openvpn3" `
  "-DOPENCONNECT_INCLUDE_DIR=$root/.tools/vpn-windows" `
  "-DCMAKE_TOOLCHAIN_FILE=$root/.tools/vpn-windows/vcpkg/scripts/buildsystems/vcpkg.cmake" `
  "-DVCPKG_INSTALLED_DIR=$root/.tools/vpn-windows/core-build/vcpkg_installed" `
  -DVCPKG_MANIFEST_MODE=OFF -DVCPKG_TARGET_TRIPLET=x64-windows `
  "-DPKG_CONFIG_EXECUTABLE=$root/.tools/vpn-windows/core-build/vcpkg_installed/x64-windows/tools/pkgconf/pkgconf.exe"
& $cmake --build .tools/vpn-windows/engine-build --config Release --parallel 2
& .tools/vpn-windows/engine-build/Release/termterm-packet-selftest.exe
```

WSL laboratuvar komutuna `--desktop-fixture .lab/vpn-desktop --desktop-duration 7200` eklemek,
testler geçince loopback TCP 31194/31195/31198/31443 yönlendirmelerini ve geçici profilleri tutar.
Dizin önceden boş ve yalnız geçerli Windows kullanıcı SID'sine açık ACL ile oluşturulmalıdır.
Bu bekleme sırasında ikinci laboratuvar koşusu başlatılmaz. Test bittiğinde `.lab/vpn-desktop/stop`
işaretini oluşturmak beklemeyi bitirir; betik süreçleri, namespace'leri ve profil/anahtar dosyalarını
temizler ve WSL host rota/DNS snapshot'ını karşılaştırır. Windows network snapshot'ı driverless
test tarafından alınmaz; Windows ağ nesnesi testi ayrı kanıt gerektirir.

```powershell
cargo run --locked --manifest-path tests/vpn-engine-probe/Cargo.toml -- `
  .lab/vpn-desktop/profiles.json .tools/vpn-windows/engine `
  artifacts/vpn-windows/engines-real-packet-20261022.json
```
