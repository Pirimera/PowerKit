# PowerKit kullanıcı kılavuzu

[English](../README.md)

PowerKit, tek bir tepsi uygulamasında toplanan küçük Windows 11 araçlarıdır.
Bu kılavuz araçların nasıl kullanıldığını anlatır. Arayüz Türkçe, İngilizce ve
Arapçadır; bu kılavuz Türkçe arayüzdeki adları kullanır.

| Araç | Ne yapar |
| --- | --- |
| [AlwaysOn](always-on.md) | Windows'u, istenirse ekranı da uyanık tutar |
| [AlwaysTop](always-top.md) | Pencereleri diğerlerinin üstünde tutar |
| [ScreenRuler](screen-ruler.md) | Ekrandaki pikselleri ölçer |
| [ImageResizer](image-resizer.md) | Bir ya da çok sayıda resmi yeniden boyutlandırır |
| [MediaRenamer](media-renamer.md) | Fotoğraf ve videoları çekim zamanına göre yeniden adlandırır |
| [PowerStore](power-store.md) | Paket yöneticileriniz üzerinden yazılım kurar, günceller ve kaldırır |
| [PowerMouse](power-mouse.md) | Fare düğmelerini yeniden eşler ve bir Eylem halkası ekler |

## Gereksinimler ve kurulum

- ARM64 ya da x64 bir bilgisayarda Windows 11.
- Windows App Runtime 2.4 veya üstü. Microsoft Store bunu PowerKit ile
  birlikte kurar.

PowerKit Microsoft Store üzerinden ve aynı Store paketini kuran winget ile
dağıtılır. Store listesi henüz yayında değil. Yayımlanana kadar PowerKit
yalnızca kaynak koddan derlenebilir; [geliştirme
kılavuzuna](../../development.md) bakın. GitHub Releases bir indirme kanalı
değildir.

Güncellemeler Store'dan gelir. PowerKit'in kendi güncelleyicisi yoktur.

## Pencere ve tepsi simgesi

PowerKit arka planda çalışır ve bildirim alanında (sistem tepsisi) bir simge
gösterir. Ayarlar penceresinin solunda bir gezinti şeridi vardır: önce Ana
Sayfa, ardından her araç için bir sayfa, en altta Ayarlar.

- Pencerenin X düğmesi pencereyi gizler. PowerKit çalışmaya devam eder.
- Çıkmak için tepsi menüsünde ya da Ayarlar sayfasında **PowerKit'ten çık**
  komutunu seçin.
- PowerKit Windows ile başladığında tepside kalır ve pencereyi açmaz.
- PowerKit'i Başlat menüsünden yeniden çalıştırmak, zaten çalışan kopyanın
  penceresini açar.

## Ana Sayfa

Ana Sayfa her aracı bir anahtarla listeler. Bir aracı kapatmak onu gezinti
şeridinden ve tepsi menüsünden kaldırır ve Windows genelinde durdurur.
Başlangıçta PowerMouse dışındaki bütün araçlar açıktır; PowerMouse siz açana
kadar kapalıdır.

Ana Sayfa ayrıca sürümü, mimariyi, PowerKit'in bir MSIX paketinden çalışıp
çalışmadığını, ayar dosyasının yerini ve sürüm notlarını gösterir.

## Tepsi menüsü

Menüyü açmak için tepsi simgesine tıklayın. Her aracın bir girdisi vardır:

| Girdi | İçerik |
| --- | --- |
| Power Store | Kullanılabilir güncelleme sayısı, **Tümünü güncelle**, **Güncellemeleri denetle**, **PowerStore'u aç** |
| Always On | [AlwaysOn](always-on.md) sayfasında anlatılan kipler; etkin olan işaretlidir |
| Always Top | **Pencereyi üstte tut**, saydamlık komutları ve **Ses çal** |
| Screen Ruler | Ölçümü başlatır ya da durdurur |
| Media Renamer | Geçerli önizlemenin durumu ve **MediaRenamer'ı aç** |
| Image Resizer | **Resimleri yeniden boyutlandır** |
| Power Mouse | PowerMouse'u açıp kapatan bir onay işareti |
| Kısayollar | Her aracın geçerli kısayolu |

Media Renamer ve Image Resizer başlangıçta listelenmez, çünkü çoğunlukla Dosya
Gezgini'nden başlatılırlar. Menünün hangi girdileri hangi sırayla göstereceğini
Ayarlar sayfasındaki **Tepsi menüsü** bölümünden seçin. Menü **PowerKit'i aç**
ve **PowerKit'ten çık** ile biter.

## Ayarlar

Ayarlar sayfası PowerKit'in bütününü ilgilendiren ayarları içerir. Her aracın
kendi ayarları o aracın sayfasındadır.

| Ayar | Ne yapar |
| --- | --- |
| Başlangıçta çalıştırma | Windows'ta oturum açtığınızda PowerKit'i başlatır. Varsayılan olarak kapalıdır |
| Dil | Windows varsayılanı, Türkçe, English ya da العربية |
| Tema | Windows varsayılanı, Açık ya da Koyu |
| Akrilik şeffaflığı | Pencere zemininin arkadaki masaüstünü ne kadar örttüğünü %50 ile %100 arasında ayarlar. Pencere hiçbir zaman tümüyle saydam olmaz |
| Sistem tepsisi simgesini göster | Kapalıyken tepsi simgesini gizler. O durumda pencereyi açmak için PowerKit'i Başlat menüsünden yeniden çalıştırın |
| Tepsi menüsü | Tepsi menüsünün listelediği araçlar ve sıraları |
| Ayarları varsayılanlara döndür | Yalnızca bu sayfadaki ayarları sıfırlar. Araçların ayarları ve geçmişleri korunur |

