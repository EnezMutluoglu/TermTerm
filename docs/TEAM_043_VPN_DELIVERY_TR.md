# Masaüstü 0.4.3 için Team Service 0.4.2 kurulumu

Bu teslim servis **0.4.2**, API **1**, şema **66** ve teknik minimum istemci **0.4.1** kullanır. Takım yöneticisi gerekli istemci sürümü olarak **0.4.3** seçebilir. Bu seçim yalnız ilgili takımı etkiler; kişisel güncellemeler isteğe bağlıdır. Servis, kasa ve bağlantı verilerinin şifreli taşıma/saklama biçimini değiştirmez. Uygulanmış migration SQL dosyaları değişmedi.

Yeni Linux x64 sunucuda servis arşivi, Debian 13 image arşivi, image kimlik JSON'u ve SHA256SUMS.txt birlikte gerekir. Image root yerine UID/GID 10001 ile çalışır. Aşağıdaki komutları indirdiğiniz dosyaların bulunduğu dizinde çalıştırın:

```bash
sha256sum --check --ignore-missing SHA256SUMS.txt
sudo docker load --input TermTerm-Team-Image-0.4.2-vpn-0.4.3-linux-x64.tar.gz
tar -xzf TermTerm-Team-Service-0.4.2-vpn-0.4.3-linux-x64.tar.gz
sudo docker image inspect termterm/team-service:0.4.2-vpn-0.4.3 --format '{{.Id}}'
cd team-service-linux-x64
```

Image ID, indirilen JSON'un `imageId` değeriyle aynı olmalıdır. Özel `deployment.env` dosyanızda `TERMTERM_SERVICE_IMAGE=termterm/team-service:0.4.2-vpn-0.4.3` kullanın. `latest` kullanmayın. Arşivin Compose varsayılanı da bu etikete bağlıdır. Eski 0.4.2 paketlerini veya doğrulanmış yedekleri silmeyin.

Yeni veritabanı için [kurulum rehberindeki](TEAM_SERVICE_INSTALL_TR.md) DB/migration/HTTPS adımlarını izleyin; o rehberdeki 0.4.2 indirme ve image adı örnekleri yerine yukarıdaki bu teslim adlarını kullanın. Paket içinde PostgreSQL hazırlama SQL'i, rol örnekleri, config örnekleri ve 1–66 migration dosyalarının tamamı bulunur. **İlk kurulum**: DBA veritabanını ve sınırlı rolleri hazırlar, migrator hesabıyla `python3 deploy/manage.py migrate --service-role <servis_rolü>` çalıştırılır. Uygulama ve servis kendiliğinden migration yapmaz.

Aynı sunucuda [Unix soketi kurulumu](TEAM_SOCKET_KURULUM_TR.md), ayrı sunucuda [doğrulamalı PostgreSQL TLS kurulumu](TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md) kullanın. Çalışanlar yalnız HTTPS servisine bağlanır; DB parolası masaüstüne verilmez. İç CA varsa yalnız public CA sertifikası çalışanlara dağıtılır. HTTPS doğrulamasını kapatmayın. Servis rolüne doğrudan Team tablo yetkisi veya DB sahipliği vermeyin.

Yayımlanmış v0.4.2 kurulumu **şema64** kullanır; bu teslim **065 Authenticator ve 066 VPN** migration'larıyla **şema66** gerektirir. Servis sürümü 0.4.2 olarak kalır. Mevcut şema64 DB için [plan/apply/confirm](TEAM_UPGRADES_TR.md) akışı zorunludur: özel doğrulanmış yedek, ayrı boş/özel geri yükleme DB'si, bakım/durdurma, iki migration ve checksum kontrolü, ardından HTTPS+DB kimliğiyle doğrulama. `manage.py migrate` ile mevcut DB yükseltmesini atlamayın. Zaten şema66 kullanan kurulumlarda uygulanmış migration checksum'larını ve readiness yanıtını doğrulayın. Restore aracı mevcut DB'yi otomatik temizlemez.

Servis hazır olduktan ve çalışanların 0.4.3 paketleri doğrulandıktan sonra Team yöneticisi istemci sürümleri ekranında 0.4.3 seçebilir. 0.4.1/0.4.2 istemcileri ilgili takım için güncelleme gereksinimi alır; 0.4.3 normal giriş/çalışmayı sürdürür. Genel teknik minimum 0.4.1 olarak kalır. Bu belge hiçbir servisi başlatmaz, DB'yi değiştirmez veya güncelleme yayımlamaz.
