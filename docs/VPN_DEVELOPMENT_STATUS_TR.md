# VPN geliştirmesi — 0.4.3-dev.1

Tarih: 22 Ekim 2026. Dal: `codex/feature-vpn-workspace`.
Temel: v0.4.2 (`b749b35`), PostgreSQL servis kurulum kiti kontrolü `d9d9671` ile taşındı.

## Kurulu uygulamanın korunması

Kullanıcının açık talebi: kurulu kararlı TermTerm silinmeyecek veya değiştirilmeyecek.
Bu çalışmada Windows kurucusu/kaldırıcısı çalıştırılmadı, kısayollar ve kullanıcı kasaları değiştirilmedi.
VPN kaynakları ayrı worktree'de; Desktop/termterm altındaki 0.5 SQL geliştirmesine yazılmadı.
VPN geliştirme kimliği `local.termterm.desktop.vpndev`; kurucu üretimi ve güncelleme uçları kapalı.
Bu kimlik yalnız yapılandırma düzeyinde doğrulandı; Windows VPN uygulaması henüz derlenmedi.

## Son ekleme: gerçek eşzamanlı motor bağlantıları — 22 Ekim

**Masaüstü ürünü henüz gerçek VPN kullanımına hazır değildir.** Bu aşamada protokol motorları
ve gerçek ağ testleri geliştirildi; aşağıdaki kanıtlar Windows panelinden bağlantı anlamına gelmez.

- OpenVPN 3 de OpenConnect gibi önce ağ planını bildiriyor; onaydan önce TUN/rota oluşturulmuyor.
  OpenVPN Core'un otomatik Linux rota düzenlemesi yerine TunBuilder kullanılıyor. Sunucunun
  tam tünel, hariç rota, DNS, MTU ve adres bilgileri saklanıyor; desteklenmeyen politika gizlenmiyor.
- İki OpenVPN 3 ve bir OpenConnect/AnyConnect tüneli **aynı istemcide eşzamanlı** kuruldu.
  Her hedefin doğru arayüzü kullandığı, ICMP trafiği ve tünel başına eşzamanlı 1 MiB TCP aktarımının
  SHA256 bütünlüğü doğrulandı. Bu, Fortinet + GlobalProtect + OpenVPN üçlüsünün testi değildir.
- Paylaşılan Rust rezervasyon politikası bağlantı tamamlanmadan ağı ayırıyor. Çakışan dördüncü
  bağlantı ağ değişmeden reddedildi. Ortadaki tünelin kapatılması diğer ikisinin trafiğini kesmedi.
- Kontrol kanalının kapanması ve motorun çökmesinden sonra tünele ait rotalar temizlendi.
  TUN yalnız bu sürece ait ve kalıcı değil; mevcut bir arayüze bağlanmak yerine özel oluşturuluyor.
- VPN sunucusu kapatıldığında yalnız etkilenen bağlantı durdu; diğer tünelin trafiği sürdü.
  OpenVPN taşıma hatasında beş saniyelik otomatik yeniden denemeyi beklemiyor. Yeniden bağlantı
  için yeni oturum ve yeni ağ rezervasyonu gerekli; eski onay sessizce tekrar kullanılmıyor.
- OpenVPN şifreli özel anahtar parolası, statik OTP ve gerçek sunucunun istediği CRV1 dinamik OTP
  uygulandı. Doğru/yanlış parola veya OTP ve iptal yolları gerçek OpenVPN sunucusunda sınandı.
  Challenge kimliği karşılaştırmasında doğru yanıtı iptal sayan JSON sayı türü hatası düzeltildi.
  Dinamik oturum çerezi UI olaylarına gönderilmiyor; OTP kalıcı profil alanı değil.
- OpenConnect realm/grup seçimi ve izin verilen seçenek kontrolü eklendi. Kayıtlı grup ve
  bilinmeyen grupta açık seçim isteme, gerçek ocserv grubuyla doğrulandı. Derlenen kütüphanenin
  `fortinet` ve `gp` protokollerini içerdiği doğrulandı; bu ürünlerin sunucu testi hâlâ ortam bekliyor.
- Rapor kaynak/ikili SHA256 değerlerini ve test sırasında kaynağın değişmediğini içeriyor.
- Laboratuvarın anlık ürettiği sertifikalarda görülen “certificate not yet activated” hatası için
  test sertifikalarının başlangıcına beş dakikalık pay eklendi. Geçerlilik/CA kontrolü kapatılmadı;
  güvenilmeyen CA testi hâlâ reddediliyor. Bu yalnız geçici laboratuvar sertifikalarının üretimidir.

Kanıt: `artifacts/vpn-lab/concurrent-auth-20261022.json` ve aynı kaynakla tekrar koşusu
`artifacts/vpn-lab/concurrent-auth-repeat-20261022.json`: her ikisinde 28/28 geçti.
Dinamik OTP bir ara koşuda sunucudan AUTH_FAILED aldı; nedeni kesinleştirilemedi. Sır içermeyen
aşama tanılaması eklendikten sonra iki tam koşu geçti. Bu geçmiş bulgu gizlenmemeli;
gerçek üretici doğrulaması ve masaüstü bağlantı katmanı tamamlanmadan kararlılık garantisi verilmez.
Kaynak ve testler bu dalda;
laboratuvar parolaları, anahtarlar ve rapor/artifact dosyaları Git'e eklenmedi.

