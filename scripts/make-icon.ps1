# Generates the app icon assets from the master PNG.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\make-icon.ps1 `
#       -Source "C:\path\to\master.png"
#
# Produces (in assets\):
#   icon.png        512x512 master (documentation / high resolution use)
#   icon-256.png    256x256 (embedded in the UI as the window icon)
#   icon-32.rgba    raw 32x32 RGBA bytes (the tray icon, no decoder needed)
#   icon.ico        multi-size Windows icon (executable resource + window icon)

param(
    [Parameter(Mandatory = $true)][string]$Source,
    [int[]]$IcoSizes = @(16, 32, 48, 64, 128, 256)
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root "assets"
if (-not (Test-Path $assets)) { New-Item -ItemType Directory -Path $assets -Force | Out-Null }

function New-ScaledBitmap([System.Drawing.Image]$src, [int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($src, 0, 0, $size, $size)
    $g.Dispose()
    return $bmp
}

function Get-PngBytes([System.Drawing.Bitmap]$bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bytes = $ms.ToArray()
    $bw.Dispose()
    $ms.Dispose()
    return ,$bytes
}

# Build a classic icon DIB: BITMAPINFOHEADER + bottom-up BGRA pixels + AND mask.
function Get-DibBytes([System.Drawing.Bitmap]$bmp) {
    $w = $bmp.Width
    $h = $bmp.Height
    $rect = New-Object System.Drawing.Rectangle(0, 0, $w, $h)
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $raw = New-Object byte[] ($w * $h * 4)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $raw, 0, $raw.Length)
    $bmp.UnlockBits($data)

    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms, [System.Text.Encoding]::UTF8, $true)
    # BITMAPINFOHEADER
    $bw.Write([UInt32]40)         # biSize
    $bw.Write([Int32]$w)          # biWidth
    $bw.Write([Int32]($h * 2))    # biHeight (XOR + AND masks)
    $bw.Write([UInt16]1)          # biPlanes
    $bw.Write([UInt16]32)         # biBitCount
    $bw.Write([UInt32]0)          # biCompression (BI_RGB)
    $bw.Write([UInt32]0)          # biSizeImage
    $bw.Write([Int32]0)           # biXPelsPerMeter
    $bw.Write([Int32]0)           # biYPelsPerMeter
    $bw.Write([UInt32]0)          # biClrUsed
    $bw.Write([UInt32]0)          # biClrImportant
    # XOR bitmap: bottom-up rows of BGRA
    for ($y = $h - 1; $y -ge 0; $y--) {
        $start = $y * $w * 4
        $bw.Write($raw, $start, $w * 4)
    }
    # AND mask: 1 bit per pixel, rows padded to 4 bytes (all opaque)
    $maskStride = [int]([Math]::Ceiling($w / 32.0) * 4)
    for ($y = 0; $y -lt $h; $y++) {
        $bw.Write([byte[]]::new($maskStride))
    }
    $bw.Flush()
    $bytes = $ms.ToArray()
    $bw.Dispose()
    $ms.Dispose()
    return ,$bytes
}

$src = [System.Drawing.Image]::FromFile($Source)
try {
    # ---- master + UI icon ----
    foreach ($pair in @(@(512, "icon.png"), @(256, "icon-256.png"))) {
        $bmp = New-ScaledBitmap $src $pair[0]
        $out = Join-Path $assets $pair[1]
        $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        Write-Host "wrote $out"
    }

    # ---- raw RGBA for the tray icon ----
    $bmp32 = New-ScaledBitmap $src 32
    $rect = New-Object System.Drawing.Rectangle(0, 0, 32, 32)
    $data = $bmp32.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $raw = New-Object byte[] (32 * 32 * 4)
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $raw, 0, $raw.Length)
    $bmp32.UnlockBits($data)
    # .NET gives BGRA; the tray icon wants RGBA.
    for ($i = 0; $i -lt $raw.Length; $i += 4) {
        $b = $raw[$i]; $raw[$i] = $raw[$i + 2]; $raw[$i + 2] = $b
    }
    [System.IO.File]::WriteAllBytes((Join-Path $assets "icon-32.rgba"), $raw)
    $bmp32.Dispose()
    Write-Host "wrote assets\icon-32.rgba"

    # ---- multi-size .ico ----
    # GNU windres (used to embed the icon in the executable) can only read
    # classic DIB entries, so the .ico is written in the uncompressed
    # 32-bit BITMAPINFOHEADER format instead of the PNG-compressed one.
    $entries = @()
    foreach ($size in $IcoSizes) {
        $bmp = New-ScaledBitmap $src $size
        $entries += ,([pscustomobject]@{ Size = $size; Data = (Get-DibBytes $bmp) })
        $bmp.Dispose()
    }
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms, [System.Text.Encoding]::UTF8, $true)
    $bw.Write([UInt16]0)          # reserved
    $bw.Write([UInt16]1)          # type: icon
    $bw.Write([UInt16]$entries.Count)
    $offset = 6 + 16 * $entries.Count
    foreach ($e in $entries) {
        $dim = if ($e.Size -ge 256) { 0 } else { $e.Size }
        $bw.Write([byte]$dim)
        $bw.Write([byte]$dim)
        $bw.Write([byte]0)        # colour count
        $bw.Write([byte]0)        # reserved
        $bw.Write([UInt16]1)      # colour planes
        $bw.Write([UInt16]32)     # bits per pixel
        $bw.Write([UInt32]$e.Data.Length)
        $bw.Write([UInt32]$offset)
        $offset += $e.Data.Length
    }
    foreach ($e in $entries) { $bw.Write($e.Data) }
    $bw.Flush()
    [System.IO.File]::WriteAllBytes((Join-Path $assets "icon.ico"), $ms.ToArray())
    $ms.Dispose()
    Write-Host "wrote assets\icon.ico ($($entries.Count) sizes, DIB format)"
}
finally {
    $src.Dispose()
}
