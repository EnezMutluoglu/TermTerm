# TermTerm 0.3.5

Klasör ve host tıklamaları düzeltildi.

- **Klasöre tek tık:** doğrudan klasörün içine girer; bilgi paneli açılmaz. Klasör bilgileri **sağ tık → Bilgiler** üzerinden açılır.
- **Hosta tek tık:** bağlantı bilgilerini gösterir. Hızlı çift tıklamayı ayırmak için bilgi paneli yaklaşık yarım saniye bekletilir; kartlar iki tıklama arasında yer değiştirmez.
- **Hosta hızlı çift tık:** bilgi açılışı iptal edilir, o hosta tek bağlantı başlatılır. Kart ve liste görünümlerinde çalışır.
- Sağ tık, klasör/ekran değiştirme ve Escape bekleyen bilgi açılışını iptal eder. Ctrl/Command ve Shift ile çoklu seçim korunur. Klavyede klasör üzerinde Enter/Space klasörü açar; host üzerinde bilgiler açılır.

0.3.4 sürümündeki parola hatırlama düzeltmesi ve Türkçe arayüz korunur. SSH, chain/proxy, anahtarlar, SFTP, kaynak göstergeleri, kasa ve yedek biçimleri değiştirilmedi.

## İndirme ve güncelleme

- Windows x64: `TermTerm_0.3.5_x64-setup.exe` veya `TermTerm-0.3.5-windows-x64-portable.zip`.
- Linux x64: `TermTerm_0.3.5_amd64.deb`, `TermTerm-0.3.5-1.x86_64.rpm`, `TermTerm_0.3.5_amd64.AppImage`.
- Kaynak kodu: `TermTerm-0.3.5-source.zip`.

Windows ve AppImage kullanıcıları **Güncellemeler → Güncellemeleri denetle** üzerinden güncelleyebilir (eski İngilizce arayüz: **Updates → Check for updates**). Güncelleme kullanıcı tarafından başlatılır; mevcut imza anahtarı korunur. DEB/RPM paketlerini paket yöneticisiyle elle kurun. Windows kurucusu ve portable ZIP WebView2 çevrimdışı önkoşulunu ve Mosh dosyalarını içerir. Portable arşivin tamamını çıkarın.

Linux paketleri x64, glibc 2.34+ ve WebKitGTK 4.1 hedefler; sistem bağımlılıkları paket yöneticisiyle kurulur. AppImage FUSE olmadan `--appimage-extract-and-run` ile açılabilir. RPM için dağıtım üzerinde kurulum doğrulaması yapılmadı.

Kullanıcının isteğiyle uzun regresyon/donanım testleri tekrarlanmadı; bu değişiklik için kısa tıklama/menü testleri ve paket kontrolleri uygulandı. Önceki donanım anahtarı ve üreticiye özel şifreli import sınırları devam eder. macOS beklemede; Mac CI çalıştırılmadı. Windows Authenticode sertifikası yoktur; güncelleme paketleri ayrı kriptografik imzayla korunur. Ayrıntılar: `VALIDATION_0.3.5.md`.