## Önceki adım: öncelikli üç VPN için arayüz — 12 Ekim

OpenVPN, Fortinet ve GlobalProtect önceliklendirildi. `src/VpnPreview.tsx` uygulamanın geliştirme
gezinmesine bağlanan etkileşimli **önizlemedir**. Profil türü, dosya adı seçimi/sürükleme, HTTPS
adres kontrolü, klasör/arama, taslak ekleme/düzenleme/silme ve birden fazla profili hazırlama vardır.
Dosya içeriği okunmaz; parola/OTP alınmaz; taslaklar yalnız bellekte yaşar. Bağlantı kontrolü
motorun hazır olmadığını gösterir; hiçbir profil bağlıymış gibi sunulmaz. Üretim menüsüne eklenmedi.

Windows Edge üzerinde 4 Playwright arayüz testi geçti; 1600×1000 ve 1366×768 kontrol edildi.
Görüntüler `artifacts/vpn-ui/` altında: genel ekran, OpenVPN dosya seçimi, Fortinet formu,
GlobalProtect formu ve üç profilli bağlantı hazırlığı. Bunlar native VPN bağlantı kanıtı değildir.
Rust çekirdeğinde üçüncü VPN'in iki mevcut VPN'in rotalarını değiştirmeden tümüyle karşılaştırılması
eklendi: o aşamada 7 test geçti. Bu bölüm tarihsel önizleme sonucudur; güncel sayılar aşağıdadır.

Önizleme: `pnpm dev --port 1427`, ardından `http://127.0.0.1:1427/?preview&previewVpn`.
Tekrar kontrol: `pnpm exec playwright test --config tests/ui/vpn.config.ts`.

## Tamamlanan bölüm

- Ubuntu 24.04 WSL üzerinde iki ayrı ağ namespace'i: VPN sunucusu ve istemci.
- Dış ağa varsayılan rota/NAT verilmeden, her çalıştırmada oluşturulan test sertifikaları ve kimlikleri.
- libopenconnect ile gerçek AnyConnect uyumlu ocserv bağlantısı ve tünel içinden trafik.
- OpenVPN 3 Core ile gerçek OpenVPN 2 sunucusuna sertifikalı bağlantı ve tünel içinden trafik.
- Ayrı parola/OTP formları; doğru ve yanlış OTP, yanlış parola, güvenilmeyen sertifika ve iptal.
- Motorların zorla öldürülmesinden sonra laboratuvar rotalarının temizliği.
- Motorların ana Linux ağ namespace'inde çalışmayı reddetmesi.
- On aileli profil veri modeli, parola hatırlama doğrulaması ve kalıcı OTP alanının reddi.
- OpenVPN/WireGuard import ön ayrıştırıcıları: kaynak değişmeden, komut/hook yürütmeden;
  OpenVPN'de göreli ek dosyaları birleştirme ve klasör dışına erişimi engelleme.
- IPv4/IPv6 rota ve DNS alan çakışmalarını reddeden saf politika testleri.
- Sabit Core kaynak/ruhsat hash'leri, lisans metinleri ve tekrar çalıştırılabilir laboratuvar hazırlığı.

İlk zorla kapatma testi, vpnc-script'in VPN sunucusuna eklediği rotayı geride bıraktığını yakaladı.
OpenConnect artık bu betiği çalıştırmıyor: tüneli açıp ağ planını özel IPC'den bildiriyor;
laboratuvar sürücüsü yalnız tünele bağlı test rotalarını kuruyor. Tünel dosya tanımlayıcısı
kapandığında bu rotalar da kalkıyor. **Bu, üretim için yetkili ağ servisinin tamamlandığı anlamına gelmez.**

## Son doğrulama

| Kontrol | Sonuç | Kanıt / sınır |
|---|---|---|
| VPN Rust çekirdeği | Windows ve Linux'ta 10'ar test geçti | `cargo test --locked --manifest-path vpn-core/Cargo.toml`; eşzamanlı rezervasyon, eski oturum kapanışı, DNS doğrulaması |
| Geliştirme kimliği ve kaynak sabitleme | 2 test geçti | `node --test scripts/test-vpn-isolation.mjs` |
| Gerçek laboratuvar | Aynı kaynakta iki kez 28/28 geçti | `artifacts/vpn-lab/concurrent-auth-20261022.json` ve `concurrent-auth-repeat-20261022.json`; protokol envanteri, kimlik doğrulama, üç tünel, TCP bütünlüğü, çakışma, realm ve temizlik |
| PostgreSQL kurulum kiti kontrolü | Önceki koşuda 1 test geçti | İlgili kaynak değişmedi; bu adımda tekrar çalıştırılmadı |
| Linux motor derlemesi | Geçti | CMake, kilitli Rust politika yardımcısı; upstream kaynaklar değiştirilmedi |
| Ana Linux rota/DNS değişmemesi | Geçti | Önce/sonra IPv4/IPv6 rota ve resolv.conf hash karşılaştırması |
| Geçici namespace/süreç temizliği | Geçti | Testin finally temizliği; testten sonra namespace listesi boş |

