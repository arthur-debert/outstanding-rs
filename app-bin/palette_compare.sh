#!/usr/bin/env bash
#
# palette_compare.sh — Compare standard vs theme-interpolated 256-color palettes
#
# Shows two renderings of the 256-color palette side by side:
#   1. STANDARD: The default hardcoded 256-color RGB values
#   2. INTERPOLATED: Generated from a base16 theme via trilinear
#      interpolation in LAB colorspace (per jake-stewart's algorithm)
#
# Uses truecolor (24-bit) escape codes for both so you can compare
# the actual color values regardless of your terminal's palette settings.
#
# Usage:
#   ./palette_compare.sh                    # uses default xterm base16 colors
#   ./palette_compare.sh THEME_NAME         # uses a built-in theme
#
# Built-in themes: xterm, solarized-dark, gruvbox-dark, catppuccin-mocha

set -euo pipefail

THEME="${1:-xterm}"

python3 - "$THEME" <<'PYEOF'
import sys, math

# ─── Color conversion helpers ────────────────────────────────────────

def srgb_to_linear(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

def linear_to_srgb(c):
    c = max(0.0, min(1.0, c))
    return round((12.92 * c if c <= 0.0031308 else 1.055 * c ** (1/2.4) - 0.055) * 255)

def rgb_to_xyz(rgb):
    r, g, b = [srgb_to_linear(c) for c in rgb]
    x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b
    y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b
    z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b
    return (x, y, z)

def xyz_to_rgb(xyz):
    x, y, z = xyz
    r =  3.2404542 * x - 1.5371385 * y - 0.4985314 * z
    g = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z
    b =  0.0556434 * x - 0.2040259 * y + 1.0572252 * z
    return (linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b))

# D65 reference white
XN, YN, ZN = 0.95047, 1.00000, 1.08883

def _f(t):
    return t ** (1/3) if t > 0.008856 else 7.787 * t + 16/116

def _f_inv(t):
    return t ** 3 if t > 0.206896 else (t - 16/116) / 7.787

def rgb_to_lab(rgb):
    x, y, z = rgb_to_xyz(rgb)
    fx, fy, fz = _f(x/XN), _f(y/YN), _f(z/ZN)
    L = 116 * fy - 16
    a = 500 * (fx - fy)
    b = 200 * (fy - fz)
    return (L, a, b)

def lab_to_rgb(lab):
    L, a, b = lab
    fy = (L + 16) / 116
    fx = a / 500 + fy
    fz = fy - b / 200
    x = XN * _f_inv(fx)
    y = YN * _f_inv(fy)
    z = ZN * _f_inv(fz)
    return xyz_to_rgb((x, y, z))


# ─── Interpolation ──────────────────────────────────────────────────

def lerp_lab(t, lab1, lab2):
    return (
        lab1[0] + t * (lab2[0] - lab1[0]),
        lab1[1] + t * (lab2[1] - lab1[1]),
        lab1[2] + t * (lab2[2] - lab1[2]),
    )


# ─── Standard 256 palette (hardcoded RGB values) ────────────────────

STANDARD_ANSI_16 = [
    (0, 0, 0),       (205, 0, 0),     (0, 205, 0),     (205, 205, 0),
    (0, 0, 238),     (205, 0, 205),   (0, 205, 205),   (229, 229, 229),
    (127, 127, 127), (255, 0, 0),     (0, 255, 0),     (255, 255, 0),
    (92, 92, 255),   (255, 0, 255),   (0, 255, 255),   (255, 255, 255),
]

def standard_256_palette():
    """The default xterm 256-color palette RGB values."""
    palette = list(STANDARD_ANSI_16)
    # 216-color cube (indices 16-231)
    for r in range(6):
        for g in range(6):
            for b in range(6):
                rv = 0 if r == 0 else 55 + 40 * r
                gv = 0 if g == 0 else 55 + 40 * g
                bv = 0 if b == 0 else 55 + 40 * b
                palette.append((rv, gv, bv))
    # Grayscale ramp (indices 232-255)
    for i in range(24):
        v = 8 + 10 * i
        palette.append((v, v, v))
    return palette


