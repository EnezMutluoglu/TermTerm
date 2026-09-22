# PostgreSQL veri yapısı ve ilk bağlantı

21 Eylül 2026 — TermTerm 0.3.0.

## Veri nerede tutuluyor?

Yerel `.ttvault` her zaman temel çalışma dosyasıdır. PostgreSQL isteğe bağlı senkronizasyon ve ekip sunucusudur. Bağlantı kurulunca yerel dosya ortadan kalkmaz. DB kapalıyken de host eklenebilir, düzenlenebilir ve SSH/SFTP kullanılabilir.

PostgreSQL ilişkisel bir veritabanıdır; ayrıca MongoDB/NoSQL kurulmaz. Host, grup, kimlik, workspace ve diğer uygulama kayıtları JSON olarak serileştirilir, uygulamada şifrelenir ve `records.payload` alanına **bytea** olarak gönderilir. Sunucuda açık JSON/JSONB host belgesi veya ayrı `hosts`/`groups` tabloları yoktur. Kayıt türü, adres, parola ve özel anahtar şifreli içeriğin içindedir. Kasa/kayıt kimlikleri, üyelikler, revizyonlar, silinme durumu ve zaman gibi senkronizasyon metaverileri sunucu tarafından görülür.

Canlı CPU/RAM grafikleri ve disk ölçümleri yalnızca bellekte kalır; DB'ye veya yedeğe yazılmaz. Normal SSH ve SFTP trafiği PostgreSQL'den geçmez.

## Tablolar

Varsayılan şema `termterm` altında şu **7 tablo** vardır. Bu bilgisayardaki `termterm_dev` üzerinde varlıkları kontrol edildi; `schema_version` değeri 2'dir. Uygulamanın 0.3 sürümü ile DB şema sürümü aynı sayı olmak zorunda değildir.

| Tablo | Temel alanlar | Görevi |
|---|---|---|
| `schema_version` | `version` | Şema sürümü kaydı |
| `vaults` | `id`, `envelope`, `encrypted_name`, `revision`, `created_at` | Kasa, parola ile korunan anahtar zarfı ve sunucu revizyonu |
| `members` | `vault_id`, `principal`, `role`, `key_envelope` | PostgreSQL login'inin kasa üyeliği; owner/editor/viewer ve üye anahtar zarfı |
| `records` | `vault_id`, `id`, `revision`, `payload`, `deleted` | Şifreli uygulama kayıtları ve silme işaretleri |
| `operations` | `vault_id`, `op_id`, `record_id`, `revision`, `principal` | Yeniden gönderilen işlemin ikinci kez uygulanmasını engelleme |
| `terminal_sessions` | `id`, `vault_id`, `owner`, `writer`, `lease`, `expires_at` | Paylaşılan terminal erişimi ve tek yazıcı kontrolü |
| `terminal_frames` | `session_id`, `seq`, `sender`, `kind`, `lease`, `payload`, `expires_at` | Şifreli, kısa ömürlü ortak terminal iletileri |

Hostun grup/kimlik/chain ilişkileri şifreli kaydın içindeki kimliklerle tutulur. PostgreSQL bu iç ilişkileri SQL foreign key ile denetleyemez; uygulama doğrular ve import sırasında kimlikleri yeniden eşler. Sunucu kasa ve üyelik ilişkilerini SQL constraint/RLS ile denetler. Şifreli host içeriğinde SQL ile adres araması yapılamaz; arama açılmış yerel kasada gerçekleşir.

## Bugün ilk kurulum nasıl çalışıyor?

