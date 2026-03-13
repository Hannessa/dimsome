$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing

function New-ArgbColor {
    param(
        [int]$A,
        [int]$R,
        [int]$G,
        [int]$B
    )

    # Keep color construction terse for the bitmap helpers below.
    return [System.Drawing.Color]::FromArgb($A, $R, $G, $B)
}

function Draw-SunGlyph {
    param(
        [System.Drawing.Graphics]$Graphics,
        [float]$Size,
        [System.Drawing.Color]$CoreColor,
        [System.Drawing.Color]$RayColor,
        [float]$CoreScale = 0.34,
        [float]$InnerRayScale = 0.15,
        [float]$OuterRayScale = 0.38,
        [float]$RayThicknessScale = 0.075
    )

    # Center every shape from one origin so both icons share the same silhouette.
    $center = $Size / 2
    $coreRadius = $Size * $CoreScale / 2
    $innerRayRadius = $Size * $InnerRayScale
    $outerRayRadius = $Size * $OuterRayScale
    $rayThickness = [math]::Max(1.5, $Size * $RayThicknessScale)

    $coreBrush = [System.Drawing.SolidBrush]::new($CoreColor)
    $rayPen = [System.Drawing.Pen]::new($RayColor, $rayThickness)
    $rayPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $rayPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

    # Use eight balanced rays so the mark still reads clearly at small sizes.
    foreach ($rayIndex in 0..7) {
        $angle = (($rayIndex * 45) - 90) * [math]::PI / 180
        $innerX = $center + ([math]::Cos($angle) * $innerRayRadius)
        $innerY = $center + ([math]::Sin($angle) * $innerRayRadius)
        $outerX = $center + ([math]::Cos($angle) * $outerRayRadius)
        $outerY = $center + ([math]::Sin($angle) * $outerRayRadius)
        $Graphics.DrawLine($rayPen, $innerX, $innerY, $outerX, $outerY)
    }

    # Cap the rays with a simple center disc to keep the icon feeling clean.
    $Graphics.FillEllipse(
        $coreBrush,
        [System.Drawing.RectangleF]::new(
            $center - $coreRadius,
            $center - $coreRadius,
            $coreRadius * 2,
            $coreRadius * 2
        )
    )

    $coreBrush.Dispose()
    $rayPen.Dispose()
}

function New-AppIconBitmap {
    param([int]$Size)

    # Create a transparent square canvas for one icon frame.
    $bitmap = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    # Add a faint glow so the transparent app icon still feels polished in Windows.
    $glowRadius = $Size * 0.38
    $glowRect = [System.Drawing.RectangleF]::new(
        ($Size / 2) - $glowRadius,
        ($Size / 2) - $glowRadius,
        $glowRadius * 2,
        $glowRadius * 2
    )
    $glowPath = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $glowPath.AddEllipse($glowRect)
    $glowBrush = [System.Drawing.Drawing2D.PathGradientBrush]::new($glowPath)
    $glowBrush.CenterColor = New-ArgbColor 72 236 72 153
    $glowBrush.SurroundColors = @([System.Drawing.Color]::Transparent)
    $graphics.FillEllipse($glowBrush, $glowRect)

    # Reuse the same sun geometry as the tray icon so the brand mark stays consistent.
    Draw-SunGlyph `
        -Graphics $graphics `
        -Size $Size `
        -CoreColor (New-ArgbColor 255 236 72 153) `
        -RayColor (New-ArgbColor 255 219 39 119) `
        -CoreScale 0.34 `
        -InnerRayScale 0.15 `
        -OuterRayScale 0.38 `
        -RayThicknessScale 0.072

    # Dispose the drawing resources before returning the bitmap itself.
    $glowBrush.Dispose()
    $glowPath.Dispose()
    $graphics.Dispose()

    return $bitmap
}

function New-TrayIconBitmap {
    param([int]$Size)

    # Keep the tray asset flat and bold so it survives at tiny taskbar sizes.
    $bitmap = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    # Match the app icon geometry while keeping the tray rendering fully flat.
    Draw-SunGlyph `
        -Graphics $graphics `
        -Size $Size `
        -CoreColor (New-ArgbColor 255 236 72 153) `
        -RayColor (New-ArgbColor 255 219 39 119) `
        -CoreScale 0.34 `
        -InnerRayScale 0.15 `
        -OuterRayScale 0.38 `
        -RayThicknessScale 0.08

    $graphics.Dispose()

    return $bitmap
}

