# TermTerm 0.3.6

Terminal pencereye sığmıyor, Vim/pg_activity ekranın yalnızca bir bölümünü kullanıyor ve boş alan varken erken kaydırma oluyor sorunları düzeltildi.

- SSH kimlik doğrulaması sırasında gelen en son satır/sütun boyutu artık ilk PTY açılışında korunur; Mosh başlangıcı da aynı düzeltmeyi kullanır.
- Bağlantı açıldığında boyut tekrar doğrulanır. Pencere büyütme/küçültme, sekmeye dönüş, font yüklenmesi, Stats ve bölünmüş görünüm ölçüyü yeniler.
- Boyut bildirimleri sırayla gönderilir; hızlı pencere sürüklemede en yeni ölçü korunur. Gizli sekmeler sıfır boyuta düşürülmez.
- Çok geniş pencerelerdeki 500 sütun sınırı kaldırıldı; güvenli üst sınır 4096 satır/sütundur.

Mevcut klasör/host davranışı, Türkçe arayüz, parola hatırlama, SSH anahtarları ve kasa biçimi korunur. Henüz tamamlanmayan Team/kurumsal yetki geliştirmesi bu kararlı sürüme dahil değildir.

## İndirme ve güncelleme

Windows x64: `TermTerm_0.3.6_x64-setup.exe` veya `TermTerm-0.3.6-windows-x64-portable.zip`.
Linux x64: `TermTerm_0.3.6_amd64.deb`, `TermTerm-0.3.6-1.x86_64.rpm`, `TermTerm_0.3.6_amd64.AppImage`.
Kaynak: `TermTerm-0.3.6-source.zip`.

Windows ve AppImage: **Güncellemeler → Güncellemeleri denetle**. Kontrol ve kurulum kullanıcı tarafından başlatılır; paket mevcut güncelleme anahtarıyla doğrulanır. Açık terminal bağlantılarını güncellemeden önce kapatın. DEB/RPM paket yöneticisiyle elle kurulur; APT/YUM deposu bulunmaz.

Windows paketlerinde Mosh ve çevrimdışı WebView2 vardır. Windows Authenticode sertifikası yoktur. Linux tabanı Ubuntu 22.04, glibc 2.34+ ve WebKitGTK 4.1'dir; her Linux dağıtımı için test iddiası yoktur. macOS ve Mac CI kullanıcı isteğiyle bekletildi. Donanım anahtarları ve üreticiye özel şifreli import sınırlamaları devam eder. Ayrıntılar: `VALIDATION_0.3.6.md`.
