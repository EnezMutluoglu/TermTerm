# TermTerm 0.4.1 — kararlı paket doğrulaması

12 Ekim 2026 · şema64 / API1 / cache8. Kullanıcı 0.4.1 yayınını açıkça onayladı; Windows agent/Pageant, gerçek şirket OTP, macOS/FIDO2/Hello ortam sınırlamalarını kabul etti. 0.5 SQL Workspace çalışması korunur ve bu pakete dahil değildir.

Güncel kanıt: [0.4.1 doğrulama raporu](VALIDATION_0.4.1.md). Windows normal kimlikli 0.4.0→0.4.1 yükseltmesi, otomatik parola hatırlama, host/sır/ayar korunması, kurulu Team girişi ve Linux paket açılışları geçti. İki platform güncelleme imzası ve bozulmuş dosya reddi doğrulandı. Team servisinin systemd ve iki Compose kurulumu, HTTPS/PG, 63→64 yedekli yükseltme/kesinti/devam ve yönetici sürüm politikası geçti.

Windows Rust63, ayrıca kişisel PostgreSQL2, frontend26, Linux Team istemci33/servis6 ve kapsamlı gerçek HTTPS entegrasyonu bu kararlı hazırlıkta tekrar geçti. Önceki22 native,44+1 UI,2 Mosh kabulü aşağıda ayrı tarihsel kanıt olarak korunur.

## Tarihçe — aşağıdaki eski yayın kararları güncel durum değildir

# Kararlı yayın kontrolü — 12 Ekim 2026

Kullanıcının eksik yoksa yayın koşulu henüz karşılanmıyor. Ayrıntılı açık liste `TEAM_041_RELEASE_CHECKLIST.md` son bölümündedir. Mevcut 0.4.1-dev.5 beş paketinin kaynak/bütünlük kontrolü yeniden geçti. Bu kontrolde 26 frontend birim ve 33 Team istemci testi geçti. Önceki22/22 native, Windows/Linux kurulum ve gerçek PostgreSQL/TLS kanıtları aşağıda korunuyor; bu tur tekrar çalıştırılmış gibi sunulmuyor.

Kılavuz ve yayın dosyası denetimi güncellendi; uygulama/servis/migration kodu değişmedi. Eksikler: gerçek kararlı0.4.1 paketleri (RPM ve kaynak dahil), güncelleme imzaları ve kurulu kararlı sürümden yükseltme/kişisel veri korunması/güncelleme akışı doğrulaması. Windows agent/Pageant ve üretim OTP ortamları doğrulanmadı. Koşul sağlanmadığından yayın yapılmadı.

---

# Proxy hatırlama ve kalıcı chain geri yükleme — 12 Ekim 2026

**Güncel kaynak: 0.4.1-dev.5 · şema64 / API1 / cache8.** Geliştirme dalı korunur; Release/main/tag/güncelleme yayını yapılmadı. 0.5 kaynaklarına dokunulmadı. Aşağıdaki eski tarihli bölümler tarihçedir.

- Proxy kullanıcı adı/parolası kişisel kimlik şifrelemesi ve şirket kurtarma zarflarıyla saklanır. Ortak host içinde sır yoktur. Ayrı `user_proxy_profiles` tablosu takım/hesap/host ve özel kimlik sahipliğini birleşik foreign key ile bağlar. Normal Team listesi ve proxy durum IPC'si parola döndürmez.
- Kaydedilen sır tam proxy türü/adresi/portuna bağlıdır. Adres değişirse bağlantı kapalı kalır; kullanıcı yeniden kaydeder veya unutur. Kayıt SSH/SFTP/tünel/Mosh kontrol kanalı ve şifreli çevrimdışı bağlantı önbelleğinde kullanılır. Cache8 önceki veriyi koruyarak açılır; eski istemci yeni biçimi değiştiremez.
- Geçmiş hostun kalıcı geri yüklemesi chain/proxy ve içeriği güncel yetki önizlemesiyle birlikte uygular. Chain değişikliği yönetim yetkisi ister. Host ve chain tek transaction'da yazılır; eşzamanlı değişiklik reddedilir, tekrar gönderim ikinci revizyon yaratmaz. Kişisel kimlikler ve bağlı hop içerikleri kendiliğinden geçmişe dönmez.
- Uygulanmış001–063 değişmedi;064 yeni migration. Ana laboratuvar özel yedek, ayrı veritabanına restore, migration ve HTTPS/database-bound confirm akışıyla yükseltildi.

| Kontrol | Güncel kanıt |
|---|---|
| İstemci | Windows33/33 ve Linux33/33; şifreli proxy önbelleği yeniden açma, düz metin yokluğu, yetki iptali ve endpoint bağlama |
| Servis | Linux6/6; gerçek PostgreSQL18/TLS kapsamlı istemci testi geçti (320sn): proxy iki hesap izolasyonu, unutma/CAS, kalıcı chain, tekrar gönderim ve eski önizleme reddi dahil |
| Veritabanı/kurulum | Temiz64 kurulum, systemd/Caddy/CA/yeniden başlatma;63→64 özel yedek/restore, veri korunması, kesinti/devam, checksum ve downgrade reddi geçti |
| Frontend | TypeScript,25/25 birim ve44/44 Team/SFTP/terminal arayüz testi geçti |
| Windows Rust |63 geçti,8 açıkça ignored; çocuk süreç DPAPI ve dayanıklılık yardımcıları ayrıca çalıştırıldı. Eski kişisel PostgreSQL/signed-release ortam testleri bu koşuda çalıştırılmadı |
| Gerçek Windows proxy |5/5 native: HTTP/SOCKS5, kalıcı chain, çıkış/girişten sonra sıfır proxy sorusu, SSH çıktısı, SFTP bütünlüğü, yerel/uzak/dinamik tünel trafiği, unutma ve yanlış parola reddi |
| Gerçek Mosh |2/2 gerçek terminal panelinde HTTP/SOCKS5 SSH başlangıcı + WSL UDP trafiği; workspace başlangıç komutu, terminal boyutu, giriş/çıktı ve kapanış |
| Nihai birleşik native |22/22 geçti,367,5sn: hesap/hatırlama, izinler, iki proxy türü, MFA, kişisel profiller, çakışma/geçmiş, ortak kimlik, chain/SFTP/üç tünel, ikinci hesap ve birleşik yetki iptali, audit/sabitleme, kurtarma/döndürme, günlük, taşıma ve gerçek workspace |
| Güncel paketler |Linux deb/AppImage son kaynakta kurulum, gerçek pencere, test portu yokluğu ve kaldırma geçti. Paketlenmiş servis temiz systemd/Caddy/TLS/yeniden başlatma geçti. Compose ayrı PostgreSQL ve mevcut TLS PostgreSQL seçenekleri geçti. Windows NSIS sessiz kurulum, release binary eşitliği, paketli Mosh, gerçek pencere, test portu yokluğu, düzgün kapanış ve kaldırma geçti |

Birleşik native denemelerinde eski onayın Yenile sonrasında kalması düzeltildi ve UI regresyon testi geçti. Büyük senaryolar WDIO toplam süre sınırına takıldığı için işlem kontrolleri korunarak ayrı testlere bölündü. Bir sonraki tünel reddi laboratuvar sunucusundaki bir saatlik sertifikanın sona ermesiydi (`Certificate invalid: expired`); taze SSH fixture ile son22 testin tamamı geçti. Başka bir koşudaki Kimliklerim geçişi takılması tekrarlanmadı; ürün hatası düzeltildi sayılmadı.

İlk temiz kurulum denemesi eski servis binary'si nedeniyle başarısızdı; yeni binary ile geçti. İlk restore testi yardımcı yanıt alanlarını birebir karşılaştırıyordu; işlem kimliği/sürüm/sonuç kontrolüyle tekrar güvenliği doğrulandı. İlk Mosh testindeki yapay ConPTY cevaplayıcıyla resize sonrası ilk karakter kaybı gözlendi; gerçek terminal paneli kullanılan son iki test geçti. Aradaki UI hazırlığında yanlış kasa seçimi düzeltildi. Bu başarısız koşular geçmiş sayılmadı.

Windows agent/Pageant ve gerçek şirket OTP sağlayıcısı doğrulanmadı. FIDO2/Hello ve macOS kullanıcı isteğiyle sonraya bırakıldı. Mosh UDP erişimi ayrıca gerekir; SSH proxy UDP'yi taşımaz. Bu yerel geliştirme teslimi kararlı yayın değildir; Windows agent/Pageant ve üretim OTP sağlayıcısı doğrulaması ayrıca bekler.

---

# Team SSH kimlik doğrulama — 12 Ekim 2026

**0.4.1-dev.4 / şema63 / API1 / cache7 değişmedi.** Release/main/tag/feed ve 0.5 kaynakları değiştirilmedi.

- Kabul edilmiş anahtar/sertifika/agent ilk aşamasından sonra sunucunun istediği parola veya keyboard-interactive MFA aşamasına devam edilir. RSA imza çeşidi ilk aşama kabul edildikten sonra tekrar denenmez. Yanlış Team kimliğinde başka anahtara/parolaya sessiz geçiş yapılmaz.
- MFA en fazla 8 tur, tur başına 16 soru ve 16 KiB soru metniyle sınırlıdır. İptal ve yanlış cevap bağlantıyı durdurur. Tek kullanımlık cevaplar kimlik kaydına yazılmaz. Hop numarası soruda gösterilir.
- Agent içindeki normal anahtar ile sertifikalı kimlik farklı seçiciler kullanır; sertifika kendi imzalama yolundan gönderilir. Seçili anahtar yüklenmemişse açık hata döner. Agent listeleme ve imzalama beklemeleri sınırlıdır.
- Kayıtlı SSH parolası ve şifreli özel anahtarın parolası mevcut şifreli kimlik deposundan alınır; Team hesap parolasını hatırlama Windows DPAPI üzerinden devam eder.

| Son kaynakta kontrol | Sonuç |
|---|---|
| Windows Rust SSH uyumluluk | 13/13; RSA 2048/3072/4096, Ed25519, ECDSA, PEM/PKCS8/OpenSSH/PPK, yanlış parola, değişmiş sunucu anahtarı, sertifika imzası |
| Linux Rust SSH uyumluluk | 14/14; ayrı gerçek OpenSSH ssh-agent süreci ile RSA ve sertifika imzalayıp bağlantı dahil; özel anahtar uygulamaya aktarılmadı |
| Team kimlik sözleşmesi | 2/2; boş kimlikte fallback reddi, tam normal/sertifika agent seçicisi |
| Frontend | TypeScript, 25/25 birim testi, agent normal/sertifika seçimi ve erişilemeyen agent UI testi geçti |
| Windows DPAPI | Team girişini şifreli saklama, profil bağlama, unutma ve ayrı süreçten yeniden açma geçti |
| Windows native + HTTPS/PostgreSQL | 5/5, 110,5 saniye; lider, hatırlanan hesap, kaynaklar, özel kimlik ve gerçek SSH senaryoları |
| Normal Windows geliştirme derlemesi | Son kaynakta 79 saniyede geçti; E2E/WDIO bağımlılığı ve frontend test kancası yok. Kurucu veya Release üretilmedi. |

Native SSH senaryoları: kayıtlı parola, şifreli RSA, anahtar→parola, sertifika→iki MFA sorusu, anahtar→MFA, parola→MFA, doğrudan etkileşimli giriş, bağlantıda sorulan parola→MFA, yanlış anahtar/parola/kod, iptal, iki hopta ayrı MFA ve Team çıkış/girişinden sonra kayıtlı parolayla sıfır soru. Başarılı bağlantılarda gerçek SSH kanalındaki çıktı doğrulandı.

