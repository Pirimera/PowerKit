# PowerKit

Windows 11 için Rust ile geliştirilen hafif araç paketi. Birincil hedef yerel ARM64; x64 ikinci hedeftir.

## 0.2.0 — Velopack kurulum ve güncelleme

- Süresiz veya 30/60/120 dakika boyunca otomatik uykuyu engelleme.
- Ekranı da açık tutma seçeneği.
- Yerel Windows kontrol penceresi ve bildirim alanı menüsü.
- Süre dolduğunda, durdurulduğunda ve süreç kapandığında güç isteğini kaldırma.
- Aynı oturumda tek süreç; yeniden açıldığında mevcut pencereyi gösterme.
- Ekran tercihini `%LOCALAPPDATA%\PowerKit\settings.txt` altında atomik kaydetme.
- Her açılışta AlwaysOn kapalı başlar; eski zamanlayıcı yeniden başlamaz.
- Explorer yeniden başladığında bildirim alanı simgesini yeniden ekleme.
- Uyku dönüşünde kalan süreyi yeniden değerlendirme.
- Per-Monitor V2 DPI desteği ve ölçeklenen Segoe UI kontrolleri.
- Bozuk ayarda dosyayı koruyup güvenli varsayılanla açılma.
- Bildirim alanı kullanılamıyorsa kontrol penceresiyle çalışma.

Penceredeki X, simge mevcutsa pencereyi gizler ve AlwaysOn çalışmaya devam eder. Tam çıkış için **PowerKit'ten çık** kullanın. Bildirim alanı yoksa X uygulamayı kapatır. Süreli oturumlarda süre uyku sırasında da ilerler; sonraki mesaj döngüsünde sona erer. Manuel uyku, kapak kapatma ve ekran koruyucu engellenmez.

Velopack kurulum ve otomatik güncelleme eklendi. [Kurulum, GitHub Actions ve yayın rehberi](docs/RELEASING.md). Repo private iken kurucuyu GitHub oturumuyla indirin; anonim otomatik güncelleme repo public olduğunda çalışır. Otomatik başlangıç, özel süre girişi ve markaya özel simge henüz yoktur. AlwaysTop, ScreenRuler, ImageResizer ve MediaRenamer sonraki aşamalardır. MouseMapper kapsam dışıdır.

## Kaynak düzeni

- `src/always_on.rs`: platformdan bağımsız oturum ve sonlanma modeli.
- `src/windows.rs`: Windows arayüzü, tek süreç kontrolü ve güç API sınırı.
- `src/settings.rs`: sürümlü ayar doğrulama ve atomik kaydetme.
- `scripts/cargo.ps1`: ARM64/x64 derleme yardımcısı.

## Derleme

Rust 1.99.1, MSVC araçları, Windows SDK ve ARM64 TLS bağımlılığı için LLVM clang gerekir. Standart geliştirme ortamında:

```powershell
rustup target add aarch64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
cargo test --target aarch64-pc-windows-msvc
cargo clippy --all-targets --target aarch64-pc-windows-msvc -- -D warnings
```

Alternatif olarak mevcut taşınabilir ARM64 geliştirme araçlarının kökünü `POWERKIT_TOOLCHAIN_ROOT` ortam değişkeniyle verin. Kök altında `toolchain/rustup`, `toolchain/cargo` ve `msvc` bulunmalıdır:

```powershell
./scripts/cargo.ps1 -Target arm64 -CargoArgs build,--release,--offline
./scripts/cargo.ps1 -Target arm64 -CargoArgs test,--offline
```

Araçların veya MX4Lite kaynak kodunun bu depoya kopyalanması gerekmez. Uygulama MX4Lite'a bağımlı değildir. Rust 1.99.1 ile doğrulandı; manifestteki minimum Rust sürümü ayrıca test edilmedi.

## Doğrulama

[Doğrulama raporu ve fiziksel test listesi](docs/ALWAYSON-VALIDATION.md).

~~~powershell
./scripts/Test-AlwaysOn.ps1
./scripts/Test-AlwaysOn.ps1 -MeasureIdle
~~~

İlk komut derlenen ARM64 EXE'yi otomatik bulur. Pakette EXE ile aynı klasördedir. Self-test kullanıcının ayarlarına ve aktif AlwaysOn oturumuna dokunmaz; ayrı mutex/pencere sınıfı kullanır. Geri getirme testi sırasında kendi penceresi kısa süre görünebilir. Normal test yaklaşık 2,5 saniye, boşta ölçüm 15 saniye sürer. Hata ayrıntısını dosyaya almak için POWERKIT_DIAGNOSTIC_FILE ortam değişkeni kullanılabilir.

AlwaysOn ve güncelleme durum testleri ile Windows mesaj döngüsü self-testini çalıştırın; önceki sürümün masaüstü doğrulaması raporda korunmuştur. Fiziksel uyku/uyanma ve tray menüsü/Explorer yeniden başlatma kabul testleri henüz tamamlanmadı.

## GitHub ve yerel kaynak

Kaynak deposu: https://github.com/Pirimera/PowerKit

Yerel geliştirme: D:/powerkit/repos/PowerKit.

Bu sürüm yerelde hazırlanır; push ayrıca yapılır. Yayın öncesi lisans seçimi yapılmalıdır.

Taşınabilir LLVM kullanılıyorsa POWERKIT_LLVM_ROOT değişkenini LLVM köküne ayarlayın. Paketleme için .NET 10 SDK ve vpk 1.2.161 gerekir; uygulamanın kendisi .NET gerektirmez.

[0.2.0 kurulum/güncelleme doğrulaması](docs/INSTALLER-VALIDATION.md).
