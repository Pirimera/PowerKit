# ImageResizer

[Kullanıcı kılavuzu](README.md) · [English](../image-resizer.md)

ImageResizer tek bir resmi ya da bütün bir seçimi tek adımda yeniden
boyutlandırır. Bir şey yazmadan önce her zaman ne yazacağını gösterir.

## Resimleri yeniden boyutlandırma

1. Dosya Gezgini'nde bir ya da daha çok resim seçin, sağ tıklayın ve **Image
   Resizer ile yeniden boyutlandır** komutunu seçin. Image Resizer sayfasındaki
   **Buradan resim seç…** düğmesini ya da tepsi menüsündeki **Resimleri yeniden
   boyutlandır** öğesini de kullanabilirsiniz.
2. Açılan pencerede bir boyut seçin.
3. **Önizle**'yi seçin. Liste her dosyanın yeni adını ve yeni ölçülerini
   gösterir. Henüz hiçbir şey yazılmaz.
4. **Yeniden boyutlandır**'ı seçin.

Önizlemeden sonra bir boyutu ya da seçeneği değiştirmek önizlemeyi geçersiz
kılar; yeniden boyutlandırmadan önce tekrar önizleyin. İşlem çalışırken iptal
edilebilir; sıra gelmemiş dosyalara dokunulmaz.

## Boyutlar

| Boyut | Şunun içine sığar |
| --- | --- |
| Küçük | 854 × 480 piksel |
| Orta | 1366 × 768 piksel |
| Büyük | 1920 × 1080 piksel |
| Telefon | 320 × 568 piksel |
| Özel | Piksel, yüzde, inç ya da santimetre olarak girdiğiniz genişlik ve yükseklik |

Özel boyutun bir de sığdırma biçimi vardır:

| Sığdırma | Sonuç |
| --- | --- |
| Sığdır | Resim boyutun içine sığacak şekilde ölçeklenir ve oranlarını korur |
| Doldur | Resim boyutu doldurur, sığmayan yerleri kırpılır |
| Uzat | Resim oranlarını korumadan tam olarak o boyutu alır |

## Seçenekler

Pencere Image Resizer sayfasındaki seçeneklerle açılır; pencerede yaptığınız
değişiklikler de oraya kaydedilir.

| Seçenek | Ne yapar |
| --- | --- |
| Yalnızca küçült, büyütme | Zaten boyuttan küçük olan resimleri olduğu gibi bırakır |
| Özgün resimlerin üzerine yaz | Sonucu özgün dosyanın üzerine yazar; aşağıya bakın |
| Yönü yoksay | Genişlik ve yüksekliği yer değiştirebilir sayar; dikey bir resim boyutun dikey biçimine sığdırılır. Varsayılan olarak açıktır |
| Görüntüyü etkilemeyen veriyi sil | Kamera bilgileri gibi üst verileri sonuçtan çıkarır |
| Değiştirilme tarihini koru | Sonuç, özgün dosyanın değiştirilme tarihini korur |
| Dosya adı | Kopyanın adı. Varsayılan `%1 (%2)` |
| JPEG kalitesi | 1 ile 100 arası. Varsayılan 90 |
| PNG geçişim, TIFF sıkıştırma | O biçimlerin kodlama ayrıntıları |
| Yedek kodlayıcı | Özgün dosyanın biçimi yazılamadığında kullanılan biçim. Varsayılan JPEG |

Dosya adı parçaları:

| Parça | Anlamı |
| --- | --- |
| `%1` | Özgün dosya adı |
| `%2` | Boyutun adı |
| `%3`, `%4` | Seçtiğiniz genişlik ve yükseklik |
| `%5`, `%6` | Sonucun genişliği ve yüksekliği |

## Özgün dosyaya ne olur

- Varsayılan olarak ImageResizer özgün dosyanın yanına bir kopya yazar;
  örneğin `Tatil (Küçük).jpg`. Kopya var olan bir dosyanın üzerine hiçbir
  zaman yazılmaz; ad doluysa sonuna bir sayı eklenir.
- **Özgün resimlerin üzerine yaz** açıkken özgün dosya önce `.bak` dosyası
  olarak saklanır. Yeni resim yazıldıktan sonra ImageResizer bu yedeği Geri
  Dönüşüm Kutusu'na taşımayı dener.

## Bilmekte fayda var

- Desteklenen dosyalar: BMP, DIB, GIF, HEIC, JPEG, PNG, TIFF, WDP ve WebP.
  HEIC ve WebP, Windows kodekleri üzerinden okunur. Böyle bir dosya
  okunamıyorsa Microsoft Store'dan HEIF Image Extensions ya da WebP Image
  Extensions uzantısını kurun.
- Tek seferde en çok 1000 dosya işlenir.
- Hareketli GIF gibi çok kareli bir dosyanın yalnızca ilk karesi yeniden
  boyutlandırılır.
- ImageResizer resimleri yapay zekâ ile büyütmez.
