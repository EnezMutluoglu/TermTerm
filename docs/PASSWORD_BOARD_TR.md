# Parola üretici ve panosu

Kasa açıkken sol alttaki **Parola üretici** düğmesine basın. Varsayılan uzunluk 21 karakterdir; 4–256 karakter seçilebilir. Küçük harf, büyük harf, rakam, noktalama (`.,;:!?`), sembol ve parantezler ayrı kutulardır. Her seçili türden en az bir karakter bulunur. Boşluk ve tırnaklar eklenmez. Benzer karakterleri dışlama seçeneği vardır. Rastgelelik Rust tarafında işletim sisteminin kriptografik kaynağından alınır; dağılımda modulo yanlılığı kullanılmaz.

**Parola üret ve kaydet** tek işlemde üretir ve şifreli geçmişe ekler. Son 50 kayıt tutulur; 51. kayıtta en eski kayıt çıkarılır. Tarih-saat cihaz saatinden UTC olarak saklanır ve yerel saatle gösterilir. Parolalar başlangıçta gizlidir. Satırdaki göz, kopyala ve sil düğmelerini kullanabilirsiniz. Geçmişin tümünü silmek ayrı onay ister. Kopyalama işletim sistemi panosuna yapılır.

Kişisel modda pano açık `.ttvault` kasasına aittir. Şifreleme mevcut kasa anahtarıyla XChaCha20-Poly1305 kullanır; açık metin parola/tarih diske yazılmaz. Kasa kapanınca panel kapanır. Portable `.ttvault` kopyası geçmişi içerir; `.ttbackup`, CSV/OpenSSH export ve kişisel PostgreSQL senkronizasyonu parola geçmişini içermez.

Team modunda geçmiş hesabın cihazdaki şifreli `.ttteam` önbelleğine aittir; ortak kasaya, PostgreSQL'e, işlem günlüğüne veya başka üyeye gönderilmez. Aynı kullanıcı aynı cihazda tekrar giriş yapınca geçmiş açılır. Başka cihazda ayrıca oluşur. Hesap önbelleği silinirse geçmiş de kaybolur. Bu özellik hostların gerçek SSH parolalarını otomatik değiştirmez.
