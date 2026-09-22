# TermTerm 0.3.1 — klasör görünümü

22 Eylül 2026, Windows x64 ve Linux amd64 Debian paketi.

## Değişiklik

- Hosts ana ekranında yalnızca grup klasörleri görünür. Her host kendi klasörü açıldığında listelenir.
- Gruba atanmamış hostlar `Ungrouped` sanal klasöründe gösterilir. Bu görünüm kasaya yeni grup yazmaz veya mevcut grup ilişkilerini değiştirmez.
- Klasör ve sol menü sayaçları bütün alt klasörleri içerir. Klasör içi araç çubuğu o seviyedeki doğrudan host sayısını gösterir.
- Klasör sayaçları aramadan bağımsızdır ve grup taşıma/silme sonrasında yeniden hesaplanır. Döngülü bozuk ilişkiler sayacı kilitlemez.
- Paketleme betikleri sürüm numarasını proje yapılandırmasından alır.

## Kontroller

- TypeScript derleme kontrolü geçti.
- Windows x64 üretim derlemesi ve NSIS kurucu üretimi geçti; test özelliği etkinleştirilmedi.
- 10 birim testi geçti; klasör, boş grup, taşıma/silme, döngü, sanal Ungrouped ve klavye/girdi kuyruğu kontrolleri dahil.
- 5 Playwright arayüz testi geçti. Ana ekranda host kartı yok; grup ve alt grup gezinme, Ungrouped içeriği ve dar ekran kontrol edildi.
- 10.000 hostta arama: 37,3 ms (bu koşu), hedef 150 ms.
- Gerçek export ile yerel önizleme: 686 host, 81 gerçek grup, 5 ana klasör (4 gerçek + Ungrouped), ana ekranda 0 host kartı; Ungrouped içinde 429 host. JavaScript hatası yok.
- Termius yerel veri kopyası LevelDB sıra numarasına göre ayrıca denetlendi: exporttaki grup ilişkileri en son yaşayan kayıtlarla aynı. 257 host gruba bağlı, 429 hostun grup alanı boş; ilişki farkı ve kayıp kayıt yok.

## Linux Debian paketi

- `TermTerm_0.3.1_amd64.deb`: 12.756.552 bayt; paket adı `term-term`, sürüm `0.3.1`, mimari `amd64`.
- Ubuntu 24.04 WSL içindeki ayrı Ubuntu 22.04 chroot ortamında üretim derlemesi tamamlandı. Önceden indirilmiş bağımlılıklar kullanıldı; kaynak klasörüne `node_modules`, `dist` veya Rust `target` eklenmedi.
- Linux'ta 10 birim ve 5 Chromium arayüz testi geçti. 10.000 host araması: 109,4 ms.
- Paket, test ortamındaki 0.3.0 üzerine kuruldu; `dpkg` durumu `install ok installed`, sürüm 0.3.1. `apt-get check` ve `dpkg --verify term-term` geçti. Bu kontrol için sistem paketi indirilmedi.
- Kurulu çalıştırılabilir dosyanın SHA-256 değeri `.deb` içindeki dosyayla aynı. Tauri'nin paketleme sırasında eklediği `DEB` işareti nedeniyle paket öncesi ham ikiliyle birebir karşılaştırma kullanılmaz.
- ELF x86-64, en yüksek GLIBC gereksinimi 2.34; `ldd` eksik kütüphane göstermedi. Masaüstü girdisi, ikon, lisanslar, Mosh kütüphaneleri ve xterm-256color bilgisi pakette bulundu.
- Paket içindeki Mosh 1.3.2 çalıştırıldı. Üretim uygulamasının gerçek penceresi Xvfb/DBus altında temiz test ayarlarıyla açıldı. Root/headless testine özel WebKit değişkenleri uygulama veya paketin varsayılanına eklenmedi.
- Normal Cargo bağımlılık grafiğinde WebDriver sürücüsü yok. Üretim arayüzünde kişisel Termius önizlemesi veya WebDriver işaretleri bulunmadı.
- Debian masaüstü, Secret Service ve bütün bağlantı yöntemleri bu paketleme turunda yeniden sınanmadı. 0.3.1 için yalnızca `.deb` istendi; AppImage üretilmedi.
- SHA-256: `24f9c80177e6a466211934e31fa4a14df1ec4a941881975d1ce20ebd16fb8257`.

## Kapsam

Bu değişiklik kasa/yedek biçimini ve SSH, SFTP veya sync servislerini değiştirmez. Önceki sürümün bağlantı testleri yeniden yapılmış sayılmaz; önceki raporlardaki donanım ve üretici import sınırlamaları geçerlidir. Linux üretim paketinin kurulumu ve native pencere açılışı yukarıdaki ortamda doğrulandı. macOS derlemesi kullanıcı isteğiyle beklemektedir; macOS native doğrulaması yapılmadı.

Termius kişisel verileri, parolalar ve SSH anahtarları kaynak kodu veya genel kurulum paketlerine eklenmez. İçe aktarım ayrı, parola korumalı `.ttbackup` dosyasıdır.
