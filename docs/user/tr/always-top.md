# AlwaysTop

[Kullanıcı kılavuzu](README.md) · [English](../always-top.md)

AlwaysTop bir pencereyi diğerlerinin üstünde tutar; böylece başka bir
uygulamada çalışırken bir video, bir not ya da bir terminal görünür kalır.
Aynı anda istediğiniz kadar pencere sabitlenebilir.

## Sabitleme ve bırakma

| Nereden | Nasıl |
| --- | --- |
| Klavye | Pencereye tıklayın, ardından Win+Ctrl+T tuşlarına basın. Aynı kısayol sabitlemeyi kaldırır |
| Tepsi menüsü | **Always Top** > **Pencereyi üstte tut**, en son kullandığınız pencereyi sabitler |
| Always Top sayfası | **Sabitle veya bırak** aynı işi yapar |

Sabitlenen pencere renkli bir kenarlık alır. Tepsi menüsü ve sayfa,
sabitlenen pencerenin başlığını ya da kaç pencerenin sabitlendiğini gösterir.

Bir pencereyi sabitlemek diğerlerinin sabitlemesini kaldırmaz. Pencereyi
kapatmak ya da PowerKit'ten çıkmak sabitlemeyi kaldırır.

## Sabitlenmiş pencereyi saydamlaştırma

Win+Ctrl+- öndeki sabitlenmiş pencerenin opaklığını azaltır, Win+Ctrl++
artırır; %20 ile %100 arasında %10'luk adımlarla. Tepsi menüsünde aynı
komutlar vardır. Sabitleme kalkınca pencere normal opaklığına döner.

## Ayarlar

| Ayar | Ne yapar |
| --- | --- |
| Etkinleştirme kısayolu | Öndeki pencereyi sabitler ya da bırakır. Varsayılan Win+Ctrl+T |
| Saydamlığı artır, Saydamlığı azalt | Varsayılanlar Win+Ctrl++ ve Win+Ctrl+- |
| Sistem vurgu rengini kullan | Kenarlık Windows vurgu rengini alır. Bir **Kenarlık rengi** seçmek için kapatın |
| Kenarlık opaklığı | %10 ile %100 arası. Varsayılan %100 |
| Kenarlık kalınlığı | 1 ile 32 piksel arası. Varsayılan 4 |
| Ses çal | Bir pencere sabitlendiğinde ya da bırakıldığında ve opaklığı değiştiğinde bir Windows sesi çalar |

## Bilmekte fayda var

- Sabitlemeler ve pencere opaklığı kaydedilmez. PowerKit yeniden başladıktan
  sonra hiçbir pencere sabitlenmiş değildir. Kısayollar ve kenarlık ayarları
  kaydedilir.
- PowerKit'in kendi pencereleri ve görev çubuğu sabitlenemez.
- Yönetici olarak çalışan bir uygulamanın penceresi sabitlenemez.
- Penceresi simge durumuna küçültüldüğünde kenarlık gizlenir.
