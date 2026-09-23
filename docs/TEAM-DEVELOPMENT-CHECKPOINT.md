# Team geliştirme — devam notu

23 Eylül 2026. Kullanıcının isteğiyle çalışma durduruldu. Bu belge tamamlanma veya yayın raporu değildir.

- Dal: `feature/team-access-history`, taban: develop `5695ae0c1ffcbdb8e210e4cbc27b131e2ba05249`.
- Geliştirme sürümü: `0.4.0-dev.1`.
- Kararlı 0.3.5, kullanıcının kasaları ve güncelleme kaynağı değiştirilmedi. Release, tag, main birleştirmesi veya GitHub/macOS derleme işi yapılmadı.
- Kullanıcı geri dönene kadar geliştirme, derleme ve test çalıştırılmayacak.

## Kaydedilen çalışma

- `migrations/team/`: ayrı Team şeması, hesaplar, bcrypt parolalar, hash'lenmiş oturum anahtarları, üyeler, kaynak izinleri, yetki devri, kayıtlar/anahtar zarfları, işlem kuyruğu, çakışmalar, geçmiş, sabitlenmiş sürümler ve audit tabloları. Uygulama rolüne yalnız beş açık RPC izni veriliyor; içerik tablolarında ek RLS var.
- `src-tauri/src/team/crypto.rs`: kullanıcı kimlik anahtarı, sealed-box anahtar dağıtımı, sürüme bağlı ayrı veri/sır anahtarları, XChaCha şifreleme ve metadata doğrulaması.
- `src-tauri/src/team/cache.rs`: ayrı format 2 şifreli `.ttteam` önbelleği, dosya kilidi, sıralı işlem günlüğü, 24 saat düzenleme sınırı, izin temizliği ve yerel bağlantı tercihleri. Kişisel kasa biçimi değiştirilmedi.
- `src-tauri/src/team/mod.rs`: hesap/oturum, Team RPC, yerel kuyruk eşitleme, izinli bağlantı bağımlılıkları, geçmiş/geçici kullanım/geri yükleme, karar ve taşıma servisleri. Token'lar frontend'e verilmiyor.
- Mevcut SSH/chain, SFTP, import/export/backup ve kasa komutlarına Team kontrolü eklendi. Bağlantı/SFTP audit özeti üzerinde çalışma var.
- `src/Team.tsx`, `src/team.css`: Türkçe Team giriş/kayıt, ortak kasalar, üyeler ve matris, bekleyen kararlar, geçmiş, bağlantı/politika, sürüm kullanımı ve taşıma önizlemesi. App/Onboarding bağlantıları eklendi.
- Ayrı geliştirme/e2e uygulama kimlikleri, laboratuvar betiği, gerçek PostgreSQL/Rust/native testleri eklendi.

## Gerçekten çalıştırılan kontroller

Bu sonuçlar ara kaynak sürümlerine aittir. Aşağıdaki son değişikliklerden sonra bütün ağaç yeniden derlenmedi; son dosyaların tamamı geçti denemez.

- Gerçek PostgreSQL, TLS ve sınırlı uygulama rolü: hesap/giriş, token/GUC taklidi ve doğrudan SQL reddi, beş RPC izin listesi, klasör devralması, tek-host gezinme yolu, açık engel, yetki devri sınırı, CAS/idempotency, karar, geçmiş budama/sabitleme ve üyeliği kaldırılmış kullanıcının aday teslimi geçti (`tests/team_postgres.py`).
- Windows Rust: üç Team birim testi ve gerçek PostgreSQL şifreleme/ACL/geçmiş testi geçti. Gerçek DB testi anahtarların üyeye yeniden sarılması, sırların gizlenmesi ve izin reddini içeriyor.
- Windows native WDIO: `tests/desktop/team.mjs` ikinci koşuda 3/3 geçti (yaklaşık 1 dakika 32 saniye). Gerçek Team oluşturma, sınırlı operatör, bağımsız jump bağlantısının reddi, izinli gerçek SSH chain ve SFTP dosya bütünlüğü, 12 güncellemede 11 sürüm ve geçici sürüm/güncele dönüş denendi.
- Linux Ubuntu 22.04 chroot: kopyalanmış ara kaynak üzerinde 4 Team Rust testi, gerçek TLS PostgreSQL testi dahil, geçti. Derleme 8 dakika 8 saniye, testler 6,19 saniye. Mevcut `updater.rs` unreachable-expression uyarısı var. Linux GUI/yeni paket doğrulanmadı.
- TypeScript ve Vite ara kaynakta geçti.
- Gerçek native görüntüler: `artifacts/team-vaults.png`, `artifacts/team-permissions.png`, `artifacts/team-audit.png`. Üye görüntüsünde veri yüklemesinin bitmesini bekleyen daha iyi çekim gerekli.
- macOS, donanım anahtarları ve üretim sunucuları test edilmedi. Uzun süreli izleme yapılmadı.

