# MediaRenamer

[Kullanıcı kılavuzu](README.md) · [English](../media-renamer.md)

MediaRenamer fotoğraf ve videoları çekildikleri tarih ve saate göre yeniden
adlandırır; örneğin `IMG_4821.JPG`, `2026-05-01_14-03-22.JPG` olur. Dosyalar
klasörlerinde kalır. İçerikleri ve üst verileri hiçbir zaman değiştirilmez,
hiçbir dosyanın üzerine yazılmaz ve her yeniden adlandırma geri alınabilir.

## Dosyaları yeniden adlandırma

1. Dosya ekleyin. Şunlardan herhangi biri olur:
   - Fotoğraf, video ya da klasörleri Media Renamer sayfasına sürükleyin.
   - Sayfada **Dosya ekle** ya da **Klasör ekle**'yi seçin.
   - Dosya Gezgini'nde dosyalara ya da klasörlere sağ tıklayıp **MediaRenamer
     ile yeniden adlandır** komutunu seçin.
2. Listeyi denetleyin. Her satır geçerli adı, yeni adı, dosyanın çekim zamanını
   ve bir durumu gösterir.
3. Farklı adlar istiyorsanız **Ad şablonu**nu değiştirin. Yeni adlar siz
   yazdıkça güncellenir.
4. Yeniden adlandırmak istemediğiniz satırların işaretini kaldırın.
5. **Yeniden adlandır**'ı seçin.

5. adımdan önce diskte hiçbir şey değişmez.

Bir klasör yalnızca doğrudan içindeki dosyaları ekler. Altındaki her şeyi de
almak için klasörü eklemeden önce **Alt klasörleri dahil et**'i açın. Gizli
dosyalar ve sistem dosyaları atlanır; bir liste en çok 20.000 dosya alır.

## Ad şablonu

Şablon, süslü parantez içinde belirteçler içeren bir metindir. Parantezlerin
dışındaki metin olduğu gibi kullanılır. Belirteç adları büyük/küçük harfe
duyarlıdır: `{MM}` ay, `{mm}` dakikadır.

| Belirteç | Değer |
| --- | --- |
| `{yyyy}`, `{yy}` | Yıl, dört ya da iki basamaklı |
| `{MM}`, `{dd}` | Ay, gün |
| `{HH}`, `{mm}`, `{ss}` | Saat (24 saatlik), dakika, saniye |
| `{fff}` | Dosya kaydetmişse milisaniye |
| `{make}`, `{model}` | Kamera markası ve modeli |
| `{original}` | Uzantısı olmadan geçerli dosya adı |
| `{counter}`, `{counter:5}` | Listedeki sıra; 3 basamaklı ya da verdiğiniz basamak sayısıyla |

Varsayılan `{yyyy}-{MM}-{dd}_{HH}-{mm}-{ss}` şablonudur. **Hazır şablonlar**
birkaç yaygın şablon sunar; **Belirteç ekle** şablonun sonuna bir belirteç
ekler. **Varsayılana dön** kayıtlı varsayılana geri döner.

- Bir dosya için değeri olmayan belirteç (örneğin bir ekran görüntüsünde
  kamera modeli), önündeki ayırıcıyla birlikte atlanır.
- Dosya uzantısı korunur.
- Windows'un dosya adında izin vermediği karakterler `_` olur.
- Bilinmeyen bir belirteç ya da eşleşmeyen parantez içeren şablon hata
  gösterir ve uygulanamaz.

## Durumlar