Trafik kontrolleri kısa ICMP ve eşzamanlı 1 MiB TCP/SHA256 testleridir. TermTerm SSH/SFTP
bütünlüğü, yüksek trafik, IPv6 tünel trafiği ve uyku/uyanma bu sonuçlardan çıkarılamaz.
DNS çakışma mantığı birim testidir;
gerçek DNS değişimi/geri yükleme laboratuvarda henüz uygulanmadı.

## Sıradaki işler — tamamlandı sayılmayacak

1. Linux/Windows yetkili ağ servisi: istemci doğrulama, sabit işlem listesi, hesap/oturum nesli,
   rota/DNS sahipliği, tam tünel, süreç ölümü ve yeniden başlatma temizliği. Laboratuvar kilidi
   bunlar doğrulanmadan kaldırılmayacak.
2. OpenConnect realm/grup değişimi, yeniden giriş ve sunucuya özel formların Fortinet/GlobalProtect testleri.
   OpenVPN statik/dinamik OTP ve şifreli anahtar testleri geçti; üretici OTP sağlayıcısı testi değildir.
   Ocserv testi Fortinet/GlobalProtect/F5 testi sayılmaz.
3. Windows OpenConnect/OpenVPN motorları ve imzalı sürücü gereksinimleri; WireGuard gerçek
   adaptörü; IKEv2, SonicWall ve Check Point için destek matrisi. Mevcut on aile yalnız veri modelidir.
4. VPN arayüz önizlemesini gerçek Tauri komutları/olaylarına bağlama; son kullanılanlar, şifreli
   profil/ek saklama ve import, hatırlanan kimlikler ve OTP ekranı. Önizleme kalıcı kayıt değildir.
5. Kişisel kasa/backup sürüm geçişi ve eski istemcinin yeni VPN verisini değiştirmeden reddi.
6. Team 065+ migration, ortak/kişisel kimlik ayrımı, yetkiler, geçmiş, çevrimdışı günlük,
   host/klasör VPN ilişkileri ve yetki iptali. Mevcut Team şeması değiştirilmedi.
7. SSH/chain/SFTP ön bağlantı bağımlılığı; aynı VPN işini paylaşma; VPN kopunca oturum/aktarım
   durdurma. Çoklu VPN'de çakışma ve DNS'in gerçek işletim sistemi doğrulaması.
8. Dağıtılacak her motorun kesin sürümü, eşleşen kaynakları, ikili hash'leri ve tüm bağımlılık
   lisansları; Windows/Linux geliştirme paketleri ve gerçek ekran görüntüleri.

Release/tag/main/güncelleme yayını yapılmadı. macOS/ücretli CI başlatılmadı.
Gerçek şirket VPN profilleri sağlanmadı; bu ürün/platform kombinasyonları ortam bekliyor.

## Laboratuvarı tekrar çalıştırma

Komutlar Ubuntu 24.04 WSL içinde, ayrı derleme dizininde çalışır. İlk hazırlık için bağımlılık
kurulumunu ayrıca seçin; mevcut kaynak veya paket hash'i uyuşmazsa araç durur, körlemesine güncellemez.

```bash
cd /mnt/c/Users/sonx/.codex/worktrees/vpn-workspace/termterm
sudo python3 scripts/prepare-vpn-lab.py --build-dir /root/termterm-vpn-build --install-dependencies
sudo python3 scripts/test-vpn-lab.py \
  --engines /root/termterm-vpn-build/engine \
  --ocserv-root /root/termterm-vpn-build/ocserv \
  --policy /root/termterm-vpn-build/policy-target/debug/vpn-policy-lab \
  --report artifacts/vpn-lab/concurrent-auth.json
```

Hazırlık için proje Rust araç zinciri de kurulu olmalıdır. Tekrar derlemede
`--install-dependencies` gerekmez. Betik ocserv paketini yalnız kendi dizinine
açar; ocserv sistem servisi kurmaz. Test kimlikleri/anahtarları geçici dizinden silinir, rapora
parola/OTP yazılmaz. OpenVPN 2 yalnız laboratuvar sunucusudur; ürün istemcisi Core 3 kullanır.

Windows çekirdek/config kontrolleri:

```powershell
& "$env:USERPROFILE/.cargo/bin/cargo.exe" test --locked --manifest-path vpn-core/Cargo.toml
node --test scripts/test-vpn-isolation.mjs
python tests/team_setup_package.py
```

Masaüstü geliştirme için yalnız `pnpm vpn:dev` / `pnpm vpn:build` kullanılacak; bu aşamada
bağlantı servisi olmadığından bu komutlar çalışan VPN ürününün teslimi olarak sunulmayacak.
