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

function New-AppIconBitmap {
    param([int]$Size)

    # Create a transparent square canvas for one icon frame.
    $bitmap = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    # Scale the badge geometry from the requested output size.
    $padding = [math]::Round($Size * 0.09)
    $badgeSize = $Size - ($padding * 2)
    $badgeRect = [System.Drawing.RectangleF]::new($padding, $padding, $badgeSize, $badgeSize)

    # Paint the pink badge body with a soft diagonal gradient.
    $gradient = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
        [System.Drawing.PointF]::new($badgeRect.Left, $badgeRect.Top),
        [System.Drawing.PointF]::new($badgeRect.Right, $badgeRect.Bottom),
        (New-ArgbColor 255 236 72 153),
        (New-ArgbColor 255 251 113 133)
    )
    $graphics.FillEllipse($gradient, $badgeRect)

    # Add a subtle ring so the icon holds its shape on light backgrounds.
    $ringPen = [System.Drawing.Pen]::new((New-ArgbColor 75 255 255 255), [math]::Max(1, $Size * 0.03))
    $graphics.DrawEllipse($ringPen, $badgeRect)

    # Layer in a glossy highlight to keep the app icon from feeling flat.
    $highlightRect = [System.Drawing.RectangleF]::new(
        $badgeRect.Left + ($badgeRect.Width * 0.1),
        $badgeRect.Top + ($badgeRect.Height * 0.08),
        $badgeRect.Width * 0.62,
        $badgeRect.Height * 0.46
    )
    $highlightBrush = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
        [System.Drawing.PointF]::new($highlightRect.Left, $highlightRect.Top),
        [System.Drawing.PointF]::new($highlightRect.Left, $highlightRect.Bottom),
        (New-ArgbColor 120 255 255 255),
        (New-ArgbColor 0 255 255 255)
    )
    $graphics.FillEllipse($highlightBrush, $highlightRect)

    # Build the crescent by cutting a second ellipse out of the base moon shape.
    $crescentBaseBrush = [System.Drawing.SolidBrush]::new((New-ArgbColor 255 255 248 252))
    $crescentCutBrush = [System.Drawing.SolidBrush]::new((New-ArgbColor 255 219 39 119))
    $crescentBaseRect = [System.Drawing.RectangleF]::new(
        $badgeRect.Left + ($badgeRect.Width * 0.22),
        $badgeRect.Top + ($badgeRect.Height * 0.2),
        $badgeRect.Width * 0.42,
        $badgeRect.Height * 0.58
    )
    $crescentCutRect = [System.Drawing.RectangleF]::new(
        $crescentBaseRect.Left + ($badgeRect.Width * 0.14),
        $crescentBaseRect.Top + ($badgeRect.Height * 0.04),
        $crescentBaseRect.Width,
        $crescentBaseRect.Height
    )
    $graphics.FillEllipse($crescentBaseBrush, $crescentBaseRect)
    $graphics.FillEllipse($crescentCutBrush, $crescentCutRect)

    # Draw the star accent as a dot plus a small cross flare.
    $sparkBrush = [System.Drawing.SolidBrush]::new((New-ArgbColor 255 255 244 250))
    $sparkCenterX = $badgeRect.Left + ($badgeRect.Width * 0.72)
    $sparkCenterY = $badgeRect.Top + ($badgeRect.Height * 0.33)
    $sparkRadius = [math]::Max(1.0, $Size * 0.035)
    $graphics.FillEllipse(
        $sparkBrush,
        [System.Drawing.RectangleF]::new(
            $sparkCenterX - $sparkRadius,
            $sparkCenterY - $sparkRadius,
            $sparkRadius * 2,
            $sparkRadius * 2
        )
    )

    $sparkPen = [System.Drawing.Pen]::new((New-ArgbColor 220 255 244 250), [math]::Max(1, $Size * 0.02))
    $sparkLength = $Size * 0.07
    $graphics.DrawLine($sparkPen, $sparkCenterX - $sparkLength, $sparkCenterY, $sparkCenterX + $sparkLength, $sparkCenterY)
    $graphics.DrawLine($sparkPen, $sparkCenterX, $sparkCenterY - $sparkLength, $sparkCenterX, $sparkCenterY + $sparkLength)

    # Dispose the drawing resources before returning the bitmap itself.
    $gradient.Dispose()
    $ringPen.Dispose()
    $highlightBrush.Dispose()
    $crescentBaseBrush.Dispose()
    $crescentCutBrush.Dispose()
    $sparkBrush.Dispose()
    $sparkPen.Dispose()
    $graphics.Dispose()

    return $bitmap
}

function New-TrayIconBitmap {
    param([int]$Size)

    # Use a simplified silhouette so the tray icon stays legible at 32px.
    $bitmap = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    $pink = New-ArgbColor 255 219 39 119
    $light = New-ArgbColor 255 255 244 250

    $baseBrush = [System.Drawing.SolidBrush]::new($light)
    $cutBrush = [System.Drawing.SolidBrush]::new($pink)
    $sparkBrush = [System.Drawing.SolidBrush]::new($light)
    $sparkPen = [System.Drawing.Pen]::new($light, [math]::Max(2, $Size * 0.08))
    $sparkPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $sparkPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

    $baseRect = [System.Drawing.RectangleF]::new(
        $Size * 0.15,
        $Size * 0.12,
        $Size * 0.48,
        $Size * 0.72
    )
    $cutRect = [System.Drawing.RectangleF]::new(
        $Size * 0.34,
        $Size * 0.12,
        $Size * 0.46,
        $Size * 0.72
    )

    $graphics.FillEllipse($baseBrush, $baseRect)
    $graphics.FillEllipse($cutBrush, $cutRect)

    # Reuse the small sparkle so the tray icon still reads as Dimsome.
    $sparkCenterX = $Size * 0.74
    $sparkCenterY = $Size * 0.3
    $sparkLength = $Size * 0.08
    $sparkRadius = [math]::Max(1.5, $Size * 0.065)

    $graphics.DrawLine($sparkPen, $sparkCenterX - $sparkLength, $sparkCenterY, $sparkCenterX + $sparkLength, $sparkCenterY)
    $graphics.DrawLine($sparkPen, $sparkCenterX, $sparkCenterY - $sparkLength, $sparkCenterX, $sparkCenterY + $sparkLength)
    $graphics.FillEllipse(
        $sparkBrush,
        [System.Drawing.RectangleF]::new(
            $sparkCenterX - $sparkRadius,
            $sparkCenterY - $sparkRadius,
            $sparkRadius * 2,
            $sparkRadius * 2
        )
    )

    $baseBrush.Dispose()
    $cutBrush.Dispose()
    $sparkBrush.Dispose()
    $sparkPen.Dispose()
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