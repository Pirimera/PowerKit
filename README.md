# PowerKit

Windows 11 için Rust ile geliştirilen hafif araç paketi. Birincil hedef yerel ARM64; x64 ikinci hedeftir.

## 0.1.0 — AlwaysOn prototipi

- Süresiz veya 30/60/120 dakika boyunca otomatik uykuyu engelleme.
- Ekranı da açık tutma seçeneği.
- Yerel Windows kontrol penceresi ve bildirim alanı menüsü.
- Süre dolduğunda, durdurulduğunda ve süreç kapandığında güç isteğini kaldırma.
- Aynı oturumda tek süreç; yeniden açıldığında mevcut pencereyi gösterme.
- Ekran tercihini `%LOCALAPPDATA%\PowerKit\settings.txt` altında atomik kaydetme.
- Her açılışta AlwaysOn kapalı başlar; eski zamanlayıcı yeniden başlamaz.
- Explorer yeniden başladığında bildirim alanı simgesini yeniden ekleme.
- Bildirim alanı kullanılamıyorsa kontrol penceresiyle çalışma.

Penceredeki X, simge mevcutsa pencereyi gizler ve AlwaysOn çalışmaya devam eder. Tam çıkış için **PowerKit'ten çık** kullanın. Bildirim alanı yoksa X uygulamayı kapatır. Süreli oturumlarda süre uyku sırasında da ilerler; sonraki mesaj döngüsünde sona erer. Manuel uyku, kapak kapatma ve ekran koruyucu engellenmez.

Henüz installer, otomatik başlangıç, otomatik güncelleme, özel süre girişi veya markaya özel simge yoktur. AlwaysTop, ScreenRuler, ImageResizer ve MediaRenamer sonraki aşamalardır. MouseMapper kapsam dışıdır.

## Kaynak düzeni

- `src/always_on.rs`: platformdan bağımsız oturum ve sonlanma modeli.
- `src/windows.rs`: Windows arayüzü, tek süreç kontrolü ve güç API sınırı.
- `src/settings.rs`: sürümlü ayar doğrulama ve atomik kaydetme.
- `scripts/cargo.ps1`: ARM64/x64 derleme yardımcısı.

## Derleme

Rust stable, MSVC araçları ve Windows SDK gerekir. Standart geliştirme ortamında:

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

`powerkit.exe --smoke-test`, güç modlarını değiştirir, bir saniyelik süreyi çalıştırır ve yaklaşık 2,5 saniyede çıkar. Çıkış kodu 0 başarıdır. Etkileşimli masaüstü olmayan ortamlarda bildirim alanı bulunmayabilir; bu test menünün görsel doğrulandığı anlamına gelmez. Hata ayrıntısını dosyaya yazmak için isteğe bağlı `POWERKIT_DIAGNOSTIC_FILE` kullanılabilir.

7 test: zaman aşımı sınırı/uykudan dönüş, taşma, süresiz durum, hatalı ayarlar, atomik kayıt, gerçek Windows güç bayrakları ve kapsam sonunda güç isteğinin bırakılması.

Gerçek masaüstünde kalan kabul testleri: menü ve klavye gezinmesi, ayar değişikliğinin yeniden açılışta korunması, Explorer yeniden başlatma, uzun süreli bekleme, fiziksel uyku/uyanma ve farklı ekran ölçekleri.

## GitHub

Yerel depo `main` dalında hazırlanmıştır. GitHub'da boş `PowerKit` reposu oluşturulabilir; README, lisans ve .gitignore otomatik eklenmemelidir. İlk yayın öncesi lisans seçimi yapılmalıdır. Uzak depo veya push bu çalışma sırasında oluşturulmaz.