Bağımsız loopback sunucusu `tests/ssh_auth_lab.py`; test ortamı `.tools/auth-venv`, bağımlılıklar `tests/ssh-auth-requirements.txt`. Kullanıcı komutu çalıştırmaz, yalnız kendisine chain bağlantısını kabul eder ve test bitince kapanır. İlk iki native deneme Russh test sunucusunun partial-success bayrağını sıfırlaması nedeniyle durdu; hatalı fixture kaldırıldı. İstemci güvenlik kontrolü gevşetilmeden [Paramiko sunucu API'si](https://docs.paramiko.org/en/stable/api/server.html) ile bağımsız doğrulama geçti. Test CA/host anahtarı yalnız bellekte üretilir.

Sınırlar: MFA laboratuvar protokol sorularıdır; şirket OTP sağlayıcısı testi değildir. Windows OpenSSH agent hizmeti kapalıdır; Windows agent/Pageant doğrulandı sayılmadı. FIDO2/Hello ve macOS sonraya bırakılmıştır. Proxy parolasını kalıcı hatırlama, kalıcı tarihsel route geri yükleme, proxy/Mosh/uzak tünel birleşik doğrulaması ve nihai kurucular önceki açık işler olarak devam eder.

# Team chain / proxy — 12 Ekim 2026

**Geliştirme sürümü 0.4.1-dev.4 · şema63 / API1 / cache7.** Release/main/tag/feed ve 0.5 kaynakları değiştirilmedi. Aşağıdaki eski tarihli bölümler tarihçedir.

- Makine ekleme/düzenlemeye HTTP CONNECT ve SOCKS5 proxy alanları eklendi. Proxy adresi/portu/kimlik doğrulama tercihi şifreli host içeriğinde; parola ortak host kaydında tutulmaz. Her hop mevcut kişisel SSH kimliğini kullanır. SSH/SFTP/tünel/Mosh kontrol kanalı aynı native bağlantı planından geçer.
- 063 migration: host revizyonuna bağlı ayrı route başlık/adım tabloları, FK/RLS/indeksler; chain değişikliği ve yeni şifreli host revizyonu tek transaction. Aynı plan/işlem tekrarı ikinci sürüm oluşturmaz. Uygulanmış001–062 değiştirilmedi.
- Geçici eski sürüm seçimi, o revizyonda kaydedilen chain ve proxy'yi kullanır. Hop içerikleri ve kimlikler güncel kalır. Güncel izinler yeniden denetlenir; eski hop güncel chain'den çıkarılmışsa güncel bağımsız bağlantı izni gerekir. Silinmiş/izinsiz bağımlılıkta güncel chain'e sessiz geçiş yoktur. Yükseltmeden önce tutulmayan route geçmişi uydurulmaz; açık hata verir.
- Geçmiş seçimi değiştirilirken eski çevrimdışı bağlantı planları temizlenir; farklı seçimin rotasına sessiz dönüş engellenir. Yeni seçim için ilk bağlantı çevrimiçi doğrulanmalıdır.
- Proxy metadata doğrulaması, şifreli çevrimdışı route kaydı ve endpoint değişiminde cache geçersizleşmesi test edildi. Cache7 eski uygulamalar tarafından değiştirilmeden reddedilir; kişisel kasa biçimi değişmedi.

Doğrulama:

- Windows31 ve Linux31 istemci; Linux6 servis birim testi geçti. TypeScript derleme kontrolü geçti.
- Gerçek TLS PostgreSQL18/HTTPS kapsamlı Rust istemci testi geçti; yeni chain sürümü, tekrar güvenliği, eski doğrudan rota ve güncel üç adımlı rota seçimleri dahil (son koşu213,91s).
- Gerçek Windows native4/4: lider, kasa/klasör/host, özel kimlik/kurtarma ve authenticated HTTP/SOCKS5 + geçmiş chain. Her iki proxy ile gerçek SSH çıktısı,4096bayt SFTP yükle/indir eşitliği, yerel ve dinamik tünelden HTTP trafik ve yanlış proxy parolasında bağlantı reddi geçti (70,6s). İlk koşu başarılı reddin ardından kapanmış oturuma tekrar close gönderen test temizliğinde durdu; temizlik düzeltildi.
- Gerçek62→63 plan/apply/confirm: özel yedek, ayrı boş/private DB'ye geri açma, şifreli kayıt/ilişki eşitliği, kesinti/devam, tekrar, checksum/downgrade reddi ve HTTPS/CA/DB kimliği doğrulaması geçti. İlk denemede sentetik eski kaydın olmayan sürümüne backfill FK hatası çıktı; uygulanmamış063 yalnız mevcut sürümlerden backfill yapacak şekilde düzeltildi; başarısız plan/yedek korunup yeni planla yükseltildi.
- Temiz63kurulum, sınırlı PostgreSQL rolü, systemd+Caddy/CA ve restart geçti.
- Son kaynakla normal Windows geliştirme derlemesi geçti; E2E/test sürücüsü bağımlılığı ve frontend kancası yok. Yerel kurucu/Release üretilmedi.
- Son form düzenlemesi sonrası iki native ekran testi ayrıca geçti (24,5s); ekleme/düzenleme görüntüleri yeniden alınıp görsel olarak kontrol edildi.
- Gerçek masaüstü ekranları: artifacts/team-v2/windows-host-create.png ve windows-host-edit.png (özel veri içermeyen laboratuvar).

Sınırlar: proxy parolası şu aşamada her yeni bağlantıda sorulur; kalıcı kişisel proxy kimliği seçimi eklenmedi. Tarihsel route desteği **geçici kullan** yolundadır; kalıcı host içerik geri yüklemesi chain yapılandırmasını ayrıca değiştirmez. Bu turda proxy üzerinden Mosh/UDP ve uzak tünel gerçek trafiği denenmedi; ortak taşıma kodunu kullanmaları test kanıtının yerine geçmez. Önceki birleşik native sürücü kopması ve nihai Windows/Linux kurucu paketleri yayın listesinde açık kalır. FIDO2/Hello ve macOS kullanıcı tercihiyle bekliyor.

# Yayın hazırlığı ve kayıtlı Team girişi — 11 Ekim 2026

**Henüz kararlı yayına hazır değil.** 0.4.1-dev.3; şema62/API1/cache6. Kullanıcı FIDO2 ve Windows Hello çalışmalarını sonraya bıraktı; bunlar artık bu sürümün yayın engeli değil ve test edilmiş sayılmıyor. Öncelik, Team servis/takım/kullanıcı bilgilerinin ve isteğe bağlı parolanın hatırlanması.

- Team girişine **Beni hatırla**, **Kayıtlı hesabımla devam et** ve **Bu cihazda beni unut** eklendi. Başarılı, etkinleştirilmiş hesap girişi sonrası bağlantı alanları saklanır. Parola Windows DPAPI / Linux Secret Service / macOS Keychain mevcut native yolu üzerinden saklanır; frontend'e geri verilmez. Geçici aktivasyon parolası hatırlanmaz. Güvenli depo hatasında giriş başarılı kalır, kayıt hatası açıkça gösterilir; düz metin parola yedeği yoktur. Önbellek/günlük unutma işlemiyle silinmez.
- Kayıtlı sır servis+takım+kullanıcı+CA bilgisine bağlanır; bağlantı alanlarının değiştirilmesi eski sırrın otomatik kullanılmasını engeller. Güvenli depo yazımı atomik geçici dosya ile yapılıyor. `.ttsettings` yerel metadata dosyası Git/Syncthing/teslimden dışlanır.
- Gecikmiş eski takım yanıtının yeni takımı engellemesi düzeltildi. Global güncelleme hataları yalnız güncel profile yeniden kontrol tetikler. Native kontrol eski oturum neslini kapatamaz. Güncel uyumlu politika giriş ekranına döndürür; kapanmış workspace/SSH kendiliğinden canlanmaz. Ağ/TLS hatası bilinen zorunluluğu kaldırmaz.
- Kullanılmayan14import ve Editor.onConnect zinciri temizlendi; ek sıkı TypeScript kullanılmayan tanım kontrolü geçti. Vite bütün Rust target klasörlerini dışlar. Servis politika kontrolü ve işlem sorgusu istek kapsamındaki tek DB bağlantısını kullanır; rol/şema/migration kilidi kontrolleri korunur.
- Ana disposable WSL laboratuvarı61→62 yükseltildi. Eski61kaynağıyla açık checksum benimsemesi öncesinde özel yedek alındı; standart plan/apply ayrı private DB'ye restore doğrulaması yaptı; HTTPS/CA ve DB kimliğiyle confirm tamamlandı. Özel kanıt `/var/lib/termterm-team-lab-upgrade-62`; Git/teslime girmez.

Doğrulama: Windows+Linux30istemci/6servis testi geçti; TypeScript25test geçti. Ortak Team/terminal37 ve ayrı SFTP4 Playwright geçti. Windows DPAPI Team saklama/bağlama/unutma ve ayrı alt süreçte yeniden açma geçti. Gerçek Windows/HTTPS/PostgreSQL native koşusunda lider aktivasyonu, kayıtlı parolayla tekrar giriş ve frontend'e sır dönmediği doğrulandı. İlk birleşik koşu eski native_test_writer yardımcı binary'sinde durdu; güncel kaynakla yeniden derlendi ve test hazırlığına zorunlu yardımcı derlemesi eklendi. İkinci koşuda 15 test geçti: birleşik iki hesap/farklı RSA/chain/SFTP/üç tünel/yetki iptali dahil. Taşıma önizlemesindeki bekleme nedeniyle bütün koşu geçmedi; hata tanısı ve sayfalı erişim önizlemesi test adımı eklendi. Sonraki koşu 9 testten sonra İngilizce/Türkçe seçici farkında durdu; Team native yapılandırması Türkçe arayüzü açıkça seçiyor. Son birleşik koşuda SSH/SFTP/tünel/izin iptali tekrar geçti; yönetim/audit adımında native sürücü ECONNREFUSED ile bağlantısını kaybetti. Neden doğrulanmadığından bütünleşik kabul açık kalır. Ardından ayrı kısa native koşu 47,5 saniyede 7/7 geçti: lider, kasa/host, özel kimlik, kurtarma anahtarı değişimi, ortak kurtarma, taşıma ve gerçek SSH/workspace. Taşıma ürün kodunda değişiklik yapılmadı; ayrı koşunun geçmesi ilk bütünleşik beklemenin nedenini kanıtlamaz.

Dal incelemesi (11 Ekim 2026): origin güncellendi. develop/main/v0.4.0 aynı5965237commit'inde. Eski terminal/kaynak-istatistik/Team0.4.0 dalları develop geçmişine zaten dahil; yeni commit birleştirmesi yok. Team0.4.1 ve Desktop0.5 iş klasörlerinde kaydedilmemiş kaynak değişiklikleri mevcut; hazır diye merge edilmedi, silinmedi. 0.5 kaynaklarına dokunulmadı.

| Dal | İnceleme sonucu | İşlem |
|---|---|---|
| `codex/team-passwords-040` | `develop` ile aynı commit | Merge gerekmiyor |
| `codex/feature-network-disk-stats`, `codex/fix-terminal-resize-036` | Commitler develop geçmişinde | Merge gerekmiyor |
| `feature/terminal-usability`, `fix/folder-host-clicks`, `fix/remember-vault-password`, `feature/team-access-history` | Commitler develop geçmişinde | Merge gerekmiyor |
| `codex/feature-team-personal-auth` | Team 0.4.1 değişiklikleri ayrı worktree'de; henüz commit/teslim tamamlanmadı | Ayrı korunuyor |
| `codex/feature-database-workspace` | 0.5 SQL Workspace değişiklikleri Desktop klasöründe | Ayrı korunuyor |

Eski dalın görünür olması kodunun eksik kaldığını göstermiyor. Hiçbir dal silinmedi, main değiştirilmedi.


**Açık:** tarihsel chain/Team proxy, birleşik native kabul, kalan gerçek agent/MFA kanıtı ve güncel Windows/Linux/servis kurucu paketleri. Eski0.4.0aktarımı kapsam dışı. Paketler halen eski; yayın/main/tag/feed değişmedi. Normal Windows geliştirme derlemesi 55,97 saniyede geçti. `src-tauri/target/debug/termterm.exe` E2E/WDIO içermeyen normal binary'dir; normal bağımlılık ağacı ve frontend test kancası taraması geçti. Son strict TypeScript ve diff kontrolü geçti. Kişisel kasa hatırlama regresyonunda 3 test ve ayrı alt süreçte parola açma geçti; bozuk saklı sır kasayı değiştirmiyor. Bu turdaki HTTPS/SSH test süreçleri kapatıldı; kurucu teslimi henüz hazırlanmadı.

---

# Team yöneticisinin seçtiği zorunlu güncelleme — 8 Ekim 2026

**Güncel kapsam:** Kullanıcının talimatıyla kullanılmamış v0.4.0 Team aktarımı tamamlanacak işler listesinden çıkarıldı. Gelecekteki v2 yerinde yükseltme devam ediyor. Sürüm **0.4.1-dev.3**, servis şeması **62**, API **1**, şifreli cache **6**. Yayın/main/tag/feed/macOS/ücretli CI yok.

- Team → **Sürüm politikası**: yalnız takım sahibi, servis paketinin desteklenen istemci listesinden sürüm seçer; etki önizlemesi ve ayrı onay gerekir. Üye veya başka takım kimliği politika değiştiremez. Kişisel kullanımda güncelleme isteğe bağlıdır.
- `062_client_update_policy.sql`: takım başına güncel karar ve sınırlı karar geçmişi ayrı ilişkisel tablolarda; PK/FK/RLS, açık fonksiyon izinleri, beklenen revizyon, tekrar güvenli karar, mevcut audit kaydı. Uygulanmış 001–061 dosyaları değişmedi.
- Servis API çağrılarında takımı sunucudaki erişim/yenileme/aday teslimi token hash'inden bulur. Teknik API/istemci sınırı ile takım sahibinin seçimi birlikte uygulanır; login ve refresh de denetlenir. Uyumsuz istemci 426 alır; yetki/ağ hatası gibi çevrimdışı kabul edilmez. Logout kullanılabilir.
- Masaüstü Team açılışı/profil seçimi, giriş, odak/yeniden bağlantı ve 30 saniyelik denetimde uyarı alır. Native oturum kapanışı SSH/SFTP/tünel izleyicilerini uyandırır. Kişisel kasa açık kalabilir; kayıt ve bekleyen şifreli günlük silinmez. Öğrenilen zorunluluk servis + takım bazında ayrı SQLite sürüm bilgisinde saklanır. Gecikmiş düşük revizyon yeni kararı geri alamaz.
- Güncelleme mevcut sahibin onayladığı kaynak/imza yolunu kullanır. Team servisinin keyfi URL'sinden kurucu alınmaz; yetersiz sürüm kurulmaz. Geliştirme sürümü stable feed açmaz; deb/rpm paket yöneticisiyle yükseltilir. Uygun paket yoksa Team kilitli kalıp açıklama gösterir.
- Tamamen çevrimdışı bilgisayar yeni sunucu kararını ancak tekrar bağlanınca öğrenebilir. Bu bir uzaktan sürekli bildirim servisi değildir; bağlı uygulamada en geç bir sonraki 30 saniyelik kontrol/istek ile görünür.
- Eski geçiş arayüzü normal Team menüsünden kaldırıldı. Mevcut salt okunur arşiv/önizleme kaynakları ve bağımsız testleri korunur; tam aktarım uygulanmış sayılmaz ve artık yayın engeli değildir.

Doğrulama:

- Windows ve Linux: **6 servis + 29 istemci Rust testi**; sürüm/API reddi, semantik sürüm karşılaştırması, disk/oturum/günlük sınırları. Son ilave test, zorunluluk bilinirken internetsiz yeniden girişin reddi ve eski yanıtla yeni politikanın düşürülememesini doğrular.
- **25 TypeScript birim testi**; **36 ortak Team/terminal/SFTP arayüz testi** tek son koşuda geçti. Ek yönetici onay/başarı testi ayrı koşuda geçti: toplam **37** senaryo. Önceki iki SFTP koşusunda bir tıklama ve bir sayfa yükleme zaman aşımı oldu; son ortak koşu geçti. Üründe SFTP değişikliğiyle düzelmiş gibi sunulmaz; test ortamı gecikmesi notu korunur.
- Gerçek PostgreSQL18/TLS + HTTPS/Caddy üzerinde **61→62**: özel yedek/ayrı DB'ye geri açma, şifreli satır/ilişki parmak izi eşitliği, tekrar, yanlış hedef, başarısız geri açmada kaynağın korunması, kesinti/devam, checksum ve downgrade reddi geçti.
- Aynı gerçek HTTPS testinde yönetici seçimi, üye reddi, eski revizyon reddi, idempotent karar ve tek audit, iki takım izolasyonu, eski istemcinin login/canlı istekte reddi, uyumlu istemcinin devam etmesi geçti. Gelecekteki 0.4.2 kararı yalnız izole test DB'sinde canlandırıldı; böyle bir Release yayımlanmadı.
- **Temiz 62 migration kurulumu**, kısıtlı DB rolü, systemd/Caddy/CA, bootstrap, checksum, yeni şema reddi ve servis yeniden başlatma geçti. İlk kurulum testindeki sürüm başlığı olmayan isteğin eski 401 beklentisi, yeni 426 sözleşmesine uygun sürüm başlığı gönderilerek düzeltildi.
- Yönetim ve zorunlu güncelleme görüntüleri `artifacts/team-v2/team-update-policy-browser.png` ve `mandatory-update-browser.png`: gerçek React arayüzünün **sentetik test görünümü**, native/üretim bağlantı kanıtı değildir.

Son normal Windows geliştirme derlemesi **1 dakika 19 saniyede** tamamlandı; final Linux servis/istemci testi de geçti. `src-tauri/target/debug/termterm.exe` normal geliştirme binary’sidir; E2E/WDIO içermez. Kurucular yenilenmedi. Test süreçleri kapandı. İlk kurulum laboratuvar betiği artık mevcut uyumsuz şemayı yedeksiz yükseltmeyi reddeder.

Yayın sınırları: Tarihsel chain/Team proxy, önceki birleşik SFTP yetki iptali gecikmesi, gerekli gerçek donanım/ileri kimlik doğrulama ve son Windows/Linux/servis teslim paketleri açık kalır. Eski kurucular güncel değildir; bu tur kurucu/Release hazırlanmadı. Ana eski disposable e2e laboratuvarı şema61 olarak korunmuştur; yeni şema62 testleri ayrı DB'lerde yapıldı. Ana laboratuvara yeni servis başlatmadan açık yükseltme akışı uygulanmalıdır; eski `scripts/team-v2-lab.py` ile mevcut DB'yi yedeksiz güncellemeyin.

Özel yükseltme kanıtı: `/var/lib/termterm-team-upgrade-tests/4aee765d06a4477293fe4cadc94d43cf`. İçinde sır/yedek olabilir; Git veya teslim paketine girmez.

---

# Team günlük kullanım denetimi — 8 Ekim 2026

**0.4.1-dev.3; kararlı yayına hazır değil.** Son kullanıcı mesajındaki yayın izni eksik kalmaması koşuluna bağlı. Tam v0.4.0 aktarımı, eski geçmiş/audit, tarihsel chain/Team proxy ve nihai paket kabulü açık. Üstelik birleşik Windows kabul koşusunda aşağıdaki SFTP iptal kontrolü henüz kapanmadı. Ayrı testin geçmesi bu koşuyu geçmiş saydırmaz. Güncel engeller [yayın kontrol listesinde](TEAM_041_RELEASE_CHECKLIST.md).

Bu turdaki değişiklikler:

- Kimliklerim normal kayıt kartı/liste görünümüne geçti: ad arama, özel/ortak filtresi, tek tıklamada yalnız metadata/izin bilgileri, sağ tık ve Shift+F10 ile işlemler. Kart seçmek sır görüntüleme/export çağrısı yapmaz. Menü, güncel izinleri kullanır; sunucu denetimleri korunur.
- Ortak kimliği şirket dosyasıyla kurtarma **Team yönetimine** taşındı; alıcı önizlemesi ve yeniden doğrulama korunur. Günlük host/klasör/üst terminal/SFTP düzeni değişmedi.
- Team terminal girişi kişisel terminaldeki sıralı, sınırlı, Unicode koruyan InputQueue yoluna alındı. Kapatma, kilitleme ve sunucu kapanışında bekleyen giriş temizlenir. Toplu giriş yalnız bağlı oturumlara gider; iki oturumun altına düşünce kapanır. Ctrl+Shift+B, Ctrl+Shift+L ve üst sekme sağ tık işlemleri bağlandı. Bölünmüş görünüm ve toplu giriş pane araç çubuğundadır.
- Team SFTP kapanışında backend `sftp-closed` bildirimi üretir. Dosya yöneticisi eşleşen eski liste/seçim/işlem panelini temizler, geç klasör yanıtını reddeder, ilgili bekleyen aktarımı iptal eder. Eski bir bağlantının gecikmiş bildirimi yeni bağlantıyı etkilemez.
- Kimlik ekranı dışında Yenile işleminin gereksiz credentials sorgusu düzeltildi. Kasa ve DB biçimi değişmedi: şema61/API1/cache6.

Kanıt ve sınırlar:

- TypeScript derlemesi ve 25 birim testi geçti. Team/terminal/SFTP ortak UI koşusunda 32 test geçti; yeni SFTP kapanış testi ilk koşuda aynı hostun hem Son kullanılanlar hem klasörde görünmesi nedeniyle test seçicisine takıldı. Seçici klasör bölgesine daraltıldı; son SFTP 4/4 koşusu geçti. Aynı kaynakta toplam33 arayüz senaryosu doğrulandı. Kişisel terminal regresyonundaki10 test de geçti.
- Team pano testinin ilk çok hızlı tuş girdisi xterm'in asenkron mod ayrıştırmasını beklemiyordu. Test iki render karesi bekleyecek şekilde düzeltildi; bracketed-paste ve kapatmada bekleyen giriş temizliği geçti.
- Windows native sürücüsü sağ tık `contextmenu` olayını üretmiyor. İlk bütünleşik koşudaki buna bağlı hatalarda test yardımcıları düzeltildi; gerçek tarayıcı sağ tık/Shift+F10 Playwright ile ayrıca geçti. Native koşu gerçek Tauri/HTTPS/PostgreSQL kullanır; fixture olaylarının gerçek fare sayılmadığı korunur.
- İkinci birleşik native koşunun büyük SSH senaryosu iki dakikalık test sınırında sürücü bağlantısını kaybetti. Yalnız bu senaryonun bütçesi240 saniye yapıldı; üçüncü koşuda ilk8 hesap/izin/kimlik/geçmiş testi geçti. Gerçek farklı RSA anahtarları, chain, SFTP bütünlük, üç tünel ve ikinci üye bağlantısı aşamalarından sonra **mevcut SFTP'nin izin iptalinde kapanma kontrolü15 saniyede doğrulanamadı**. Bu birleşik senaryo başarısızdır; kesin nedeni henüz doğrulanmadı.
- Aynı laboratuvar hesabı üzerinde ayrı testte yalnız SFTP iptali10,019 saniyede; SSH+SFTP+tünel eşzamanlı iptali10,026 saniyede kapandı. Son kapanış bildirimi eklenmiş kaynakla üç bağlantı ve `sftp-closed` olayı10,037 saniyede, test toplam22,4 saniyede geçti. Bu odaklı test, önceden hazırlanmış disposable native Team hesabını kullanır; bağımsız kurulum kabulü değildir.
- Ekran görüntüsü `artifacts/team-v2/windows-identities.png` gerçek Windows uygulamasındandır. `identities-browser.png` sentetik UI fixture görünümüdür; SSH kanıtı olarak sunulmaz.

Eski Desktop/artifacts kurucuları hâlâ şema60 kaynaklarını içeriyor; güncel diye sunulmaz. Bu tur Release/main/tag/latest.json değiştirilmedi; macOS/ücretli CI çalıştırılmadı. Son odaklı native gösterimde6/6 test56,9 saniyede geçti: lider girişi, normal host/klasör gezinmesi, özel kimliğin sağ tıkla varsayılan seçimi, Team yönetiminden ortak kurtarma, klasör taşıma ve iki gerçek SSH panelinde workspace başlangıç komutu. İlk iki6'lı koşunun workspace testindeki önceki testten kalan profil revizyonu/klasör bağlamı düzeltildi; ürün yetki denetimi gevşetilmedi. Güncel `windows-documents.png` gerçek uygulamada incelendi: iki üst sekme, tam yüksekliği kullanan iki panel, başlangıç çıktıları ve altındaki ortak araç çubuğu doğrulandı. Son normal Windows geliştirme derlemesi43,11 saniyede tamamlandı. `src-tauri/target/debug/termterm.exe` artık E2E sürümü değildir; normal bağımlılıklarda WDIO ve frontend paketinde test hook bulunmadığı kontrol edildi. Kurucu/ZIP/Linux/servis teslim paketleri bu tur yenilenmedi. Teste ait TLS ve SSH süreçleri durduruldu.

---

# Yayın hazırlığı — hesap eşleme ve bağlantı düzeltmeleri (8 Ekim 2026)

**Kararlı 0.4.1 henüz hazır değil.** Güncel açık iş listesi artık [TEAM_041_RELEASE_CHECKLIST.md](TEAM_041_RELEASE_CHECKLIST.md) içinde tek tabloda tutuluyor. Aşağıdaki eski bölümlerdeki açık/kapalı durumlar tarihçedir; bu bölüm günceldir. Şema 61/API 1/cache 6 değişmedi. Yayın/main/tag/feed yok; 0.5 kaynakları korunuyor.

Bu turdaki kod değişiklikleri:

- v0.4.0 önizlemesine kaynak → hedef hesap seçimi eklendi. Aynı kullanıcı adına otomatik eşleme yok; birebir hesap eşlemesi zorunlu, iki kaynak hesap tek hedefte birleştirilemez. Başka takımda/olmayan hesap, rol farkı, kapalı hesabın etkinleştirilmesi ve ilk girişini tamamlamamış hedef engel olarak görünür.
- Kural/engel kayıtlarında hedef hesap kimliği de gösterime hazırlanır. Parola/anahtar/token alanları hedef üye görünümüne alınmaz. Kaynak hash'i ve canlı hedef üyeler/ACL revizyonu eşleşmeden kontrol geçmez; işlem boyunca oturum değişirse sonuç reddedilir. Eşleme için arşiv yeniden açılır; işlem sonunda parola silinir. Kaynak hesaplar sayfalanır, hedef araması ilk100+seçili hesap ile sınırlıdır.
- **Hesap eşlemesinin başarılı olması tam aktarım değildir.** Henüz kayıt yazılmaz; `canApply=false`. Alıcı kimlik sürümleri/anahtarları uygulama aşamasında yeniden doğrulanmalı; atomik hedefe uygulama, geçmiş/audit, ilişki/ACL eşitliği ve geri dönüş açıktır.
- Çevrimiçiyken dolmuş veya saat geri alınmasıyla kapanmış yerel düzenleme yetkisi, sunucu yetki kontrolüyle yenileniyor. Yerel taslak, generation ve eski server revision korunuyor; eşzamanlı değişiklik çatışma üretmeye devam ediyor. Ağ kesintisinde süre yenilenmiyor; açık izin reddinde önbellek iptal ediliyor. Başka kaydın 24 saatlik yetkisi kendiliğinden yenilenmiyor.
- **Mosh workspace gerçek hatası düzeltildi:** başlangıç komutu UDP adresi parametresine yanlış gönderiliyordu. Yeni `MoshOptions` adlandırılmış alanları adres/komutu ayırıyor. Doğrulanmış SSH bağlantısında `mosh-server -- /bin/sh -c` ile yetkilendirilmiş başlangıç komutu çalışıyor, ardından kullanıcı shell'i açılıyor. Boş komutta önceki sabit bootstrap korunur; NUL/16KiB sınırı ve shell argüman kaçışlaması var. Kişisel Mosh başlangıç komutu aynı yolu kullanıyor.
- Teslim doğrulaması artık SOURCE_FINGERPRINT dosyasını güncel kaynakla karşılaştırıyor; uyuşmayan eski paketlere başarılı yeni teslim manifesti yazmıyor. Servis paketinin compatibility sözleşmesini karşılaştırıyor; .ttmigration/.ttlogin/.dump dosyalarını da özel veri olarak reddediyor.

Test kanıtı:

- Windows Rust son toplu koşu: **60 geçti, 7 atlandı, 11,43 sn**. Atlananlar harici eski DB/imzalı paket ortamları ve alt süreç yardımcıları; bazı yardımcılar ana dayanıklılık testince ayrıca çalışır. Geçiş eşleme dahil odaklı5 test de geçti.
- Linux Rust: **4 servis + 26 istemci birim testi geçti**. Yeni test süre/geri saat/yeniden yetki, taslak/base revision ve diğer kaydın süresinin korunmasını doğrular.
- Gerçek HTTPS/PostgreSQL/Rust istemci: **1 geniş senaryo geçti, 144,37 sn**. Test saati yerel günlükte ileri alınıp eski saate dönüş zorlandı; çevrimiçi yenileme ardından iki istemci değişikliği gerçek çatışmaya gitti. Önceki turdaki aralıklı hatanın kesin kökeni kanıtlanmadı; bu davranış açığı deterministik senaryoyla giderildi. İptal testinin beklenen TLS EOF fixture çıktısı başarısızlık değildir.
- Team+SFTP UI: **28/28, 57,9 sn**. Eşlemenin arşiv/hedef görünümüne bağlanması, yanlış parola, hedef değişince ret, parola temizliği ve hiçbir kayıt/yetki yazılmaması dahil. Bunlar sentetik UI testidir.
- Gerçek Windows native ilk lider girişi ve host/klasör işlemi: **2/2, 27,9 sn**.
- **Workspace çevrimdışı kabul kapandı:** üç ayrı native süreçte warm **9,3 sn**, offline **6,4 sn**, resume **6,9 sn** geçti. Servis gerçekten durduruldu; uygulama yeniden açıldı; şifreli önbellekten workspace+RSA SSH başlangıç komutu çalıştı. Host/workspace düzenlemesi ve yeni host toplam3 kuyruk işlemi olarak kaldı; servis açılınca içerik/ilişki/revizyon eşitliğiyle DB'ye gitti. İlk warm koşusu test ekibinde kurtarma dosyası hazırlanmamış olduğundan reddedildi; fixture hazırlığı tamamlanınca geçti.
- **Mosh workspace kabul kapandı:** ilk test adres/komut karışıklığını yakaladı (DNS hatası). Düzeltmeden sonra Windows paketli Mosh → SSH jump → WSL gerçek UDP sunucu, workspace başlangıç işareti ve sonraki kullanıcı komutu **9,2 sn** geçti. WSL NAT için test UDP rölesi kullanıldı; üretim proxy desteği iddiası değildir.
- Teslim betiği eski Desktop/artifacts paketlerini **beklendiği gibi reddetti**; yeniden derlenmiş paket var diye sunulmadı. Python/TypeScript syntax ve diff kontrolü geçti. Native test executable'ı teslim edilmedi. Son normal Windows geliştirme derlemesi **54,59 sn** içinde tamamlandı; test sürücüsü bağımlılığı ve frontend hook bulunmadığı kontrol edildi. Bu bir NSIS/ZIP paket yenilemesi değildir.

Sırada: tam eski sürüm aktarımı ve tarihsel route/Team proxy; nihai Windows/Linux kurucu/servis paketleri ve bütünleşik temiz kurulum doğrulaması. Donanım/agent/MFA/üretim ve macOS kapsamları doğrulanmadan geçmiş sayılmaz. Mosh CLI kaynağı: [resmî mosh-server kılavuzu](https://github.com/mobile-shell/mosh/blob/master/man/mosh-server.1).

---

# Güncel çalışma — Team servis yükseltme altyapısı (8 Ekim 2026)

**0.4.1-dev.3 geliştirme dalı; şema 61, API 1, Team cache 6. Release/main/tag/feed değişmedi.** Bu bölüm önceki teslim notlarından yenidir. Desktop teslim klasöründeki paketler hâlâ şema 60 kaynaklarından üretilmiştir; aşağıdaki yükseltme araçları ve önceki v0.4.0 önizlemesi için paketler henüz yenilenmedi.

Tamamlanan geliştirme:

- `compatibility.json`: servis/API/şema/bakım protokolü sözleşmesi; Rust servisindeki kapıyla otomatik tutarlılık testi. Uygulanmış 001–060 dosyaları değiştirilmedi; 061 kurulum kimliği ve DB kapsamı ekler. Bundan sonraki SQL numarası 062.
- `deploy/upgrade.py plan`: salt okunur plan; hedef DB, migration hesabı, sınırlı servis rolü, sürüm sırası ve uygulanmış dosya hash kontrolü. Eski/yeni paket veya yanlış hedef sessizce kabul edilmez.
- `apply`: tüm servislerin durdurulduğu bakım akışı, oturum boyunca DB kilidi, özel dizinde tam DB yedeği, aynı snapshot üzerinden ayrı/boş/özel DB'ye gerçek restore ve Team tablo sayısı karşılaştırması. Var olan doğrulama DB nesneleri silinmez. Üretimde her satırın kriptografik eşitliğini ölçmez; bu sınır kılavuzda yazılıdır.
- Aşama günlüğü ve atomik revision/checksum; kesintiden sonra aynı plan/paketle devam. Doğrulanmış ilk yedek korunur. Geriye migration ve yıkıcı otomatik rollback yoktur. İlk kurulum aracı mevcut DB'nin yeni sürüme yedeksiz geçişini reddeder.
- `confirm`: doğrulanmış HTTPS/CA, API/şema sözleşmesi, kurulum UUID ve DB kapsamı kontrolü; başka DB'deki aynı sürüm başarılı sayılmaz. Bakım kilidinde istekler 503 `service_maintenance` alır; yetki verilmiş sayılmaz.
- Türkçe yükseltme/geri dönüş kılavuzu; paket betiklerinde sözleşme ve kılavuz dahil. AGENTS.md gelecekteki şema değişiklikleri için temiz kurulum, eski veriyi koruma, kesinti/devam ve geri dönüş doğrulamasını zorunlu kılar. DB dump dosyaları Git/Syncthing hariç tutulur.

Gerçek doğrulama:

- Linux Rust: **4 servis + 25 istemci birim testi geçti**. Canlı ortam isteyen test normal birim koşusunda atlanır; aşağıda ayrıca çalıştırıldı.
- Temiz PostgreSQL 18/TLS kurulumu: **61 migration**, checksum/replay, sınırlı rol, bootstrap, systemd+Caddy HTTPS/CA ve servis yeniden başlatma geçti.
- Gerçek 60 → 61 yükseltme: iki hesap, host, explicit deny ACL, şifreleme kimliği/zarfı, geçmiş ve işlem kayıtlarının satır hash'leri yükseltilen kaynakta ve yedekten açılan DB'de eşit kaldı. Payload'lar sentetik şifreli blob'lardır; bu test şifre çözme doğrulaması değildir. Yanlış DB/CA, servisi durdurma şartı, dolu doğrulama DB'sinin korunması, replay, bakım isteği reddi/yeniden açılması geçti.
- Yalnız özel test paketindeki 062/063 ile migration ortasında süreç kesildi: tamamlanan revision ve checksum birlikte kaldı, yarım transaction geri alındı, aynı planla devam tekrar kayıt üretmedi. Downgrade ve değişmiş uygulanmış migration reddedildi. Repo 062/063 dosyası oluşturulmadı.
- Son geniş HTTPS/PostgreSQL/Rust istemci regresyonu: **1 canlı senaryo geçti, 130,55 sn**; gerçek kripto, iki kullanıcı izolasyonu, izin, günlük/çakışma ve kurtarma akışları dahil. Önceki koşu `offline_edit_expired` ile 87,84 sn'de başarısızdı; aynı kaynakla tekrarında hata oluşmadı. WSL timesync günlüğünde başlangıçta saatin geri düzeltilmesi var, ancak ilk hatanın kesin nedeni kanıtlanmadı. Bu ilk koşu gizlenmedi; saat kararlılığı takip maddesi olarak kalır. Bağlantı kesme testindeki beklenen TLS EOF fixture çıktısı son testi başarısız kılmadı.
- Python syntax ve `git diff --check` geçti. Mevcut masaüstü UI/SSH kodu bu yükseltme çalışmasında değiştirilmedi; yeni Windows/native veya Compose paket testi yapıldı sayılmadı.

Özel laboratuvar kanıt dizini: `/var/lib/termterm-team-upgrade-tests/dce5ea8a981c4816b9a49b125a086b9e` (kaynak/paket içine alınmaz). Kendi HTTP/Caddy test süreçleri kapatıldı.

Açık kapsam: v0.4.0 hesap eşleme/aktivasyon, yetkili yeniden sarma, staging/apply, tam izin/ilişki doğrulaması ve geri dönüş hâlâ tamamlanmalı. Tarihsel chain/proxy ve önceki rapordaki gerçek donanım/agent/MFA, belge çevrimdışı/Mosh kabul açıkları kapanmış sayılmaz. Güncel Windows/Linux yerel paketlerinin yeniden üretilmesi de bekliyor. **Bu ekleme %100 Team teslimi veya kararlı sürüm onayı değildir.**

---

# Güncel çalışma — v0.4.0 geçiş arşivi ve önizlemesi (8 Ekim 2026)

**Geçiş kapsamı kısmi. Hedefe aktarma henüz uygulanmadı.** Önceki teslim klasöründeki kurucular bu yeni adımı içermez; bu çalışmada geliştirme kaynağı ve ayrı Windows deneme executable'ı yenilenir. Release/main/tag/güncelleme yayımlanmaz.

- Eski Team → Bağlantı ve politika alanında yalnız çevrimiçi takım sahibi, güncel sunucu kayıtlarını ve üyelik/ACL/yetki devrini `.ttmigration` arşivine çıkarabilir. Bekleyen yerel değişiklikler arşive otomatik dahil edilmez.
- Kaynak okuma tek `REPEATABLE READ READ ONLY` transaction kullanır; mevcut sınırlı PostgreSQL uygulama hesabıyla çalışır. Serbest SQL veya migration uygulanmaz. Sahiplik sunucunun doğruladığı oturumdan alınır.
- Şifreleme Argon2id + XChaCha20-Poly1305, sürümlü ve bağlama bağlıdır. En az 12 karakter arşiv parolası, sınırlı dosya boyutu, atomik yeni dosya yazımı; var olan dosya üzerine yazılmaz. Parolalar/token'lar arşiv metadata'sına girmez. Çözülmüş kayıtlar önizleme IPC'sinden çıkmaz.
- Yeni Team → Eski Team geçişi: çevrimiçi hedef sahibi arşivi okuyabilir. Kaynak/önerilen hedef kimlikleri, klasör/kimlik/chain bağımlılıkları, üyeler ve açık engeller gösterilir. Farklı hedef takıma farklı deterministik kayıt kimlikleri üretilir. Kullanıcı adına bakarak hesap/izin eşlemesi yapılmaz.
- Açılamayan içerik, eksik/silinmiş ilişki, klasör döngüsü, desteklenmeyen tür ve dönüştürülmemiş alanlar raporlanır. Kayıt görünümü 100 satır/sayfa; üye/izin özeti ilk 100 satırdır. Arşiv tam DB yedeği değildir: eski revizyonlar/audit içermez.
- Kaynak dosya, hedef servis ve eski giriş değişmez. `canApply=false`; önizlemede yazma düğmesi yoktur. Şifreli geçiş arşivleri Git/Syncthing kaynak hariç tutmalarına eklendi.

Doğrulama:

- Windows Rust Team odaklı: **9 geçti, 1 yardımcı test atlandı** (yardımcı süreç ayrı kesinti testi içinde çalışır). Üç yeni test: sırların önizlemeye sızmaması/ilişkiler/açık engel, parola/bozulma/sürüm/var olan dosya korunması, bozuk ilişkiler.
- Gerçek eski PostgreSQL TLS testi: **1 geçti, 7,68 sn**. Sahip okuması, operatör reddi ve kaynak kayıt/üyelik/ACL eşitliği; önceki şifreleme/geçmiş kontrolleri dahil. İlk deneme WSL bağlantı reddiyle başarısız oldu; dağıtımı test süresince ayakta tutunca temiz koşu geçti.
- Son Team/SFTP arayüz regresyonu: **27/27, 52,1 sn** geçti. Ayrı normal Windows geliştirme derlemesi **1 dk 08 sn** tamamlandı; native/UI test sürücüsü ve hook bulunmadığı doğrulandı.
- Yeni arayüz testi: yanlış parola, parola alanının temizlenmesi, tekrar tıklama engeli, açık engeller, salt okunur davranış ve dosya değişince eski sonucun temizlenmesi geçti. `migration-preview.png` sentetik UI testidir, gerçek native aktarım kanıtı değildir.

Sıradaki işler: hedef hesap eşleme ve aktivasyon; sürüm/izin bağlı anahtar yeniden sarma; staging/atomik commit ve tekrar güvenliği; tam ilişki/izin eşitliği; geri dönüş/denetlenmiş eski giriş kapatma. Geçmiş/audit ve temsil edilemeyen alanların geçiş politikası da tamamlanmalıdır. Bu adım tam geçiş veya %100 kapsam olarak sayılmaz.

---

# Güncel belge/workspace doğrulaması — 8 Ekim 2026

**0.4.1-dev.3 geliştirme kapsamı halen bütünüyle tamamlanmadı. Beş yerel geliştirme paketi bu bölümdeki son kaynaklardan yeniden üretildi ve kurulum kontrolleri geçti.** Aşağıdaki eski “snippet/workspace uygulanmadı” satırları tarihçedir.

Yeni uygulama:

- Team komut paketleri ve çalışma alanları: ayrı normalize `snippets`, `workspaces`, `workspace_entries` tabloları; şifreli ad/içerik; klasör, açık engel/devralma, taşıma, geçmiş, çatışma ve kalıcı işlem günlüğüyle bütünleşme.
- Klasör/arama ve sağ tık düzenleme/silme; komut önizlemesi ve hedef seçimi; en fazla 16 host, sekme/bölünmüş görünüm, hedef başına başlangıç komutu ve Stats tercihi.
- Yerel IPC yalnız belge kimliği ve önizleme revizyonu/yerel neslini kabul eder. Rust her hedefi yeniden yetkilendirir; frontend kaynaklı ham başlangıç komutu kabul etmez. Geçersiz hedef veya değişen önizleme reddedilir. Başlangıç komutu SSH/shell kurulduktan sonra gönderilir. SSH/Mosh ortak başlangıç yolu kullanılır; bu eklemede Mosh başlangıç komutu ayrıca gerçek çalıştırılmadı.
- Team cache sürümü 6. Değişmeyen sunucu yenilemesi yerel nesli gereksiz arttırmaz; değişen içerik/izin hâlâ CAS'ı değiştirir. Eski kişisel kasa biçimi korunur.

Kanıt:

- PostgreSQL belge testi: listeleme, devralma/engelleme, workspace okumasının host yetkisi vermemesi, tekrar güvenliği, 11 sürüm, ilişki ve atomik hata, doğrudan SQL reddi geçti.
- Gerçek HTTPS/PostgreSQL/Rust kriptografi koşusu: **230 sn geçti**; belge şifre çözümü, alıcı anahtar dağıtımı, eski sürüm geri yükleme ve önceki hesap/chain/kurtarma kontrolleri dahil.
- Windows istemci: **25/25**; şifreli belge günlüğünü yeniden açma ve aynı veriyi yenilemede önizleme korunması dahil.
- Team/SFTP arayüzü: **26/26, 59,6 sn** geçti. `documents-ui.png` sentetik arayüz örneğidir.
- Gerçek Windows native: **3/3, 25,5 sn** — lider girişi, kasa kurulumu, gerçek iki SSH paneli ve komut çıktısı. Eski revizyon ve hedef değiştirme IPC'si reddedildi. `windows-documents.png` gerçek uygulama görüntüsüdür ve görsel olarak incelendi.
- Önceki native başlangıç denemeleri yanlış E2E ayarı nedeniyle sürücüye erişemedi; doğru ayrı test yapılandırmasıyla temiz koşu geçti. E2E binary/paketleri dağıtılmayacak.
- Linux son kaynak: **4 servis + 25 istemci** testi geçti.
- Yeni 60 migration temiz kurulumu, tekrar/dosya hash denetimi, daha yeni şema reddi, ayrıcalığı sınırlı servis hesabı, gerçek systemd + Caddy HTTPS/CA ve yeniden başlatma geçti.
- PostgreSQL audit/depolama alanı regresyonu: takım ayrımı, 100 satırlık sayfalama, 90 günlük saklama ve 1.000 satırlık sınırlı temizlik geçti.
- Taşıma PostgreSQL regresyonu geçti. Uzun izleme, macOS veya ücretli CI çalıştırılmadı.

## Güncel paket kontrolleri

- Windows normal optimize derleme **9 dk 31 sn**; Linux normal optimize derleme **11 dk 04 sn**; Linux servis **1 dk 29 sn**. Test sürücülü executable dağıtılmadı.
- Güncel Windows NSIS kurucusu gerçekten kuruldu: executable hash karşılaştırması, paketli Mosh çalıştırma, gerçek pencere, test portunun bulunmaması ve kaldırma geçti.
- Güncel Linux `.deb` kuruldu/açıldı/kaldırıldı. AppImage çıkarılıp AppRun ile gerçek pencere açıldı. Doğrudan FUSE başlatması bu kanıta dahil değildir.
- Son optimize servis paketinin hem ayrı PostgreSQL'li hem mevcut PostgreSQL'e bağlı Compose kurulumu geçti: şema 60, HTTPS/CA doğrulaması, yanlış CA reddi, TLS'siz DB bağlantısı reddi, doğrudan tablo erişimi reddi ve yeniden başlatma. Geçici Compose servisleri test sonunda durduruldu.
- Teslim: Windows NSIS + taşınabilir ZIP, Linux `.deb` + AppImage, Linux Team Service tar.gz. `SHA256SUMS.txt` paket bütünlüğünü; `SOURCE_FINGERPRINT.json` yerel kaynak durumunu kaydeder. Özel laboratuvar verileri dağıtılmaz.
- Masaüstü klasörü: `C:/Users/sonx/Desktop/TermTerm-0.4.1-dev.3`. `DELIVERY.json` genel kapsamın tamamlanmadığını açıkça belirtir. Kurucular imzasız geliştirme paketleridir.

## Kalan kapsam

- v0.4.0 tam aktarım/eşleme/izin eşitliği/tekrar/geri dönüş akışı henüz uygulanmadı.
- Geçmiş sürümle chain route ilişkisi ve Team proxy tamamlanmadı.
- Gerçek agent/MFA/donanım, macOS ve erişilmeyen üretim ortamları doğrulanmadı. Yeni belge akışının çevrimdışı terminal/Mosh uçtan uca denemesi henüz ayrı yapılmadı; kalıcı şifreli belge günlüğü birim testi ve mevcut çevrimdışı bağlantı regresyonu vardır.

Bu nedenle genel kapsam için “%100” veya “kararlı yayın hazır” denmez. Yeni Release/main/tag/otomatik güncelleme yayını yapılmayacak. 0.5 çalışma ağacı ve kullanıcı kasaları korunmuştur.

---

# Paketleme ve temiz kurulum doğrulaması — 8 Ekim 2026

**0.4.1-dev.3 yerel değerlendirme paketleri oluşturuldu ve kurulum kontrolleri geçti. Genel özellik kapsamı halen tamamlanmadı; kararlı yayın için hazır olduğu iddia edilmiyor.** Release/main/tag/güncelleme yayını yapılmadı, 0.5 kaynakları değiştirilmedi.

Bu çalışmada kapanan teslim işleri:

- Ayrı Ubuntu 22.04 Team derleme kökü oluşturuldu. Windows NSIS ve Linux `.deb`/AppImage paketleri derlendi. `termterm-team-dev` executable ve `local.termterm.desktop.teamdev` kimliği kararlı uygulamadan ayrıdır.
- Windows gerçek sessiz kur/kaldır testi geçti: kurulan executable hash'i, Mosh yardımcısı, pencere açılışı, düzgün kapanış ve test portunun bulunmaması. Mevcut kararlı kurulum korunur.
- Linux izole ortamda `.deb` kuruldu, gerçek X11 pencere açıldı ve paket kaldırıldı. AppImage çıkartılıp kendi AppRun girişinden gerçek pencere açıldı. FUSE üzerinden doğrudan AppImage çalıştırma bu testin kapsamı değildir. Test sürücüsü/özel test verisi ve kararlı binary yolu çakışması denetlendi.
- Linux servis paketi hazırlandı. Gerçek systemd + Caddy HTTPS + mevcut PostgreSQL TLS testi geçti. Ayrı PostgreSQL kuran Compose ve mevcut PostgreSQL'e bağlanan Compose seçenekleri de gerçek paketle geçti. Yanlış CA, TLS'siz DB erişimi ve doğrudan tablo okuma reddedildi; servis yeniden başlatıldı.
- Yeni açık migration aracı tek oturum kilidi, dosya SHA256 denetimi, tekrar çalıştırma ve ayrı migration/servis hesapları kullanır. 58 migration temiz veritabanına uygulandı. İlk lider geçici parolası 0600 özel dosyaya yazılır, loglanmaz. Mevcut çıktı dosyası ikinci takım oluşturmadan reddedilir.
- Paket/şema sürümü uyuşmuyorsa servis `/ready` ve DB işlemleri 503 verir; otomatik migration yapmaz. Gerçek testte daha yeni şema eklenip reddi ve geri alındıktan sonra hazır duruma dönmesi doğrulandı.
- Windows Rust çekirdeği: **54 geçti, 7 atlandı**; atlananlar ayrı laboratuvar/imzalı yayın veya yardımcı alt süreç işaretli testlerdir. Atlananlar geçmiş sayılmaz. TypeScript: **25/25**. Son Team/SFTP arayüzü: **24/24, 32,9 sn**. Önceki arayüz koşusunun üç başlangıç zaman aşımı sonrası test sunucusu yeniden açıldı; kaynak davranışı değiştirilmeden temiz koşu geçti.
- Türkçe ayrı servis kurulum kılavuzu `TEAM_SERVICE_INSTALL_TR.md`; iki Compose yolu, systemd, TLS/CA, ilk lider, backup ve geri dönüş adımlarını içerir.

Son servis doğrulaması: Linux **4 servis + 23 istemci** testi geçti; gerçek HTTPS/PostgreSQL kriptografi ve iki hesap izolasyonu **119,96 sn** geçti. Kesinti senaryosunda beklenen TLS EOF kaydı oluştu, test başarılı tamamlandı. Son optimize servis binary'si tekrar paketlenip her iki Compose DB seçeneğiyle doğrulandı. Test systemd birimi, Compose servisleri ve HTTPS fixture işlemleri durduruldu; disposable veri/volume'lar tekrar test için korundu.

## Halen tamamlanmamış uygulama kapsamı

1. Team tipli snippet/workspace, şifreli günlük/geçmiş, hedef bazlı yetki ve bağlantı sonrası başlangıç komutları.
2. v0.4.0 tam geçiş: salt okunur önizleme, hesap/kaynak eşleme, içerik/izin eşitliği, tekrar güvenliği ve geri dönüş.
3. Geçmiş sürümle chain route ilişkisi, Team proxy ve erişilebilir ortamda ileri kimlik doğrulama kontrolleri.
4. macOS, gerçek donanım anahtarları, erişilmeyen üretim ortamları doğrulanmadı.

Paketler mevcut çalışan özellikleri denemek içindir. Yukarıdaki uygulama açıkları kapanmadan genel kapsam için “%100” veya “kararlı yayın hazır” denmez. Aşağıdaki kayıtlar önceki tarihçedir; güncel durum için bu bölüm esas alınır.

---

# Güncel durum — 8 Ekim 2026, 0.4.1-dev.3

Çalışma devam ediyor. Aşağıdaki tarihçeden farklı bir durum varsa bu bölüm esas alınır. **Genel Team kapsamı ve nihai Windows/Linux teslimi henüz tamamlanmadı.** Release/main/tag/güncelleme yayını yapılmadı; ayrı 0.5 çalışma ağacına dokunulmadı.

## Bu çalışmada tamamlanan ve doğrulananlar

- **SFTP makine seçimi:** iki panelde mini klasör gezgini, iç içe klasörler, Türkçe isim araması, başarılı bağlantıları en üste alan son bağlantılar bölümü. Son bağlantılar bu SFTP çalışma oturumunun belleğinde tutulur; uygulama yeniden açıldığında kalıcı geçmiş olduğu iddia edilmez. Team'de yalnız bağlantı izni olan hostlar görünür; takım/kasa/hesap değişince panel durumu temizlenir.
- **SFTP hata düzeltmesi:** yeni bağlantı başarısız veya henüz beklemedeyken eski sunucunun dosyaları/işlemleri aktif kalmaz. Sonradan tamamlanan eski bağlantı kapatılır; başarısız veya geçersizleşmiş bağlantı son kullanılanlara eklenmez.
- Playwright son kaynakta **23/23, 38,3 sn geçti**: 21 Team + 2 SFTP senaryosu. Klasör/isim/son bağlantılar, eski dosyaların temizlenmesi, bağlantı yarışı ve Team izin filtresi dahil.
- Gerçek Windows native ana koşu **14 senaryo geçti**; SFTP'de isim arama → gerçek laboratuvar bağlantısı → dosya listeleme → son bağlantının ilk sıraya alınması doğrulandı. `artifacts/team-v2/windows-sftp.png` gerçek uygulama görüntüsüdür. Yeni taşıma senaryosu ayrı odaklı native koşuda geçti: hesap/kasa/taşıma **3/3, 21,7 sn**. Önizleme öncesi eski üst klasör, onaydan sonra yeni üst klasör ve eski klasörün boşalması gerçek backend üzerinden doğrulandı. Bu, önceki 14 testle birlikte 15 ayrı senaryonun kanıtıdır; son kaynakta tek 15/15 toplu koşusu değildir.
- **056–058 migration'ları** ayrı laboratuvar DB'sine uygulandı. Kapsam yöneticisinin chain düzenlemesi delegasyon sınırlarıyla denetlenir. Hop değişikliklerinde yeni anahtarlar ve sonradan verilen host izinlerinde bağımlı hop anahtarları transaction içinde güncellenir. Başka bir hosta bağımsız bağlanma veya sırrını dışa aktarma izni oluşmaz.
- Gerçek HTTPS/PostgreSQL/Rust kriptografi koşusu **124,22 sn geçti**. Güncellenen jump içeriğinin çözülmesi, yeni alıcının chain erişimi ve yalnız hedefe yetkili yöneticinin başka alıcıya sınırlı erişim vermesi doğrulandı. Linux **3 servis + 23 istemci** testi geçti. SQL sınır/tekrar/eksik zarf/eski önizleme ve doğrudan özel fonksiyon erişim reddi testleri geçti.
- Taşıma SQL sınırları: yönetici uygulaması, değişen kimlik ve route nedeniyle eski önizlemenin reddi geçti. 10.000 satırlık sorgu planlarında bekleyen taşıma 0,102 ms, anahtar sayfası 0,094 ms, erişim farkı sayfası 0,157 ms; ilgili indeksler kullanıldı. Bunlar laboratuvar ölçümüdür.
- Taşıma iptali ağ yanıtını süresiz bekletmez; yerel bekleyen plan temizlenir. Uygulama yanıtı kaybolmuşsa kayıt sonucunun kontrol edilmesi gerektiği gösterilir.
- TypeScript kontrolü ve Windows derlemeleri geçti. Son normal geliştirme derlemesi **1 dk 21 sn** içinde tamamlandı; `src-tauri/target/debug/termterm.exe` artık E2E özelliği kapalı derlemedir. Bağımlılık ağacında native test sürücüsü, frontend paketinde test kancası bulunmadığı doğrulandı. Bu yürütülebilir dosya nihai NSIS/ZIP tesliminin tamamlandığı anlamına gelmez. Önceki E2E derlemeleri dağıtılmayacak.

Laboratuvar HTTPS ve SSH fixture servisleri son testten sonra temiz biçimde durduruldu.

Ek SFTP denetimi: yüklenen/başarısız klasöre sürükleyerek aktarım ve dosya işlemleri engellenir; eski liste ve seçim temizlenir. Gecikmiş eski klasör yanıtı yeni sonucu değiştiremez. Son SFTP **3/3, 5,3 sn**; mevcut kişisel SFTP bağlam menüsü, kısmi silme/yeniden deneme ve görünüm regresyonu **3/3, 7,6 sn** geçti. TypeScript ve diff boşluk kontrolü geçti. Native taşıma görüntüsü görsel olarak incelendi.

## Kalan kapsam

| Alan | Açık iş |
|---|---|
| Bağlantılar | Geçmiş sürümle chain route ilişkisinin tamamlanması, Team proxy, erişilebilir ortamda agent/MFA/donanım doğrulaması |
| Kaynaklar | Team tipli snippet/workspace, şifreli günlük/geçmiş/izin ve bağlantı sonrası başlangıç komutları |
| Geçiş | v0.4.0 salt okunur önizleme, eşleme, aktarım, içerik/ilişki/izin eşitliği, tekrar güvenliği ve geri dönüş |
| Teslim | Temiz Compose (mevcut/ayrı TLS PostgreSQL), systemd/CA kurulumu, normal Windows NSIS/ZIP, Linux deb/AppImage, servis paketi, SHA256 ve gizlilik/açılış doğrulaması |
| Son doğrulama | Yeni özellikler tamamlandıktan sonra son kaynak/paket durumunda odaklı bütünleşik regresyon |

macOS, gerçek donanım ve erişilmeyen üretim ortamları geçti sayılmaz. Nihai paketler tamamlanmadan yayın onayı istenmeyecek.

---

# Kullanıcı isteğiyle duraklatıldı — 7 Ekim 2026

Çalışma kaydedildi. Ayrıntılı devam sırası `.tools/RESUME_TEAM_041.md` içinde. **Genel kapsam tamamlanmadı, nihai kurulum paketleri üretilmedi.** Kararlı yayın/main/tag/güncelleme yapılmadı; 0.5 kaynaklarına dokunulmadı.

Son doğrulanan durum:

- Windows native **14/14 geçti (3 dk 26,8 sn)**: hesaplar, kişisel kimlikler, izinler, SSH/chain/SFTP/tüneller, geçmiş/kararlar, şirket anahtarı değiştirme ve ortak kimlik kurtarma. Gerçek ekran görüntüleri görsel olarak kontrol edildi.
- Windows gerçek Team Mosh/UDP testi **6,7 sn geçti**.
- Kişisel kullanıcı adı kurtarma, arşivden geri yükleme ve şirket anahtarı değişimine dahil edilme: gerçek HTTPS/PostgreSQL **107,95 sn geçti**; Linux 3 servis + 23 istemci testi geçti.
- Arayüz **19/19** geçti; ardından yeni taşıma ekranının ayrı testi de geçti. 20 testin son kaynak durumunda toplu koşusu henüz yapılmadı.
- Host/klasör taşıma, erişim farkı, kapsam yöneticisi sınırı, yeni alıcılara geçmiş anahtarları, chain bağımlılıklarının yeniden sarılması ve atomik uygulama kodlandı. PostgreSQL kabul testi geçti. Son gerçek HTTPS/PostgreSQL şifre çözme ve taşıma testi 110,72 sn içinde geçti. Yeni Windows E2E deneme derlemesi 1 dk 9 sn içinde tamamlandı; bu test derlemesi teslim paketi değildir. Yeni 15. native taşıma testi henüz çalıştırılmadı.

Test servisleri temiz biçimde durduruldu; çalışan iş bırakılmadı. Devam yalnız kullanıcı istediğinde yapılacak.

Kalan ana işler: taşımanın native ve kalan sınır durumu doğrulaması; Team snippet/workspace; genel chain izin/bağımlılık yenileme ve ileri bağlantı kontrolleri; v0.4.0 tam geçişi; temiz servis kurulumu; Windows/Linux teslim paketleri ve son doğrulama raporu. Erişilemeyen donanım/ortamlar geçti sayılmayacak.

---

# Yeni doğrulama — 7 Ekim 2026, dev.3

Kapsam henüz tamamlanmadı; Windows/Linux teslim paketleri hazır değil. Yayın yapılmadı.

- Gerçek PostgreSQL/HTTPS: ortak kimlik kurtarma 99,80s; kişisel kullanıcı adı kurtarma ve şirket anahtarı değişimine dahil etme 100,69s geçti. SQL izolasyon/CAS testleri, Linux 3 servis + 23 istemci testi geçti.
- Windows native: 13/14 geçti; şirket anahtarı değiştirme gerçek masaüstünde doğrulandı. Ortak kurtarma senaryosunda otomasyon özel kaydı seçti; seçici düzeltildi, tekrar koşusu bekleniyor.
- Kişisel profil geçmişi: açılabilen sürüm önizlemesi, yeni revizyonla geri yükleme ve eski revizyon reddi eklendi. UI toplam 19/19 (42,7s) geçti; bu yeni geçmiş akışının gerçek HTTPS testi sürüyor.
- Yeni 050–052 migration dosyaları kişisel kullanıcı adı sürümlerini, kullanıcı/şirket anahtar zarflarını ayrı ilişkisel tablolarda saklar. Önceki kurtarma zarfı bulunmayan içerik değişmeden korunur; açılamayan arşiv için açık yeniden giriş gerekir.
- Boş seçili kimlikten sessiz parola sorusuna geçiş engellendi. Mosh başlatma iptali eklendi; Rust kontrolü geçti, yeni native koşusu bekleniyor.

Açık kalan ana işler: chain yetki kapsamı/bağımlılık yenileme, erişim farkıyla taşıma, Team snippet/workspace, v0.4.0 tam aktarımı, temiz kurulum ve paket teslimi; erişilemeyen agent/MFA/donanım kontrolleri. Önceki ayrıntılar aşağıda tarihçe olarak korunur.

---

# Güncel durum — 7 Ekim 2026, 0.4.1-dev.3

Yalnız `codex/feature-team-personal-auth` çalışma ağacı değiştirildi. 0.5 kaynakları, kurulu kararlı uygulama ve kullanıcı kasaları korunuyor. Release/main/tag/feed/push/macOS/ücretli CI yapılmadı. **Genel kapsam henüz tamamlanmadı; nihai paket teslimi hazır değil.**

## Son doğrulanan işler

- Windows native 12/12 geçti (3m37,8s): iki kullanıcı, iki hop, RSA4096 sertifika/RSA2048 anahtar, SSH/SFTP, üç tünel, yetki ve geçmiş/karar akışları.
- Üç ayrı Windows süreç koşusu: çevrimiçi önbellek hazırlama 7,8s; uygulamayı kapatıp çevrimdışı açma, gerçek SSH, düzenleme + yeni host oluşturma 6,8s; servis dönünce iki işlemi gerçek PostgreSQL'e sırayla tek kez uygulama 5,5s. `windows-offline.png` gerçek uygulamadır ve görsel kontrol edildi.
- Migration 043/044: kapsamlı çevrimdışı oluşturma, ayrı yeni-kayıt karar tablosu, güncel alıcılara şifreleme, sahibi onaylamadan Hosts'a eklememe; SQL ve gerçek HTTPS şifre çözme testleri geçti.
- Migration 045: kapalı hesabın önceden verilmiş cihaz izniyle yalnız taslak teslimi. Normal oturum oluşturmaz, kayıt okuyamaz/uygulayamaz; sunucu yalnız izin özetini saklar. Gerçek kapalı hesap + Rust istemci + PostgreSQL testi ve UI testi geçti.
- Ortak Mosh taşıyıcısı Team chain/profil çözümlemesine bağlandı. Windows paketli Mosh istemcisi → WSL gerçek sunucu, SSH hop ve UDP komut çıktısı testi **6,4s geçti**. Laboratuvar WSL NAT için test UDP rölesi kullanır; üretim UDP proxy desteği iddiası değildir. İlk testlerde ConPTY yanıtı ve test rölesinin kaynak adresi düzeltildi; başarısız koşular geçerli kanıt sayılmaz.
- Kişisel SSH agent kimliği: belirli SHA256 parmak izi, platform sağlayıcısı, şifreli kayıt/önbellek, sessiz başka anahtara geçmeme. UI seçim/hata testi geçti. Bu bilgisayarda Windows ssh-agent kapalı olduğundan gerçek agent/hardware imzası henüz doğrulanmadı.
- Migration 046/047: parçalı şirket kurtarma anahtarı değişimi, yeniden doğrulama, kesintiden devam, kayıp son yanıtın makbuzu, işlem sırasında yeni kayıt ve eksik zarf kontrolü. SQL ve gerçek HTTPS kriptografi testleri geçti. Yeni dosya yeniden açılıp her eski/yeni zarf ile içerik doğrulanır. Güncel özel/ortak kimlik sürümleri kapsanır; kişisel kullanıcı adı kapsamı halen açık.
- Son Linux servis 3/3, istemci 22/22; gerçek HTTPS/PostgreSQL testi **100,86s** geçti (rotasyon dahil). Testin bilerek kapattığı TLS bağlantısında fixture EOF kaydı beklenen kesinti verisidir.
- TypeScript kontrolü ve Team arayüz testleri **17/17 (49,9s)** geçti. `rotation-preview.png` sentetik UI görüntüsüdür; gerçek native rotasyon kanıtı henüz değildir.

## Açık işler

| Alan | Durum / kalan |
|---|---|
| Bağlantılar | Chain manager kapsamı, alıcı/revizyon değişiminde otomatik hop zarfı yenileme, tarihsel route, Team proxy; gerçek agent/MFA/donanım doğrulaması |
| Offline | Temel soğuk açılış/yeni kayıt/24h/karar/kapalı hesap teslimi doğrulandı; nihai yeni özelliklerle toplu regresyon gerekli |
| Kurtarma | Ortak kimlik kurtarmasının yeni önizleme akışı kodlandı, test çalışıyor (048); kişisel kullanıcı adı ve açılabilen arşiv kurtarma henüz tamamlanmadı |
| Kaynaklar | Erişim farkıyla host/klasör taşıma; tipli Team snippet/workspace ve günlük/geçmiş entegrasyonu |
| Geçiş | v0.4.0 tam önizleme/eşleme/aktarım/izin doğrulaması/idempotency/geri dönüş |
| Teslim | Temiz Compose (mevcut/ayrı PostgreSQL), systemd/CA, güncel kılavuzlar, Windows NSIS/ZIP + Linux deb/AppImage + servis paketi, SHA256/gizlilik/açılış kontrolü |

Mevcut Windows debug EXE E2E test özelliklidir; normal teslim paketi değildir. Final kaynak durumuyla test erişimleri kapalı olarak yeniden derlenecek. Aşağıdaki kayıtlar tarihçedir; durum uyuşmazlığında bu bölüm esas alınır.

---

# TermTerm 0.4.1-dev.3 — güncel çalışma durumu (2026-10-07)

**Geliştirme devam ediyor; tam teslim ve %100 tamamlanma iddiası yok.** Release, main/tag, güncelleme yayını, macOS veya ücretli CI yapılmadı. 0.5 kaynakları değiştirilmedi.

## Son doğrulama ve yeni tamamlanan işler (7 Ekim, dev.3)

- Windows native ana takım senaryoları **12/12, 3m32,8s** geçti. Sonraki üç ayrı uygulama sürecinde çevrimiçi hazırlık, servis kapalıyken parola ile yeniden açılış/gerçek SSH/yerel düzenleme, servis geri geldiğinde tek uygulama ve boş kuyruk doğrulandı. İlk çevrimdışı ekran bekleme testi 15 saniyede zaman aşımına uğradı; daha uzun bekleme ile geçti. Ardından tekrarlanan bağlantı denemelerini azaltan 5 saniyelik ağ geri çekilmesi eklendi; yeni Windows koşusu sürüyor.
- Önbellek formatı **5**: çevrimdışı gezinme klasörleri, yeni kayıt oluşturma kapsamları ve taslak ilişkileri. Kişisel kasa/yedek biçimi değişmedi. Eski uygulamalar desteklemediği önbelleği değiştirmeden reddeder.
- Host adresi/portu/kullanıcı politikası değişince ilgili tüm kök/hop planları aynı transaction içinde geçersizleşir; salt ad değişikliği bağlantıyı düşürmez. Disk yazma hatasında hem kayıt hem plan korunur. Profil, anahtar ve chain yazımı öncesi eski kimlik önbelleği temizlenir; kaybolan yanıt eski sırla bağlantıya neden olmaz.
- Migration **043**: takım/hesap/cihaz/kasa/klasöre bağlı çevrimdışı oluşturma izni ve tekrar güvenli makbuz. Önceden indirilmemiş kapsamda oluşturma reddedilir; 24 saat ve saat geri alma sınırları korunur.
- Migration **044**: yetkisi kaldırılmış üyenin yeni kayıt adayı için ayrı ilişkisel karar ve zarf tabloları. Onaylanana kadar Hosts listesinde kayıt oluşmaz. Lider güncel yetki/anahtar alıcılarıyla onaylar veya reddeder. Kayıt karşılaştırması yerel bekleyen taslağı sunucudaki kayıt gibi göstermez.
- `tests/team_v2_creation.py`, `tests/team_v2_offline.py`, `tests/team_v2_accounts.py`: gerçek PostgreSQL üzerinde kapsam/cihaz/yetki izolasyonu, aday teslimi, tekrarlama, yalnız lider kararı ve doğrudan SQL reddi geçti.
- Windows Rust istemci **22/22** ve TypeScript kontrolü geçti. Linux istemci **22/22**, servis **3/3** geçti. Son gerçek TLS/PostgreSQL entegrasyonu **200,76s** geçti; soğuk açılış, çevrimdışı oluşturma + ikinci düzenleme + üç sıralı işlem, ayrıca yetkisi kaldırılan ayrı kullanıcının gerçek şifreli adayını liderin çözerek onaylaması dahil. Kesilen laboratuvar TLS bağlantısında sunucu EOF kaydı oluştu; test sonucu geçti, kayıt gizlenmedi.
- Playwright Team **14/14** geçti. Yeni kayıt karar görüntüsü sentetik UI testinden; `windows-offline.png` gerçek Windows uygulamasındandır. İkisi görsel olarak incelendi.
- Yeni Windows E2E yürütülebilir dosyası derlendi; E2E erişimli bu dosya teslim paketi değildir. Güncel native/regresyon ve çevrimdışı yeni kayıt tekrar doğrulaması devam ediyor.

**Açık:** Oturumu/hesabı kapatılmış kullanıcı için oturumsuz yalnız-aday teslimi henüz yok. Chain manager/agent/Mosh/proxy/yeniden sarma, kurtarma anahtarı değişimi/ortak ve kullanıcı adı kurtarma, taşıma/snippet/workspace, v0.4.0 aktarımı ve temiz kurulum/final paketler halen tamamlanmadı. Genel kapsam %100 değildir.

## Önceki çalışma kanıtları (aşağıdaki sayılar tarihsel)

## Bu çalışma turunda uygulananlar

- Migration 037–042: sıralı Team chain, hedefe bağlı şifreli hop zarfları, sunucuda hedef/hop yetki kontrolü, revizyon ve alıcı karşılaştırmalı önizleme, tekrar güvenli uygulama, süresi dolan kullanılmamış planların sınırlı temizliği. Önceki migration dosyaları korunur.
- Her hopta kişisel profil/default çözümlemesi; bağımlı hopa ayrı terminal veya kayıt okuma hakkı verilmez. Ortak çözümleme gerçek SSH, SFTP ve üç tünel türüne bağlıdır.
- RSA kullanıcı sertifikası testinde eski SHA-1 algoritması hatası bulundu ve RSA-SHA2 imzasıyla düzeltildi. Sunucu güvenlik ayarı düşürülmedi. Sertifika/özel anahtar eşleşmesi denetlenir.
- Kayıt oluşturma ve düzenleme aynı şifreli kalıcı günlükten geçer; `localGeneration` yokken doğrudan sunucu yazımı kaldırıldı. Eski yerel revizyon/tekrarlanan belirsiz işlem reddedilir. Gerçek iki istemci çakışması testi eklendi.
- TLS hatası çevrimdışı erişime dönüştürülmez. Kanıtlanmış bağlantı kesintisi, yetki reddi ve sonucu belirsiz istek ayrılır.
- Hesaba ve servis adresine bağlı şifreli bağlantı planı önbelleği; parolayla açılan `.ttlogin` kimlik indeksi ve tokenların yalnız şifreli `.ttcache` içinde saklanması. İlk giriş çevrimiçidir. Token/sır frontend'e dönmez.
- Rust canlı servis testinde bağlantı gerçekten kesilerek istemci kapatıldı; yanlış parola reddedildi, doğru parola ile önbellek açıldı, bağlantı planı kullanıldı, düzenleme kuyruğa alındı ve ağ geri gelince gerçek PostgreSQL'e tek kez uygulandı. Yönetim işlemi çevrimdışı reddedildi.
- Çevrimdışı uyarısı, Türkçe hata açıklamaları ve chain önizlemesinde çözülen hop adları eklendi.

## Kanıtlar ve doğrulama sınırı

- `tests/team_v2_chain.py`: gerçek PostgreSQL 18 üzerinde sıra, köke bağlı erişim, bağımsız hop reddi, eski anahtar/plan, tekrar, temizleme ve fonksiyon/tablo erişim sınırları geçti; migration 042 izole laboratuvara uygulandı.
- RSA-SHA2 sertifika imzası birim testi: SHA-256/SHA-512 imzaları gerçek public key ile doğrulandı; değiştirilmiş veri ve farklı anahtar reddedildi.
- Windows native koşusu: **12/12 geçti (3m14,4s)**. İki hop, sertifikalı RSA4096/raw RSA2048, iki kullanıcı, SFTP ve üç tünel trafiği dahil. Bu koşu yeni soğuk çevrimdışı giriş kodundan **önceydi**.
- Son Linux Rust istemci **18/18**, servis **3/3**. Son gerçek HTTPS/PostgreSQL entegrasyonu **65,84s geçti**; yeni iki masaüstü istemcisi ve kesinti/soğuk açılış/geri dönüş testleri dahil.
- TypeScript kontrolü ve Playwright **12/12 geçti (34,4s)**. Sentetik UI doğrulaması gerçek servis/Windows testi yerine sayılmaz.
- Güncel Windows E2E derlemesi ve yeni 13. native çevrimdışı SSH testi hazırlanıyor; sonuç henüz teslim kanıtı değil. Normal üretim erişimleri olmayan geliştirme derlemesi ve paketler bundan sonra üretilecek.

## Kalan işlerin sınırları

1. Chain: manager kapsamı, otomatik alıcı/kimlik yenileme, geçmiş chain konfigürasyonu, Mosh/agent/proxy adaptörleri ve gerçek MFA. Düz ve açık sıralı route var; iç içe chain otomatik genişletilmez. Donanım doğrulanmadı.
2. Offline: çevrimdışı yeni kayıt için önceden verilmiş kapsam yetkisi; hesap kapatıldıktan sonra oturumsuz sınırlı aday teslimi; profil/host değişikliklerinde önbellek geçersizleştirmesinin tamamlanması; Windows soğuk giriş ve gerçek SSH doğrulaması. Soğuk açılış şu ana kadar Rust/HTTPS katmanında doğrulandı.
3. Kurtarma: şirket anahtarı değiştirme ve kesinti/devam; ortak kimlik/kullanıcı adı arşiv kurtarma. Önceki özel kimlik kurtarma akışı mevcut.
4. Kaynaklar: erişim farkıyla taşıma önizlemesi; Team snippet/workspace servisleri, günlük/geçmiş/başlangıç komutları.
5. v0.4.0 geçişi: tam önizleme/eşleme/şifreli aktarım/doğrulama/idempotency/geri dönüş akışı.
6. Kurulum/teslim: temiz Compose (mevcut ve ayrı PostgreSQL) + systemd/CA testi; güncel kılavuzlar; Windows NSIS/ZIP, Linux deb/AppImage ve servis paketi; SHA256 ve paket gizlilik kontrolleri.

Aşağıdaki bölüm önceki dev.2 kontrol noktasıdır; yeni durum ve kanıtlar için yukarıdaki bölüm esas alınır.

---

# TermTerm 0.4.1 — geliştirme ve doğrulama durumu

7 Ekim 2026. Dal: `codex/feature-team-personal-auth`, taban: v0.4.0. Masaüstü/servis/istemci: **0.4.1-dev.2**.

**Tam 0.4.1 planı henüz tamamlanmadı; yayın onayı istenecek aşamada değil.** Main, tag, GitHub Release ve güncelleme kaynağı değiştirilmedi. 0.5.0-dev.12 çalışma alanı korunuyor.

## 7 Ekim — kalıcı günlük ve güvenli yeniden gönderim

- `.ttcache` sürümü3: şifreli kayıt/izin görüntüleri, değişmez işlem taslakları ve kalıcı cihaz sırası. Kayıt ve günlük aynı SQLite transaction'ında; eski known-host verileri migration ile korunur. İleri format değiştirilmeden reddedilir.
- Önceki işi silmeden sıraya ekleme, yerel CAS,24 saat sınırı ve saatin geri alınmasıyla yazmanın kapanması uygulandı. Kayıt başına yetki zamanı ayrıca sınırlandırılır; başka kaydın yenilenmesi eski yetkiyi uzatmaz.
- Sunucu makbuzu takım/hesap/cihaz ile sınırlıdır. Kayıp yanıtta işlem kimliği ve ilk gönderim korunur. Yeni gönderim alıcıları güncel yetkiden alınır.
- Yetkisi kaldırılan yazarın eski düzenleme makbuzuyla yalnız aday teslimi yapabildiği yol eklendi. Kaynak okunmaz/değiştirilmez; aday güncel karar yetkililerine şifrelenir. Ret sonrası erişilebilir kayıt kaldırılır, taslak ayrı şifreli günlükte kalır.
- Çözülen karar yerel kuyruğa yansır. Kaybeden sürümün sonraki düzenlemeleri sessizce uygulanmaz; ayrıca karar teslimine gider. Son1000 yerel sonuç makbuzu korunur; eski sonuçlar100'lük partilerle temizlenir. Bekleyen işler temizlenmez.
- Masaüstünde kalıcı cihazla giriş, indirilen kayıtların şifreli önbelleği ve mevcut kayıt düzenlemelerinin günlüğe yazılması bağlandı. Bekleyen sayı/en eski zaman ve **Şimdi eşitle** görünür. Açık/görünür çalışma alanı15 saniyede sınırlı gönderim dener; hata başarılı kayıt diye gösterilmez.
- **Tam çevrimdışı Team henüz tamamlanmadı:** çevrimdışı ilk ekran açılışı, token yenileme süresi dolmuş uzun kesintiler, önbellekteki SSH kimlikleriyle bağlantı ve çevrimdışı yeni kayıt oluşturma açık. İlk giriş hâlen çevrimiçi gerekir.

Yeni kanıt: Windows ve Linux istemci16/16, Linux servis3/3 birim testi geçti. Gerçek PostgreSQL aday teslimi/hesap izinleri geçti. Son Rust HTTPS koşusu44,35s: yeniden açılış sonrası iki düzenleme, eşzamanlılık, karar sonrası alt değişikliğin ayrıca teslimi, gerçek şifre çözme ve masaüstü servis sınırı geçti.

Windows native son koşu **12/12 başarılı,2m52,8s**: önceki SSH/SFTP/tünel/izin/kurtarma testleri ve yeni kalıcı günlük/yeniden giriş/karar çözümü. Önceki koşuda otomasyon bağlantısı kesilmişti; o koşu başarısız olarak korunur. Arayüzde10 senaryo toplu koşuda, yeni günlük senaryosu test saatinin kurulma sırası düzeltildikten sonra odaklı1/1 geçti;11/11 tek koşu iddiası değildir. `windows-journal.png` gerçek Windows görüntüsü; `journal-pending.png` sentetik veriyle hata/bekleme arayüzü. İkisi görsel olarak incelendi.

## Masaüstü kullanım düzeni

Team, kurulan Windows/Linux uygulamasının içinde çalışır. HTTPS yalnız Rust ile Team Service arasındaki iletişimdir. PostgreSQL parolası masaüstü kullanıcısına verilmez; SSH/SFTP bilgisayardan hedefe gider.

- Normal TermTerm ile aynı üst başlık, sol menü, host/klasör kartları ve liste görünümü kullanılır.
- Girişte erişilebilen tek kasa varsa doğrudan açılır; birden fazla kasa seçiciden değiştirilir.
- Klasör tek tıkla açılır; host tek tıkla sağ bilgi panelini, çift tıkla bağlantıyı açar. İşlemler sağ tıktadır.
- Üstte Kasa, SFTP ve her SSH bağlantısı için ayrı sekme vardır. Sayfa değiştirmek açık terminali yeniden oluşturmaz.
- Ctrl+Tab/Ctrl+Shift+Tab üst sekmeleri, önceki/sonraki terminal ve kapatma kısayolları ortak tanımları kullanır. Son terminal kapanınca Kasa açılır.
- Team menüsünde üyeler, izinler, bekleyen kararlar ve kurtarma kurulumu bulunur. Bunlar Hosts ekranına yığılmaz.
- Aynı SFTP bileşeni ve aynı xterm terminali/Stats bileşeni kullanılır. Özel kimlik ve tünel ekranları ayrı günlük menülerdedir.
- Yalnız indirilebilen yetkili hostlar sayılır. Sadece gezinmek için açılan üst klasörlerde gizli kardeş sayıları gösterilmez.

## Uygulanan servisler

| Alan | Durum |
|---|---|
| Veri modeli | `termterm_team_v2`, 001–036 migration; hesap, profil, kimlik, host/klasör/hop, izin, anahtar zarfı, işlem/karar/geçmiş ayrı ilişkisel tablolar |
| Hesap | Lider oluşturma/reset/disable, 21 karakter tek gösterimlik geçici parola, 24 saat süre, ilk giriş aktivasyonu |
| Oturum | bcrypt12, 15 dakika access/30 gün dönen refresh, yalnız token hash, replay iptali; token Rust içinde |
| Yetki | Sınırlı DB rolü, açık fonksiyon listesi, RLS; sahibin klasör/host read/connect/edit planı, etki önizlemesi ve atomik anahtar paylaşımı |
| Kişisel profil | Hosta özel → kişisel varsayılan → kullanıcıya sor; kilitli ortak kullanıcı adı; şifreli kişisel kullanıcı adı |
| Bağlantı | Gerçek doğrudan SSH, SFTP, yerel/dinamik/uzak tünel; aynı kişisel kimlik çözümlemesi, yanlış anahtarda sessiz fallback yok |
| Yetki kaldırma | Açık SSH/SFTP/tünelde tekrar denetim; açık ret bağlantıyı kapatır. Ağ kesintisi yetki reddi sayılmaz |
| Şifreleme | Argon2id/XChaCha20-Poly1305, sealed-box zarfları; takım/hesap/kayıt/revizyon/amaç bağlı bağlam |
| Yerel güven kaydı | Kullanıcı kimliğine sarılan anahtarla şifreli SQLite `.ttcache`; servis+takım+hesap ayrımı; oturumlar arasında sunucu parmak izi saklama |
| Kararlar | CAS/replay denetimi, şifreli aday teslimi, alan karşılaştırması; karşılaştırma sonrası değişen revizyonu reddetme |
| Geçmiş | Güncel + önceki10, geçici sürüm kullan/güncele dön, kalıcı geri yüklemeyi yeni revizyon olarak yazma, dönüş noktası sabitleme |
| Kurtarma | Şifreli şirket dosyasıyla liderin yeniden doğrulanması, gerekçe, alıcı/public key önizlemesi, özel kimliklerin tüm saklanan sürümlerini yeniden sarma; alıcı cihazında gerçek çözme ve bütünlük kontrolü sonrası onay. İlerleme/iptal/yeniden deneme; parola ve anahtarlar Rust içinde |
| Reset sonrası profil | Eski kişisel kullanıcı adı açılamıyorsa açık uyarı ve ayrı onayla yeniden girilebilir. Önceki şifreli içerik son 10 arşivde korunur; eski metnin kendiliğinden kurtarıldığı iddia edilmez |
| Yetki devri | Sahip, kapsam yöneticisine kaynak ve izin bazında sınır verir. Yönetici kendi yetkisini artıramaz, alt kaynak engelini üst klasör izniyle aşamaz; devrin kaldırılması bekleyen planı geçersiz kılar |
| Ortak kimlik | Özel kaynaktan bağımsız şifreli kopya; kapsam/alıcı önizlemesi, ayrı connect/reveal/export izinleri, kaynak sahibi/sahip tarafından paylaşımı kaldırma. Kopyanın kaydı kaybolan yanıttan sonra tekrar okunabilir |
| Ortak kimlik güncelleme | Aynı kayıt kimliğinde yeni şifreli sürüm; mevcut içeriği koruma veya aynı türde özel kaynaktan kopyalama; alıcı önizlemesi, revizyon/izin/kimlik denetimi, atomik uygulama ve tekrar makbuzu. Çıkarılan alıcıların eski zarfları silinir; yeni alıcıya yalnız yeni sürüm verilir. Güncel + önceki10 sürüm korunur |
| İşlem geçmişi | Sahibe özel, 100 satırlık kararlı sayfalama; 90 günlük varsayılan saklama, otomatik ve elle sınırlı temizlik. İşlem başına en fazla 1.000 audit ve 100'er eski izin/paylaşım önizlemesi; bekleyen kararlar ve uygulanmış paylaşım makbuzları korunur |
| Bakım ve alan | Servis açılışında ve her dakika süresi gelen bir takımın sınırlı işi işlenir; tamamlanan takım 24 saat sonra, dolu parti 1 dakika sonra tekrar ele alınır. Son bakım zamanı ve şifreli kayıt/anahtar verisinin mantıksal boyutu görünür; fiziksel DB/indeks boyutu değildir |
| Lider onaylı geçmiş temizliği | 20'li sayfalama, en fazla100 sabitleme için önizleme/ayrı onay/iptal; seçim değişirse reddetme, diğer kullanıcıların sabitlemesini koruma, tekrar makbuzu ve audit. Sabitlemeler yaşa göre kendiliğinden silinmez |
| Fiziksel alan | DB toplam boyutu ile Team şemasının tüm takımlara ait tablo/indeks boyutları ayrı gösterilir; yalnız seçili takıma aitmiş gibi sunulmaz |

Geçici geçmiş seçimi hesap bazındadır. Eski host içeriği kullanılsa bile güncel bağlantı yetkisi ve zorunlu kullanıcı adı politikası uygulanır. Host geçmişi kimlik/anahtarları veya uzaktaki dosya değişikliklerini geri almaz.

Geçici sürümden güncele dönmek veya sürüm değiştirmek, artık sabitlenmeyen ve son 10 dışındaki sürümleri aynı transaction içinde temizler. Başka kullanıcının seçimi ve kalıcı geri yükleme dönüş noktası korunur. Terk edilmiş hesap sabitlemeleri otomatik düşmez; lider önizleyip açıkça kaldırabilir.

Ortak kimlik güncellemelerinde oluşturucu/takım sahibi yetkili kapsamda çalışır. Yetki devri olan yönetici mevcut sır okuma hakkı ve kapsam yetkileriyle işlem yapabilir; kendini yükseltemez veya yeni reveal/export yetkisi dağıtamaz. Şirket sahibi/oluşturucu zorunlu alıcıları önizlemede görünür. Uygulanmış güncelleme makbuzları korunur; süresi dolmuş uygulanmamış güncelleme/temizlik önizlemeleri bakımda100'lük partilerle silinir.

## Doğrulananlar

- Son gerçek Windows Tauri/WebView + HTTPS + PostgreSQL koşusu: **11/11 geçti (1 dakika 57 saniye)**. Yeni ortak kimlik güncelleme ekranı → revizyon2 → gerçek RSA4096 SSH/export; liderin sabitleme önizlemesi/onayı; önceki hesap/ACL/karar/geçmiş/audit/kurtarma kontrolleri dahil.
- Gerçek PTY117×43, SFTP yükleme/indirme SHA256 eşitliği, üç tünel türünde gerçek trafik, yetki kaldırılınca kapanma.
- Güncel host portu kullanılamaz durumdayken eski sürümle gerçek SSH; çıkış/giriş sonrası şifreli known-host kaydı korunur. Kasa/SFTP/terminal gezinmesinde bağlantı yeniden açılmaz.
- Playwright'ta **9 senaryo geçti**; yeni sabitleme senaryosunun yanlış menü seçicisi düzeltildi ve ayrıca **1/1 odaklı test geçti**. On senaryonun tamamında başarılı sonuç vardır; tek koşuda10/10 iddia edilmez. Ortak kimlik güncellemede eski önizleme hatası/iptal ve liderin ayrı onayı dahil. Veri katmanı sentetiktir, gerçek DB testinin yerine geçmez. Önceki frontend birim sonucu **25/25**.
- Aynı Playwright koşusunda gerçek Ctrl+Tab/ters sekme tuşları da geçti. Native sürücünün özel tuşlarda modifier kaybetmesi nedeniyle native kısayol kontrolü açık modifier içeren DOM olaylarıyla yapıldı; fiziksel klavye testi olarak sayılmaz.
- Önceki Windows masaüstü Rust sonucu **53 geçti / 7 ignored**. Ignored senaryolar geçti sayılmaz. Bu geliştirmede Linux servis3/istemci8 birim testi geçti; ortak kimliğin yeni sürümünü gerçek şifrelemeyle açma ve alıcı çıkarma dahil Rust HTTPS entegrasyonu1 geçti (32,81 saniye).
- PostgreSQL hesap, kişisel kimlik, kaynak, ACL, eşzamanlılık, geçmiş ve indeks testlerinin tamamı 001–019 üzerinde geçti.
- Ek gerçek PostgreSQL delegasyon, paylaşım, audit saklama/izolasyon testleri geçti. 025 sonrası sorgu planında host0,144ms, ACL0,148ms, audit cursor0,077ms, zaman sıralı audit0,126ms; cursor için planner primary-key indeksini seçti. Bunlar bütün sistem performans vaadi değildir.
- Gerçek Rust HTTPS/şifreleme entegrasyonunda yönetici anahtar dağıtımı, ortak kopyanın üyede açılması, reveal/export reddi, özel kaynağın değişmesinden bağımsızlık ve aynı paylaşım sonucunun tekrar okunması geçti.
- Kurtarmada yanlış şirket anahtarı/parola, liderin alıcı yerine onay verememesi, yeniden sarma öncesi doğrulama reddi, alıcının gerçek çözmesi ve onay makbuzu tekrarı geçti. Reset sonrası eski kullanıcı adının açılamadığı bildirildi; ayrı onayla yeni şifreli profil kaydı doğrulandı.
- 027–029 için gerçek PostgreSQL: profil arşivinin 10 kayıtla budanması, doğrudan tablo erişimi reddi, bakım yetkisi/zamanlaması, geçmiş seçimi bırakılınca anında budama ve diğer kullanıcının sabitlemesini koruma geçti. Güncel hesap/fonksiyon izin listesi testi geçti.
- Servisin gerçek başlangıcında otomatik bakımın süresi dolan laboratuvar kaydını temizlediği doğrulandı. Bakım tetiklemek için dış HTTP endpoint bulunmadığı da kontrol edildi.
- 030–033 SQL kontrolleri: güncellemede tekrar/tahrifat/eksik zarf/iptal/eski revizyon reddi, özel kaynak bağımsızlığı,11 sürüm sınırı, çıkarılan alıcının eski zarflarının silinmesi; kapsam yöneticisinin sır/export hakkını artıramaması ve devrin kaldırılmasıyla planın geçersizleşmesi geçti. Süresi dolmuş güncelleme önizlemeleri temizlenirken uygulanmış makbuzlar korundu.
- Liderin pin temizliğinde salt okunur üyenin reddi, seçimin değiştiği önizlemenin reddi, iptal, diğer hesabın korumasını sürdürme, son korumada budama ve tekrar makbuzu gerçek DB'de geçti. Fiziksel boyut API'si native ekranda da doğrulandı.
- Yeni 10.000 kayıt fixture'ında `revision_pin_id` ile pin sayfalama0,123ms, `credential_updates_pending` ile plan sorgusu0,070ms. Planlayıcı seçenekleri zorlanmadı; ölçümler genel yük/arayüz performansı vaadi değildir.
- Windows SSH senaryosuna ortak RSA4096 kopyası, gerçek anahtar dosyası export eşitliği, var olan dosyayı ezmeme ve paylaşım kaldırılınca açık SSH'nin kapanması eklendi; geçti. Native işlem geçmişi okuma/temizleme de geçti. Test düzeneğinde modal kapanışını beklemeyen çağrı ve hatalı düğme seçicisi düzeltildi; son odaklı koşu 1/1 başarılı.
- 10.000 hostluk sorgu planı fixture'ında klasör0,139ms, ACL0,409ms, audit0,072ms. Bu UI hız testi veya tüm sistem yük testi değildir.
- Windows ekranları: `artifacts/team-v2/windows-folder.png`, `windows-members.png`, `windows-ssh.png`, `windows-sftp.png`, `windows-history.png`, `windows-sharing.png`, `windows-shared-update.png`, `windows-pins.png`, `windows-audit.png`, `windows-recovery.png`. Yeni ortak kimlik güncelleme, pin onayı ve alan istatistikli audit görüntüleri görsel olarak incelendi. Görseller yalnız ayrılmış laboratuvar kayıtlarıdır.

## Yayın öncesi açık işler

1. Her hopta kişisel profil ve bağlantıya özel bağımlılık anahtarları. Yeni Team v2'de hop bulunan kayıt şimdilik güvenli biçimde reddedilir; kişisel kasa chain davranışı değişmedi. Team v2 Mosh/ileri kimlik adaptörleri de tamamlanmadı.
2. Kalıcı günlük ve masaüstü kuyruk göstergesi uygulandı. Tam çevrimdışı giriş, uzun kesintide token yenilemesi, önbellekteki SSH kimlikleriyle bağlantı ve yeni kayıt oluşturma açık; uçtan uca offline kasa henüz tamamlanmadı.
3. Şirket kurtarma anahtarı döndürme ve ortak kimlik kurtarma kapsamı. Özel kimlikler için ekran/yeniden sarma/alıcı doğrulaması tamamlandı. Eski kişisel kullanıcı adı otomatik kurtarılmaz; açık onayla yeniden giriş ve şifreli arşiv vardır. Arşiv geri yükleme ekranı henüz yoktur.
4. Klasör taşıma/referans önizlemesi; eski snippet/workspace kayıtlarının yeni Team servis akışları.
5. v0.4.0 yedek/önizlemeli aktarım/geri dönüş; eski Team girişinin ancak doğrulanmış geçişten sonra kaldırılması.
6. Temiz Linux Compose/systemd/CA kurulum testi; Windows/Linux kurucu, taşınabilir paket, servis paketi ve nihai doğrulama.

macOS, donanım anahtarları, üretim kimlikleri ve erişilemeyen ortamlar test edilmedi. Uzun izleme yapılmadı. E2E test özellikli yürütülebilir dosya dağıtım paketi değildir. Yeni kararlı yayın yalnız tamamlanma raporu ve sürüme özel kullanıcı onayından sonra yapılabilir.

Yerel Windows geliştirme derlemesi: `pnpm team:build` başarılı; `src-tauri/target/debug/termterm.exe`, `local.termterm.desktop.teamdev` kimliği, E2E özelliği olmadan. Bu dosya kararlı kurucu/yayın değildir.