# ─── Theme-interpolated palette (jake-stewart's algorithm) ──────────

def generate_256_palette(base16, bg=None, fg=None):
    """Generate 256 colors by trilinear LAB interpolation of base16 corners."""
    base8_lab = [rgb_to_lab(c) for c in base16[:8]]
    bg_lab = rgb_to_lab(bg) if bg else base8_lab[0]
    fg_lab = rgb_to_lab(fg) if fg else base8_lab[7]

    palette = list(base16)  # colors 0-15

    # 216-color cube (indices 16-231): trilinear interpolation
    for r in range(6):
        c0 = lerp_lab(r / 5, bg_lab, base8_lab[1])       # bg → red
        c1 = lerp_lab(r / 5, base8_lab[2], base8_lab[3])  # green → yellow
        c2 = lerp_lab(r / 5, base8_lab[4], base8_lab[5])  # blue → magenta
        c3 = lerp_lab(r / 5, base8_lab[6], fg_lab)        # cyan → fg
        for g in range(6):
            c4 = lerp_lab(g / 5, c0, c1)
            c5 = lerp_lab(g / 5, c2, c3)
            for b in range(6):
                c6 = lerp_lab(b / 5, c4, c5)
                palette.append(lab_to_rgb(c6))

    # Grayscale ramp (indices 232-255)
    for i in range(24):
        t = (i + 1) / 25
        lab = lerp_lab(t, bg_lab, fg_lab)
        palette.append(lab_to_rgb(lab))

    return palette


# ─── Themes ──────────────────────────────────────────────────────────

def hex_to_rgb(h):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))

THEMES = {
    "xterm": {
        "base16": [
            "#000000", "#cd0000", "#00cd00", "#cdcd00",
            "#0000ee", "#cd00cd", "#00cdcd", "#e5e5e5",
            "#7f7f7f", "#ff0000", "#00ff00", "#ffff00",
            "#5c5cff", "#ff00ff", "#00ffff", "#ffffff",
        ],
    },
    "solarized-dark": {
        "base16": [
            "#073642", "#dc322f", "#859900", "#b58900",
            "#268bd2", "#d33682", "#2aa198", "#eee8d5",
            "#002b36", "#cb4b16", "#586e75", "#657b83",
            "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
        ],
        "bg": "#002b36",
        "fg": "#fdf6e3",
    },
    "gruvbox-dark": {
        "base16": [
            "#282828", "#cc241d", "#98971a", "#d79921",
            "#458588", "#b16286", "#689d6a", "#a89984",
            "#928374", "#fb4934", "#b8bb26", "#fabd2f",
            "#83a598", "#d3869b", "#8ec07c", "#ebdbb2",
        ],
        "bg": "#282828",
        "fg": "#ebdbb2",
    },
    "catppuccin-mocha": {
        "base16": [
            "#1e1e2e", "#f38ba8", "#a6e3a1", "#f9e2af",
            "#89b4fa", "#cba6f7", "#94e2d5", "#bac2de",
            "#585b70", "#f38ba8", "#a6e3a1", "#f9e2af",
            "#89b4fa", "#cba6f7", "#94e2d5", "#a6adc8",
        ],
        "bg": "#1e1e2e",
        "fg": "#cdd6f4",
    },
}


# ─── Rendering ───────────────────────────────────────────────────────

RESET = "\033[0m"

def bg24(r, g, b):
    """Truecolor background escape."""
    return f"\033[48;2;{r};{g};{b}m"

def fg24(r, g, b):
    """Truecolor foreground escape."""
    return f"\033[38;2;{r};{g};{b}m"

def contrast_fg(r, g, b):
    """Pick black or white foreground for readability."""
    luminance = 0.299 * r + 0.587 * g + 0.114 * b
    return fg24(0, 0, 0) if luminance > 128 else fg24(255, 255, 255)

def print_color_block(palette, idx):
    """Print a single color swatch with its index number."""
    r, g, b = palette[idx]
    label = f"{idx:>3}"
    sys.stdout.write(f"{bg24(r, g, b)}{contrast_fg(r, g, b)}{label}{RESET}")