function Save-PngFrame {
    param(
        [System.Drawing.Bitmap]$Bitmap,
        [string]$Path
    )

    # Write each rendered frame directly as a PNG so ICO assembly can reuse them.
    $Bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
}

function Save-RgbaBytes {
    param(
        [System.Drawing.Bitmap]$Bitmap,
        [string]$Path
    )

    # Flatten the tray icon into RGBA bytes for Tauri's raw icon loader.
    $bytes = New-Object byte[] ($Bitmap.Width * $Bitmap.Height * 4)
    $index = 0

    for ($y = 0; $y -lt $Bitmap.Height; $y++) {
        for ($x = 0; $x -lt $Bitmap.Width; $x++) {
            $pixel = $Bitmap.GetPixel($x, $y)
            $bytes[$index] = $pixel.R
            $bytes[$index + 1] = $pixel.G
            $bytes[$index + 2] = $pixel.B
            $bytes[$index + 3] = $pixel.A
            $index += 4
        }
    }

    [System.IO.File]::WriteAllBytes($Path, $bytes)
}

function Save-IcoFromPngs {
    param(
        [string[]]$PngPaths,
        [string]$Destination
    )

    # Assemble a Windows ICO file manually so each PNG frame is preserved as-is.
    $fileStream = [System.IO.File]::Open($Destination, [System.IO.FileMode]::Create)
    $writer = [System.IO.BinaryWriter]::new($fileStream)

    try {
        $writer.Write([UInt16]0)
        $writer.Write([UInt16]1)
        $writer.Write([UInt16]$PngPaths.Count)

        $offset = 6 + (16 * $PngPaths.Count)
        $entries = @()

        foreach ($pngPath in $PngPaths) {
            $bytes = [System.IO.File]::ReadAllBytes($pngPath)
            $image = [System.Drawing.Image]::FromFile($pngPath)
            $widthByte = if ($image.Width -ge 256) { 0 } else { [byte]$image.Width }
            $heightByte = if ($image.Height -ge 256) { 0 } else { [byte]$image.Height }

            $entries += [pscustomobject]@{
                Width = $widthByte
                Height = $heightByte
                Bytes = $bytes
                Offset = $offset
            }

            $offset += $bytes.Length
            $image.Dispose()
        }

        # Write the ICO directory entries before appending the image payloads.
        foreach ($entry in $entries) {
            $writer.Write([byte]$entry.Width)
            $writer.Write([byte]$entry.Height)
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([UInt16]1)
            $writer.Write([UInt16]32)
            $writer.Write([UInt32]$entry.Bytes.Length)
            $writer.Write([UInt32]$entry.Offset)
        }

        foreach ($entry in $entries) {
            $writer.Write($entry.Bytes)
        }
    }
    finally {
        $writer.Dispose()
        $fileStream.Dispose()
    }
}

$iconsDir = Join-Path $PSScriptRoot "..\src-tauri\icons"
$iconsDir = [System.IO.Path]::GetFullPath($iconsDir)

$frameSizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$tempPngs = @()

try {
    # Render every frame size first so the ICO and standalone PNG stay in sync.
    foreach ($size in $frameSizes) {
        $bitmap = New-AppIconBitmap -Size $size
        $framePath = Join-Path $iconsDir ("icon-{0}.png" -f $size)
        Save-PngFrame -Bitmap $bitmap -Path $framePath
        $bitmap.Dispose()
        $tempPngs += $framePath
    }

    Copy-Item (Join-Path $iconsDir "icon-256.png") (Join-Path $iconsDir "icon.png") -Force
    Save-IcoFromPngs -PngPaths $tempPngs -Destination (Join-Path $iconsDir "icon.ico")

    # Emit the simplified tray icon in both PNG and raw RGBA formats.
    $trayBitmap = New-TrayIconBitmap -Size 32
    Save-PngFrame -Bitmap $trayBitmap -Path (Join-Path $iconsDir "tray-icon.png")
    Save-RgbaBytes -Bitmap $trayBitmap -Path (Join-Path $iconsDir "tray-icon.rgba")
    $trayBitmap.Dispose()
}
finally {
    # Clean up the temporary PNG frames once the final icon assets are written.
    foreach ($tempPath in $tempPngs) {
        if (Test-Path $tempPath) {
            Remove-Item $tempPath -Force
        }
    }
}


