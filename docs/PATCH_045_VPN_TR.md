# 0.4.5 VPN bağlantı kapatma düzeltmesi

Oturum komutları özgün broker bağlantısını kullanır; kayıp bağlantı üzerinden yeni broker başlatılmaz. Windows kontrol kanalı canlı süreç kimliğini ve başlangıç zamanını doğrular. Bir profil kapatılırken diğer profilin komutları yanlış sürece yönlendirilmez.

Kapatma onayı tek başına başarı sayılmaz: masaüstü gerçek terminal durumu bekler, başarısız veya kaybolan oturumu başarılı göstermez. Arayüz de başarısız durumu yapay bir bağlantı kesildi durumuyla değiştirmez.

Odaklı kontroller: 13 Rust VPN testi, 2 gerçek Windows named-pipe alt süreç kontrolü, 5 üretim adayı paket kimliği testi, 46 yayın sözleşmesi testi ve 2 React arayüz regresyonu geçti. İki gerçek Windows OpenVPN motorunun özel paket kanalı testi de geçti.

Son canlı Windows testi, aynı kaynaklardan derlenen ayrı vpndev tanı brokeriyle gerçek Wintun üzerinde geçti: ikinci tünel kapatıldıktan sonra ilk tünel 3 doğrulanmış yanıt daha taşıdı; yeniden bağlantı ve eski nesil reddi doğrulandı. Kimliği doğrulanmış kapanışta sıfır oturum/jurnal, tam IP/rota/arayüz/DNS başlangıç durumu ve boş yerel arayüz envanteri doğrulandı. Test ortamı kapatıldı.

Bu test paketlenmiş üretim brokerinin birebir ikilisini veya masaüstü GUI entegrasyonunu çalıştırmış sayılmaz. Aynı kaynak ilişkisi ve kesin ikili özetleri yerel kabul raporundadır. İmzalı Wintun sürücüsü sistemde kayıtlı kalabilir. macOS, donanım anahtarları ve üretim VPN sunucusu kimlik doğrulaması test edilmiş sayılmaz.

Bu kayıt yayın öncesi doğrulama kapsamını belgeler. Kurulu stable uygulama değiştirilmedi. Windows/Linux 0.4.5 için verilen manuel yayın onayı, canlı kapatma ve diğer tünelin çalışması koşuluna ve son kabul raporuna dayanır; CI yayımlamaz. Team hizmeti ve veritabanı şeması değişmedi. Kesin paket özetleri, Linux paket eşleştirme kapsamı ve kalan sınırlamalar yerel doğrulama raporundadır.
