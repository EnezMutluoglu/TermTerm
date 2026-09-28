# TermTerm 0.3.6 doğrulama raporu

28 Eylül 2026. Kullanıcı terminal ölçekleme düzeltmesinin kararlı sürüm olarak GitHub'da yayımlanmasını onayladı. Sürüm 0.3.5 tabanından ayrı `codex/fix-terminal-resize-036` dalında hazırlandı. `feature/team-access-history` ve 0.4.0-dev.1 içeriği dahil edilmedi. macOS/GitHub CI çalıştırılmadı.

## Düzeltme

SSH kimlik doğrulaması beklenirken `wait_close` ilk Resize iletilerini tüketiyor, uzak PTY sabit 100×30 açılıyordu. xterm ölçüsü değişmeyince yeni bir resize gelmiyordu. Bekleme artık en son ölçüyü saklar. Bağlantı kurulduğunda aynı ölçü olsa bile tekrar gönderilir. Mosh başlatma yolu da kaydedilen ölçüyü kullanır.

Frontend ölçümleri animation-frame ile birleştirilir; IPC çağrıları sıralanır ve en son bekleyen ölçü korunur. Gizli panel sıfır boyuta indirilmez. Pencere, font yükleme, Stats, panel ve sekme geçişleri ölçüm başlatır. Çok geniş ekranlarda 500 sütun sınırlaması 4096 satır/sütun üst sınırına yükseltildi. SSH kimlik doğrulama algoritmaları, anahtar doğrulaması, kasa biçimi ve Team kodu değiştirilmedi.

## Kontroller

| Kontrol | Sonuç |
|---|---|
| TypeScript/Vite | Windows ve Linux üretim derlemeleri geçti |
| Vitest | 23/23; boyut sıralama, son ölçüyü koruma, hata sonrası tekrar ve kapanış dahil |
| Tarayıcı terminal kontrolleri | 11/11; mevcut 10 terminal/kısayol testi ve yeni resize testi. Bağlantıda aynı ölçüyü yeniden gönderme, büyütme/küçültme, gizli sekmeden dönüş, Stats, bölünmüş görünüm ve 500 sütunu aşan alan doğrulandı. Bu katman sentetik IPC kullanır |
| Rust | Boyut bekleme/iptal testi Windows ve Linux'ta geçti |
| Gerçek Windows SSH | 4/4 geçti (14 saniye): ilk ölçü, büyütme/küçültme ve Stats; Vim tam ekran; pg_activity tam ekran ve alt menünün son satıra yerleşmesi; chain, split/focus ve gizli sekmeden dönüş. 1920×1080 Windows ekranında Vim 212×40, pg_activity küçük pencerede 29 satırdan büyüyünce 40 satıra geçti |
| Linux paketleri | Ubuntu 22.04 chroot: DEB kurulum, APT/ldd/dpkg kontrolü ve GUI açılışı geçti. AppImage gerçek GUI açtı. RPM sürüm/mimari/digest/içerik doğrulandı; RPM dağıtımında kurulum yapılmadı. GLIBC üst gereksinimi 2.34, Mosh çalıştı |
| Windows NSIS/portable | NSIS CRC, derlenen EXE ile eşit payload (Tauri bundle işareti hariç), Mosh/lisanslar ve çevrimdışı WebView2 doğrulandı. Portable ZIP ve kaynak arşivi SHA-256 manifest/allowlist denetimi geçti |
| İmzalar/kaynak | Windows kurucusu ve Linux AppImage imzaları mevcut public key ile doğrulandı; bir bayt değiştirildiğinde ikisi de reddedildi. Kaynak arşivindeki 281 dosya Git dosya listesi ve çalışma ağacıyla birebir karşılaştırıldı; özel veriler ve laboratuvar sırları taraması geçti |

Native test: `TERMTERM_SPEC=./tests/desktop/terminal-resize.mjs` ile WebdriverIO. `TERMTERM_BINARY`, ayrı `.e2e` uygulama kimliğiyle `--debug --features e2e --config src-tauri/tauri.e2e.conf.json --no-bundle` derlemesine işaret eder. Üretim paketlerinde E2E sürücüsü yoktur.

Test önkoşulu mevcut `scripts/ssh-lab.sh` laboratuvarıdır (127.0.0.1:22222, `.lab/ssh.json`). WSL'de Vim, Python 3 ve pg_activity 3.6.0 kullanıldı. PostgreSQL 18'in yalnız test kümesinde, 55432 portu ve `termterm_e2e` üzerinde `pg_monitor` yetkili geçici `termterm_resize_monitor` hesabı kullanıldı; CA doğrulaması açık tutuldu. Parola terminal komutuna yazılmadı; SSH lab kullanıcısına ait 0600 pgpass ve ayrı CA dosyası kullanıldı. Bu özel dosyalar depoya/arşive alınmadı. Test sırasında WSL canlı tutulmalıdır; systemd servisleri tek başına dağıtımın boşta kapanmasını önlemiyordu.

Native ölçüm, xterm'in gerçek cursor-position cevabıyla hücre ölçüsünü hesaplar; uzak `stty size`, Vim `&lines/&columns` ve pg_activity'nin son satırdaki menü konumuyla karşılaştırır. Üretim koduna test için ölçüm arayüzü eklenmedi. İlk test geliştirmesinde WebDriver seçicileri, kısmi çıktı okuma ve WSL yaşam döngüsü sorunları düzeltildi; başarı satırları son tamamlanmış koşuyu ifade eder.

## Sınırlar

- Windows x64 NSIS/portable ve Linux x64 DEB/RPM/AppImage. macOS/ARM/Alpine paketi yok; Mac CI kullanılmadı.
- Kullanıcının çalışan kurulu Windows uygulamasına ve kasalarına dokunulmadı. NSIS yeniden kurulum/kaldırma testi mevcut kurulu/açık uygulama nedeniyle tekrarlanmadı; kurucu arşivi ve gerçek payload doğrulandı.
- Native kabul testi Windows istemciden Linux laboratuvarına SSH ve chain kullandı. Kullanıcının üretim sunucuları, native Linux istemcisinden SSH resize, uzak Windows/macOS ve gerçek Mosh oturumu bu patch için tekrar denenmedi. Mosh derleme/çalıştırma ve ortak Rust boyut yolu kontrolü, gerçek Mosh oturum testi yerine sunulmaz.
- pg_activity test hesabı DB izleme yetkisine sahiptir; PostgreSQL süreç dosyasına işletim sistemi erişimi olmadığı için pg_activity'nin yerel sistem sayaçları devre dışıdır. Bu test tam ekran yerleşimini ve yeniden boyutlandırmayı doğrular.
- Önceki FIDO2/Hello, seri port ve üreticiye özel şifreli import sınırları devam eder. Uzun yük/Stats testi tekrarlanmadı.
- Windows Authenticode sertifikası yok; updater imzaları mevcut anahtarı kullanır. DEB/RPM güncellemesi paket yöneticisiyle elle yapılır.