## Kısayollar

| Kısayol | Eylem |
| --- | --- |
| Win+Ctrl+Shift+A | AlwaysOn'u açar ya da kapatır |
| Win+Ctrl+T | Öndeki pencereyi sabitler ya da bırakır |
| Win+Ctrl++ ve Win+Ctrl+- | Sabitlenmiş pencerenin opaklığını artırır ya da azaltır |
| Win+Ctrl+Shift+M | ScreenRuler'ı başlatır ya da durdurur |

- Bunlar varsayılanlardır. Birini değiştirmek için aracının sayfasında
  kısayola tıklayın, ardından yeni tuşlara basın.
- Varsayılan bir kısayolu başka bir uygulama kullanıyorsa PowerKit aynı tuşları
  Ctrl yerine Alt ile dener; örneğin Win+Alt+T.
- Kaydedilemeyen bir kısayol sayfada nedeniyle birlikte "Etkin değil" olarak
  işaretlenir. Onun için başka tuşlar seçin.

## Komut satırı

`powerkit.exe` aşağıdaki bağımsız değişkenleri kabul eder. PowerKit zaten
çalışıyorsa isteği çalışan kopya yerine getirir.

| Bağımsız değişken | Etkisi |
| --- | --- |
| `--background` | Pencereyi açmadan tepside başlar |
| `--always-on on` | Süresiz uyanık tutar |
| `--always-on 90` | 90 dakika uyanık tutar |
| `--always-on off` | AlwaysOn'u kapatır |
| `--always-on-pid 1234` | O kimlikteki işlem çalıştıkça uyanık tutar |
| `--resize <dosyalar>` | ImageResizer penceresini o resimlerle açar |
| `--rename <dosyalar ya da klasörler>` | Onları MediaRenamer'da önizleme olarak açar |

## PowerKit verilerini nerede tutar

Her şey `%LOCALAPPDATA%\PowerKit` klasöründedir. Klasör kaldırma işleminden
sonra da kalır; temiz bir başlangıç için elle silin.

| Dosya ya da klasör | İçerik |
| --- | --- |
| `settings.txt` | Bütün ayarlar |
| `media-renamer-history.txt` | MediaRenamer'ın, Geri al için kullanılan yeniden adlandırma geçmişi |
| `power-store-history.txt` | PowerStore'un işlem geçmişi |
| `power-store-installed.txt` | Sonraki açılışta ilk gösterilen son paket listesi |
| `icons`, `icon-database.json` | İndirilen paket logoları |
| `mouse-assets` | Desteklenen farelerin indirilen görselleri |

PowerKit çalışan oturumları kaydetmez: yeniden başlatıldıktan sonra AlwaysOn
kapalıdır ve hiçbir pencere sabitlenmiş değildir.

Ağı kullanan özellikler şunlardır: PowerStore, paket yöneticileriniz üzerinden
ve paket logoları için (logo indirme PowerStore ayarlarından kapatılabilir);
PowerMouse, tanınan bir Logitech farenin görselini indirmek için.

## Sorun giderme

| Sorun | Ne yapmalı |
| --- | --- |
| "Ayarlar için Windows App Runtime 2.4 veya üstü gerekir" | Windows App Runtime'ı kurun ya da güncelleyin, sonra PowerKit'i yeniden başlatın |
| Bir kısayol hiçbir şey yapmıyor | Aracın sayfasını açın. Kısayolun altındaki "Etkin değil" notu, o tuşları başka bir uygulamanın kullandığını gösterir; başka tuşlar kaydedin |
| "Ayar okunamadı; varsayılan tercihlerle başlatıldı" | Ayar dosyası bozulmuş. PowerKit siz bir ayarı değiştirene kadar dosyaya dokunmaz; önce bir kopyasını alabilirsiniz |
| Tepsi simgesi görünmüyor | Gizlenmiş olabilir. PowerKit'i Başlat menüsünden yeniden çalıştırın ve Ayarlar'da **Sistem tepsisi simgesini göster** seçeneğini açın. Windows simgeyi görev çubuğunun taşma alanında da tutabilir |
| Bir araç gezinti şeridinde yok | Kapatılmış. Ana Sayfa'dan açın |
| AlwaysTop ya da PowerMouse yönetici olarak çalışan bir uygulamada hiçbir şey yapmıyor | Windows, normal bir uygulamanın yükseltilmiş bir uygulamayı denetlemesine izin vermez ve PowerKit hiçbir zaman yönetici olarak çalışmaz. Böyle bir pencere sabitlenemez; o pencere öndeyken PowerMouse fare düğmelerini olduğu gibi bırakır |

## Yardım ve geri bildirim

Hataları ve soruları [GitHub Issues](https://github.com/Pirimera/PowerKit/issues)
üzerinden iletin; nelerin eklenmesi gerektiği [SUPPORT](../../../SUPPORT.md)
dosyasında yazar. Güvenlik sorunlarını [SECURITY](../../../SECURITY.md)
dosyasında anlatıldığı gibi bildirin.
