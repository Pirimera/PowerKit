# AlwaysOn 0.1.1 — doğrulama ve fiziksel kabul

## Doğrulanan

- Windows 11 ARM64 / Rust 1.99.1 release derlemesi.
- 9 test: zaman sınırı/taşma, süresiz durum, uyku dönüşü uzlaştırma, hatalı ayarlar, atomik kaydetme, başarısız değiştirmede hedefin korunması ve geçici dosya temizliği, Windows güç bayrakları ve RAII temizliği.
- Clippy -D warnings; rustfmt.
- İzole Windows mesaj döngüsü: süresiz/durdur komutları, ekran bayrağı, simüle uyku dönüşü, gerçek bir saniyelik zamanlayıcı ve aktifken Çık komutuyla güç isteğinin bırakılması.
- Aynı self-test: küçültülmüş pencereyi geri getirme, 96/144/192 DPI kontrol ölçüleri, olmayan tray simgesini algılama, simüle Explorer mesajında durumun korunması.

## Gerçek masaüstü

Computer Use ile ARM64 uygulamasında süresiz açma, Tab+Space ile durdurma, ekran tercihinin kaydedilmesi, çıkış/yeniden açılışta tercihin korunması ve AlwaysOn'un kapalı başlaması doğrulandı. Küçültülmüş pencere ikinci açılışta aynı pencere kimliğiyle geri geldi. Mevcut ekran ölçeğinde kontroller ve açıklamalar kesilmiyor.

Tray simgesi ekleme başarılıydı; doğrudan tray menüsüne tıklama ve gerçek Explorer yeniden başlatma yapılmadı. Test sonunda ekran tercihi başlangıçtaki kapalı değerine döndürüldü ve uygulamadan çıkıldı.

## Kaynak tüketimi

İzole, gizli, AlwaysOn kapalı ve tray simgesi olmayan test sürecinde 10 saniyelik örnek: CPU zamanı artışı **0 ms**, working set **10,80 MiB**, private bytes **1,77 MiB**. Bu bir masaüstü performans garantisi değildir; ölçüm çözünürlüğü ve test ortamı ile sınırlıdır.

Tekrar ölçmek için scripts/Test-AlwaysOn.ps1 -MeasureIdle kullanılır. Test 15 saniyede kendiliğinden çıkar. Normal self-test yaklaşık 2,5 saniye sürer. Test kullanıcının ayarlarını okumaz/yazmaz ve normal uygulamayla aynı mutex'i kullanmaz.

## Kullanıcının fiziksel testleri

Önce çalışmalarınızı kaydedin. PowerToys Awake gibi diğer uyku engelleyicileri ve video oynatmayı kapatın. Windows ekran/uyku sürelerini not edin; test için kısaltırsanız sonrasında eski değerlerine dönün. PowerKit güç planını değiştirmez.

| Test | İşlem | Beklenen |
|---|---|---|
| Başlangıç karşılaştırması | PowerKit kapalıyken normal uyku süresini bekleyin. | Sistem uyuyabilmeli; başka engelleyici varsa önce onu ayırın. |
| Sistem açık, ekran serbest | Süresiz modu açın, ekran seçeneği kapalı olsun; giriş yapmadan normal zaman aşımlarını geçirin. | Ekran kapanabilir; sistem otomatik uyumaz. |
| Ekran da açık | Ekran seçeneğini açıp aynı süre bekleyin. | Ekran ve sistem açık kalır. |
| Durdurma ve çıkış | Önce Durdur'u, ayrı denemede aktifken Çık'ı kullanın. | Normal boşta kalma süresi sonunda otomatik uyku geri gelir. |
| Süre dolması | 30 dakika modunu seçin. | Süre bitince durum kapalı olur; ardından normal Windows uyku süresi uygulanır. |
| Manuel uyku/uyanma | Süreli modda Başlat menüsünden Uyku seçip uyandırın. | Manuel uyku engellenmez, süre uzamaz; geçmişse kapalı olur. |
| Bildirim alanı | X ile gizleyin, PowerKit simgesinden menüyü açıp durdurun. | Menü çalışır; gizlemek AlwaysOn'u durdurmaz. |
| Explorer | Uygun zamanda Görev Yöneticisi'nden Windows Gezgini'ni yeniden başlatın. | Simge geri gelir; oturum süresi yeniden başlamaz. |
| Farklı DPI | Varsa farklı ölçekli monitörler arasında pencereyi taşıyın. | Kontroller ve metin kesilmez. |
| Pil/adaptör | Güç testlerini iki durumda ayrı deneyin. | Her güç kaynağında ilgili Windows politikalarıyla uyumlu davranır. |

Bu fiziksel testler henüz yapılmadı. Sonuçları test adı + geçti/kaldı + gözlenen davranış olarak bildirin.

## Sınırlar ve kaynaklar

Manuel uyku/kapak kapatma ve ekran koruyucu engellenmez: [Windows API davranışı](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate). Uyku dönüşünde [PBT_APMRESUMEAUTOMATIC](https://learn.microsoft.com/en-us/windows/win32/power/pbt-apmresumeautomatic) işlenir; [Per-Monitor V2 manifesti](https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process) kullanılır.

Portable, imzasız paket; installer, otomatik başlangıç, servis ve özel süre girişi yoktur. x64 bu aşamada yeniden doğrulanmadı. Oturumlar yeniden açılışta otomatik devam etmez.
