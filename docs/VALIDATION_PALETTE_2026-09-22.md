# Palet düzeltmesi — 22 Eylül 2026

Dal: `feature/terminal-usability`. Sürüm: `0.3.3-dev.1` geliştirme kaynağı.

## Sonuç

- Arayüzdeki sabit renkler `src/palette.css` rollerine taşındı. Çakışan eski CSS tanımları birleştirildi; isim uzunluğuna göre renklenen host ikonları kaldırıldı.
- Seçili host, klasör/menü, sekme ve SFTP satırı ortak mavi seçim yüzeyini kullanır. Genel ikonlarda ve kartlarda eski yeşil/mor tonlar kaldırıldı.
- CPU/RAM grafikleri aynı mavi ailesindedir; metinler nötrdür. Disk %90'ın üzerindeyken özet ve ayrıntı aynı hata rengini gösterir. Bağlantı/başarı yeşil, uyarı sarı, hata kırmızı rollerinde kalır.
- Terminal ANSI renkleri ve seçilebilir temalar korunur. Bileşen kuralları için [renk sistemi kılavuzu](UI_PALETTE.md) eklendi.

## Bu değişiklikte doğrulananlar

- `pnpm build`: TypeScript ve Vite üretim ön yüz derlemesi geçti.
- `pnpm test:unit`: 13/13 geçti.
- `pnpm test:ui`: son tam koşuda 20/20 geçti (38,4 saniye). 10.000 host araması bu koşuda 19,1 ms.
- Seçili host/SFTP/menü renk eşitliği; kontrol edilen temel metin ve grafiklerde en az 4.5:1 kontrast; disk uyarısı; terminal temasının arayüz renginden bağımsızlığı doğrulandı.
- 16 terminal, sekme/odak, clipboard, Unicode, fonksiyon tuşları, terminal araması, kapatılmış oturuma geç yapıştırmanın engellenmesi, Updates, 1366/1920 piksel ve %100/%150 CSS ölçek senaryoları geçti.
- İlk koşuda terminal arama testinin seçicisinde eski bozuk UTF-8 metni bulundu. Testin üç nokta ve Türkçe karakter örnekleri düzeltildi; arama ve Unicode senaryoları son tam koşuda geçti.
- Açılış, host seçimi, chain düzenleyicisi, SFTP seçimi, terminal Stats, Updates ve Appearance ekran görüntüleri gözle incelendi. Görseller `artifacts/palette-review/` altında.
- `git diff --check` geçti; uygulama bileşenlerinde palet dışı sabit CSS/JSX renk kalmadığı kontrol edildi.

## Sınır ve paket durumu

Bu tur tarayıcı kontrolleri sentetik kasa, dosya ve ölçüm verileri kullanır. Gerçek SSH/SFTP/işletim sistemi ölçümü yeniden sınanmadı; bağlantı altyapısı değiştirilmedi. macOS çalıştırması veya fiziksel DPI/klavye doğrulaması yapılmadı.

`artifacts/release-0.3.3-dev.1` içindeki önceki Windows/Linux kurucuları ve kaynak arşivi önceki teslimatın anlık görüntüsüdür; bu palet düzeltmesini içermez. Bu tur native kurucu yeniden üretilmedi, stable yayın/güncelleme kaynağı değiştirilmedi. [Önceki paket doğrulaması](VALIDATION_0.3.3-dev.1.md).
