# TermTerm 0.4.0

- **Team:** PostgreSQL hesapları, ortak kasalar, klasör/host bazında izinler, sahip ve kapsam yöneticisi; yalnız izin verilen bağlantılar görünür. İlk kurulum ayrı migration hesabı gerektirir; kişisel kasa sunucusuz çalışır.
- **Çevrimdışı ve geçmiş:** şifreli yerel kuyruk, 24 saat düzenleme politikası, bağlantı dönünce eşitleme; eşzamanlı değişikliklerde karar ekranı. Güncel kayıt + önceki 10 sürüm, geçici kullanım ve kalıcı geri yükleme.
- **Parola üretici:** sol alttaki düğmeden 4–256 karakter, harf/rakam/noktalama/sembol/parantez kutuları ve benzer karakterleri dışlama. Varsayılan 21 karakter. Son 50 parola tarih-saatle yerelde şifreli tutulur; gizleme/kopyalama/silme. Team'de kişisel pano takımla paylaşılmaz.
- 0.3.7 ağ/disk ölçümleri, doluluk uyarıları ve terminal ölçekleme düzeltmesi korunur. Kişisel kasa/yedek biçimi korunur.

Windows x64 kurucu ve taşınabilir ZIP; Linux x64 DEB/RPM/AppImage. **Güncellemeler → Güncellemeleri denetle** Windows/AppImage içindir; DEB/RPM paket yöneticisiyle elle kurulur. Mac paketi/CI bu yayında yoktur.

Team kurulumu için kaynak arşivindeki `docs/TEAM_GUIDE_TR.md` ve `migrations/team` betiklerini kullanın. Var olan kişisel kasa otomatik ortak kasaya dönüşmez. Team için TLS sunucu doğrulaması ve sınırlı uygulama rolü zorunludur.

Parola geçmişi yalnız yerel kasada/hesap önbelleğindedir; `.ttbackup` ve bağlantı exportlarına eklenmez. Kişisel `.ttvault` taşınabilir kopyası içerir. Ayrıntılar `docs/PASSWORD_BOARD_TR.md`.

Sınırlar: macOS, donanım anahtarları, üretim sunucuları ve iki ayrı bilgisayarda ortak terminal konuk arayüzü doğrulanmadı. SMTP/e-posta doğrulama/parola sıfırlama yoktur. FIDO2/Hello ve bazı üretici şifreli import sınırları sürer. Windows Authenticode sertifikası yoktur; NSIS/AppImage updater anahtarıyla doğrulanır. Kesin test sonuçları: `VALIDATION_0.4.0.md`.
