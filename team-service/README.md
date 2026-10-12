# TermTerm Team Service — 0.4.2

Windows/Linux masaüstü TermTerm uygulamasının hesap, yetki ve şifreli eşitleme servisidir; tarayıcı arayüzü sunmaz. SSH/SFTP trafiği masaüstünden hedefe gider. Bu paket 0.4.2 sürümüdür.

## Kurulum

Aynı sunucu/alan adı olmayan kurulum için `docs/TEAM_SOCKET_KURULUM_TR.md`: owner/appuser şablonu, Unix soketi, IP ile yerel HTTPS ve çalışanlara public CA dağıtımı.

Hazır Linux x64 Docker image'ı Debian 13 Trixie slim tabanlıdır. `TermTerm-Team-Image-<sürüm>-linux-x64.tar.gz` dosyası `docker load` ile yüklenir; servis paketiyle birlikte kullanılır. Sunucuda Rust derlemesi gerekmez. `deploy/prepare-database.sql` DBA'nın elle kurduğu PostgreSQL için ilk rol/DB hazırlığını sağlar; ardından açık `manage.py migrate` adımı gerekir. Kaynak sınırları `deploy/deployment.env.example` içindedir. Image üretimi `scripts/package-team-image.py` ile yerelde yapılır; script GitHub'a yayın yapmaz.

Paket içindeki `docs/TEAM_SERVICE_INSTALL_TR.md` (kaynak ağacında `../docs/TEAM_SERVICE_INSTALL_TR.md`) mevcut TLS PostgreSQL, ayrı PostgreSQL kuran Compose ve systemd seçeneklerini anlatır. Migration ve ilk lider oluşturma ayrı, açık operatör adımlarıdır. Servis DB şeması değiştirmez veya kendiliğinden yönetici hesabı oluşturmaz.

Mevcut veritabanının yükseltilmesi `docs/TEAM_UPGRADES_TR.md` içindeki **plan → doğrulanmış yedek → apply → HTTPS confirm** akışıyla yapılır. İlk kurulum komutu, mevcut veritabanını yeni şemaya yedeksiz yükseltmeyi reddeder. Uygulanmış migration dosyaları değiştirilmez; `compatibility.json` servis/API/şema uyumunu tanımlar. Bakım süresince servis durdurulur; otomatik downgrade veya Release yayını yapılmaz.

- Normal servis rolü tablo sahibi/superuser/BYPASSRLS olamaz; doğrudan tablo yetkileri reddedilir.
- Normalleştirilmiş tablolar: hesap/cihaz/oturum, klasör/host/hop, kişisel/ortak kimlik/sürüm/zarf, kişisel profil, ACL/delegasyon, işlem/karar/geçmiş/kurtarma. API fonksiyonları açık izin listesiyle açılır.
- Hesap/izin/şifreli veri HTTPS üzerinden taşınır. PostgreSQL bağlantısı uzak sunucuda CA ile doğrulanır; aynı sunucuda açıkça seçilmiş Unix soketi/parolasız HBA veya loopback SCRAM kullanılabilir. Kullanıcı bilgisayarına DB parolası dağıtılmaz.
- 15 dakikalık erişim ve 30 günlük dönen refresh token'ı; sunucuda yalnız hash. Yeni hesap geçici parolası 21 karakter/24 saat, ilk girişte değiştirilir.
- Özel sırlar normal lider ekranına verilmez. Şirket kurtarma dosyasının private key'i masaüstünde şifreli kalır; gerekçeli ve denetim kayıtlı kurtarma kullanılır.
- `/v1/register`, `/v1/sql`, `/v1/rpc` ve `/v1/bootstrap` uçları yoktur. Token, özel anahtar, parola ve sorgu parametreleri loglanmaz.
- 16 eşzamanlı istek, 256 KiB istek gövdesi, 20 saniye istek/15 saniye SQL/5 saniye bağlantı-kilit bütçesi vardır. Sonucu belirsiz yazma yeni işlem kimliğiyle otomatik tekrarlanmaz.
- `/health` süreç canlılığıdır. `/ready` sınırlı DB bağlantısını ve paketle aynı şema/API sürümünü gerektirir; uyumsuz şemada 503 döner.

## İstemci sürümünü zorunlu kılma

Kişisel güncellemeler isteğe bağlıdır. Team sahibi masaüstündeki **Team → Sürüm politikası** ekranından desteklenen istemci sürümünü seçer ve önizlemeyi onaylar. Karar yalnız kendi takımına uygulanır; API erişimi ve istemci ekranı birlikte denetler. `compatibility.json` içindeki `clientVersions` paketle doğrulanmış seçenekler, `minimumClientVersion`/`apiVersion` teknik sınırdır. Bu pakette 0.4.1 ve 0.4.2 istemci seçenekleri listelenir; teknik alt sınır 0.4.1 kalır. Yeni tablo eklemek kendi başına otomatik istemci güncelleme kararı değildir. Ayrıntılar `docs/TEAM_041_GUIDE_TR.md` ve `docs/TEAM_UPGRADES_TR.md` içindedir.

## Doğrulama

Yerel test komutları kaynak ağacındadır:

```bash
python3 scripts/team-v2-lab.py
bash scripts/team-v2-linux.sh
python3 tests/team_v2_http.py
python3 tests/team_v2_install.py
python3 tests/team_v2_compose.py
python3 tests/team_v2_upgrade.py
```

İlk üçü yalnız izole WSL laboratuvarını kullanır. Kurulum/yükseltme testleri ayrı ve koşuya özel veritabanları, kurulum testi ayrıca ayrılmış systemd birimi kullanır. Compose testi ayrı test projesinde çalışır. Kendi süreçlerini sonunda durdururlar; test parolaları özel dizinlerde kalır ve pakete girmez.

Kapsam, tamamlanan testler ve **halen açık uygulama işleri** `TEAM_041_STATUS.md` içindedir. Kurulum paketinin doğrulanması bütün Team özelliklerinin tamamlandığı anlamına gelmez. macOS/donanım/üretim testleri erişim olmadan geçti sayılmaz.
