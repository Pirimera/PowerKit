//! WIC resize. The worker thread owns the factory; the UI thread only receives the finished plan.
use super::resize_io::{self, Staging};
use crate::image_resizer::{
    Container, Encoder, FramePlan, PlannedWrite, PngInterlace, ResizeSettings, SourceImage,
    TiffCompression, can_copy_bytes,
};
use std::{io, path::Path};
use windows::{
    Win32::{
        Foundation::GENERIC_READ,
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_ContainerFormatBmp, GUID_ContainerFormatGif,
            GUID_ContainerFormatJpeg, GUID_ContainerFormatPng, GUID_ContainerFormatTiff,
            GUID_ContainerFormatWmp, GUID_WICPixelFormat8bppIndexed, GUID_WICPixelFormat24bppBGR,
            GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameDecode, IWICBitmapSource,
            IWICImagingFactory, WICBitmapDitherTypeErrorDiffusion, WICBitmapDitherTypeNone,
            WICBitmapEncoderNoCache, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
            WICBitmapPaletteTypeFixedHalftone256, WICDecodeMetadataCacheOnDemand, WICRect,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
                StructuredStorage::{IPropertyBag2, PROPBAG2, PROPVARIANT, PropVariantClear},
            },
            Variant::{VARIANT, VT_BOOL, VT_R4, VT_UI1},
        },
    },
    core::{GUID, Interface, PCWSTR, PWSTR},
};
struct ComGuard(bool);
impl ComGuard {
    fn new() -> io::Result<Self> {
        // SAFETY: this runs on the resize worker, which has no other COM users.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            return Err(super::fault::Fault::ImagingUnavailable.into());
        }
        Ok(Self(hr.0 == 0))
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}
pub(super) struct Images {
    // Declaration order is drop order. Release the factory before CoUninitialize.
    factory: IWICImagingFactory,
    _com: ComGuard,
}
impl Images {
    pub(super) fn new() -> io::Result<Self> {
        let com = ComGuard::new()?;
        // SAFETY: the factory is used only on this worker thread. It is the first
        // field, so Drop releases it before ComGuard calls CoUninitialize.
        let factory = unsafe {
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(wic_error)?
        };
        Ok(Self { factory, _com: com })
    }
    pub(super) fn probe(&self, path: &Path) -> io::Result<SourceImage> {
        let wide = wide_path(path);
        // SAFETY: wide is a live NUL path. The decoder is released at the end of this function.
        let decoder = unsafe {
            self.factory
                .CreateDecoderFromFilename(
                    PCWSTR(wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .map_err(wic_error)?
        };
        let format = unsafe { decoder.GetContainerFormat().map_err(wic_error)? };
        let frame = unsafe { decoder.GetFrame(0).map_err(wic_error)? };
        let (width, height, dpi_x, dpi_y) = frame_info(&frame)?;
        Ok(SourceImage {
            width,
            height,
            dpi_x,
            dpi_y,
            container: container_from(format),
        })
    }
    pub(super) fn write(&self, plan: &PlannedWrite, settings: &ResizeSettings) -> io::Result<()> {
        let current = self.probe(&plan.source)?;
        if current.width != plan.source_width || current.height != plan.source_height {
            return Err(super::fault::Fault::SourceChanged.into());
        }
        let directory = plan.source.parent().ok_or(super::fault::Fault::NoFolder)?;
        let staging = Staging::create(directory)?;
        let modified = if settings.keep_date {
            std::fs::metadata(&plan.source)?.modified().ok()
        } else {
            None
        };
        if can_copy_bytes(plan, settings) {
            std::fs::copy(&plan.source, &staging.0)?;
        } else {
            encode(&self.factory, &plan.source, &staging.0, plan, settings)?;
        }
        resize_io::commit(&staging.0, plan, modified, true)?;
        Ok(())
    }
}
fn frame_info(frame: &IWICBitmapFrameDecode) -> io::Result<(u32, u32, f64, f64)> {
    let mut width = 0u32;
    let mut height = 0u32;
    let mut dpi_x = 0f64;
    let mut dpi_y = 0f64;
    // SAFETY: out parameters are live locals. The frame pointer stays valid for this call.
    unsafe {
        frame.GetSize(&mut width, &mut height).map_err(wic_error)?;
        if frame.GetResolution(&mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96.0;
            dpi_y = 96.0;
        }
    }
    Ok((width, height, dpi_x, dpi_y))
}
fn encode(
    factory: &IWICImagingFactory,
    source: &Path,
    destination: &Path,
    plan: &PlannedWrite,
    settings: &ResizeSettings,
) -> io::Result<()> {
    let wide = wide_path(source);
    let decoder = unsafe {
        factory
            .CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .map_err(wic_error)?
    };
    let frame = unsafe { decoder.GetFrame(0).map_err(wic_error)? };
    let (_, _, dpi_x, dpi_y) = frame_info(&frame)?;
    let prepared = prepare(factory, &frame, &plan.frame, plan.encoder)?;
    let dest = wide_path(destination);
    let stream = unsafe { factory.CreateStream().map_err(wic_error)? };
    unsafe {
        stream
            .InitializeFromFilename(PCWSTR(dest.as_ptr()), GENERIC_WRITE)
            .map_err(wic_error)?;
    }
    let encoder = unsafe {
        factory
            .CreateEncoder(&container_guid(plan.encoder), std::ptr::null())
            .map_err(wic_error)?
    };
    unsafe {
        encoder
            .Initialize(&stream, WICBitmapEncoderNoCache)
            .map_err(wic_error)?;
    }
    let mut encoded = None;
    let mut options = None;
    unsafe {
        encoder
            .CreateNewFrame(&mut encoded, &mut options)
            .map_err(wic_error)?;
    }
    let encoded = encoded.ok_or(super::fault::Fault::FrameUnavailable)?;
    if let Some(bag) = options.as_ref() {
        let _ = write_options(bag, plan.encoder, settings);
    }
    unsafe {
        encoded.Initialize(options.as_ref()).map_err(wic_error)?;
        encoded
            .SetSize(plan.frame.output_width, plan.frame.output_height)
            .map_err(wic_error)?;
        encoded.SetResolution(dpi_x, dpi_y).map_err(wic_error)?;
    }
    let mut format = pixel_format(plan.encoder);
    unsafe {
        encoded.SetPixelFormat(&mut format).map_err(wic_error)?;
    }
    let source = if format == pixel_format(plan.encoder) {
        prepared
    } else {
        convert(factory, &prepared, &format, false)?
    };
    unsafe {
        encoded
            .WriteSource(&source, std::ptr::null())
            .map_err(wic_error)?;
        copy_metadata(&frame, &encoded, settings.remove_metadata);
        encoded.Commit().map_err(wic_error)?;
        encoder.Commit().map_err(wic_error)?;
    }
    Ok(())
}
fn prepare(
    factory: &IWICImagingFactory,
    frame: &IWICBitmapFrameDecode,
    plan: &FramePlan,
    encoder: Encoder,
) -> io::Result<IWICBitmapSource> {
    let mut source = convert(factory, frame, &GUID_WICPixelFormat32bppBGRA, false)?;
    if !plan.unchanged {
        let scaler = unsafe { factory.CreateBitmapScaler().map_err(wic_error)? };
        unsafe {
            scaler
                .Initialize(
                    &source,
                    plan.scaled_width,
                    plan.scaled_height,
                    WICBitmapInterpolationModeFant,
                )
                .map_err(wic_error)?;
        }
        source = scaler.cast().map_err(wic_error)?;
        if let Some(crop) = plan.crop {
            let clip = unsafe { factory.CreateBitmapClipper().map_err(wic_error)? };
            let rect = WICRect {
                X: crop.x as i32,
                Y: crop.y as i32,
                Width: crop.width as i32,
                Height: crop.height as i32,
            };
            unsafe { clip.Initialize(&source, &rect).map_err(wic_error)? };
            source = clip.cast().map_err(wic_error)?;
        }
    }
    match encoder {
        Encoder::Jpeg => convert(factory, &source, &GUID_WICPixelFormat24bppBGR, false),
        Encoder::Gif => convert(factory, &source, &GUID_WICPixelFormat8bppIndexed, true),
        _ => Ok(source),
    }
}
fn convert(
    factory: &IWICImagingFactory,
    source: impl windows::core::Param<IWICBitmapSource> + Copy,
    format: &GUID,
    indexed: bool,
) -> io::Result<IWICBitmapSource> {
    let converter = unsafe { factory.CreateFormatConverter().map_err(wic_error)? };
    unsafe {
        if indexed {
            let palette = factory.CreatePalette().map_err(wic_error)?;
            palette
                .InitializePredefined(WICBitmapPaletteTypeFixedHalftone256, false)
                .map_err(wic_error)?;
            converter
                .Initialize(
                    source,
                    format,
                    WICBitmapDitherTypeErrorDiffusion,
                    &palette,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .map_err(wic_error)?;
        } else {
            converter
                .Initialize(
                    source,
                    format,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .map_err(wic_error)?;
        }
        converter.cast().map_err(wic_error)
    }
}
fn write_options(
    bag: &IPropertyBag2,
    encoder: Encoder,
    settings: &ResizeSettings,
) -> windows::core::Result<()> {
    match encoder {
        Encoder::Jpeg => write_variant(
            bag,
            "ImageQuality",
            variant_f32(settings.jpeg_quality.clamp(1, 100) as f32 / 100.0),
        ),
        Encoder::Png => match settings.png {
            PngInterlace::Default => Ok(()),
            PngInterlace::On => write_variant(bag, "InterlaceOption", variant_bool(true)),
            PngInterlace::Off => write_variant(bag, "InterlaceOption", variant_bool(false)),
        },
        Encoder::Tiff => {
            let method = match settings.tiff {
                TiffCompression::Default => return Ok(()),
                TiffCompression::None => 1,
                TiffCompression::Ccitt3 => 2,
                TiffCompression::Ccitt4 => 3,
                TiffCompression::Lzw => 4,
                TiffCompression::Rle => 5,
                TiffCompression::Zip => 6,
            };
            write_variant(bag, "TiffCompressionMethod", variant_u8(method))
        }
        Encoder::Bmp | Encoder::Gif | Encoder::Wmp => Ok(()),
    }
}
fn write_variant(bag: &IPropertyBag2, name: &str, variant: VARIANT) -> windows::core::Result<()> {
    let mut wide = wide_str(name);
    let property = PROPBAG2 {
        dwType: 1,
        vt: unsafe { (*variant.Anonymous.Anonymous).vt },
        cfType: 0,
        dwHint: 0,
        pstrName: PWSTR(wide.as_mut_ptr()),
        clsid: GUID::zeroed(),
    };
    // SAFETY: the name buffer and variant stay alive for this single property write.
    unsafe { bag.Write(1, &property, &variant) }
}
fn variant_f32(value: f32) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let header = &mut *variant.Anonymous.Anonymous;
        header.vt = VT_R4;
        header.Anonymous.fltVal = value;
    }
    variant
}
fn variant_bool(value: bool) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let header = &mut *variant.Anonymous.Anonymous;
        header.vt = VT_BOOL;
        header.Anonymous.boolVal =
            windows::Win32::Foundation::VARIANT_BOOL(if value { -1 } else { 0 });
    }
    variant
}
fn variant_u8(value: u8) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let header = &mut *variant.Anonymous.Anonymous;
        header.vt = VT_UI1;
        header.Anonymous.bVal = value;
    }
    variant
}
fn copy_metadata(
    frame: &IWICBitmapFrameDecode,
    encoded: &windows::Win32::Graphics::Imaging::IWICBitmapFrameEncode,
    remove: bool,
) {
    let reader = unsafe { frame.GetMetadataQueryReader() };
    let writer = unsafe { encoded.GetMetadataQueryWriter() };
    let (Ok(reader), Ok(writer)) = (reader, writer) else {
        return;
    };
    for path in [
        "/app1/ifd/{ushort=274}",
        "/app1/ifd/exif/{ushort=274}",
        "/ifd/{ushort=274}",
        "/ifd/exif/{ushort=274}",
    ] {
        copy_query(&reader, &writer, path);
    }
    if !remove {
        for path in ["/app1", "/app13", "/xmp", "/com", "/ifd"] {
            copy_query(&reader, &writer, path);
        }
    }
}
fn copy_query(
    reader: &windows::Win32::Graphics::Imaging::IWICMetadataQueryReader,
    writer: &windows::Win32::Graphics::Imaging::IWICMetadataQueryWriter,
    path: &str,
) {
    let wide = wide_str(path);
    let mut value = PROPVARIANT::default();
    // SAFETY: the query name is live. The variant is cleared before this function returns.
    unsafe {
        if reader
            .GetMetadataByName(PCWSTR(wide.as_ptr()), &mut value)
            .is_ok()
        {
            let _ = writer.SetMetadataByName(PCWSTR(wide.as_ptr()), &value);
        }
        let _ = PropVariantClear(&mut value);
    }
}
fn pixel_format(encoder: Encoder) -> GUID {
    match encoder {
        Encoder::Jpeg => GUID_WICPixelFormat24bppBGR,
        Encoder::Gif => GUID_WICPixelFormat8bppIndexed,
        _ => GUID_WICPixelFormat32bppBGRA,
    }
}
fn container_guid(encoder: Encoder) -> GUID {
    match encoder {
        Encoder::Jpeg => GUID_ContainerFormatJpeg,
        Encoder::Png => GUID_ContainerFormatPng,
        Encoder::Bmp => GUID_ContainerFormatBmp,
        Encoder::Gif => GUID_ContainerFormatGif,
        Encoder::Tiff => GUID_ContainerFormatTiff,
        Encoder::Wmp => GUID_ContainerFormatWmp,
    }
}
fn container_from(format: GUID) -> Container {
    if format == GUID_ContainerFormatJpeg {
        Container::Jpeg
    } else if format == GUID_ContainerFormatPng {
        Container::Png
    } else if format == GUID_ContainerFormatBmp {
        Container::Bmp
    } else if format == GUID_ContainerFormatGif {
        Container::Gif
    } else if format == GUID_ContainerFormatTiff {
        Container::Tiff
    } else if format == GUID_ContainerFormatWmp {
        Container::Wmp
    } else {
        Container::Other
    }
}
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn wide_str(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn wic_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}
const GENERIC_WRITE: u32 = 0x4000_0000;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_resizer::{FallbackEncoder, Fit, ResizeSettings, Size, Unit, plan_batch};
    fn write_bmp(path: &Path, width: u32, height: u32) {
        let row = (width * 3 + 3) & !3;
        let pixels = row * height;
        let offset = 54u32;
        let size = offset + pixels;
        let mut data = vec![0u8; size as usize];
        data[0] = b'B';
        data[1] = b'M';
        data[2..6].copy_from_slice(&size.to_le_bytes());
        data[10..14].copy_from_slice(&offset.to_le_bytes());
        data[14..18].copy_from_slice(&40u32.to_le_bytes());
        data[18..22].copy_from_slice(&width.to_le_bytes());
        data[22..26].copy_from_slice(&(height as i32).to_le_bytes());
        data[26..28].copy_from_slice(&1u16.to_le_bytes());
        data[28..30].copy_from_slice(&24u16.to_le_bytes());
        std::fs::write(path, data).unwrap();
    }
    #[test]
    fn synthetic_bmp_is_resized_without_touching_a_failed_target() {
        let dir = std::env::temp_dir().join(format!(
            "powerkit-wic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("wide.bmp");
        write_bmp(&source, 4, 2);
        let images = Images::new().unwrap();
        let probed = images.probe(&source).unwrap();
        assert_eq!((probed.width, probed.height), (4, 2));
        let settings = ResizeSettings {
            custom: Size {
                name: "custom".into(),
                fit: Fit::Fit,
                width: 2.0,
                height: 2.0,
                unit: Unit::Pixel,
            },
            selected: 4,
            filename: "%1".into(),
            fallback: FallbackEncoder::Bmp,
            ..ResizeSettings::default()
        };
        let plan = plan_batch(
            std::slice::from_ref(&source),
            &settings,
            &[Some(probed)],
            |path| path == source,
            crate::image_resizer::display_name,
        );
        let crate::image_resizer::PlanItem::Ready(ready) = &plan[0] else {
            panic!("plan");
        };
        assert_eq!(
            (ready.frame.output_width, ready.frame.output_height),
            (2, 1)
        );
        images.write(ready, &settings).unwrap();
        let output = images.probe(&ready.destination).unwrap();
        assert_eq!((output.width, output.height), (2, 1));
        assert_eq!(images.probe(&source).unwrap().width, 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
