# PowerKit

![PowerKit](../../brand/social/readme-header-1280x240.png)

Rust ile geliştirilen hafif Windows araç paketi. Birincil hedef Windows 11 ARM64,
ikinci hedef x64. macOS ve Linux [yol haritasındadır](../roadmap.md#platforms),
henüz uygulanmamıştır. Microsoft ile bağlantılı olmayan bağımsız, açık kaynaklı
bir projedir.

Araçların nasıl kullanıldığı [kullanıcı kılavuzunda](../user/tr/README.md)
anlatılır.

| Araç | Ne yapar |
| --- | --- |
| [AlwaysOn](../user/tr/always-on.md) | Windows'u, istenirse ekranı da uyanık tutar |
| [AlwaysTop](../user/tr/always-top.md) | Pencereleri diğerlerinin üstünde tutar |
| [ScreenRuler](../user/tr/screen-ruler.md) | Ekrandaki pikselleri kenar algılamayla ölçer |
| [ImageResizer](../user/tr/image-resizer.md) | Resimleri toplu olarak yeniden boyutlandırır |
| [MediaRenamer](../user/tr/media-renamer.md) | Fotoğraf ve videoları çekim zamanına göre, önizleme ve geri alma ile yeniden adlandırır |
| [PowerStore](../user/tr/power-store.md) | WinGet, Scoop, Chocolatey ve geliştirici paket yöneticilerinin paketlerini tek tabloda kurar, günceller ve kaldırır |
| [PowerMouse](../user/tr/power-mouse.md) | Her farenin düğmelerini yeniden eşler ve bir Eylem halkası ekler; Bluetooth ile bağlı MX Master 4 için ek düğmeler, pil, DPI ve tekerlek ayarları |

Yedi araç da uygulanmıştır; MediaRenamer'ın fiziksel kabul testleri ve
PowerStore'un etkileşimli yükleyici denetimleri sürüyor. Arayüz Türkçe,
İngilizce ve Arapçadır; katkıcı belgelerinin ana dili İngilizcedir.

## Kurulum

Ürün dağıtımı Microsoft Store (ana kanal) ve Store paketini gösteren
winget'tir. MSIX şablonu ve yerel paketleme betiği hazırdır; Store listesi
henüz yayında değildir. GitHub Releases ürün indirme kanalı değildir.

Paket MSIX'tir ve Windows App Runtime 2.4 veya üstünü Store bağımlılığı olarak
bildirir. Store listesi yayımlanana kadar geliştiriciler paketsiz derlemeyi
çalıştırır ya da Geliştirici Modu açıkken kendinden imzalı bir test MSIX'i
kurar; Store paketi mağaza tarafından yeniden imzalanır. Uygulama içi
güncelleme yoktur.

## Bağlantılar

- [Kullanıcı kılavuzu](../user/tr/README.md)
- [Ana rehber](../../README.md)
- [Katkı süreci](../../CONTRIBUTING.md)
- [Geliştirme ve test](../development.md)
- [Yayınlama](../releasing.md)
- [Fiziksel kabul listesi](../testing.md)
- [Yol haritası](../roadmap.md)
- [MIT lisansı](../../LICENSE)
