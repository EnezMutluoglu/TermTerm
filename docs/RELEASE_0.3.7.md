# TermTerm 0.3.7

Terminalin sağ üstündeki Stats/İzleme paneli genişletildi.

- **Ağ trafiği:** gelen ve giden hızlar ayrı ayrı Mbit/s cinsinden gösterilir.
- **Disk etkinliği:** okuma/yazma ayrı ayrı MiB/s gösterilir. Satıra tıklayarak ağ arayüzü ve disk aygıtı seçilir; farklı aygıtların sayaçları toplanmaz.
- **Doluluk uyarıları:** %90 ve üzeri doluluk “Kritik”, kullanılabilir alanı kalmayan disk “Dolu” olarak gösterilir. Kök disk dışındaki mount'lar da ana panelde uyarılır; dolu/kritik olanlar listede en üsttedir.
- Türkçe etiketler, dar/bölünmüş görünüm ve kapat/aç davranışı hazırdır. Eksik veya eski hız örnekleri sıfır sayılmaz.

0.3.6 terminal ölçekleme düzeltmesi korunur. Kasa ve yedek biçimi değişmez. Team/kurumsal yetki geliştirmesi bu sürüme dahil değildir.

## İndirme ve güncelleme

Windows x64: `TermTerm_0.3.7_x64-setup.exe` veya `TermTerm-0.3.7-windows-x64-portable.zip`.
Linux x64: `TermTerm_0.3.7_amd64.deb`, `TermTerm-0.3.7-1.x86_64.rpm`, `TermTerm_0.3.7_amd64.AppImage`.
Kaynak: `TermTerm-0.3.7-source.zip`.

Windows ve AppImage: **Güncellemeler → Güncellemeleri denetle**. Kurulum kullanıcı tarafından başlatılır; mevcut güncelleme anahtarı korunur. Güncellemeden önce açık terminal oturumlarınızı kapatın. DEB/RPM paket yöneticisiyle elle kurulur.

Hızlar bağlı makinenin seçili aygıtına aittir; yalnız SSH/SFTP aktarımını göstermez. İlk hız için iki örnek gerekir. Ölçümler sudo/ajan istemez. Windows ve Linux gerçek sayaçları sınandı; macOS sağlayıcısı hazırlanmış olmakla birlikte Mac üzerinde doğrulanmadı ve macOS paketi yayımlanmadı.

Windows paketleri Mosh ve çevrimdışı WebView2 içerir; Authenticode sertifikası yoktur. Linux tabanı Ubuntu 22.04, glibc 2.34+ ve WebKitGTK 4.1'dir. Her Linux dağıtımında kurulum iddiası yoktur. Önceki FIDO2/Hello ve üreticiye özel şifreli import sınırları devam eder. Ayrıntılar: `VALIDATION_0.3.7.md`.
