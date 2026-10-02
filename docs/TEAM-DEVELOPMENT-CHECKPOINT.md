# Team geliştirme — devam notu

23 Eylül 2026. Kullanıcının devam isteği üzerine WSL laboratuvarıyla çalışma sürdürüldü. Bu belge kararlı yayın onayı değildir.

- Dal: `feature/team-access-history`, develop tabanı `5695ae0c1ffcbdb8e210e4cbc27b131e2ba05249`.
- Geliştirme sürümü: **0.4.0-dev.1**. Önceki duraklatma kaydı: `2bc2d07`.
- Kararlı 0.3.5, main, kullanıcı kasaları ve güncelleme kaynağı değiştirilmedi. Tag, GitHub Release, kaynak push'u, Mac/GitHub CI işi yapılmadı.

## Tamamlanan çalışma

- `migrations/team/001–005`: hesap/oturum, sınırlı RPC izin listesi ve RLS; ACL/delegasyon; CAS işlemleri, karar, kayıt geçmişi/pinler ve audit; kısa ömürlü ortak terminal. 002–005 tekrar uygulanabilir. Geliştirme ve e2e DB'leri güncellendi.
- Yerel şifreli önbellek: atomik sıralı kuyruk, bağımlılık sırası, yeni bekleyen kayıtların görünümü, 24 saat sınırı, saat geri alma reddi, hot-journal kurtarma ve yanlış parolada dosyayı koruma.
- Yetki iptali: veri/bağlantı malzemesi temizliği ve kasa bağlantılarını kapatma; düzenlemesi iptal olmuş kullanıcının aday/audit teslimi; son başarılı alıcı listesinin bu sınırlı teslim için korunması; teslim edilmiş gizli taslakların temizliği.
- Online karar/taşıma/restore niyetlerinin kayıp yanıt sonrası kurtarılması; bağımlılık anahtarlarının yeniden sarılması; host kapsamlı edit; yeni bağımlılıkta yetki genişletmeyi engelleme; üyeyi tekrar eklerken eski izinleri canlandırmama.
- Geçmiş: güncel + 10, geçici kullanım ve güncele dönüş, kalıcı restore/dönüş pini, silinmiş kayıt geri yükleme, eksik referans uyarıları.
- Native SSH/chain, SFTP, import/export/backup, API Bridge ve ortak terminalde Team kontrolü. Seçili yedek bağımlılıkları ve anahtar dosyası dahil etme/eksik dosya raporu.
- Türkçe Team arayüzü; matris, okunur alan karşılaştırması, geçmiş, taşıma önizlemesi; kişisel senkronizasyon akışından ayrılmış Team modu.
- Normal ve e2e uygulama kimlikleri ayrı. Yerel deneme paketini `scripts/package-team-dev.ps1` üretir; kararlı yayın yapamaz.

## Kanıt ve dosyalar

Kesin sayılar ve ortam sınırları [doğrulama raporundadır](VALIDATION_TEAM_0.4.0-dev.1.md). [Kullanım ve DB kılavuzu](TEAM_GUIDE_TR.md).

- Genel Windows Rust: 46 geçti / 7 atlandı; yeni seçili yedek testi hedefli koşuda geçti. Windows/Linux Team: 8 giriş noktası geçti (biri çökme testi yardımcısı); gerçek TLS PostgreSQL dahil.
- Frontend: TypeScript/Vite ve 21 Vitest testi geçti.
- Gerçek SQL: token rotation, SQL/rol/GUC reddi, ACL/devir, host-only edit, iki istemci CAS/silme yarışı, idempotency, karar, history/pin, iptal edilmiş üyeden aday/audit teslimi, üyeyi tekrar eklemede sıfır erişim.
- Native: 5 senaryo geçti (2 dk 21,8 sn). DB gerçekten kapatılıp açıldı; offline edit + SSH, geri gönderme/login sonrası kalıcılık, gerçek chain/SFTP, iptalde SSH kapanması, karar/merge, geçmiş/move/restore, yedek-import/hash kontrolü.
- Ek odaklı native dosya-yedeği kontrolü 1/1 geçti (16,7 sn): `tests/desktop/team-backup.mjs`; ana koşunun `.lab/team-*/demo.json` yolunu `TERMTERM_TEAM_DEMO` ile alır.
- Görüntüler: `artifacts/team-permissions.png`, `team-decisions.png`, `team-history.png`, `team-audit.png`.
- Normal (e2e içermeyen) Windows uygulaması derlendi ve açıldı; pencere yanıt veriyor. Paket ZIP CRC, SHA-256, x64 PE ve çalışma dosyası denetimleri geçti. ZIP 241.912.475 bayt; özel kasa/profil içermez. Uygulama açık bırakıldı; PID `.lab/team-demo-process.pid`.
- Üretilen paket: `artifacts/team-0.4.0-dev.1/`. Özel yerel giriş kılavuzu `.lab/TEAM_DEMO_LOCAL.txt`; paket/Git dışında tutulur.

