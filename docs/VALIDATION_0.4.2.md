# TermTerm 0.4.2 doğrulama raporu

12 Ekim 2026. Kapsam: Team DB için Unix soketi/parolasız HBA ve loopback SCRAM seçenekleri; owner/appuser rol şablonu; IP üzerinden yerel HTTPS kurulum kılavuzu. Kişisel kasa biçimi, Team şema 64 / API 1 / cache 8 değişmedi. Minimum istemci 0.4.1 korunur; 0.4.2 yönetici sürüm listesine eklenir.

## Kaynak doğrulaması

- Frontend: 27 birim testi geçti; Windows/Linux frontend üretim derlemesi geçti.
- Service Rust: 7 test geçti; açık loopback/Unix transport seçimi, TLS varsayılanı, TCP parola gereksinimi ve uzak plaintext adresi reddi.
- Operator transport testi: soket için parolasız ortam; TCP için özel pgpass; uzak adres, PGHOSTADDR kaçışı ve zayıf TLS modu reddi geçti.
- Gerçek izole PostgreSQL 18: owner/appuser + en_US.UTF-8 libc, 64 migration, tekrarlı doğrulama ve lider bootstrap geçti. Güncel ve gelecekteki Team tablolarında app/developer/readonly doğrudan erişimi yok; sahip owner.
- Gerçek Debian 13 container: DB ssl=off, CA dosyası yok, Unix soketi ve parolasız hesaplarla API hazır. Caddy IP sertifikası, HTTPS Team girişi, yanlış parola ve güvenilmeyen CA reddi geçti.
- TLS regresyonu: mevcut CA/hostname doğrulaması, yanlış CA/hostname ve plaintext reddi geçti.

## Son paket doğrulaması

Paket ve yükseltme kontrolleri sürüyor; bu bölüm tamamlanmadan yayın yapılmaz.

## Sınırlar

Windows agent/Pageant ve gerçek şirket OTP sağlayıcısı bu yamada yeniden doğrulanmadı; macOS/FIDO2/Windows Hello önceki kararla ertelendi. Üretim şirket ağı/firewall test edilmedi. TCP üzerinden TLS kapatmak yalnız literal loopback IP için mümkündür. Unix soketinde trust, sokete ulaşabilen yerel süreçlerin izinli DB kullanıcılarını kullanabilmesi anlamına gelir; çok kullanıcılı sunucuda peer/SCRAM tercih edilmelidir.

Yerel HTTPS sertifikası çalışanlara public CA dosyası olarak dağıtılır; doğrulama kapatılmaz. Yeni uzun yük/Stats/SSH testleri bu servis yapılandırma yamasında tekrarlanmaz; bunlar önceki 0.4.1 kanıtlarıdır, yeni test diye sayılmaz.
