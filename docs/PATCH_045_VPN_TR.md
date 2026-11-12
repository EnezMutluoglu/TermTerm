# 0.4.5 VPN bağlantı kapatma düzeltmesi

Oturum komutları özgün broker bağlantısını kullanır; kayıp bağlantı üzerinden yeni broker başlatılmaz. Windows kontrol kanalı canlı süreç kimliğini ve başlangıç zamanını doğrular. Böylece bir profil kapatılırken diğer profilin komutları yanlış sürece yönlendirilmez.

Kapatma onayı tek başına başarı sayılmaz: masaüstü gerçek terminal durumu bekler, başarısız veya kaybolan oturumu başarılı göstermez. Arayüz de başarısız durumu yapay bir “bağlantı kesildi” durumuyla değiştirmez.

Odaklı kontroller: 13 Rust VPN testi, 2 gerçek Windows named-pipe alt süreç kontrolü, 5 üretim adayı paket kimliği testi, 46 yayın sözleşmesi testi ve 2 React arayüz regresyonu geçti. İki gerçek Windows OpenVPN motoru ayrı test sunucularına bağlandı; her biri 128 paket taşıdı, biri doğal olarak kapandıktan sonra diğeri 32 doğrulanmış paket daha taşıdı. Bu test özel paket kanalları kullanır; Wintun veya yetkili broker testi değildir.

Canlı, yükseltilmiş Windows broker/Wintun iki-profil yaşam döngüsü henüz doğrulanmadı; ilk deneme UAC öncesinde süre sınırına ulaştı. Yerel adayın yayın kapısı kapalıdır. macOS, donanım anahtarları ve üretim VPN sunucusu kimlik doğrulaması test edilmiş sayılmaz. Kesin paket özetleri ve Linux paket eşleştirme kapsamı yerel aday raporundadır.

Bu kayıt bir yerel kaynak kontrol noktasıdır; stable uygulama kurulmadı, etiket/push/yayın yapılmadı ve canlı güncelleme akışı değiştirilmedi. Team hizmeti ve veritabanı şeması değişmedi.
