# VPN geliştirmesi — 0.4.3-dev.1

Tarih: 12 Ekim 2026. Dal: `codex/feature-vpn-workspace`.
Temel: v0.4.2 (`b749b35`), PostgreSQL servis kurulum kiti kontrolü `d9d9671` ile taşındı.

## Kurulu uygulamanın korunması

Kullanıcının açık talebi: kurulu kararlı TermTerm silinmeyecek veya değiştirilmeyecek.
Bu çalışmada Windows kurucusu/kaldırıcısı çalıştırılmadı, kısayollar ve kullanıcı kasaları değiştirilmedi.
VPN kaynakları ayrı worktree'de; Desktop/termterm altındaki 0.5 SQL geliştirmesine yazılmadı.
VPN geliştirme kimliği `local.termterm.desktop.vpndev`; kurucu üretimi ve güncelleme uçları kapalı.
Bu kimlik yalnız yapılandırma düzeyinde doğrulandı; Windows VPN uygulaması henüz derlenmedi.

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
| VPN Rust çekirdeği | 6 test geçti | `cargo test --locked --manifest-path vpn-core/Cargo.toml` |
| Geliştirme kimliği ve kaynak sabitleme | 2 test geçti | `node --test scripts/test-vpn-isolation.mjs` |
| Gerçek laboratuvar | 11 kontrol geçti | `artifacts/vpn-lab-report.json`; 9 bağlantı/hata/temizlik, 2 ana ağda çalışmayı reddetme |
| PostgreSQL kurulum kiti kontrolü | 1 test geçti | `python tests/team_setup_package.py` |
| Tekrar hazırlanabilir Linux derlemesi | Geçti | `prepare-vpn-lab.py`, kaynak ve lisans hash doğrulaması |
| Ana Linux rota/DNS değişmemesi | Geçti | Önce/sonra IPv4/IPv6 rota ve resolv.conf hash karşılaştırması |
| Geçici namespace/süreç temizliği | Geçti | Testin finally temizliği; testten sonra namespace listesi boş |

Trafik kontrolleri kısa ICMP testleridir. SSH/SFTP bütünlüğü, yüksek trafik, IPv6 tünel trafiği,
çoklu VPN ve uyku/uyanma bu sonuçlardan çıkarılamaz. DNS çakışma mantığı birim testidir;
gerçek DNS değişimi/geri yükleme laboratuvarda henüz uygulanmadı.

## Sıradaki işler — tamamlandı sayılmayacak

1. Linux/Windows yetkili ağ servisi: istemci doğrulama, sabit işlem listesi, hesap/oturum nesli,
   rota/DNS sahipliği, tam tünel, süreç ölümü ve yeniden başlatma temizliği. Laboratuvar kilidi
   bunlar doğrulanmadan kaldırılmayacak.
2. OpenVPN statik/dinamik OTP ve şifreli anahtar parolası; OpenConnect realm/grup değişimi,
   yeniden giriş ve sunucuya özel formlar. Ocserv testi Fortinet/GlobalProtect/F5 testi sayılmaz.
3. Windows OpenConnect/OpenVPN motorları ve imzalı sürücü gereksinimleri; WireGuard gerçek
   adaptörü; IKEv2, SonicWall ve Check Point için destek matrisi. Mevcut on aile yalnız veri modelidir.
4. Gerçek VPN paneli, klasör/arama/son kullanılanlar, şifreli profil/ek saklama, dosya seçimi,
   hatırlanan kimlikler ve OTP ekranı; Tauri komutları/olayları. Henüz uygulamaya bağlı değildir.
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
  --report artifacts/vpn-lab-report.json
```

Tekrar derlemede `--install-dependencies` gerekmez. Betik ocserv paketini yalnız kendi dizinine
açar; ocserv sistem servisi kurmaz. Test kimlikleri/anahtarları geçici dizinden silinir, rapora
parola/OTP yazılmaz. OpenVPN 2 yalnız laboratuvar sunucusudur; ürün istemcisi Core 3 kullanır.

Windows çekirdek/config kontrolleri:

```powershell
& "$env:USERPROFILE/.cargo/bin/cargo.exe" test --locked --manifest-path vpn-core/Cargo.toml
node --test scripts/test-vpn-isolation.mjs
python tests/team_setup_package.py
```

Masaüstü geliştirme için yalnız `pnpm vpn:dev` / `pnpm vpn:build` kullanılacak; bu aşamada
VPN paneli olmadığından bu komutlar çalışan VPN ürününün teslimi olarak sunulmayacak.