| Durum | Anlamı |
| --- | --- |
| Hazır | Dosya yeniden adlandırılacak |
| Değişmeyecek | Dosya zaten yeni adı taşıyor |
| Tarih yok | Dosyada çekim zamanı kayıtlı değil; dışarıda bırakılır |
| Çakışma çözüldü | Başka bir dosya aynı yeni adı aldığı için bir sonek eklendi |
| Yalnızca çevrimiçi | Bu bilgisayarda bulunmayan bir bulut dosyası. Okunmaz, çünkü okumak onu indirirdi |
| Okunamadı | Üst veri okunamadı |
| … gerekli | Bu biçim için bir Windows uzantısı eksik; [Desteklenen dosyalar](#desteklenen-dosyalar) bölümüne bakın |
| Ad çakışması çözülemedi | Boş bir ad bulunamadı |

**Tarihi olmayan dosyalar:** onları dosyanın son değiştirilme zamanına göre
adlandırmak için **Tarih yoksa değiştirilme zamanını kullan**'ı açın.
Kopyalanan ya da düzenlenen bir dosyanın değiştirilme zamanı çekim zamanından
farklı olabilir; bu satırları denetleyin.

**Aynı yeni adı alan iki dosya:** dosyada kayıtlıysa ikincisine milisaniye
eklenir (`2026-05-01_14-03-22_457`), değilse `_1`, `_2` diye devam eder.

Arama kutusu ve durum filtreleri listeyi daraltır. Bir satırın
**Ayrıntılar**ı zamanın nereden geldiğini ve dosyanın bütün üst verisini
gösterir.

## Live Photo ve eş dosyalar

Aynı klasörde aynı adı paylaşan dosyalar birbirine aittir: bir fotoğraf ile
Live Photo videosu, bir RAW dosyası ile JPEG'i, `.aae` ve `.xmp` eş dosyaları.
Tek satır olarak görünür, aynı yeni adı alır ve kendi uzantılarını korurlar.
Yalnızca fotoğrafı eklemiş olsanız bile eş dosyalar eklenir.

## Geri alma

**Geçmiş** her yeniden adlandırma işlemini listeler. **Geri al** bir işlemin
dosyalarına eski adlarını geri verir. Geçmiş, PowerKit yeniden başladığında
korunur; bir işlem sonradan da geri alınabilir.

- Bir dosya yalnızca o zamandan beri değişmediyse ve eski adı hâlâ boşsa geri
  taşınır. Diğer dosyalar atlanır ve listelenir.
- Geri alma hiçbir dosyayı silmez ve hiçbirinin üzerine yazmaz.
- PowerKit bir işlemin ortasında kapandıysa Geçmiş o işlemi "Yarım kaldı,
  kurtarıldı" olarak gösterir ve yapılan kısmı geri alabilir.
- Geçmiş en son 50 işlemi tutar.

Önizleme ile **Yeniden adlandır** arasında bir dosya değişir ya da kaybolursa
hiçbir şey yeniden adlandırılmaz. Dosyaları yeniden denetlemek için
**Yenile**'yi seçin.

## Ayarlar

Dişli düğmesi ayarları açar.

| Ayar | Ne yapar |
| --- | --- |
| Varsayılan şablon | Sayfanın açıldığı şablon. **Geçerli şablonu varsayılan yap** kullanımdaki şablonu kaydeder |
| Klasör eklerken alt klasörleri dahil et | O anahtarın başlangıç durumu |
| Tarih yoksa değiştirilme zamanını kullan | O anahtarın başlangıç durumu |
| Uzantıları küçük harfe çevir | `.JPG` yerine `.jpg` yazar. Varsayılan olarak kapalıdır |

## Desteklenen dosyalar

| Tür | Uzantılar | Zamanın kaynağı |
| --- | --- | --- |
| JPEG | `.jpg` `.jpeg` `.jpe` | EXIF |
| HEIF | `.heic` `.heif` `.hif` | EXIF. HEIF Image Extensions gerekir |
| RAW | `.dng` `.cr2` `.cr3` `.nef` `.arw` | EXIF. Raw Image Extension gerekir |
| PNG, WebP | `.png` `.webp` | Dosyada varsa EXIF. WebP için WebP Image Extensions gerekir |
| Video | `.mp4` `.mov` `.m4v` `.3gp` | Dosyada saklanan kayıt zamanı |
| AVI | `.avi` | Windows okuyabiliyorsa kodlama tarihi |

Yukarıda adı geçen uzantılar Microsoft Store'dan gelir. PowerKit onları
kurmaz.

## Bilmekte fayda var

- Ad, çekim yerindeki yerel saati gösterir. Yurt dışında çekilen bir telefon
  fotoğrafı, telefonda gördüğünüz saati korur.
- Videoların çoğu zamanı saat dilimi olmadan UTC olarak saklar. MediaRenamer
  bunu bu bilgisayarın saat dilimiyle çevirir; seyahatte kaydedilen bir video
  iki dilim arasındaki fark kadar kayabilir. iPhone videoları kendi saat
  dilimini kaydeder ve bundan etkilenmez.
- Android telefonda kaydedilen bir video, kaydın başladığı zamana göre
  adlandırılır.
- PNG, WebP ve AVI dosyalarında çoğu zaman çekim zamanı yoktur; "Tarih yok"
  görünürler.
- Eklediğiniz klasördeki diğer dosyalar atlanır ve sayılır, yeniden
  adlandırılmaz.
- HEIC, Live Photo ve RAW dosyaları, eksik uzantılar ve yalnızca çevrimiçi
  bulut dosyaları henüz gerçek cihazlardan gelen dosyalarla denenmedi. Böyle
  dosyaları yeniden adlandırmadan önce önizlemeye dikkatle bakın.