## Sonraki çalışma sınırları

1. Kullanıcının Team akışını denemesinden sonra geri bildirimleri bu dalda işle. Yayın için açık sürüm onayı gerekir.
2. İki ayrı bilgisayarda native ortak terminal konuk görünümü henüz denenmedi; mevcut kanıt iki gerçek hesaplı SQL/Rust taşıma + native oturum sahibi yoludur.
3. Linux ortak servisler geçti; Linux GUI/Team kurucusu, macOS, donanım anahtarları ve üretim sunucusu test edildi sayılmaz. Mac/GitHub CI'ı kullanıcı istemeden başlatma.
4. 90 günlük audit bakımı politika ekranından sınırlı çağrıyla çalışır; bağımsız DB zamanlayıcı kurulmadı. Import yerelde atomik, uzak tarafta kayıt bazındadır.
5. SMTP/e-posta doğrulama/sıfırlama bu denemenin dışındadır. Önceki sürümün FIDO2/Hello ve üretici import sınırları geçerlidir.

## Yerel devam bilgileri

- WSL Ubuntu-24.04, PostgreSQL 18 `termterm`, 55432; `main:5432` korunur. Yalnız `termterm_team_dev` / `termterm_team_e2e` bu işe ait.
- Profiller `.lab/team-connection.json`, `.lab/team-windows.json`, demo profili `.lab/team-demo-profile.json`; TLS `.lab/ca.crt`. Parolaları/token'ları loglama veya Git'e ekleme.
- Test/demo hesapları `.lab/team-*/demo.json`; en son tamamlanan tam koşu `team-muect55x`. SSH lab `127.0.0.1:22222`.
- Windows target `%LOCALAPPDATA%/TermTerm-build/folder-fix-20260922/src-tauri/target`; PATH `%USERPROFILE%/.cargo/bin`, `CARGO_BUILD_JOBS=2`, `TERMTERM_RELEASE_CHANNEL=development`.
- WSL'yi açık tutan görev yardımcısı `scripts/lab-keepalive.sh` devam eder; PID `.lab/team-keepalive.pid`. WSL'yi global kapatma veya başka uygulamanın sürecini durdurma.
- Linux yardımcı `.tools/team-linux-check.sh`, Ubuntu22.04 chroot `/root/termterm-build/jammy`, çalışma `/work`.
- Native loglar `.tools/wdio-team` ve `.tools/wdio-team-backup`.

```powershell
$env:PATH="$env:USERPROFILE/.cargo/bin;$env:PATH"
$env:CARGO_TARGET_DIR=Join-Path $env:LOCALAPPDATA 'TermTerm-build/folder-fix-20260922/src-tauri/target'
$env:CARGO_BUILD_JOBS='2'
$env:TERMTERM_RELEASE_CHANNEL='development'
$env:TERMTERM_TEAM_PROFILE=Join-Path $PWD '.lab/team-windows.json'
pnpm test:unit
cargo test --manifest-path src-tauri/Cargo.toml --lib team:: -- --include-ignored
pwsh -File scripts/package-team-dev.ps1
```

Test executable'ını teslim uygulaması olarak kullanma; normal deneme derlemesi `e2e` özelliği olmadan hazırlanır. Laboratuvarın gerçek çalışması, üretim güvenlik onayı anlamına gelmez.
