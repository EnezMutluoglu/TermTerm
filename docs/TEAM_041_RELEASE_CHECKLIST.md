# TermTerm 0.4.1 yayın kontrolü

12 Ekim 2026 · şema64 / API1 / cache8. Kullanıcının bu sürüm için açık yayın onayı vardır. Güncel kanıt ve ilk başarısız denemeler [VALIDATION_0.4.1.md](VALIDATION_0.4.1.md) içinde tutulur.

- İşlevsel Team kabulü: kişisel/ortak kimlik, proxy hatırlama, geçmiş chain, SSH/SFTP/üç tünel, Mosh, iptal ve çevrimdışı/kurtarma akışları doğrulandı.
- Normal Windows kurulumu ve 0.4.0'dan yükseltme: otomatik kasa açılışı, host/sır/ayar korunması ve kurulu Team girişini hatırlama geçti.
- Linux DEB/AppImage gerçek açılışı; RPM metadata/digest doğrulandı. RPM dağıtımında native açılış denenmedi.
- Paketli servis: systemd, ayrı PostgreSQL ve mevcut TLS PostgreSQL ile Compose, CA, sınırlı rol ve restart geçti.
- Güncelleme: mevcut anahtarla Windows/AppImage imzaları ve değiştirilmiş paket reddi geçti. Team politika/oturum izolasyonu ve liderin sürüm seçimi doğrulandı.
- Veri: kişisel kasa biçimi korunur. Team64 temiz kurulum ve63→64 yedek/restore/kesinti/devam/HTTPS confirm geçti. SQL byte'ları değişmedi; Git satır sonu dönüştürmesi checksum'lı migration'larda kapatıldı.
- Dağıtım: yayın allowlist'i Windows NSIS/ZIP, Linux DEB/RPM/AppImage, kaynak ZIP, Team Service arşivi, üç Türkçe kılavuz, doğrulama raporu, updater feed ve SHA256 içerir. Son arşiv taraması yayımlama öncesinde zorunludur.

Windows agent/Pageant ve şirket OTP doğrulanmadı; macOS/FIDO2/Hello ertelendi. Bunlar tamamlanmış gösterilmez. Kullanılmamış0.4.0 Team aktarımı kapsam dışıdır; gelecek v2 yükseltmeleri kapsam içindedir. Windows Authenticode imzası yoktur. 0.5 değişiklikleri ve ücretli/macOS CI bu yayına dahil değildir.
