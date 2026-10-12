# TermTerm 0.4.2 doğrulama raporu

12 Ekim 2026. Kapsam: Team DB için Unix soketi/parolasız HBA ve loopback SCRAM seçenekleri; owner/appuser rol şablonu; IP üzerinden yerel HTTPS kurulum kılavuzu. Kişisel kasa biçimi, Team şema 64 / API 1 / cache 8 değişmedi. Minimum istemci 0.4.1 korunur; 0.4.2 yönetici sürüm listesine eklenir.

## Kaynak doğrulaması

- Frontend: 27 birim testi geçti; Windows/Linux frontend üretim derlemesi geçti.
- Service Rust: 7 test geçti; açık loopback/Unix transport seçimi, TLS varsayılanı, TCP parola gereksinimi ve uzak plaintext adresi reddi.
- Operator transport testi: soket için parolasız ortam; TCP için özel pgpass; uzak adres, PGHOSTADDR kaçışı ve zayıf TLS modu reddi geçti.
- Gerçek izole PostgreSQL 18: owner/appuser + en_US.UTF-8 libc, 64 migration, tekrarlı doğrulama ve lider bootstrap geçti. Güncel ve gelecekteki Team tablolarında app/developer/readonly doğrudan erişimi yok; sahip owner.
- Gerçek Debian 13 container: DB ssl=off, CA dosyası yok, Unix soketi ve parolasız hesaplarla API hazır. Caddy IP sertifikası, HTTPS Team girişi, yanlış parola ve güvenilmeyen CA reddi geçti.
- TLS regresyonu: mevcut CA/hostname doğrulaması, yanlış CA/hostname ve plaintext reddi geçti.

## Son paket doğrulaması

- Windows x64 NSIS üretildi. Gerçek 0.4.1 → 0.4.2 kurulumunda kasa/sır/font ayarı, DPAPI otomatik açılış, Mosh yardımcısı ve kaldırma geçti. Kurulu masaüstünden soketli DB arkasındaki IP/HTTPS servisine aktivasyon, ortak kasa, giriş hatırlama, çıkış/yeniden giriş ve sürüm politikası geçti.
- Windows Rust: 63 geçti / 8 ortam gerektiren test normal koşuda atlandı. Updater imza + bozma testleri Windows NSIS ve Linux AppImage için ayrıca geçti.
- Linux x64 DEB/AppImage/RPM üretildi. DEB ve AppImage gerçek X11 penceresi, kurulum/kaldırma, test hook'u/listener olmaması geçti. RPM paket bütünlüğü/metadata doğrulandı; ayrı RPM dağıtımında çalıştırılmadı.
- Son Debian 13 servis imajı ile Unix soketi, loopback SCRAM ve eski doğrulamalı TLS akışları geçti. İmaj save/load SHA256, binary/kimlik, nonroot kullanıcı ve Compose soket mount/CPU/RAM/pids/read-only/restart ayarları doğrulandı.
- Soket sahibiyle aynı-şema plan → özel yedek → ayrı DB'ye restore → HTTPS confirm geçti. TLS laboratuvarında 63→64, kesinti/devam, veri/şifreli içerik korunması, eski servis reddi ve yönetici sürüm politikası geçti.
- Uygulanmış 001–064 migration dosyaları değişmedi. Paket arşivleri kaynak parmak izi, özel dosya/sır taraması ve SHA256 ile teslim edilir. Docker runtime kaynak kimliği 7bfcc5f; sonraki değişiklikler kılavuz/test/paketleme düzeltmeleridir.

İlk paralel fixture koşusunda bir soket/HTTPS hazır olma zaman aşımı görüldü; aynı son imajla bağımsız tekrar ve soket yedekli yükseltme testi geçti. Süre artırılarak gizlenmedi. İlk Linux imzalama komutunda key-path seçeneği düzeltilip imza ve bozulmuş dosya reddi yeniden doğrulandı. Paketleme WSL/Windows worktree Git yolu hatası düzeltildi; kaynak kimliği değişmeden açıkça kaydedildi.

## Sınırlar

Windows agent/Pageant ve gerçek şirket OTP sağlayıcısı bu yamada yeniden doğrulanmadı; macOS/FIDO2/Windows Hello önceki kararla ertelendi. Üretim şirket ağı/firewall test edilmedi. TCP üzerinden TLS kapatmak yalnız literal loopback IP için mümkündür. Unix soketinde trust, sokete ulaşabilen yerel süreçlerin izinli DB kullanıcılarını kullanabilmesi anlamına gelir; çok kullanıcılı sunucuda peer/SCRAM tercih edilmelidir.

Yerel HTTPS sertifikası çalışanlara public CA dosyası olarak dağıtılır; doğrulama kapatılmaz. Yeni uzun yük/Stats/SSH testleri bu servis yapılandırma yamasında tekrarlanmaz; bunlar önceki 0.4.1 kanıtlarıdır, yeni test diye sayılmaz.
