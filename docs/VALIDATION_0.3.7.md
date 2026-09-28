# TermTerm 0.3.7 doğrulama raporu

28 Eylül 2026. Kullanıcı “güncelleme çık” diyerek ağ/disk hızları ve doluluk uyarılarının 0.3.7 kararlı sürümünü onayladı. 0.3.6 tabanlı `codex/feature-network-disk-stats` dalı kullanıldı. Team 0.4.0-dev.1 değişiklikleri ayrı tutuldu; macOS ve GitHub CI çalıştırılmadı.

## Kapsam

Gelen/giden ağ hızı Mbit/s, seçili disk okuma/yazma hızı MiB/s; ağ arayüzü/disk seçimi. Her cihaz ayrı ölçülür. İlk veya eski/bozuk örnek sıfır gibi gösterilmez. Kök dışındaki mount'larda da %90+ kritik ve sıfır kullanılabilir alan için dolu uyarısı bulunur; sorunlu diskler liste başındadır. [Kaynaklar, kapsam ve teknik ayrıntılar](RESOURCE_ACTIVITY.md).

0.3.6 terminal boyutlandırma düzeltmesi korunur. Bağlantı yetkilendirmesi, sunucu anahtarı doğrulaması, kasa/yedek biçimi, PostgreSQL ve Team değiştirilmedi. Ağ/disk örnekleri kalıcı depoya yazılmaz.

## Doğrulama

Yeni özellikler 0.3.7-dev.1 üzerinde test edildi; kararlı geçişte uygulama sürümü, belgeler ve kaynak paketine deneme yapılandırmasının dahil edilmesi değişti. Bu nedenle uzun testler tekrarlanmadı.

| Kontrol | Sonuç |
|---|---|
| Birim testleri | 25 Vitest geçti. Birim dönüşümü, %90 sınırı, rezerve alan/0 boş alan, bilinmeyen değer dahil |
| Arayüz | 4 yeni test geçti: hızlar/aygıt seçimi/eski-hatalı veri/Stats; kök olmayan kritik ve dolu diskler; dar split ve xterm alanı; Türkçe etiketler. Mevcut 2 palet ve 1 terminal resize testi de geçti |
| Rust | Windows ve Linux'ta 7'şer ölçüm testi geçti. Gerçek CIM ve Linux probe, fark alma/sayaç sıfırlama/aygıt kaybı, Windows/macOS örnek parser'ları |
| Native Windows | 2/2 gerçek oturum testi geçti (27 saniye). Linux SSH chain üzerinden ağ/disk ölçümleri, kontrollü 8 MiB yazma/fsync/doğrudan okuma, ölçümün terminal çıktısını değiştirmemesi, kapat/aç; Windows yerel terminalinde gerçek ağ/fiziksel disk hızları |
| Deneme uygulaması | Ayrı kimlikli Windows debug uygulaması gerçek pencere açtı/kapandı; test sürücüsü normal dependency graph'ta yok, 4445 E2E portu açılmadı |
| Kararlı Windows/Linux derlemeleri | 0.3.7 production derlemeleri geçti. Windows NSIS; Ubuntu 22.04 DEB/RPM/AppImage. Her iki normal dependency graph test sürücüsünden arındırılmış |
| Windows paketi | NSIS CRC ve EXE payload eşitliği geçti; Mosh/Cygwin, lisanslar ve çevrimdışı WebView2 içerikleri doğrulandı |
| Linux paketleri | DEB 0.3.6 üzerine kuruldu, paket/bağımlılık kontrolü geçti; DEB ve AppImage gerçek grafik penceresi açtı. Mosh çalıştı, GLIBC 2.34. RPM metadata/digest/payload kontrolü geçti; RPM dağıtımında kurulum yapılmadı |
| İmza ve kaynak arşivi | Windows NSIS ve AppImage updater imzası doğrulandı; değiştirilmiş dosya reddi testleri geçti. SHA-256 manifesti, Windows taşınabilir runtime/önkoşullar/lisanslar ve 291 dosyalık kaynak arşivinin birebir eşitliği doğrulandı |

Kritik/dolu disk uyarıları sentetik mount örnekleriyle sınandı; kullanıcının diski doldurulmadı. Türkçe önizleme “Örnek ölçümler” olarak işaretlidir. Gerçek disk etkinliği test dosyası silindi. Kişisel kasalar, host exportları, anahtarlar ve `.lab` verileri kaynak/arşivlere alınmaz.

## Sınırlar

- Windows x64; Linux x64 DEB/RPM/AppImage. macOS/ARM/Alpine paketi yok. macOS sağlayıcısı kaynak ve parser örnekleri düzeyinde hazırlanmıştır; gerçek Mac üzerinde doğrulanmadı.
- Linux testi Ubuntu 22.04 chroot ve WSL Linux hedefiyle yapıldı; her dağıtımda kurulum veya native Linux istemcisinde tüm SSH akışlarının tekrar doğrulandığı iddia edilmez. RPM dağıtımında kurulum testi yapılmadı.
- Windows'ta mevcut kullanıcı kurulumunu değiştiren kurulum/kaldırma testi yapılmaz; NSIS içeriği, hash/CRC, derlenen EXE eşitliği ve gerekli çalışma dosyaları kontrol edilir.
- Linux I/O paneli alttaki tam blok aygıtlarını gösterir; ağ arayüzleri ve diskler toplam olarak sunulmaz. Kernel/device erişimi kısıtlı ortamda sayaç kullanılamıyor gösterilir. macOS disk etiketleri I/O registry kimliğidir.
- Önceki FIDO2/Hello, seri port ve üreticiye özel şifreli import sınırları sürer. Yeni üretim sunucusu testi veya uzun yük testi yapılmadı.
- Windows Authenticode sertifikası yoktur; Windows kurucusu ve AppImage mevcut updater anahtarıyla imzalanır. DEB/RPM güncellemesi elle paket yöneticisiyle yapılır; APT/YUM deposu yoktur.