1. PostgreSQL sunucusu, veritabanı, migration sahibi ve ayrı uygulama login'leri hazırlanır. Uygulama bunları otomatik oluşturmaz. Bu PC'deki WSL betikleri `18/termterm:55432`, `termterm_dev` ve `termterm_e2e` ortamlarını hazırlar; `18/main:5432` ayrı kalır.
2. Migration sahibi sırayla `migrations/001_sync.sql`, `002_shared_terminal.sql` ve `003_schema_version.sql` dosyalarını uygular. `CREATE SCHEMA/TABLE IF NOT EXISTS` kullanılır; mevcut tablo verileri silinmez. Bu, rastgele veya uyumsuz mevcut şemayı otomatik yükselten genel bir sistem değildir. `grant_app.sql` her uygulama login'ine gereken izinleri verir. Günlük uygulama profili migration hesabını kullanmaz.
3. **Settings → PostgreSQL sync** ekranına adres, port, DB, şema, uygulama kullanıcısı/parolası ve TLS CA girilir. **Save profile** bu bilgileri yerel şifreli kasaya kaydeder. **Test connection**, doğrulanmış TLS üzerinden bağlantıyı sınar ve şema/sürüm durumunu bildirir. **Create / upgrade schema** ayrı migration kullanıcısı/parolasıyla şemayı hazırlar. Migration parolası kaydedilmez. DB ve PostgreSQL login oluşturma işlemleri sunucu yöneticisinde kalır.
4. Sunucu boşsa **Upload local vault**, yeni uzak kasayı ve owner üyeliğini oluşturur; yerel kayıtlar gönderilir. Aynı kasa kimliği zaten varsa bunun üzerine yeni kasa yüklenmez; mevcut uzak kasa akışı kullanılır.
5. Sunucuda kasa varsa **Open remote vault**, yetkili olunan kasayı seçtirir. DB parolasına ek olarak o üyenin **kasa parolası** gerekir. Kayıtlar doğrulanıp çözülür ve seçilen **yeni `.ttvault` dosyasına** kaydedilir; mevcut dosyanın üzerine yazılmaz. Yeni cihaz kimliği kullanılır ve eski gönderim kuyruğu taşınmaz. Yeni kasada sync profili gerektiğinde yeniden kaydedilir ve arka plan eşitleme açıkça etkinleştirilir.
6. Sonraki değişiklikler önce yerelde ve gönderim kuyruğunda işlemsel kaydolur. Sunucu benzersiz işlem kimliğini ve artan revizyonu kullanır. Bağlantı kesilirse yerelde devam edilir; arka plan eşitlemesi açıkken yeniden bağlanma denenir. LISTEN/NOTIFY ile bildirim, ayrıca 15 saniyelik yedek kontrol vardır. Eşzamanlı değişiklikler **Keep local / Keep remote / Keep both** ekranına düşer. Silmeler tombstone ile taşınır.

WSL laboratuvarı için mevcut komutlar:

```powershell
scripts/lab.ps1 setup
wsl -d Ubuntu-24.04 -u root -- bash /mnt/c/Users/sonx/Desktop/termterm/scripts/migrate-lab.sh
scripts/lab.ps1 status
```

Proje başka klasördeyse WSL yolunu değiştirin. Genel bir sunucuda SQL dosyaları `psql -v ON_ERROR_STOP=1` ile migration sahibi olarak uygulanmalıdır; `grant_app.sql`, `-v app_role=uygulama_login_adi` değişkenini kullanır. Verilen SQL dosyalarının varsayılan şeması `termterm`'dür. Uygulamadaki şema alanını değiştirmek sunucuda o şemayı kendiliğinden oluşturmaz.

`sync_prepare` Settings ekranındaki migration işlemiyle çağrılır. Desteklenenden yeni şema sürümü değiştirilmeden reddedilir. Uygulama login'ine CREATE/superuser yetkisi vermek gerekmez. Mevcut, elle değiştirilmiş uyumsuz tablolar için otomatik onarım sözü verilmez.

## Mevcut kasayı bağlama ve hedef değiştirme

**Preview & connect** yerel kayıtları, gönderim kuyruğunu ve uzaktaki aynı kasa kimliğini karşılaştırır. Yerelde olup sunucuda olmayan kayıtlar upload, farklı kayıtlar conflict, yalnızca sunucuda olanlar download olarak görünür. Onaylanan önizleme token'ı yerel içerik ve sunucu revizyonuna bağlıdır; bu arada içerik değişirse yeniden önizleme gerekir.

Tek yerel kasa dosyasının aktif DB hedefi adres/port/veritabanı/şema üzerinden bağlanır. Başka hedefe eşitleme önce yeni önizleme gerektirir. Yeni hedefe geçerken farklı yerel kayıtlar korunur; sessizce uzak kayıtla değiştirilmez, çakışmaya alınır. PostgreSQL devre dışı bırakıldığında dosya kullanılmaya devam eder.

İki ayrı kasa kimliğini birleştirmek için uzak kasa yeni dosyaya açılır; diğer kasanın `.ttvault` veya `.ttbackup` dosyası import önizlemesiyle kopyala/güncelle/atla politikalarından biriyle aktarılır. Tek dosyayı iki bağımsız DB'ye aynı anda eşitlemek desteklenmez. Üye kaldırılması daha önce edinilmiş anahtarı geri alamaz; otomatik kasa anahtarı rotasyonu bu sürümde yoktur.
