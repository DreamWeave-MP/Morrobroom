import re
import unittest

from support import REPOSITORY  # noqa: F401  (puts tools/ on sys.path)
from dreamweave.model import PALETTES

BLOCK_START = re.compile(r'^(?=:root$|\[data-palette="[a-z]+"\]$)', re.M)
BLOCK_NAME = re.compile(r'^(?::root|\[data-palette="([a-z]+)"\])$', re.M)
COLOR_TOKEN = re.compile(r"^\s+--dw-([a-z0-9-]+):\s*(#[0-9a-fA-F]{6})\s*$", re.M)
SURFACES = ("bg-0", "bg-1", "bg-2", "bg-3")
GISCUS_ORDER = ("bg-0", "bg-1", "bg-2", "bg-3", "line", "line-strong", "accent", "text", "text-muted", "text-faint")
SURFACE_STEP = 0.036
STEP_TOLERANCE = 0.006


def channels(color: str) -> tuple[float, float, float]:
    return tuple(int(color[index:index + 2], 16) / 255 for index in (1, 3, 5))


def linear(channel: float) -> float:
    return channel / 12.92 if channel <= 0.04045 else ((channel + 0.055) / 1.055) ** 2.4


def luminance(color: str) -> float:
    red, green, blue = (linear(channel) for channel in channels(color))
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast(foreground: str, background: str) -> float:
    lighter, darker = sorted((luminance(foreground), luminance(background)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def oklab_lightness(color: str) -> float:
    red, green, blue = (linear(channel) for channel in channels(color))
    long = (0.4122214708 * red + 0.5363325363 * green + 0.0514459929 * blue) ** (1 / 3)
    medium = (0.2119034982 * red + 0.6806995451 * green + 0.1073969566 * blue) ** (1 / 3)
    short = (0.0883024619 * red + 0.2817188376 * green + 0.6299787005 * blue) ** (1 / 3)
    return 0.2104542553 * long + 0.7936177850 * medium - 0.0040720468 * short


def read_palettes() -> dict[str, dict[str, str]]:
    """Each palette's colors, with the purple defaults in :root filling what a palette leaves out."""
    text = (REPOSITORY / "sass/_tokens.sass").read_text()
    declared: dict[str, dict[str, str]] = {}
    for block in BLOCK_START.split(text):
        name = BLOCK_NAME.match(block.split("\n", 1)[0])
        if not name:
            continue
        colors = {token: value.lower() for token, value in COLOR_TOKEN.findall(block)}
        if colors:
            declared[name.group(1) or "purple"] = colors
    return {name: {**declared["purple"], **colors} for name, colors in declared.items()}


class Palettes(unittest.TestCase):
    palettes = read_palettes()

    def test_every_palette_the_config_accepts_is_defined(self):
        self.assertEqual(sorted(self.palettes), sorted(PALETTES))

    def test_text_is_readable_on_every_surface(self):
        for name, colors in self.palettes.items():
            for surface in SURFACES:
                with self.subTest(palette=name, surface=surface):
                    self.assertGreaterEqual(contrast(colors["text"], colors[surface]), 4.5)
                    self.assertGreaterEqual(contrast(colors["text-muted"], colors[surface]), 4.5)
                    self.assertGreaterEqual(contrast(colors["text-faint"], colors[surface]), 4.5 if surface != "bg-3" else 3)
                    self.assertGreaterEqual(contrast(colors["accent"], colors[surface]), 4.5, "the accent is the link color")

    def test_surfaces_step_up_by_the_same_lightness_in_every_palette(self):
        for name, colors in self.palettes.items():
            lightness = [oklab_lightness(colors[surface]) for surface in SURFACES]
            for darker, lighter in zip(lightness, lightness[1:]):
                with self.subTest(palette=name):
                    self.assertAlmostEqual(lighter - darker, SURFACE_STEP, delta=STEP_TOLERANCE)

    def test_giscus_themes_carry_the_palettes_colors(self):
        for name, colors in self.palettes.items():
            theme = (REPOSITORY / f"sass/giscus/{name}.sass").read_text().lower()
            arguments = re.search(r"\+giscus-theme\(([^)]*)\)", theme).group(1)
            with self.subTest(palette=name):
                self.assertEqual([argument.strip() for argument in arguments.split(",")], [colors[token] for token in GISCUS_ORDER])

    def test_the_favicon_mark_carries_the_palettes_colors(self):
        for name, colors in self.palettes.items():
            mark = "mark.svg" if name == "purple" else f"mark-{name}.svg"
            drawing = (REPOSITORY / "static/img" / mark).read_text().lower()
            with self.subTest(palette=name):
                self.assertIn(f'fill="{colors["bg-1"]}"', drawing)
                self.assertIn(f'stroke="{colors["accent"]}"', drawing)


if __name__ == "__main__":
    unittest.main()