def print_section_header(text):
    sys.stdout.write(f"\n  \033[1m{text}\033[0m\n\n")

def print_row(palette, indices, indent="    "):
    """Print a row of color blocks."""
    sys.stdout.write(indent)
    for idx in indices:
        print_color_block(palette, idx)
    sys.stdout.write(f"{RESET}\n")

def print_palette_grid(palette, label):
    """Print the full palette in organized sections."""
    print_section_header(label)

    # Base 16
    sys.stdout.write("    ANSI base 16:\n")
    print_row(palette, range(0, 8))
    print_row(palette, range(8, 16))
    sys.stdout.write("\n")

    # 216-color cube, shown as 6 planes of 6x6
    sys.stdout.write("    216-color cube (6 planes of 6x6):\n")
    for plane in range(6):
        for row in range(6):
            start = 16 + plane * 36 + row * 6
            print_row(palette, range(start, start + 6))
        sys.stdout.write("\n")

    # Grayscale ramp
    sys.stdout.write("    Grayscale ramp:\n")
    print_row(palette, range(232, 244))
    print_row(palette, range(244, 256))
    sys.stdout.write("\n")


def print_side_by_side(std_palette, interp_palette):
    """Print both palettes side-by-side for easy comparison."""

    def row_str(palette, indices):
        parts = []
        for idx in indices:
            r, g, b = palette[idx]
            label = f"{idx:>3}"
            parts.append(f"{bg24(r, g, b)}{contrast_fg(r, g, b)}{label}{RESET}")
        return "".join(parts)

    cols = 12  # colors per row in side-by-side mode

    print_section_header("SIDE-BY-SIDE COMPARISON (Standard left │ Interpolated right)")

    # Base 16
    sys.stdout.write("    ANSI base 16:\n")
    for start in (0, 8):
        indices = list(range(start, start + 8))
        left = row_str(std_palette, indices)
        right = row_str(interp_palette, indices)
        sys.stdout.write(f"    {left}  │  {right}\n")
    sys.stdout.write("\n")

    # 216-color cube
    sys.stdout.write("    216-color cube:\n")
    for plane in range(6):
        for row_in_plane in range(6):
            start = 16 + plane * 36 + row_in_plane * 6
            indices = list(range(start, start + 6))
            left = row_str(std_palette, indices)
            right = row_str(interp_palette, indices)
            sys.stdout.write(f"    {left}  │  {right}\n")
        sys.stdout.write("\n")

    # Grayscale
    sys.stdout.write("    Grayscale ramp:\n")
    for start in (232, 244):
        end = min(start + 12, 256)
        indices = list(range(start, end))
        left = row_str(std_palette, indices)
        right = row_str(interp_palette, indices)
        sys.stdout.write(f"    {left}  │  {right}\n")
    sys.stdout.write("\n")


# ─── Main ────────────────────────────────────────────────────────────

def main():
    theme_name = sys.argv[1] if len(sys.argv) > 1 else "xterm"

    if theme_name not in THEMES:
        print(f"Unknown theme: {theme_name}")
        print(f"Available: {', '.join(THEMES.keys())}")
        sys.exit(1)

    theme = THEMES[theme_name]
    base16 = [hex_to_rgb(h) for h in theme["base16"]]
    bg = hex_to_rgb(theme["bg"]) if "bg" in theme else None
    fg = hex_to_rgb(theme["fg"]) if "fg" in theme else None

    std_palette = standard_256_palette()
    interp_palette = generate_256_palette(base16, bg, fg)

    print(f"\n  Theme: \033[1m{theme_name}\033[0m")
    print(f"  Standard palette uses hardcoded xterm RGB values.")
    print(f"  Interpolated palette derives 240 colors from the 16 base colors via LAB trilinear interpolation.")

    print_palette_grid(std_palette, "STANDARD 256-COLOR PALETTE (default xterm RGB values)")
    print_palette_grid(interp_palette, f"INTERPOLATED 256-COLOR PALETTE (from {theme_name} base16)")
    print_side_by_side(std_palette, interp_palette)

main()
PYEOF
