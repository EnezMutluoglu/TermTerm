# Windows tanı kaynakları — çalıştırma paketi değildir

4 Kasım 2026 kaynak koruma kopyası. [Güncel VPN devri](../docs/HANDOFF_VPN_TR.md).

- `wintun-close-probe-20261103-v1`: özgün saf adaptör Rust kaynağı + Cargo kilidi. Önceki native olgu yaklaşık 1 s'de kayıt yokluğudur; özgün wrapper `ok=false/ExitCode=null` kaldı.
- `wintun-configured-probe-20261103-v2`: yarım kalan Rust kaynağı + Cargo kilidi. **E0603:** `src/main.rs:123` private `network::route_apply_order` çağrısı. **Üç inert test çalışmadı; derlenebilir/çalıştırılabilir teslim değildir.** Bu kopyada hata düzeltilmedi.
- `test-wintun-probe-exit-code-20261103-v1.ps1`: yalnız inert `cmd /c exit 0/17` çocukları ve PowerShell handle/ExitCode self-check kaynağı; ağ/UAC işlemi yapmaz. PowerShell 5.1'de yeniden kontrol içindir; çıktı yanındaki ignored test klasörüne yazılır, mevcut çıktının üstüne yazmaz.

Rust dosyaları özgün `.lab` kaynağından byte eşit kopyadır; göreli broker bağımlılığı yeni konumdan da çözülür. Disposable GUID/adres/domain sabitleri yeni makine onayı veya gerçek profil değildir. Native Rust `--probe...` komutları ağ değiştirir; bu klasördeki kaynakları doğrudan çalıştırmayın. Eski tanı UAC/native wrapper'ları, sabit EXE/DLL hash'leri, ham baseline, journal, rapor ve ikililer kopyalanmadı; eski bilgisayarda ignored `.lab`/`.tools` içinde korunur.

Sonraki geliştirici önce standalone v2 yardımcı erişimini dar yerel düzenlemeyle düzeltmeli; üretim API'sini açmamalı. Derleme/inert test, bağımsız kaynak incelemesi, yeni GUID/fixture, doğru Wintun DLL hash'i, yeni freeze/pin ve incelenmiş wrapper hazırlığı tamamlanmadan native/UAC çalışma yapılmaz. `CONFIGURED_EXE_HASH_PENDING` içeren eski wrapper tamamlanmamıştır; Git'e alınmadı. 150 s DNS öncesi bekleme, bağlı DNS/engine/trafik/Windows resolver etkilerinin deneyi değildir.

Bu devirde derleme, self-check veya native deneme çalıştırılmadı. Mevcut kaynağı korumak dışında ürün mantığı değiştirilmedi.