## En son değişen, henüz yeniden doğrulanmamış parçalar

- Kuyruk receipt temizliği: teslim edilmiş işlemdeki gizli içeriği ve hazırlanmış/karar durumlarını temizler; işlem kimliği/sıra/zaman bilgileri kalır.
- Kuyruk revizyon rebasing yalnız aynı başlangıç revizyonu için yapılır.
- Kasa bazında iptal ile hesap oturumu iptalinin ayrılması; iptalde yerel known-host temizliği.
- Bağlantı profilinin admin/doğrudan tablo erişimi/executor üyeliği açısından reddedilmesi, RPC kilidi, kimlik bilgilerini Drop sırasında sıfırlama.
- Şifreli kayıt kind/parent metadata eşleşmesi, bağlantı sırasında ağ kesintisinde önbelleğe dönüş, doğrulanmış uzak kasa seçimi.
- Sınırlı `deliver_pending` yoluna audit teslim dalı; yeni kayıt çatışmasında parent kapsamı.
- `tests/team_rpc.py`: ikinci gerçek DB istemcisiyle üye/ACL değişikliği için yardımcı betik. Henüz çalıştırılmadı.

## Devam sırası ve açık eksikler

1. Son değişiklikleri incele, formatla ve derle. Mevcut e2e binary kaynak ağacından eski; teslim paketi gibi kullanma.
2. Yalnız izole Team DB'lerini güncel migration'lara getir. `termterm_team_dev` erken 001/002 sürümünde; e2e'ye 003/004 ve bazı 002 yamaları uygulandı, son `deliver_pending` değişikliği uygulanmamış olabilir. Kullanıcı/personal DB'lerini silme veya sıfırlama. Laboratuvar kurulum betiği mevcut şemaya bütün yükseltmeleri otomatik uygulamıyor.
3. `flush_activity` için üyeliği kaldırılmış kullanıcının sınırlı audit teslim yolunu tamamla; bağlantı kapanış/SFTP özetleri kaybolmasın. `deliver_revoked` şu an taslaklara odaklı.
4. Yanıtı kaybolan çevrimiçi karar/taşıma niyetlerinin yeniden başlatma kurtarmasını tamamla. Açık sync çağrısındaki auth hatasında da cache/bağlantı iptalini uygula; bir kasadan çıkarılmayı bütün hesap iptali sayma.
5. Önbellek hot-journal/kapanma kurtarmasını ekle ve doğrula. Snapshot sonrasında henüz gönderilmemiş yeni kayıtlar kuyrukta kalıyor fakat görünümden geçici kaybolabilir; pending overlay'i düzelt.
6. Import batch ilişkilerini bağımlılık sırasına koy, döngü/eksik referansları doğrula; yerel atomiklik ve sunucuya kayıt bazında aktarım davranışını açıkça ele al. Geçmiş sürüm bağımlılık/izin uyarıları eksik.
7. Gerçek iki istemciyle aynı anda değişiklik, silme-düzenleme çatışması, tekrar gönderim, token refresh/revoke testlerini tamamla. Native karar ekranı/alan birleştirme, kalıcı restore/dönüş noktası, taşıma etkisi testleri ekle.
8. İkinci DB istemcisiyle açık SSH sırasında izin kaldır; önbellek temizliği ve bağlantı kapanışını doğrula. Saat kontrollü 24 saat testinde düzenleme durmalı, mevcut izinli bağlantı sürmeli.
9. UI'da izin verilmeyen işlemleri kapat/gizle. Normal host/klasör menüsündeki taşıma, Team taşıma önizlemesine bağlanmalı. Legacy kişisel sync/portable copy/API Bridge/ortak terminal menüleri Team'de henüz uyumlu değil: backend kapalı kalıyor, bunların Team işlevleri tamamlandı denemez.
10. Kalıcı restore dönüş pininin belirgin UI'sını ve Güncele dön davranışlarını tamamla. Matris rol şablonları ile gerçek kaynak ACL'lerinin ayrımı net kalsın.
11. PostgreSQL fonksiyon güvenliği, parent/dependency doğrulaması, zarf tamlığı, export/backup/import/IPC atlatma yollarını yeniden incele. İçerik RPC'lerindeki uygulama kontrolünün RLS ek savunmasıyla uyumunu doğrula.
12. Kişisel kasa/import/SSH/SFTP/sync kısa regresyonlarını, frontend testlerini ve Linux ortak servis testini son kaynak üzerinde çalıştır. Mac/GitHub CI başlatma.
13. Ayrı kimlikli, **e2e özelliği olmayan** Windows geliştirme uygulamasını derle; matris, karar ve geçmiş gerçek ekran görüntülerini, kurulum/DB yönergesini ve dürüst doğrulama raporunu hazırla. Yayın yapma.

