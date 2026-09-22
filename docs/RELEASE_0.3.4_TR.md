# TermTerm 0.3.4

Kasa parolasını hatırlama düzeltildi ve arayüzün varsayılan dili Türkçe oldu. Mevcut kasalar, SSH anahtarları ve bağlantı zincirleri korunur.

- **Parolayı hatırla:** kayıtlı parola sonraki açılışta otomatik kullanılır. Parolayı elle girmeniz önceki kaydı silmez. Hatırlama kutusunu bilerek kaldırırsanız kayıt silinir. **Kasayı kilitle** işlemi kasayı kilitli tutar; tekrar açmak için düğmeye basmanız gerekir.
- Giriş ekranı, menüler, sağ tık işlemleri, bağlantı düzenleyicisi, SFTP, kaynak göstergesi, yedekleme ve güncelleme ekranları Türkçeleştirildi. Sunucu/grup adları, dosya yolları, komutlar ve uzak terminal çıktıları değiştirilmez. Alt sistemlerden gelen ayrıntılı hata/teşhis metinleri İngilizce kalabilir.
- Güncelleme yüklemesi yarım kalmışsa aynı sürümün taslak yayınına devam edilir.

## İndirme ve güncelleme

| Sistem | Paket |
|---|---|
| Windows x64 | `TermTerm_0.3.4_x64-setup.exe` veya `TermTerm-0.3.4-windows-x64-portable.zip` |
| Debian/Ubuntu x64 | `TermTerm_0.3.4_amd64.deb` |
| RPM tabanlı Linux x64 | `TermTerm-0.3.4-1.x86_64.rpm` |
| Uyumlu Linux x64 masaüstü | `TermTerm_0.3.4_amd64.AppImage` |
| Kaynak kodu | `TermTerm-0.3.4-source.zip` |

0.3.2/0.3.3 Windows ve AppImage kullanıcıları **Updates → Check for updates** üzerinden geçebilir. Yeni arayüzde yol **Güncellemeler → Güncellemeleri denetle** olur. Güncelleme kullanıcı tarafından başlatılır ve mevcut imza anahtarıyla doğrulanır. DEB/RPM için yeni paketi indirip paket yöneticisiyle kurun. 0.3.1 veya geliştirme paketinden geçişte kararlı paketi bir kez elle kurun.

Eski sürüm hatırlanan parolayı sildiyse, bu güncellemeden sonra parolanızı bir kez girip **Parolayı … ile hatırla** seçeneğini işaretleyin. Parola Windows'ta DPAPI, Linux masaüstünde Secret Service ile saklanır. Güvenli depo yoksa veya kilitliyse hata gösterilir; normal parola ile kasa açılabilir. Hatırlanan parola başka bilgisayara taşınmaz. Taşınabilir kasayı başka bilgisayarda açmak için gerçek kasa parolası gerekir.

Windows kurucusu ve portable ZIP çevrimdışı WebView2 önkoşulunu ve Mosh dosyalarını içerir. Portable arşivin tamamını çıkarın. Linux paketleri x64, glibc 2.34+ ve WebKitGTK 4.1 hedefler; sistem bağımlılıkları paket yöneticisiyle kurulur. AppImage FUSE olmadan `--appimage-extract-and-run` ile açılabilir.

macOS derlemesi sahibinin isteğiyle bekletildi; Mac CI çalıştırılmadı ve Mac paketi yoktur. Windows Authenticode sertifikası bulunmaz; güncelleme paketleri ayrı kriptografik imzayla korunur. Donanım anahtarları ve üreticiye özel şifreli import sınırları önceki sürümle aynıdır. Test ayrıntıları: `VALIDATION_0.3.4.md`.