## Yerel devam bilgileri

- WSL: Ubuntu-24.04, PostgreSQL 18 `termterm`, port 55432. Yalnız `termterm_team_dev` ve `termterm_team_e2e` bu işe ait izole DB'ler.
- Sınırlı DB hesabı/profilleri: `.lab/team-connection.json`, `.lab/team-windows.json`; TLS CA `.lab/ca.crt`. Parolaları/token'ları loglama veya Git'e ekleme.
- Disposable native demo bilgileri `.lab/team-*/demo.json`; test kasaları/anahtarlar `.lab` altında. Bunlar commit edilmeyecek.
- Windows target: `%LOCALAPPDATA%/TermTerm-build/folder-fix-20260922/src-tauri/target`. `PATH`'e `%USERPROFILE%/.cargo/bin` ekle; `CARGO_BUILD_JOBS=2`, `TERMTERM_RELEASE_CHANNEL=development`.
- Linux test yardımcı betiği: `.tools/team-linux-check.sh`; chroot `/root/termterm-build/jammy`, çalışma `/work`. Bu betik önceki kaynak kopyasını kullandı.
- Native loglar `.tools/wdio-team`. Son Linux ve native test süreçleri tamamlandı. WSL'yi test boyunca açık tutan işe ait `lab-keepalive.sh` yardımcısı durduruluyor; WSL global kapatılmayacak.

Devamda kullanılacak komutlar (şimdi çalıştırılmayacak):

```powershell
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:CARGO_TARGET_DIR=Join-Path $env:LOCALAPPDATA 'TermTerm-build/folder-fix-20260922/src-tauri/target'
$env:CARGO_BUILD_JOBS='2'
$env:TERMTERM_RELEASE_CHANNEL='development'
$env:TERMTERM_TEAM_PROFILE=Join-Path $PWD '.lab/team-windows.json'
cargo test --manifest-path src-tauri/Cargo.toml --lib team:: -- --include-ignored
pnpm.cmd tauri build --debug --features e2e --config src-tauri/tauri.team-e2e.conf.json --no-bundle
$env:TERMTERM_BINARY=Join-Path $env:CARGO_TARGET_DIR 'debug/termterm.exe'
$env:TERMTERM_SPEC='./tests/desktop/team.mjs'
$env:TERMTERM_LOCALE='tr'
$env:TERMTERM_LOG_DIR='.tools/wdio-team'
node node_modules/@wdio/cli/bin/wdio.js run wdio.conf.mjs
# Son doğrulamadan sonra, test kancaları olmayan yerel geliştirme uygulaması:
pnpm.cmd tauri build --debug --config src-tauri/tauri.team-dev.conf.json --no-bundle
```

WSL yeniden çalışırken gerekli mevcut laboratuvar servislerini kontrol et. Test/derleme komutları kullanıcı devam dediğinde çalıştırılacak; bu kontrol noktası onaylanmış yayın değildir.
